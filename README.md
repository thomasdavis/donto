# donto

A bitemporal, paraconsistent quad store implemented as a PostgreSQL extension
with Lean 4 as a sidecar for shape validation, derivations, and certificates.

> **Status:** Phase 0 spike. The schema, SQL surface, Rust client, N-Quads
> loader, and CLI are runnable end-to-end. The Lean sidecar, predicate
> registry, DontoQL, SPARQL, and shape/rule machinery are later phases.
> Read [`PRD.md`](PRD.md) for the full design and [`docs/PHASE-0.md`](docs/PHASE-0.md)
> for what's in scope right now.

## Design touchstones

- **Atom is the statement** (`subject, predicate, object, context, valid_time, tx_time, flags`).
- **Paraconsistent**: contradictions coexist; consistency is a query, not a constraint.
- **Bitemporal**: every statement carries valid-time and transaction-time.
  Retraction closes tx_time; nothing is ever deleted.
- **Contexts everywhere**: provenance, snapshots, hypotheses, trust all reduce
  to named graphs (contexts) with kind and parent.
- **Open-world predicates**: the predicate space grows at runtime;
  aliases and canonicals are first-class (Phase 3).
- **Lean certifies, doesn't gate**: the sidecar is optional; the database
  stays usable without it (Phase 5+).

## Repo layout

```
sql/migrations/    Phase 0 schema and plpgsql functions (source of truth).
sql/fixtures/      Tiny N-Quads fixture for smoke tests.
crates/donto-client/  Rust client for the SQL surface (assert, match, retract, correct).
crates/donto-cli/     `donto` command-line: migrate, ingest, match, retract.
docs/              Phase plans and design notes.
scripts/           Dev infra (Postgres docker harness).
```

## Quickstart

Requires Docker, Rust (`rustup default stable`), and optionally
[`just`](https://github.com/casey/just).

```bash
# 1. Bring up Postgres 16 in a container.
./scripts/pg-up.sh

# 2. Apply migrations.
cargo run -p donto-cli -- migrate

# 3. Ingest the bundled N-Quads fixture.
cargo run -p donto-cli -- ingest sql/fixtures/lubm-tiny.nq

# 4. Query.
cargo run -p donto-cli -- match --predicate http://example.org/name
```

Or, if you have `just`:

```bash
just smoke
```

## Running the tests

The integration tests need a reachable Postgres. They self-skip if they cannot
connect.

```bash
./scripts/pg-up.sh
DONTO_TEST_DSN=postgres://donto:donto@127.0.0.1:55432/donto cargo test --workspace
```

## Phase 0 surface

```rust
use donto_client::{ContextScope, DontoClient, Object, Polarity, StatementInput};

let c = DontoClient::from_dsn("postgres://donto:donto@127.0.0.1:55432/donto")?;
c.migrate().await?;

c.ensure_context("ctx:src/wikipedia", "source", "permissive", None).await?;

let id = c.assert(&StatementInput::new(
    "ex:alice", "ex:knows", Object::iri("ex:bob"),
).with_context("ctx:src/wikipedia")).await?;

let rows = c.match_pattern(
    Some("ex:alice"), None, None,
    Some(&ContextScope::just("ctx:src/wikipedia")),
    Some(Polarity::Asserted), 0, None, None,
).await?;

c.retract(id).await?;
```

## License

Apache-2.0 (extension, sidecar). See [`LICENSE`](LICENSE) when added.
