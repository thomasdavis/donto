# donto

An evidence operating system for contested knowledge. Postgres 16 +
Rust + Go TUI. Optional Lean 4 sidecar for shape validation,
derivations, and machine-checkable certificates.

**donto is a database for claims that may be wrong.**

It stores what was said, who said it, when it was said, what it was
based on, what contradicts it, what remains unresolved, and what has
been formally certified. Traditional databases assume clean facts.
donto is for the messy interval between evidence and knowledge.

The canonical product spec is [`docs/DONTO-PRD.md`](docs/DONTO-PRD.md).
This release lands the **Trust Kernel** (policies, attestations, audit), the
**E0–E5 maturity ladder**, **modality** and **extraction-level**
overlays separate from confidence, the **eleven-relation alignment
layer** with safety flags, **n-ary frame model** with indexed roles,
and the **release builder** with reproducible manifests.

```text
claim = (subject, predicate, object, context,
         valid_time, transaction_time, polarity, maturity)
       + evidence chain
       + confidence
       + shape annotations
       + arguments (supports / rebuts / undercuts)
       + proof obligations
       + certificate
       + predicate alignment (equivalents, inverses, sub-properties)
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

**Epistemic maturity ladder.** Claims climb from raw (E0) through
candidate (E1), evidence-supported (E2), reviewed (E3), corroborated
(E4), to certified (E5). The system tells you exactly why each claim
hasn't reached the next level. Auto-promotion is gated by the
extraction level of the originating act — `model_hypothesis` cannot
auto-promote past E1, no matter how high the model confidence.

**Trust Kernel.** Source registration requires a policy. Policies
declare 15 typed actions (`read_metadata`, `read_content`, `quote`,
`view_anchor_location`, `derive_claims`, `derive_embeddings`,
`translate`, `summarize`, `export_claims`, `export_sources`,
`export_anchors`, `train_model`, `publish_release`,
`share_with_third_party`, `federated_query`). Caller authorisation
requires a non-revoked, non-expired attestation with a written
rationale. Inheritance defaults to max-restriction: a derived claim
takes the most restrictive policy of its source anchors. Every
restricted-action check goes into the audit log.

**Modality and extraction level as separate dimensions.** A claim
carries (a) machine confidence, (b) calibrated confidence, (c) human
confidence, (d) source reliability, (e) modality
(`descriptive | reconstructed | inferred | corpus_observed |
typological_summary | ...`), (f) extraction level
(`quoted | table_read | source_generalization | model_hypothesis |
human_hypothesis | ...`), and (g) maturity. None collapses into the
others.

**Predicate alignment.** LLM extractors freely mint predicates —
`bornIn`, `wasBornIn`, `birthplaceOf` all mean the same thing. The
predicate alignment layer converges them without constraining
extraction: equivalents, inverses, sub-properties, and close matches
are registered with confidence scores. Queries expand through the
closure automatically.

**Lean 4 verification.** 62 kernel-checked theorems prove model
invariants — paraconsistency, snapshot monotonicity, scope semantics,
correction identity preservation. The proofs hold for every possible
input, not just test cases.

---

## At a glance

| Component | Count |
|-----------|-------|
| SQL migrations | 95 (28 new in this release) |
| Tables | 80+ |
| SQL functions | 200+ |
| HTTP endpoints | 44 (plus Trust Kernel surface) |
| donto-client tests | 455 (212 new in this release, all green) |
| Lean modules | 21 |
| Lean theorems | 62 |
| Ingestion formats | 8 |
| Anchor kinds | 13 (typed, validated) |
| Alignment relations | 11 (with safety flags) |
| Frame types | 24 seeded (18 linguistic + 6 cross-domain) |
| Default access policies | 4 |
| Modality values | 14 |
| Extraction levels | 10 |
| Maturity ladder | E0–E5 |

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

# Launch the TUI dashboard (requires Go 1.21+)
cd apps/donto-tui && go run .

# Build the Lean verification layer (optional, requires elan / Lean 4)
cd packages/lean && lake build
```

### Migrate a genealogy database

```bash
# Import a genealogy SQLite database (research.db) into donto
cargo run -p donto-migrate -- \
  --dsn postgres://donto:donto@127.0.0.1:55432/donto \
  genealogy /path/to/research.db \
  --root ctx:genealogy/research-db

# Optional second pass for full provenance (documents, chunks, costs)
cargo run -p donto-migrate -- \
  --dsn postgres://donto:donto@127.0.0.1:55432/donto \
  genealogy-relink /path/to/research.db \
  --root ctx:genealogy/research-db
```

