# Notes for Claude (and other agents)

This file is the working contract for any AI/agent contributing to donto.
Read it once per session. Keep it terse — extend only when you've learned
something a future contributor would otherwise re-learn the hard way.

## What donto is

A bitemporal, paraconsistent quad store. Postgres extension (`pg_donto`) +
Lean 4 sidecar (`dontosrv` + `donto_engine`). Native query language:
DontoQL. Source of truth: [`PRD.md`](PRD.md). Read PRD §3 (principles)
and §2 (the maturity ladder) before changing core types.

## Non-negotiable

- **Paraconsistent.** Never reject contradictions. Two sources can disagree
  about Alice's birth year; both rows live forever.
- **Bitemporal.** `valid_time` (world) and `tx_time` (system). Retract
  closes `tx_time`. Never `delete from donto_statement`.
- **Every statement has a context.** Default is `donto:anonymous`.
  The slot is never empty.
- **Lean certifies, doesn't gate.** Ingestion never waits on the sidecar.
  Sidecar absence degrades shape/rule/cert calls only.
- **Postgres owns execution. Lean owns meaning.** DIR is the boundary.
- **No hidden ordering.** No implicit `ORDER BY`. Aggregations call it out.

## Layout

- `sql/migrations/` — SQL is the source of truth. Idempotent (`if not
  exists`, `create or replace`). Each new migration gets a sequential
  number and an entry in `donto-client/src/migrations.rs::MIGRATIONS`.
- `crates/donto-client` — typed Rust wrapper over the SQL surface.
- `crates/donto-query` — DontoQL + SPARQL subset → algebra → evaluator.
- `crates/donto-ingest` — N-Quads, Turtle, TriG, RDF/XML, JSON-LD, JSONL,
  CSV, property graph, quarantine.
- `crates/donto-migrate` — migrators from external stores (genealogy SQLite).
- `crates/dontosrv` — axum sidecar (HTTP + DIR + shapes/rules/certs).
- `crates/pg_donto` — pgrx-based Postgres extension wrapping the SQL.
- `lean/` — Lean overlay; standard library mirrored as Rust built-ins so
  donto runs without Lean.
- `docs/` — user/operator guides, per-phase plans.

## How to run

Postgres 16 in docker, then cargo:

```
./scripts/pg-up.sh
cargo run -p donto-cli --quiet -- migrate
DONTO_TEST_DSN=postgres://donto:donto@127.0.0.1:55432/donto cargo test --workspace
```

Tests self-skip if Postgres is unreachable; they don't false-pass.

## What to do (and not do)

- **Do** read PRD.md before touching `donto_statement`, contexts, polarity,
  modality, or maturity encoding.
- **Do** add new SQL functions in a new migration file, not by editing
  prior ones (re-applies are signaled by sha256 mismatch in
  `donto_migration` and that's OK, but the diff stays attributable).
- **Do** add tests that assert PRD invariants (paraconsistency, bitemporal
  correctness, scope inheritance, idempotency). See
  `crates/donto-client/tests/invariants.rs` for patterns.
- **Do** skip a test cleanly when Postgres is missing — never panic in
  setup; use the `pg_or_skip!` pattern.
- **Don't** chase performance. Perf is "kept in mind, not optimized for"
  until the PRD says otherwise. No speculative indexes. No premature
  partitioning. No micro-bench tests.
- **Don't** add features outside the PRD. Amendment first, code second.
- **Don't** delete from `donto_statement`. Use `donto_retract` /
  `donto_correct`.
- **Don't** assume a context exists. Call `donto_ensure_context` first,
  or trust the assert path which calls it for you.
- **Don't** put SQL identifiers in the `donto` schema (we live in
  `public` for Phase 0; future schema move is Phase 1+ packaging work).

## Truth model encoding

`flags smallint` packs:
- bits 0-1: polarity (0=asserted, 1=negated, 2=absent, 3=unknown)
- bits 2-4: maturity (0..4)
- bits 5-15: reserved

Helpers: `donto_pack_flags`, `donto_polarity`, `donto_maturity`.
Modality and confidence are sparse overlays (Phase 5+); not packed.

## SQL idioms

- Generated columns must be IMMUTABLE. `date::text`, `to_char(date, ...)`,
  `lower(daterange)::text` are STABLE (depend on DateStyle). Use
  `(some_date - '2000-01-01'::date)::text` for stable serialization.
- `on conflict on constraint <name>` only works for *named constraints*.
  Partial unique indexes use the inferred form: `on conflict (cols)
  where <pred>`.
- `symmetric` is reserved (used in `between symmetric`). Use `is_symmetric`.
  Same for any other SQL reserved word.
- `chr(31)` is a unit separator that's safe inside text concat for hashing.
  `char(31)` is a *type*, not a function — it'll error.

## Testing patterns

- One Postgres for all tests. Per-test isolation via unique IRI prefix.
- Cleanup at test entry, not exit (so a panic doesn't leak state).
- Migrations run once per process; tests then assume the schema is current.
- Heavy fixtures live in `sql/fixtures/`. Tiny inline data is fine in tests.
- `pg_or_skip!(connect().await)` — never panic when Postgres is absent.
