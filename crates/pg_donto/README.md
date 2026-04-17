# pg_donto

The donto Postgres extension. Built with [pgrx](https://github.com/pgcentralfoundation/pgrx).

This crate is **not** a workspace member of the top-level `Cargo.toml` —
pgrx requires Postgres dev headers and a one-time `cargo pgrx init` step
that breaks ordinary workspace builds. Build it explicitly.

## Easy path: Docker (no host sudo, no host pg-dev headers)

```bash
./scripts/pgrx-build.sh           # default: pg16
./scripts/pgrx-build.sh 17        # try pg17
```

This builds an image with the right Postgres dev headers, runs
`cargo pgrx package`, and exercises the `#[pg_test]` suite. CI uses
the same image (`.github/workflows/ext.yml`).

## Prerequisites

- Postgres 13–17 with development headers (`libpq-dev`, `postgresql-server-dev-16`
  on Debian/Ubuntu).
- Rust stable.
- pgrx-cli: `cargo install --locked cargo-pgrx --version 0.12.7`
- One-time pgrx setup against a system Postgres install:
  `cargo pgrx init --pg16 $(which pg_config)`

## Build & install

```bash
cd crates/pg_donto
cargo pgrx package         # produces target/release/pg_donto-pg16/
cargo pgrx install         # copies into the running Postgres install
```

Then in psql:

```sql
CREATE EXTENSION pg_donto;
\df donto_*
SELECT * FROM donto_version();
```

## Run the pgrx test harness

```bash
cd crates/pg_donto
cargo pgrx test pg16
```

This spins up an ephemeral Postgres, installs the extension, and runs
the `#[pg_test]` functions in `src/lib.rs`.

## Relationship to the rest of the workspace

- The SQL files in `sql/migrations/` are the source of truth for the
  schema and SQL functions.
- `pg_donto` includes those SQL files via `extension_sql_file!`. Editing
  the migrations and rebuilding the extension is sufficient.
- A small set of Rust helpers (`donto_pack_flags_rs`, `donto_polarity_rs`,
  `donto_maturity_rs`, `donto_version_rs`) shadow the plpgsql versions.
  Tests assert they agree.
- The Rust client crate (`donto-client`) talks to either the plpgsql
  surface (default) or the extension surface — both expose the same
  function names.

## Why two implementations of the same functions?

Two reasons:
1. The plpgsql versions are easy to read, easy to change, and don't
   require extension packaging. They're great for development.
2. The Rust versions are `IMMUTABLE` and `PARALLEL SAFE` from a planner
   perspective, which matters for indexability of expressions that use
   them. They also serve as differential tests: any behavior drift
   between Rust and plpgsql becomes a test failure.
