# donto

A bitemporal, paraconsistent quad store with a full evidence substrate.
Postgres 16 + Rust. Optional Lean 4 sidecar for shape validation,
derivations, and machine-checkable certificates.

**donto is a database for claims that may be wrong.**

It stores what was said, who said it, when it was said, what it was
based on, what contradicts it, what remains unresolved, and what has
been formally certified. Traditional databases assume clean facts.
donto is for the messy interval between evidence and knowledge.

```text
claim = (subject, predicate, object, context,
         valid_time, transaction_time, polarity, maturity)
       + evidence chain
       + confidence
       + shape annotations
       + arguments (supports / rebuts / undercuts)
       + proof obligations
       + certificate
```

---

## What makes donto different

**Paraconsistent.** Two sources disagree about Alice's birth year. Both
rows live forever. Contradictions are evidence, not errors.

**Bitemporal.** Every statement tracks when the fact was true in the
world (`valid_time`) and when the system learned it (`tx_time`).
Retraction closes `tx_time` — nothing is ever deleted.

**Full evidence chain.** Every claim traces back through extraction run
→ document revision → source document → agent, with span-level
anchoring to exact character offsets in the source text.

**Epistemic maturity ladder.** Claims climb from raw (Level 0) through
registry-curated (1), shape-checked (2), rule-derived (3), to
certified (4). The system tells you exactly why each claim hasn't
reached the next level.

**Lean 4 verification.** 62 kernel-checked theorems prove model
invariants — paraconsistency, snapshot monotonicity, scope semantics,
correction identity preservation. The proofs hold for every possible
input, not just test cases.

---

## At a glance

| Component | Count |
|-----------|-------|
| SQL migrations | 47 |
| Tables | 55 |
| SQL functions | 118 |
| HTTP endpoints | 35 |
| Rust test files | 57 |
| Lean modules | 21 |
| Lean theorems | 62 |
| Ingestion formats | 8 |
| Registered predicates | 1,300+ |

---

## Quick start

```bash
git clone https://github.com/thomasdavis/donto
cd donto

# Bring up Postgres 16 and apply all migrations
./scripts/pg-up.sh
cargo run -p donto-cli --quiet -- migrate

# Start the HTTP sidecar (optional — for apps that talk over HTTP)
cargo run -p dontosrv -- --bind 127.0.0.1:7878

# Run tests (requires running Postgres)
DONTO_TEST_DSN=postgres://donto:donto@127.0.0.1:55432/donto \
  cargo test --workspace

# Build the Lean verification layer (optional, requires elan / Lean 4)
cd lean && lake build
```

