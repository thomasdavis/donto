---
title: Operator Guide
description: Deployment topology, backup, observability, and failure modes
---

## Topology

```
                    +-----------+
                    |  Clients  |
                    +-----+-----+
                          |
              +-----------+------------+
              |                        |
        SQL via libpq           HTTP via dontosrv
              |                        |
              v                        v
   +----------------+         +-----------------+
   |  Postgres 16+  |  <----  |    dontosrv     |  ---->  Lean engine (donto_engine)
   |   (pg_donto)   |  pool   |  (axum + Rust)  |  fork    sidecar protocol over stdio
   +----------------+         +-----------------+
```

`pg_donto` (Phase 1+) is a Postgres extension. dontosrv is a sidecar process
that hosts the SPARQL/DontoQL HTTP endpoints, the DIR endpoint, the
shape/rule built-ins, and certificate verification. The Lean engine is
optional.

## Sizing

- One Postgres node handles the v1 target of 10^9 statements. 128 GB RAM is the reference.
- dontosrv is stateless w.r.t. the database — run as many replicas as you
  need behind a load balancer.
- The Lean engine is single-threaded per process; spawn N for parallel
  shape/rule fan-out. dontosrv pools connections to it.

## Migrations

```bash
# Idempotent. Safe to run repeatedly. Records each applied migration in
# donto_migration with its sha256.
donto migrate
```

If a migration's sha256 changes, the runner re-applies it. To inspect:

```sql
select name, applied_at, encode(sha256, 'hex') from donto_migration order by applied_at;
```

## Backup

Standard `pg_dump` works. Two notes:

- `donto_audit` grows linearly with writes. Truncate to a checkpoint before
  shipping snapshots.
- Snapshot membership tables (`donto_snapshot_member`) are large but
  compress well.

## Observability

```sql
-- Per-context statement counts and ages.
select * from donto_stats_context;

-- Maturity-level histogram per context.
select * from donto_stats_maturity order by context, maturity;

-- Predicate usage.
select * from donto_stats_predicate where use_count > 0 order by use_count desc;

-- Shape and rule history.
select * from donto_stats_shape;
select * from donto_stats_rule;

-- Audit summary.
select * from donto_stats_audit;
```

A Prometheus exporter for these views is on the Phase 10 follow-on list.
For now, scrape via a small adapter:

```bash
psql -At -c "select 'donto_statements_total{context=\"' || iri || '\"} ' || statement_count from donto_stats_context"
```

## Sidecar operational contract

- The database stays usable when dontosrv is down. SQL queries, retractions,
  and ingestion all work.
- Shape validation, rule derivation, and certificate verification calls
  through dontosrv:
  * If dontosrv is up but the Lean engine is down, requests addressed to
    `lean://` shape/rule IRIs return `sidecar_unavailable`. Built-in
    (`builtin:`) IRIs continue to work.
  * If dontosrv is down, applications get the full `sidecar_unavailable`
    response from a connection failure.
- Reports and certificates are cached in Postgres. A second call with the
  same fingerprint returns the cached result.

## Authoring shapes (Lean)

```lean
-- lean/Donto/Project/MyShape.lean
import Donto

open Donto Donto.Shapes

def myShape : Shape := StdLib.functional "ex:spouse"
```

Then build the engine:

```bash
cd lean && lake build donto_engine
```

dontosrv discovers Lean shapes via `--lean-engine /path/to/donto_engine`.
The standard library shapes ship as Rust built-ins so they need no Lean.

## Failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| `dontosrv` returns `sidecar_unavailable` for a `lean:` shape | Lean engine not running | start it; or use `builtin:` equivalent |
| Migration fails with "predicate not registered" | Curated context with unregistered predicate | `select donto_register_predicate('...')` first |
| `donto_assert` returns the same UUID twice | Idempotent re-assert (expected) | this is by design |
| Slow point queries | Missing partial-index for non-asserted polarity | add per-workload index |
| Slow `under_hypothesis(h)` queries | Hypothesis tree very deep | snapshot the curated frontier first |

## Capacity tuning

Default indexes cover the documented access patterns. To enable
the optional six-way SPO/POS/OSP/SOP/PSO/OSP set:

```sql
create index donto_statement_sop_idx on donto_statement (subject, object_iri, predicate);
create index donto_statement_pso_idx on donto_statement (predicate, subject, object_iri);
create index donto_statement_ops_idx on donto_statement (object_iri, predicate, subject) where object_iri is not null;
```

Trigram acceleration on string literals:

```sql
create extension pg_trgm;
create index donto_object_lit_trgm
    on donto_statement using gin ((object_lit ->> 'v') gin_trgm_ops)
    where object_lit ? 'v';
```

For corpora at the H1 target (10^9 statements), partition `donto_statement`
by `tx_time` month or by `context` hash; the partition keys are wired so
that the indexes above remain useful.
