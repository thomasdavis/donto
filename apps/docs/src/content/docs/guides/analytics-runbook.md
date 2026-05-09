---
title: Analytics Runbook
description: Telemetry analysis detectors, scheduling, findings, and health monitoring
---

## Overview

Two detectors ship with donto. Both write to `donto_detector_finding` and emit
one `_self`/`info` row per run for self-monitoring.

| Detector | Command | Table written |
|---|---|---|
| Rule-duration regression | `donto analyze rule-duration` | `donto_detector_finding` |
| Paraconsistency density | `donto analyze paraconsistency` | `donto_paraconsistency_density` |

Run `donto migrate` before first use. Requires migrations 0118-0120.

---

## `donto analyze rule-duration`

Detects regressions in Lean rule evaluation latency.

**Algorithm**: rolling 30-day MAD-z per rule. Flags runs where `MAD-z > k`
(default `k=5`). Also flags rules where `null_rate(duration_ms)` exceeds 30%
in the trailing 24 h -- this signals sidecar health issues.

```bash
# Default: look back 7 days, k=5.
donto analyze rule-duration

# Tune the lookback and sensitivity.
donto analyze rule-duration --since '90 days' --k 3.0

# Use a custom detector IRI (for multi-version deployment).
donto analyze rule-duration --detector-iri donto:detector/rule-duration/v2
```

Output is a single JSON line with run summary:

```json
{"run_id":"...","rules_examined":12,"anomaly_findings":1,"null_rate_findings":0,"overall_null_rate":0.02,"self_finding_id":47}
```

Exit code 0 always (findings are informational). Non-zero only on DB errors.

**Tuning `--k`**:
- Lower `k` (e.g. 3.0): more sensitive, more false positives on spiky rules.
- Higher `k` (e.g. 7.0): fewer alerts, may miss real regressions.
- Start at the default (5.0). Tighten after observing the false-positive rate
  for your workload over two weeks.

---

## `donto analyze paraconsistency`

Aggregates (subject, predicate) pairs with conflicting polarities into
`donto_paraconsistency_density`, then populates:

- `donto_v_top_contested_predicates` -- predicates with highest total conflict
- `donto_v_top_contested_subjects`   -- subjects with highest peak conflict

In addition to the density table, pairs whose `conflict_score` exceeds
`--min-emit-score` (default `0.6`) are written to `donto_detector_finding`
with `target_kind='predicate_pair'` and `severity='warning'` (or `'critical'`
above `0.9`). A `_self` info finding is always written so this detector
shows up in `donto analyze health` alongside `rule-duration`.

`--alert-sink` works the same way as for `rule-duration`: above-info
findings are forwarded to the sink in addition to being persisted.

```bash
# Default: trailing 24 h window, emit findings for pairs scoring >= 0.6.
donto analyze paraconsistency

# 90-day window (matches CI scale).
donto analyze paraconsistency --window-hours 2160

# Stricter emit threshold + forward to a JSONL file.
donto analyze paraconsistency \
  --window-hours 2160 \
  --min-emit-score 0.8 \
  --alert-sink file:///var/log/donto-conflicts.jsonl

# Pin a per-environment detector IRI for multi-version deployments.
donto analyze paraconsistency \
  --detector-iri donto:detector/paraconsistency/staging-v1

# Explicit window.
donto analyze paraconsistency \
  --start 2026-01-01T00:00:00Z \
  --end   2026-04-01T00:00:00Z
```

**Query findings after a run**:

```sql
-- Top contested predicates this week.
-- Three sort axes, pick based on what you care about:
--   total_score  — cumulative conflict volume over the window; biased toward
--                  predicates that appear in many statements (volume bias).
--   max_score    — peak conflict intensity in any single window; useful for
--                  spotting sudden, high-intensity disagreements.
--   avg_score    — mean conflict per window; surfaces predicates with
--                  consistent, sustained contestation rather than one-off spikes.
select predicate, total_score, max_score, avg_score, windows
from donto_v_top_contested_predicates
order by total_score desc   -- change to max_score or avg_score as needed
limit 20;

-- Raw density rows for a specific predicate.
select subject, window_start, distinct_polarities,
       distinct_contexts, conflict_score
from donto_paraconsistency_density
where predicate = 'ex:birthYear'
  and distinct_polarities >= 2
order by conflict_score desc;
```

**Window sizing**: shorter windows surface recent conflicts; longer windows
surface persistent disagreements. 2160 h (90 days) is a good default for
trend detection. Use 24-48 h for near-real-time alerting.

---

## Scheduling

### Linux cron