Or with [just](https://github.com/casey/just):

```bash
just pg-up
just migrate
just test
```

---

## A quick example

```sql
-- Two sources disagree about a birth year.
SELECT donto_assert('ex:alice', 'ex:birthYear', NULL,
    '{"v":1899,"dt":"xsd:integer"}'::jsonb,
    'ctx:census1900', 'asserted', 0, NULL, NULL, NULL);

SELECT donto_assert('ex:alice', 'ex:birthYear', NULL,
    '{"v":1925,"dt":"xsd:integer"}'::jsonb,
    'ctx:hospital1925', 'asserted', 0, NULL, NULL, NULL);

-- Both are visible. donto never picks a winner.
SELECT * FROM donto_match('ex:alice', 'ex:birthYear',
    NULL, NULL, NULL, 'asserted', 0, NULL, NULL);
-- Two rows: 1899 from census, 1925 from hospital.

-- Correct the record without losing history.
SELECT donto_correct(
    (SELECT statement_id FROM donto_match('ex:alice', 'ex:birthYear',
     NULL, NULL, '{"include":["ctx:census1900"]}'::jsonb,
     'asserted', 0, NULL, NULL) LIMIT 1),
    NULL, NULL, NULL,
    '{"v":1898,"dt":"xsd:integer"}'::jsonb, NULL, NULL);

-- Time travel: what did we believe last month?
SELECT * FROM donto_match('ex:alice', 'ex:birthYear',
    NULL, NULL, NULL, 'asserted', 0,
    '2026-03-01T00:00:00Z'::timestamptz, NULL);
```

---

## Evidence substrate

donto doesn't just store claims. It stores the full lifecycle of how
a claim was produced, evaluated, challenged, and certified.

```
Document (PDF, web page, record)
  → Revision (text extraction, OCR, parser version)
    → Spans (character offsets, sentences, regions)
      → Mentions (entity references, typed)
        → Coreference clusters (resolved entities)
    → Tables (rows, columns, cells with headers)
    → Content regions (figures, charts, code blocks)
  → Extraction run (model, version, prompt, chunking)
    → Extraction chunks (per-chunk provenance)
    → Statements (claims with typed literals)
      → Confidence scores (per-statement overlay)
      → Evidence links (statement ↔ span/run/document)
      → Shape annotations (pass/warn/violate)
      → Arguments (supports/rebuts/undercuts/qualifies)
      → Proof obligations (needs-coref, needs-source-support, ...)
      → Certificates (Lean-verifiable proofs)
```

Every layer is queryable, traceable, and correctable.

### Claim card

Ask donto everything it knows about a single claim:

```sql
SELECT donto_claim_card('statement-uuid');
```

Returns the statement, its evidence links, arguments, proof
obligations, shape annotations, reactions, and maturity blockers —
everything needed to understand and evaluate the claim.

### Why not higher?

Ask why a claim hasn't been promoted:

```sql
SELECT * FROM donto_why_not_higher('statement-uuid');
-- predicate_not_registered: Predicate ex:foo is not registered
-- no_shape_report: No shape validation has been run
-- no_span_anchor: Not anchored to a specific source span
-- open_obligations: 2 open proof obligation(s)
```

---

## Three query surfaces

**SQL functions** — for applications with a Postgres connection:

```sql
SELECT * FROM donto_match('ex:alice', 'ex:knows', NULL, NULL,
    '{"include":["ctx:wikipedia"]}'::jsonb,
    'asserted', 0, NULL, NULL);
```

**SPARQL 1.1 subset** — for RDF tooling:

```sparql
PREFIX ex: <http://example.org/>
SELECT ?x ?y WHERE { ?x ex:knows ?y . } LIMIT 10
```

**DontoQL** — native language with full feature access:

```dontoql
PRESET latest
MATCH ?x ex:knows ?y, ?y ex:name ?n
FILTER ?n != "Mallory"
POLARITY asserted
MATURITY >= 1
PROJECT ?x, ?n
```

---

## Ingestion

Eight parsers, all piped through the same assertion path:

```bash
donto ingest dump.nq                                    # N-Quads
donto ingest data.ttl                                   # Turtle
donto ingest graph.trig                                 # TriG
donto ingest data.rdf                                   # RDF/XML
donto ingest data.jsonld                                # JSON-LD
donto ingest export.json --format property-graph         # Neo4j / AGE
donto ingest stream.jsonl --format jsonl                 # LLM output
donto ingest data.csv --mapping mapping.json             # CSV
```

---

## HTTP sidecar (dontosrv)

`dontosrv` exposes 35 endpoints for applications that don't have a
direct Postgres connection. Axum-based, stateless, horizontally
scalable.

| Category | Endpoints |
|----------|-----------|
| Query | `/sparql`, `/dontoql`, `/search`, `/history/:subject`, `/statement/:id`, `/claim/:id` |
| Browse | `/subjects`, `/contexts`, `/predicates` |
| Write | `/assert`, `/assert/batch`, `/retract`, `/contexts/ensure` |
| Evidence | `/documents/register`, `/documents/revision`, `/evidence/link/span`, `/evidence/:stmt` |
| Arguments | `/arguments/assert`, `/arguments/:stmt`, `/arguments/frontier` |
| Shapes | `/shapes/validate` |
| Rules | `/rules/derive` |
| Certificates | `/certificates/attach`, `/certificates/verify/:stmt` |
| Obligations | `/obligations/emit`, `/obligations/resolve`, `/obligations/open`, `/obligations/summary` |
| Agents | `/agents/register`, `/agents/bind` |
| Reactions | `/react`, `/reactions/:id` |
| System | `/health`, `/version`, `/dir` |

A TypeScript client (`packages/donto-client`) mirrors the HTTP surface
for Next.js and other JS/TS applications.

---

## Use cases

donto is domain-agnostic. The same schema handles any domain where
sources disagree, evidence evolves, or claims need verification:

- **Scientific literature** — benchmark results, measurements, derived
  comparisons, proof obligations for vague claims
- **Investigative research** — contradictory records, hypothesis
  branches, temporal evidence chains
- **Legal / compliance** — contract clauses, regulatory requirements,
  section-level anchoring, audit trails
- **Medical records** — lab results, medication histories, temporal
  expressions, confidence-rated diagnoses
- **LLM extraction pipelines** — structured output → mentions →
  candidates → promoted claims with full provenance
- **Intelligence analysis** — multi-source fusion, competing
  hypotheses, confidence tiers

Ontology seeds ship with the database (migration 0044): 1,300+
predicates across schema.org, ML/AI, physics, geography, and events.
Domain-specific predicates are implicitly registered in permissive
contexts.

---

## Project layout

```
PRD.md                       Design specification (principles + maturity ladder)
CLAUDE.md                    Working contract for AI/human contributors
sql/migrations/              47 idempotent SQL migrations (schema source of truth)
sql/fixtures/                Example data for smoke tests
crates/donto-client/         Typed Rust wrapper + 57 test files
crates/donto-cli/            CLI: migrate, ingest, query
crates/dontosrv/             HTTP sidecar (35 endpoints)
crates/donto-query/          DontoQL + SPARQL parser and evaluator
crates/donto-ingest/         8 ingestion format parsers
crates/donto-migrate/        External store migrators
crates/pg_donto/             pgrx Postgres extension
lean/                        21 Lean 4 modules, 62 theorems
apps/faces/                  Next.js visualisation app
packages/donto-client/       TypeScript client for dontosrv
docs/                        Guides and references
```

---

## Documentation

- [`PRD.md`](PRD.md) — design specification. Read §3 (principles) and
  §2 (maturity ladder) before contributing.
- [`docs/MIGRATIONS.md`](docs/MIGRATIONS.md) — complete reference for
  all migrations with tables, functions, and seeds.
- [`docs/LEAN-OVERLAY.md`](docs/LEAN-OVERLAY.md) — what the Lean side
  proves and how to author shapes.
- [`docs/USER-GUIDE.md`](docs/USER-GUIDE.md) — ingestion, query,
  scopes, snapshots, hypotheses.
- [`docs/OPERATOR-GUIDE.md`](docs/OPERATOR-GUIDE.md) — sizing, backup,
  observability, sidecar topology.
- [`docs/SCHEMA-GAPS.md`](docs/SCHEMA-GAPS.md) — audit of extraction
  capabilities and domain coverage.

---

## Status

donto is **early and moving fast**. The data model, evidence substrate,
and Lean verification layer are solid. Current focus areas:

- **AI extraction pipeline** — LLM-powered observation → interpretation
  → judgment chain with full provenance
- **Claim card UI** — visual inspection of evidence, arguments,
  obligations, and certification status
- **Source-support verification** — automated checking of whether a
  source span actually supports its claim
- **More Lean certificates** — proof-carrying shapes, derivation
  trees, and certificate verifiers

Performance is not yet a goal (PRD §25). Current focus: correctness,
PRD coverage, test depth.

---

## Contributing

See [`CLAUDE.md`](CLAUDE.md) for the working contract (non-negotiables,
SQL idioms, testing patterns). Read the PRD before changing core types.

## License

Dual licensed under Apache 2.0 and MIT.
