# donto

A bitemporal, paraconsistent quad store implemented as a PostgreSQL
extension with Lean 4 as a sidecar for shape validation, derivations, and
machine-checkable certificates. Native query language: **DontoQL**.

> **Status:** all eleven phases of [PRD.md](PRD.md) §26 have first
> implementations on `main`. The Postgres SQL surface is the source of
> truth; Rust crates wrap it; dontosrv exposes HTTP and the sidecar
> protocol; the Lean overlay is in place as type and combinator
> definitions with the standard library mirrored as Rust built-ins so the
> system runs end-to-end without Lean. See [`docs/`](docs/) for the user
> guide, operator guide, and per-phase plan.
>
> 25 integration tests pass against a live Postgres 16 container.

## Repo layout

```
PRD.md                          Source of truth.
sql/migrations/                 Schema and SQL functions (0001 → 0011).
sql/fixtures/                   Tiny N-Quads fixture for smoke tests.
extension/                      pg_donto.control + Makefile (PGXS).
crates/donto-client/            Typed Rust client for the SQL surface.
crates/donto-cli/               `donto` CLI: migrate, ingest, match, query, retract, bench.
crates/donto-query/             DontoQL parser, SPARQL 1.1 subset, internal algebra, evaluator.
crates/donto-ingest/            N-Quads, Turtle, TriG, RDF/XML, JSON-LD, JSONL, CSV, property-graph.
crates/donto-migrate/           Migrators: SQLite genealogy (PRD §24).
crates/dontosrv/                Sidecar: HTTP + DIR + shape/rule/certificate handlers.
lean/                           Lean overlay: Core, IR, Shapes, Rules, Certificates, Engine.
docs/                           User guide, operator guide, phase plans.
scripts/                        Dev infra (Postgres docker harness).
```

## Quickstart

Requires Docker, Rust (`rustup default stable`), and optionally `just`.

```bash
# 1. Start Postgres 16.
./scripts/pg-up.sh

# 2. Apply migrations.
cargo run -p donto-cli --quiet -- migrate

# 3. Ingest the bundled N-Quads fixture.
cargo run -p donto-cli --quiet -- ingest sql/fixtures/lubm-tiny.nq

# 4. Query.
cargo run -p donto-cli --quiet -- query \
    'MATCH ?s <http://example.org/name> ?n PROJECT ?s, ?n'

# 5. Run the sidecar (in another terminal).
cargo run -p dontosrv

# 6. Hit the HTTP API.
curl http://127.0.0.1:7878/version
curl -X POST http://127.0.0.1:7878/dontoql \
     -H 'content-type: application/json' \
     -d '{"query":"MATCH ?s <http://example.org/name> ?n PROJECT ?s, ?n"}'
```

## Surfaces

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
```

```sparql
PREFIX ex: <http://example.org/>
SELECT ?x ?y WHERE { ?x ex:knows ?y . } LIMIT 10
```

```dontoql
PRESET latest
MATCH ?x ex:knows ?y, ?y ex:name ?n
FILTER ?n != "Mallory"
POLARITY asserted
MATURITY >= 1
PROJECT ?x, ?n
LIMIT 10
```

## Tests

```bash
./scripts/pg-up.sh
DONTO_TEST_DSN=postgres://donto:donto@127.0.0.1:55432/donto cargo test --workspace
```

Coverage areas:
- assert / retract / correct / match round-trips, idempotency
- bitemporal as-of and valid-time
- scope inheritance (descendants, ancestors, exclude)
- paraconsistency (contradictions coexist; negated/absent hidden)
- DontoQL and SPARQL parser → evaluator → live DB
- shape validation (FunctionalPredicate, DatatypeShape) via dontosrv
- transitive-closure derivation via dontosrv
- certificate attach + verify

## Migrating from existing stores

```bash
# RDF (any quad store): just ingest the dump.
donto ingest dump.nq

# Property graph (Neo4j export):
donto ingest export.json --format property-graph

# Genealogy SQLite (research.db, PRD §24):
donto-migrate genealogy /path/to/research.db --root ctx:genealogy/research-db
```

## License

Apache-2.0 (extension, sidecar). MIT for client libraries.
