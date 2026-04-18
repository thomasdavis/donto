# Changelog

All notable changes to donto are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Phase 0 spike: `donto_statement` + contexts + bitemporal indexes,
  plpgsql functions for assert/retract/correct/match/resolve_scope,
  N-Quads loader, Rust client, CLI.
- Phase 1: migration ledger (`donto_migration`), version function,
  extension control file scaffold.
- Phase 2: scope presets (`anywhere`, `raw`, `curated`, `latest`,
  `under_hypothesis`, `as_of`), snapshots with member tables.
- Phase 3: predicate registry with alias resolution and implicit
  registration in permissive contexts; rejection in curated contexts.
- Phase 4: DontoQL parser, SPARQL 1.1 subset translator, internal
  algebra, nested-loop evaluator (PRD §12).
- Phase 5: shape catalog, report cache, builtin shapes
  (FunctionalPredicate, DatatypeShape) wired through dontosrv;
  Lean project skeleton with shape combinators.
- Phase 6: derivation rule catalog, rule report cache with
  fingerprint-based idempotency, builtin rules (TransitiveClosure,
  InverseEmission, SymmetricClosure).
- Phase 7: certificate annotation overlay (7 kinds per PRD §18),
  attach + verify endpoints in dontosrv.
- Phase 8: ingestion pipelines for Turtle, TriG, RDF/XML, JSON-LD
  subset, JSONL streaming, property-graph JSON, CSV mapping, and a
  quarantine helper.
- Phase 9: SQLite genealogy migrator implementing PRD §24 mapping.
- Phase 10: observability views (`donto_stats_*`), user/operator
  guides, dual licensing, opensource hygiene.
- pgrx-based `pg_donto` extension crate that packages the SQL surface
  for `CREATE EXTENSION pg_donto`.

### Notes
- This release is the initial open source drop. Performance hypotheses
  in PRD §25 (10⁹ statements, 100k inserts/sec, sub-ms point queries)
  are aspirational; correctness and PRD coverage take priority. See
  [PRD §26 follow-ons](PRD.md#follow-ons) for what the v1 ladder
  intentionally defers.

### Lean overlay (Phase 5+ first pass)
- `lean/Donto/Theorems.lean` — kernel-checked propositions about the
  data model: polarity totality, asserted-vs-negated distinctness,
  retraction preserves identity, snapshot membership monotonicity,
  scope-exclude-wins, maturity bounded.
- `lean/Donto/Engine.lean` + `lean/Main.lean` — `donto_engine` is now a
  real DIR sidecar: line-delimited JSON over stdio, dispatch on
  `validate_request`, banner-then-loop main.
- `lean/Donto/Shapes.lean` — `parentChildAgeGap` shape (PRD §16
  example), authored in Lean and runnable via the engine.
- `crates/dontosrv/src/lean.rs` — `LeanClient`: long-lived child
  process, mutex-serialised requests, per-request timeout, fail-fast
  on a dead pipe (PRD §15 sidecar contract).
- dontosrv learns `--lean-engine PATH` (env: `DONTO_LEAN_ENGINE`) and
  forwards `lean:` shape IRIs to it. Without the flag, `lean:` IRIs
  return `sidecar_unavailable`; `builtin:` shapes still work.
- New integration test (`crates/dontosrv/tests/lean_engine.rs`) spawns
  the real Lean binary and verifies a violation is detected by Lean
  code, not Rust.
- `docs/LEAN-OVERLAY.md` documents what the Lean side proves and how
  to author/wire/run a custom shape.
- CI: `.github/workflows/lean.yml` builds Lean + runs the lean_engine
  integration tests against a real Postgres.