```
# /etc/cron.d/donto-analytics
# Run rule-duration daily at 02:00, paraconsistency weekly at 03:00 Sunday.
DONTO_DSN=postgres://donto:donto@127.0.0.1:55432/donto
PATH=/usr/local/bin:/usr/bin:/bin

0 2 * * *     donto_user  /usr/local/bin/donto analyze rule-duration --since '90 days'
0 3 * * 0     donto_user  /usr/local/bin/donto analyze paraconsistency --window-hours 2160
```

### Linux systemd timers

`/etc/systemd/system/donto-rule-duration.service`:

```ini
[Unit]
Description=donto rule-duration detector

[Service]
Type=oneshot
User=donto
Environment=DONTO_DSN=postgres://donto:donto@127.0.0.1:55432/donto
ExecStart=/usr/local/bin/donto analyze rule-duration --since 90 days
```

`/etc/systemd/system/donto-rule-duration.timer`:

```ini
[Unit]
Description=Run donto rule-duration daily

[Timer]
OnCalendar=*-*-* 02:00:00
Persistent=true

[Install]
WantedBy=timers.target
```

```bash
systemctl enable --now donto-rule-duration.timer
```

### Windows Task Scheduler

```powershell
$action = New-ScheduledTaskAction `
    -Execute "donto.exe" `
    -Argument "analyze rule-duration --since ""90 days""" `
    -WorkingDirectory "C:\donto"

$trigger = New-ScheduledTaskTrigger -Daily -At "02:00"

$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable

Register-ScheduledTask `
    -TaskName "DontoRuleDuration" `
    -Action $action `
    -Trigger $trigger `
    -Settings $settings `
    -RunLevel Highest
```

Set `DONTO_DSN` as a system environment variable or pass `--dsn` explicitly.

---

## Reading findings

```sql
-- Recent warnings and criticals, newest first.
select finding_id, detector_iri, target_kind, target_id,
       severity, observed_at,
       payload->>'mad_zscore'  as z_score,
       payload->>'null_rate'   as null_rate
from donto_detector_finding
where severity in ('warning', 'critical')
  and observed_at > now() - interval '7 days'
order by observed_at desc
limit 50;

-- Self-metrics for the most recent run of each detector.
select distinct on (detector_iri)
       detector_iri,
       observed_at                              as last_run_at,
       payload->>'findings_count'               as findings_count,
       (payload->>'null_rate_observed')::float  as null_rate
from donto_detector_finding
where target_kind = '_self'
order by detector_iri, observed_at desc;
```

---

## `donto analyze health`

Detect-the-detector: verifies that all known detectors have run recently and
that their reported `null_rate` is below the threshold.

```bash
# Default: fail if any detector is stale >24 h or null_rate >30%.
donto analyze health

# Relax stale threshold for weekly schedules.
donto analyze health --max-age-hours 200

# Stricter null-rate gate.
donto analyze health --max-null-rate 0.1
```

Output (newline-delimited JSON, one object per detector):

```json
{"detector_iri":"donto:detector/rule-duration/v1","last_run_at":"2026-05-09T02:00:00Z","age_hours":6,"last_findings_count":1,"null_rate_observed":0.02,"stale":false,"high_null_rate":false}
```

Exit code 0 if all detectors pass; non-zero if any are stale or over
`--max-null-rate`. Use in an uptime monitor or a separate cron/timer:

```bash
donto analyze health || mail -s "donto detector health failed" ops@example.com
```

---

## Alert sink configuration

By default, findings are written to `donto_detector_finding` only. To also
forward above-`info` findings to an external channel, set `$DONTO_ALERT_SINK`
or pass `--alert-sink`:

```bash
# Write to stdout (pipe to a log aggregator or jq).
DONTO_ALERT_SINK=stdout donto analyze rule-duration

# Append to a JSONL file.
DONTO_ALERT_SINK=file:///var/log/donto-alerts.jsonl \
  donto analyze rule-duration --since '90 days'

# Per-invocation flag overrides env.
donto analyze rule-duration --alert-sink file:///tmp/alerts.jsonl
```

The sink only receives findings with `severity='warning'` or
`severity='critical'`. The `_self`/`info` row is DB-only regardless.

Supported sinks (Phase 0):

| Spec | Behaviour |
|---|---|
| `stdout` | JSON lines to stdout |
| `file:///abs/path` | Append JSON lines to file |
| unset | DB-only (default) |

Slack/webhook sinks are planned for Phase 10+.

Passing an unrecognized scheme (e.g. `slack://...`) now returns an error
immediately rather than silently falling back to stdout.  This is intentional:
a misconfigured sink should fail loudly so alerts are not lost.

