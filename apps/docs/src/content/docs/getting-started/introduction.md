---
title: Introduction
description: What donto is and why it exists
sidebar:
  order: 1
---

**donto** is a bitemporal, paraconsistent quad store built on Postgres 16+ with an optional Lean 4 verification sidecar.

## Core ideas

### The atom is the statement

Everything in donto is a statement: a subject-predicate-object triple with a context, two time dimensions, and polarity/maturity flags. Contexts hold statements, shapes inspect them, rules emit them, certificates justify them.

### Contradictions coexist

Two sources can disagree about Alice's birth year. donto stores both rows forever. You query under a *scope* that resolves the tension if you want a single answer — but the database never forces you to choose.

### Two kinds of time

Every statement carries:
- **`valid_time`** — when was this true in the world?
- **`tx_time`** — when did we learn about it?

Retraction closes `tx_time` but never deletes the row. You can always time-travel to see what the database believed at any point.

### Lean proves, doesn't gate

An optional Lean 4 sidecar proves invariants about the data model at the type level and runs user-authored shape validators. But ingestion never waits on the sidecar. If Lean is offline, the database keeps working — only shape/rule/certificate calls degrade.

## Architecture

```
              SQL via libpq           HTTP via dontosrv
                    |                        |
                    v                        v
         +----------------+         +-----------------+
         |  Postgres 16+  |  <----  |    dontosrv     |  ---->  Lean engine
         |   (pg_donto)   |  pool   |  (axum + Rust)  |         (optional)
         +----------------+         +-----------------+
```

## Crates

| Crate | What it does |
|-------|---|
| `donto-cli` | End-user CLI (`donto ingest`, `donto query`, `donto match`, etc.) |
| `donto-client` | Typed Rust wrapper over the SQL surface |
| `donto-query` | DontoQL + SPARQL subset parser and evaluator |
| `donto-ingest` | N-Quads, Turtle, TriG, RDF/XML, JSON-LD, JSONL, CSV, property graph ingestion |
| `donto-migrate` | Migrators from external stores |
| `dontosrv` | HTTP sidecar (axum) with DIR, shapes, rules, certificates |
| `pg_donto` | pgrx-based Postgres extension |