---

## Trust Kernel — a worked example

```sql
-- 1. Register a source. Policy is required (PRD I2: no source without policy).
SELECT donto_register_source(
    'src:fieldnotes/2024-recording-A.eaf',
    'audio',
    'policy:default/community_restricted'
);

-- 2. Assign the policy to the source.
SELECT donto_assign_policy(
    'document', 'src:fieldnotes/2024-recording-A.eaf',
    'policy:default/community_restricted',
    'community-council'
);

-- 3. Default-deny. Without an attestation, no one reads content.
SELECT donto_action_allowed(
    'document', 'src:fieldnotes/2024-recording-A.eaf',
    'read_content'
);  -- returns false

-- 4. Issue an attestation (rationale required).
SELECT donto_issue_attestation(
    'agent:researcher-jane',
    'community-council',
    'policy:default/community_restricted',
    array['read_content','derive_claims']::text[],
    'community_curation',
    'Approved for community curation under MoU 2024-03.',
    NULL,    -- no expiry
    NULL     -- no VC reference yet
);

-- 5. Authorisation now succeeds for the listed actions only.
SELECT donto_authorise(
    'agent:researcher-jane',
    'document', 'src:fieldnotes/2024-recording-A.eaf',
    'read_content'
);  -- returns true

SELECT donto_authorise(
    'agent:researcher-jane',
    'document', 'src:fieldnotes/2024-recording-A.eaf',
    'train_model'
);  -- returns false — train_model is a separate action

-- 6. Revocation is immediate for new authorisation checks.
SELECT donto_revoke_attestation('att_xyz', 'community-council', 'project ended');

-- 7. Every step lands in donto_event_log for audit.
SELECT event_type, occurred_at, actor, payload
FROM donto_event_history(
    'access_assignment',
    'src:fieldnotes/2024-recording-A.eaf',
    100
);
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

## Predicate alignment

LLM extractors mint predicates freely — the same relationship gets
called `bornIn`, `wasBornIn`, `birthplaceOf`, `placeOfBirth`. Without
alignment, queries only find exact matches and the knowledge graph
fragments.

The predicate alignment layer (PAL) solves this without constraining
extraction. Extractors keep minting whatever feels natural; PAL
converges them after the fact.

### Alignment relations (11 kinds)

| Relation | Meaning | Example |
|----------|---------|---------|
| `exact_match` | Same meaning, interchangeable | `bornIn` ↔ `wasBornIn` |
| `close_match` | Fuzzy, retrieve-together but not logically equal | `workedAt` ≈ `employedBy` |
| `broad_match` | Left is broader than right | `livedIn` ⊃ `bornIn` |
| `narrow_match` | Left is narrower than right | `birthPlace` ⊂ `associatedPlace` |
| `inverse_of` | Same relation, swaps subject and object | `parentOf` ↔ `childOf` |
| `decomposes_to` | Concept decomposes into multiple claims | `workedAt` → EmploymentEvent roles |
| `has_value_mapping` | Equivalence depends on a value mapping | WALS feature 98 ↔ Grambank GBxxx (with value-pair table) |
| `incompatible_with` | Should not be aligned | `birthDate` ✗ `deathDate` |
| `derived_from` | One schema feature designed from another |   |
| `local_specialization` | Language- or project-specific refinement |   |
| `not_equivalent` | Explicit negative: do not align (legacy alias of `incompatible_with`) |   |

Each alignment edge carries three safety flags: `safe_for_query_expansion`,
`safe_for_export`, `safe_for_logical_inference`. A typological-to-token
`close_match` may be safe for query but never for logical inference.

Legacy v0 relation names (`exact_equivalent`, `inverse_equivalent`,
`sub_property_of`, `decomposition`, `not_equivalent`) remain accepted
as aliases for one release window.

### Register an alignment

```sql
-- bornIn and wasBornIn mean the same thing
SELECT donto_register_alignment(
    'ex:bornIn', 'ex:wasBornIn', 'exact_equivalent',
    1.0,        -- confidence
    NULL, NULL, -- valid_time bounds (optional)
    NULL,       -- run_id (optional)
    NULL,       -- provenance (optional)
    'human'     -- actor
);

