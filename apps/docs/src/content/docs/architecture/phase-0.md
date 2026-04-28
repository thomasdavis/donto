---
title: "Phase 0: Spike"
description: Pure-Postgres prototype proving the core data model
---

**Goal:** Pure-Postgres prototype that proves the core data model — `donto_statement` + contexts + bitemporal queries + N-Quads round-trip.

**Exit criteria:** round-trip N-Quads, scoped pattern query, retraction, bitemporal time-travel.

## What's in scope

- `donto_statement` physical row exactly as PRD section 5 (subject, predicate, object_iri/object_lit, context, tx_time, valid_time, flags packing polarity + maturity).
- `donto_context` table with `kind`, `parent`, `mode`.
- One annotation overlay: `donto_stmt_lineage` (needed for retraction history reasoning). Other overlays are stubbed.
- A small set of plpgsql functions: `donto_assert`, `donto_assert_batch`, `donto_retract`, `donto_correct`, `donto_match`, `donto_resolve_scope`.
- Rust crate `donto-client` over `tokio-postgres` exposing the same surface as a typed API.
- N-Quads parser (`rio_turtle`).
- CLI `donto-cli` for `ingest`, `match`, `retract`.
- Tests covering: ingest, retraction, correction, scope inheritance, bitemporal as-of queries, paraconsistent retrieval.

## What's NOT in scope

- IRI hashing (Phase 1).
- Custom Postgres types (Phase 1).
- C extension packaging — Phase 0 is plain SQL + plpgsql.
- Predicate registry (Phase 3).
- DontoQL / SPARQL (Phase 4).
- Lean sidecar (Phase 5+).
- Annotation overlays beyond `donto_stmt_lineage`.
- Performance tuning — correctness first.

## Truth-model encoding

`flags smallint` packs:
- bits 0-1: polarity (0=asserted, 1=negated, 2=absent, 3=unknown)
- bits 2-4: maturity (0..4)
- bits 5-15: reserved.

Helper functions in `sql/migrations/0002_flags.sql`.

## Test database

Postgres 16+ via Docker (`scripts/pg-up.sh`). Default DSN: `postgres://donto:donto@127.0.0.1:55432/donto`.