---

## Retention

Migration `0122_detector_finding_retention.sql` exposes
`donto_detector_finding_prune(p_keep_days int default 90)`.  Without periodic
pruning `donto_detector_finding` grows without bound: every detector run writes
at least one `_self`/`info` row, and workloads with frequent anomaly windows
can accumulate thousands of warning/critical rows per day.

**What the function does**: deletes all rows in `donto_detector_finding` whose
`observed_at` is older than `p_keep_days` days.  Rows within the retention
window are untouched.  The function returns the count of deleted rows.

**Scheduling examples**:

Linux cron (add to `/etc/cron.d/donto-analytics`):

```cron
# Prune detector findings older than 90 days, daily at 04:00.
0 4 * * *   donto_user  psql "$DONTO_DSN" -c "select donto_detector_finding_prune(90);"
```

Linux systemd timer — add a `donto-finding-prune.service` / `.timer` pair
following the same pattern as the `donto-rule-duration` pair in the Scheduling
section above, with `ExecStart` set to:

```bash
psql postgres://donto:donto@127.0.0.1:55432/donto -c "select donto_detector_finding_prune(90);"
```

Windows Task Scheduler:

```powershell
$action = New-ScheduledTaskAction `
    -Execute "psql.exe" `
    -Argument "-c ""select donto_detector_finding_prune(90);"" $env:DONTO_DSN"
$trigger = New-ScheduledTaskTrigger -Daily -At "04:00"
Register-ScheduledTask -TaskName "DontoFindingPrune" -Action $action `
    -Trigger $trigger -RunLevel Highest
```

**Tuning `p_keep_days`**:

| Value | Use case |
|---|---|
| 30 | High-volume workloads, findings reviewed daily; short audit window acceptable |
| 90 | Default; covers a full quarter for trend analysis |
| 365 | Regulated environments or long-term ML model drift tracking |

**`_self` self-metric rows**: the `_self`/`info` rows written by each detector
run are subject to the same retention window.  If you need a permanent audit
trail of when detectors ran and what they reported, archive those rows to a
separate table before pruning:

```sql
insert into donto_detector_finding_archive
select * from donto_detector_finding
where target_kind = '_self'
  and observed_at < now() - interval '90 days';

select donto_detector_finding_prune(90);
```

---

## Known caveats

### `SET LOCAL donto.actor` in bulk maturity promotions

If you write custom bulk-update code that relies on `SET LOCAL donto.actor` to
stamp the correct actor into maturity audit rows (the same technique used by the
maturity-promotion path in `donto_statement`), you must wrap both the `SET LOCAL`
statement and the `UPDATE` in a single explicit transaction block.  `SET LOCAL`
only persists for the duration of the current transaction; if the two statements
execute in separate implicit transactions (as they will in many ORM and scripting
contexts), the `SET LOCAL` has no effect when the `UPDATE` runs.  The reference
implementation is the data-engineer's chunked-promotion fix — use that as the
pattern for any new bulk-update code that needs actor attribution.

---

## Troubleshooting

### High `null_rate_observed`

`null_rate_observed > 0` means `donto_derivation_report.duration_ms` was NULL
for some rule evaluations. This indicates the Lean sidecar timed out or was
unavailable during those evaluations.

1. Check sidecar logs: `journalctl -u dontosrv -n 100`.
2. Confirm the sidecar can reach the Lean engine:
   `curl http://localhost:9000/health`.
3. If transient, the next run will clear the flag once the sidecar is stable.
4. If persistent and above `--max-null-rate`, the health check will gate
   alerting pipelines.

### Detectors falsely alerting (low `k`)

Frequent `warning` findings for rules that have not actually regressed:

1. Check the payload `mad_zscore` value. If consistently 5-7, raise `--k`:

   ```bash
   donto analyze rule-duration --k 7.0
   ```

2. For rules with naturally high variance (e.g. rules over large scopes),
   consider registering a per-rule `k` override -- this is Phase 10+.

3. Exclude noisy rules from downstream queries:

   ```sql
   select * from donto_detector_finding
   where target_id not in ('ex:rule/noisy-rule-1')
     and severity = 'warning';
   ```

### `donto_v_top_contested_predicates` is empty

1. Run the paraconsistency analyzer first: `donto analyze paraconsistency`.
2. Widen the window: `--window-hours 2160`.
3. Confirm data exists in the window:

   ```sql
   select count(*)
   from donto_statement
   where upper(tx_time) is null
     and lower(tx_time) > now() - interval '90 days';
   ```

4. A clean dataset with no polarity conflicts will produce an empty view.
   This is correct behaviour, not an error.