-- parentOf is the inverse of childOf
SELECT donto_register_alignment(
    'ex:parentOf', 'ex:childOf', 'inverse_equivalent',
    1.0, NULL, NULL, NULL, NULL, 'human'
);

-- Rebuild the closure after registering alignments
SELECT donto_rebuild_predicate_closure();
```

### Querying with alignment

After rebuilding the closure, `donto_match()` expands predicates
automatically. A query for `wasBornIn` also returns statements asserted
with `bornIn`:

```sql
-- Returns rows for both bornIn and wasBornIn
SELECT * FROM donto_match('ex:alice', 'ex:wasBornIn',
    NULL, NULL, NULL, 'asserted', 0, NULL, NULL);

-- Explicit expansion control with matched_via and confidence
SELECT * FROM donto_match_aligned(
    'ex:alice', 'ex:wasBornIn', NULL, NULL, NULL,
    'asserted', 0, NULL, NULL,
    true,   -- expand_predicates
    0.8     -- min_alignment_confidence
);
-- Returns: matched_via='exact_equivalent', alignment_confidence=1.0

-- Strict mode: no expansion, exact predicate only
SELECT * FROM donto_match_strict('ex:alice', 'ex:bornIn',
    NULL, NULL, NULL, 'asserted', 0, NULL, NULL);
```

### DontoQL predicate modes

```dontoql
-- Default: expansion enabled (confidence ≥ 0.8)
MATCH ?s ex:wasBornIn ?o
PROJECT ?s, ?o

-- Strict: exact predicate match only
MATCH ?s ex:bornIn ?o PREDICATES STRICT
PROJECT ?s, ?o

-- Custom confidence floor
MATCH ?s ex:wasBornIn ?o PREDICATES EXPAND_ABOVE 90
PROJECT ?s, ?o
```

### Finding and suggesting alignments

```sql
-- Lexical similarity between two predicates
SELECT donto_predicate_lexical_similarity('ex:bornIn', 'ex:wasBornIn');
-- → 0.72

-- Suggest alignments for a predicate based on trigram similarity
SELECT * FROM donto_suggest_alignments('ex:bornIn', 0.5, 10);
-- → (ex:wasBornIn, 0.72, "Was born in"), ...

-- Auto-align a batch of predicates above a confidence threshold
SELECT donto_auto_align_batch(
    ARRAY['ex:bornIn', 'ex:diedIn', 'ex:livedAt'],
    0.7,     -- min_similarity
    'sweep'  -- actor
);

-- Embedding-based candidate lookup (for LLM extraction pipelines)
SELECT * FROM donto_extraction_predicate_candidates(
    embedding,      -- float4[] from your model
    'text-embedding-3-small',
    'genealogy',    -- domain filter (optional)
    NULL, NULL, 30  -- subject_type, object_type, limit
);
```

### Predicate descriptors

Rich metadata for each predicate — label, gloss, domain/range hints,
examples, and optional embeddings for semantic search:

```sql
SELECT donto_upsert_descriptor(
    'ex:bornIn',
    'Born in',                        -- label
    'Place where a person was born',  -- gloss
    'Person',                         -- subject_type (domain)
    'Place',                          -- object_type (range)
    'genealogy',                      -- domain
    'ex:alice',                       -- example_subject
    'ex:london',                      -- example_object
    'Alice was born in London',       -- source_sentence
    'many_to_one',                    -- cardinality
    NULL, NULL, NULL                  -- embedding_model, embedding, metadata
);
```

### Canonical shadows

For callers that want fully-canonicalized data, the shadow table
pre-computes the canonical predicate for each statement:

```sql
-- Materialize a single statement's canonical form
SELECT donto_materialize_shadow('statement-uuid');

-- Batch rebuild for a context (or all statements)
SELECT donto_rebuild_shadows('ctx:genealogy/research-db', 10000);
```

### Event frames

Complex n-ary relations decompose into event frames instead of losing
information in a single triple:

```sql
-- Instead of: (Marie, workedAt, Sorbonne)
-- Emit a frame with roles:
SELECT donto_decompose_to_frame(
    'ex:marie-curie', 'ex:workedAt', 'ex:sorbonne',
    'ctx:biography', 'ex:EmploymentEvent',
    '{"ex:startDate":"1906","ex:endDate":"1934","ex:role":"professor of physics"}'::jsonb,
    '1906-01-01'::date, '1934-07-04'::date,
    'extraction'
);
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

### Claim lifecycle

