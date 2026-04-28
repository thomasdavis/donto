# donto-tui

Terminal UI for monitoring and exploring a live donto database. Built with Go and [Charm](https://charm.sh) (Bubble Tea, Lip Gloss, Bubbles).

## Install

```bash
# From the repo root:
cd apps/donto-tui
go build -o ../../target/donto-tui .

# Or via just:
just tui-build
```

Requires Go 1.21+.

## Run

```bash
# Against the default local dev database:
just tui

# With explicit DSN:
just tui --dsn 'postgres://donto:donto@127.0.0.1:55432/donto'

# Install LISTEN/NOTIFY triggers for real-time streaming:
just tui --install-triggers

# Or run directly:
cd apps/donto-tui && go run . --dsn 'postgres://...'
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--dsn` | `$DONTO_DSN` or `postgres://donto:donto@127.0.0.1:55432/donto` | Postgres connection string |
| `--poll` | `5s` | Dashboard refresh interval |
| `--srv` | `http://127.0.0.1:7878` | dontosrv URL for health checks |
| `--install-triggers` | off | Install LISTEN/NOTIFY triggers on connect |

## Tabs

### 1 - Dashboard

System overview with live-updating stats:

- **Health** — Postgres and dontosrv connection status
- **Totals** — Statement, context, predicate, and audit counts (live via `pg_stat_user_tables`)
- **Maturity Distribution** — Colored gauge bars (L0-L4) from a sampled approximation
- **Polarity** — Asserted/negated/absent/unknown breakdown
- **Activity** — 24-hour sparkline from the audit log
- **Obligations** — Open proof obligation summary

### 2 - Firehose

Real-time stream of all database activity:

- **Live Queries** — Shows active `pg_stat_activity` connections and their SQL (catches all clients including bulk importers)
- **Audit Log** — Committed assert/retract/correct events with parsed detail (context, polarity, etc.)
- Listens on both `donto_audit` and `donto_firehose` NOTIFY channels to capture inserts from all code paths
- `p` pause/resume, `a` filter by action, `j/k` scroll, `Enter` opens claim card

### 3 - Explorer

Browse and search statements:

- Loads 200 most recent statements by default (fast: uses audit table join)
- `/` to open filter pane (subject, predicate, context)
- Filters use indexed queries (SPO, predicate, context indexes)
- `j/k` navigate, `Enter` opens claim card, `esc` closes filter
- Full-width table with subject, predicate, object, context, polarity, maturity

### 4 - Contexts

List of all contexts with kind and parent info.

### 5 - Claim Card

Deep-dive on a single statement. Shows the full `donto_claim_card()` output: statement fields, evidence links, arguments, obligations, shape annotations. Reached by pressing `Enter` on any statement in the Explorer or Firehose.

## Keybindings

| Key | Action |
|-----|--------|
| `1`-`5` | Switch tabs |
| `Tab` / `Shift+Tab` | Next / previous tab |
| `q` / `Ctrl+C` | Quit |
| `?` | Toggle help overlay |
| `j` / `k` | Scroll down / up |
| `p` | Pause firehose |
| `a` | Cycle action filter (firehose) |
| `/` | Open filter pane (explorer) |
| `Esc` | Close filter pane |
| `Enter` | Select statement / search |

## LISTEN/NOTIFY Triggers

The TUI installs two triggers for real-time streaming:

1. `donto_audit_notify_trg` on `donto_audit` — fires on assert/retract/correct via `donto_assert()`
2. `donto_statement_notify_trg` on `donto_statement` — fires on every INSERT regardless of code path (catches bulk importers like `doks`)

Install manually:
```bash
just tui-triggers
# or
docker exec -i donto-pg psql -U donto -d donto < apps/donto-tui/sql/notify_trigger.sql
```

To remove for heavy bulk loads:
```sql
DROP TRIGGER donto_statement_notify_trg ON donto_statement;
DROP TRIGGER donto_audit_notify_trg ON donto_audit;
```
