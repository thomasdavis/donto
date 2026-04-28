---
title: TUI
description: Terminal UI for monitoring and exploring a live donto database
---

The donto TUI is a terminal dashboard for monitoring a live donto database in real-time. Built with Go and [Charm](https://charm.sh).

## Quick start

```bash
# Build
cd apps/donto-tui && go build -o ../../target/donto-tui .

# Run
./target/donto-tui --dsn 'postgres://donto:donto@127.0.0.1:55432/donto' --install-triggers
```

## Tabs

| Tab | Key | What it shows |
|-----|-----|---------------|
| Dashboard | `1` | Statement counts, maturity/polarity distribution, activity sparkline, obligations |
| Firehose | `2` | Live query stream + audit log (real-time via LISTEN/NOTIFY) |
| Explorer | `3` | Browse/search statements with indexed filters |
| Contexts | `4` | Context list with kind and parent info |
| Claim Card | `5` | Deep-dive on a selected statement |

## Firehose

The firehose shows two types of activity:

- **Live Queries** — polled from `pg_stat_activity`, shows all active SQL connections and their queries
- **Audit Events** — pushed via NOTIFY triggers on both `donto_audit` and `donto_statement` tables

This means the firehose captures activity from all clients, including bulk importers that bypass `donto_assert()`.

## Explorer

The explorer uses indexed queries for fast results on large databases:

- **Default view**: 200 most recent statements via audit table join (~37ms)
- **Subject filter**: uses the SPO btree index (<1ms)
- **Predicate filter**: uses the predicate partial index
- **Context filter**: uses the context btree index

Press `/` to open the filter pane, type a filter, and press `Enter` to search.

## Flags

```
--dsn string           Postgres DSN (default: $DONTO_DSN)
--poll duration        Dashboard poll interval (default 5s)
--srv string           dontosrv URL for health checks (default http://127.0.0.1:7878)
--install-triggers     Install LISTEN/NOTIFY triggers on connect
```