Every statement progresses through stages independently of maturity:
observed → extracted → typed → anchored → confidence-rated →
predicate-registered → shape-checked → source-supported →
obligations-clear → argued → certified.

```sql
-- See lifecycle coverage for a context
SELECT * FROM donto_lifecycle_summary('ctx:genealogy/research-db');
```

### Why not higher?

Ask why a claim hasn't been promoted:

```sql
SELECT * FROM donto_why_not_higher('statement-uuid');
-- predicate_not_registered: Predicate ex:foo is not registered
-- no_shape_report: No shape validation has been run
-- no_span_anchor: Not anchored to a specific source span
-- open_obligations: 2 open proof obligation(s)
-- active_rebuttals: Open attacks/contradictions exist
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
PREDICATES EXPAND
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

## TUI dashboard

A terminal UI (Go / Charm Bubbletea) for browsing and monitoring donto
in real time. Tabs: dashboard stats, firehose (live LISTEN/NOTIFY
stream), explorer, contexts, claim cards, and charts.

```bash
cd apps/donto-tui && go run .

# Flags:
#   --dsn          Postgres DSN (default: $DONTO_DSN or localhost:55432)
#   --poll         Dashboard refresh interval (default: 5s)
#   --srv          dontosrv URL for health checks (default: http://127.0.0.1:7878)
#   --install-triggers  Install LISTEN/NOTIFY triggers on connect
```

---

## HTTP sidecar (dontosrv)

`dontosrv` exposes 44 endpoints for applications that don't have a
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
| Alignment | `/alignment/register`, `/alignment/retract`, `/alignment/rebuild-closure`, `/alignment/runs/start`, `/alignment/runs/complete` |
| Descriptors | `/descriptors/upsert`, `/descriptors/nearest` |
| Shadows | `/shadow/materialize`, `/shadow/rebuild` |
| System | `/health`, `/version`, `/dir` |

A TypeScript client (`packages/client-ts`) mirrors the HTTP surface
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
contexts, then converged through the predicate alignment layer.

---

## Project layout

```
apps/
  donto-cli/                 CLI: migrate, ingest, query, match, retract
  dontosrv/                  HTTP sidecar (44 endpoints)
  donto-tui/                 Go/Charm TUI: dashboard, firehose, explorer, charts
  docs/                      Astro Starlight documentation site

packages/
  donto-client/              Typed Rust wrapper + test suite
  donto-query/               DontoQL + SPARQL parser and evaluator
  donto-ingest/              8 ingestion format parsers
  donto-migrate/             External store migrators (genealogy SQLite)
  pg_donto/                  pgrx Postgres extension
  sql/migrations/            95 idempotent SQL migrations (source of truth)
  sql/fixtures/              Example data for smoke tests
  sql/scripts/               Epistemic sweep and batch operations
  lean/                      21 Lean 4 modules, 62 theorems
  client-ts/                 TypeScript client (@donto/client)
  tsconfig/                  Shared TypeScript config

PRD.md                       Design specification (principles + maturity ladder)
CLAUDE.md                    Working contract for AI/human contributors
turbo.json                   Turborepo pipeline config
```

---

## Documentation

- [`PRD.md`](PRD.md) — top-level pointer to the canonical PRD.
- [`docs/DONTO-PRD.md`](docs/DONTO-PRD.md) — **canonical
  product requirements**. The canonical reference.
- [`apps/docs`](apps/docs) — Starlight documentation site with
  migration reference, schema gap audit, and guides.
- [`ANTHROPOLOGY_README.md`](ANTHROPOLOGY_README.md) — research
  philosophy and domain context.
- Historical (superseded by the canonical PRD; kept for provenance):
  [`docs/LANGUAGE-EXTRACTION-PLAN.md`](docs/LANGUAGE-EXTRACTION-PLAN.md),
  [`docs/REFACTOR-PLAN.md`](docs/REFACTOR-PLAN.md),
  [`docs/ATLAS-ZERO-FRONTIER.md`](docs/ATLAS-ZERO-FRONTIER.md).

---

## Status

donto is **early and moving fast**. The data model, evidence substrate,
predicate alignment layer, and Lean verification layer are solid.
Current focus areas:

- **Predicate alignment at scale** — embedding-based alignment,
  auto-suggest, canonical shadow materialization
- **AI extraction pipeline** — LLM-powered observation → interpretation
  → judgment chain with full provenance
- **TUI polish** — charts, claim card viewer, context explorer
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
