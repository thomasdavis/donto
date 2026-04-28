---
title: Quick Start
description: Get donto running locally in under 5 minutes
sidebar:
  order: 2
---

## Prerequisites

- Docker (for Postgres 16)
- Rust 1.78+ (`rustup`)
- Optional: Lean 4.12+ (`elan`) for the verification sidecar

## 1. Start Postgres

```bash
./scripts/pg-up.sh
```

This starts a Postgres 16 container on port 55432 with user/password `donto/donto`.

## 2. Install the CLI

```bash
cargo install --path apps/donto-cli --locked --force
```

## 3. Apply migrations

```bash
donto migrate
```

## 4. Ingest some data

```bash
donto ingest sql/fixtures/lubm-tiny.nq
```

## 5. Query

```bash
donto query 'MATCH ?s ?p ?o LIMIT 5' | jq
```

## Connection

The CLI connects via `--dsn` flag, `DONTO_DSN` env var, or defaults to `postgres://donto:donto@127.0.0.1:55432/donto`.

## Optional: Lean sidecar

```bash
cd lean && lake build && cd ..
cargo run -p dontosrv -- \
  --dsn 'postgres://donto:donto@127.0.0.1:55432/donto' \
  --bind 127.0.0.1:7878 \
  --lean-engine "$(pwd)/lean/.lake/build/bin/donto_engine"
```

Now `lean:` shape IRIs are available via the HTTP API at `localhost:7878`.

## Next steps

- [User Guide](/donto/guides/user-guide/) — ingestion formats, querying, scopes, snapshots
- [Operator Guide](/donto/guides/operator-guide/) — deployment topology, backup, observability
- [CLI Reference](/donto/reference/cli/) — full subcommand documentation
