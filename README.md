# donto

A bitemporal, paraconsistent quad store with a full evidence substrate.
Postgres extension. Optional Lean 4 sidecar for shape validation,
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

**Lean 4 verification.** 60 kernel-checked theorems prove model
invariants — paraconsistency, snapshot monotonicity, scope semantics,
correction identity preservation. The proofs hold for every possible
input, not just test cases.

---

## The numbers

| Metric | Value |
|--------|-------|
| Statements | 35.5M |
| Migrations | 45 |
| Tables | 46 |
| Active predicates | 394 |
| SQL functions | 80+ |
| Lean modules | 18 |
| Lean theorems | 60 |
| Rust test files | 57 |
| Integration tests | 230+ |
| Ingestion formats | 8 |
| HTTP endpoints | 35+ |
| Seeded units | 26 |

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

## Domains

donto is domain-agnostic. The same schema handles:

- **Scientific papers** — benchmark results as first-class entities,
  derived comparisons, proof obligations for vague claims
- **Genealogy** — contradictory records, hypothesis branches,
  non-monotonic identity, temporal expressions
- **Business data** — salon services, pricing, entity resolution
  across registries
- **Medical records** — lab results, medication histories, temporal
  expressions, confidence-rated diagnoses
- **Legal documents** — contract clauses, compliance matrices,
  section-level anchoring
- **Any LLM extraction pipeline** — structured output → mentions →
  candidates → promoted claims with full provenance

Ontology seeds ship with the database (migration 0044): 100+
predicates across schema.org, ML/AI, physics, genealogy, geography,
and events. Domain-specific predicates are implicitly registered in
permissive contexts.

---

## Install

```bash
git clone https://github.com/thomasdavis/donto
cd donto

# Bring up Postgres 16 and apply all 45 migrations.
./scripts/pg-up.sh
cargo run -p donto-cli --quiet -- migrate

# Start the HTTP sidecar.
cargo run -p dontosrv

# Run all 230+ tests.
DONTO_TEST_DSN=postgres://donto:donto@127.0.0.1:55432/donto \
  cargo test --workspace

# Build the Lean verification layer (optional).
cd lean && lake build
```

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

## Ingestion

```bash
donto ingest dump.nq                                    # N-Quads
donto ingest data.ttl                                   # Turtle
donto ingest export.json --format property-graph         # Neo4j / AGE
donto ingest stream.jsonl --format jsonl                 # LLM output
donto ingest data.csv --mapping mapping.json             # CSV
donto-migrate genealogy /path/to/research.db             # SQLite
```

Also supports TriG, RDF/XML, and JSON-LD.

---

## Project layout

```
PRD.md                       Design specification (principles + maturity ladder)
CLAUDE.md                    Working contract for AI/human contributors
sql/migrations/              45 idempotent SQL migrations (schema source of truth)
crates/donto-client/         Typed Rust wrapper + 57 test files
crates/dontosrv/             HTTP sidecar (35+ endpoints)
crates/donto-query/          DontoQL + SPARQL parser and evaluator
crates/donto-ingest/         8 ingestion format parsers
crates/donto-migrate/        External store migrators
crates/pg_donto/             pgrx Postgres extension (all 45 migrations)
lean/                        18 Lean 4 modules, 60 theorems
apps/faces/                  Next.js visualisation app
playground/                  Gitignored extraction scripts and test data
docs/                        User guide, operator guide, Lean overlay,
                             migration reference, schema gaps audit
```

## Documentation

- [`PRD.md`](PRD.md) — design specification. Read §3 (principles) and
  §2 (maturity ladder) before contributing.
- [`docs/MIGRATIONS.md`](docs/MIGRATIONS.md) — complete reference for
  all 45 migrations with tables, functions, and seeds.
- [`docs/LEAN-OVERLAY.md`](docs/LEAN-OVERLAY.md) — what the Lean side
  proves and how to author shapes.
- [`docs/USER-GUIDE.md`](docs/USER-GUIDE.md) — ingestion, query,
  scopes, snapshots, hypotheses.
- [`docs/OPERATOR-GUIDE.md`](docs/OPERATOR-GUIDE.md) — sizing, backup,
  observability, sidecar topology.
- [`docs/SCHEMA-GAPS.md`](docs/SCHEMA-GAPS.md) — audit of extraction
  capabilities and domain coverage.

## Status

donto is **early and moving fast**. The data model, evidence substrate,
and Lean verification layer are solid. What's next:

- **AI extraction pipeline** — an LLM-powered system that uses the full
  observation → interpretation → judgment chain automatically
- **Claim card UI** — one beautiful page per claim showing evidence,
  arguments, obligations, and path to certification
- **Source-support verification** — given a claim and a source span,
  determine whether the span actually supports the claim
- **More Lean certificates** — proof-carrying shapes, derivation
  trees, and certificate verifiers

Performance is not yet a goal (PRD §25). Current focus: correctness,
PRD coverage, test depth.

## Contributing

See [`CLAUDE.md`](CLAUDE.md) for the working contract (non-negotiables,
SQL idioms, testing patterns).

## License

Dual licensed under Apache 2.0 and MIT.
