# Donto Status Report — 2026-04-25

Comprehensive technical report on the donto project: architecture,
current capabilities, formal verification layer, live extraction
demonstration, and forward direction.

---

## 1. What donto is

Donto is a **bitemporal, paraconsistent quad store** built as a
PostgreSQL extension with a Lean 4 formal verification sidecar. It is
designed to be a **lossless, proof-carrying evidence substrate for
agents** — a system that stores contradictions gracefully, tracks
provenance at every layer, and uses a theorem prover to certify the
hardest claims.

The name stands for "database ontology." It runs on a single Postgres
16 node and is adoptable by any application that already speaks
Postgres.

**Key design choices that make donto different from other graph
databases:**

1. **Paraconsistent.** Two sources can disagree about Alice's birth
   year. Both rows live forever. Contradictions are a feature, not a
   failure mode. Consistency is a query, not a constraint.

2. **Bitemporal.** Every statement carries two time dimensions:
   `valid_time` (when the fact was true in the world) and `tx_time`
   (when the system learned it). Retraction closes `tx_time` — it
   never deletes the row. You can query "what did the system believe on
   March 3rd about events in 1850."

3. **Contexts as the universal overlay.** Every statement belongs to a
   named context. Contexts form a forest with types (source, snapshot,
   hypothesis, user, pipeline, trust, derivation, quarantine, custom,
   system) and modes (permissive or curated). Contexts are the single
   mechanism for provenance, versioning, counterfactual reasoning, and
   trust scoping.

4. **A semantic maturity ladder.** Statements climb from raw (Level 0)
   through registry-curated (1), shape-checked (2), rule-derived (3),
   to certified (4). Each level unlocks more donto features. Nothing
   requires uniform maturity.

5. **Lean 4 as the meaning layer.** PostgreSQL owns execution. Lean
   owns meaning. The boundary is DIR (Donto Intermediate
   Representation), a versioned JSON protocol. Lean validates shapes,
   derives rules, verifies certificates, and proves model invariants.
   The database stays fully usable when Lean is offline.

---

## 2. The codebase at a glance

| Component | Language | Lines | Purpose |
|-----------|----------|-------|---------|
| `sql/migrations/` | SQL | 3,538 | 33 idempotent migrations — the schema source of truth |
| `crates/donto-client/` | Rust | ~3,500 | Typed wrapper over the SQL surface (assert, retract, match, evidence, agents, etc.) |
| `crates/dontosrv/` | Rust | ~3,000 | Axum HTTP sidecar — 35+ routes, DIR protocol, Lean engine client |
| `crates/donto-query/` | Rust | ~2,500 | DontoQL + SPARQL subset → algebra → evaluator |
| `crates/donto-ingest/` | Rust | ~2,000 | N-Quads, Turtle, TriG, RDF/XML, JSON-LD, JSONL, CSV, property graph parsers |
| `crates/pg_donto/` | Rust | ~250 | pgrx-based Postgres extension packaging all 33 migrations |
| `lean/` | Lean 4 | 1,580 | 18 modules — core types, shapes, rules, certificates, engine, 57 theorems |
| Tests | Rust | ~5,000 | 190+ integration tests across 37 test binaries |

**Repository:** github.com/thomasdavis/donto
**Lean version:** 4.12.0 (no external dependencies)
**Postgres version:** 16

---

## 3. Current database state

The live database (running in Docker, Postgres 16) contains:

| Metric | Value |
|--------|-------|
| Total statements | 35,540,107 |
| Open (current-belief) statements | ~35.5M |
| Contexts | 3,813 |
| Registered predicates | 756 |
| Applied migrations | 33 |
| Context kinds in use | 8 (custom, source, user, derivation, quarantine, hypothesis, snapshot, system) |

**Top predicates by usage:**

| Predicate | Count |
|-----------|-------|
| `rdf:type` | 3.7M |
| `donto:status` | 1.6M |
| `donto:aboutPredicate` | 1.6M |
| `donto:confidenceLabel` | 1.2M |
| `donto:predicate` | 1.2M |
| `donto:textSpan` | 1.2M |
| `donto:extractionModel` | 1.2M |
| `ex:knownAs` | 1.1M |
| `rdfs:label` | 576K |

This data comes primarily from a genealogy research project (230K
entities, 530K claims, 541K open contradictions, 728K aliases, 156K
events) plus active LLM extraction pipelines.

---

## 4. The evidence substrate (new, built 2026-04-25)

In a single session, we built and deployed an 11-migration evidence
substrate layer. This is the foundation for donto's transformation from
"interesting database" to "evidence operating system for agents."

### 4.1 New SQL tables (migrations 0023–0033)

| Table | Migration | Purpose |
|-------|-----------|---------|
| `donto_document` | 0023 | Immutable document objects (IRI, media type, language, source URL) |
| `donto_document_revision` | 0024 | Content-hashed revisions of documents (text body, parser version) |
| `donto_span` | 0025 | Standoff spans over revisions (char offsets, tokens, sentences, pages, XPath, CSS) |
| `donto_annotation_space` | 0026 | Named feature namespaces (e.g., Universal Dependencies POS, NER labels) |
| `donto_annotation` | 0026 | Feature-value pairs attached to spans with confidence scores |
| `donto_annotation_edge` | 0027 | Relations between annotations (dependency arcs, coreference links) |
| `donto_extraction_run` | 0028 | Extraction provenance (model, version, prompt hash, temperature, seed, toolchain) |
| `donto_evidence_link` | 0029 | Links between statements and evidence (documents, spans, runs, other statements) |
| `donto_agent` | 0030 | Agent registry (humans, LLMs, rule engines, extractors, validators, curators) |
| `donto_agent_context` | 0030 | Agent-to-context workspace bindings with roles (owner, contributor, reader) |
| `donto_argument` | 0031 | Structured argumentation (supports, rebuts, undercuts, endorses, supersedes, qualifies) |
| `donto_proof_obligation` | 0032 | Unresolved extraction work (needs-coref, needs-temporal-grounding, needs-source-support, etc.) |
| `donto_vector` | 0033 | Embedding vectors for semantic retrieval (float4[], cosine similarity, nearest-neighbor search) |

### 4.2 Current evidence substrate usage

| Table | Rows |
|-------|------|
| `donto_document` | 97 |
| `donto_document_revision` | 101 |
| `donto_span` | 84 |
| `donto_annotation` | 70 |
| `donto_annotation_space` | 44 |
| `donto_annotation_edge` | 10 |
| `donto_extraction_run` | 27 |
| `donto_evidence_link` | 202 |
| `donto_agent` | 51 |
| `donto_argument` | 45 |
| `donto_proof_obligation` | 34 |
| `donto_vector` | 41 |

### 4.3 HTTP API surface

The evidence substrate added 13 new HTTP endpoints to dontosrv:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/documents/register` | POST | Register or ensure a document |
| `/documents/revision` | POST | Add a text/binary revision |
| `/evidence/link/span` | POST | Link a statement to a span |
| `/evidence/:stmt` | GET | All evidence for a statement |
| `/agents/register` | POST | Register an agent |
| `/agents/bind` | POST | Bind agent to context |
| `/arguments/assert` | POST | Assert an argument between statements |
| `/arguments/:stmt` | GET | Arguments involving a statement |
| `/arguments/frontier` | GET | Contradiction frontier |
| `/obligations/emit` | POST | Emit a proof obligation |
| `/obligations/resolve` | POST | Resolve an obligation |
| `/obligations/open` | POST | List open obligations |
| `/obligations/summary` | GET | Obligation counts by type/status |

### 4.4 DIR protocol expansion

The Donto Intermediate Representation (the JSON protocol between Rust
and Lean) grew from 12 to 23 directive types:

**Original 12:** DeclarePredicate, DeclareContext, DeclareShape,
DeclareRule, AssertBatch, Retract, Correct, ValidateRequest,
ValidateResponse, DeriveRequest, DeriveResponse, Certificate.

**New 11:** IngestDocument, IngestRevision, CreateSpan,
CreateAnnotation, StartExtraction, CompleteExtraction, LinkEvidence,
RegisterAgent, AssertArgument, EmitObligation, ResolveObligation.

---

## 5. The Lean formal verification layer

### 5.1 Architecture

```
Clients → SQL (libpq) / HTTP (dontosrv)
              ↓
  PostgreSQL 16 (pg_donto extension)
              ↕
    dontosrv (axum, Rust)
              ↕ stdio JSON
    donto_engine (Lean 4 binary)
```

Lean runs as a child process of dontosrv. Communication is
line-delimited JSON over stdin/stdout. The Lean engine receives
`validate_request` envelopes containing the shape IRI and the relevant
statements (pre-scoped by dontosrv), evaluates the shape, and returns a
`validate_response` with violations.

**Sidecar contract (PRD §15):** donto is fully usable without Lean.
Built-in shapes and rules are mirrored in Rust. When Lean is offline,
`lean:` shape IRIs return `sidecar_unavailable`; `builtin:` IRIs
continue to work.

### 5.2 Lean modules (18 files, 1,580 lines)

| Module | Lines | Purpose |
|--------|-------|---------|
| `Core.lean` | 64 | Mirror of Postgres types: Statement, Context, Polarity, Modality, Confidence, Maturity, Object, ContextScope |
| `Truth.lean` | 24 | Default visibility predicate, confidence ordering |
| `Predicates.lean` | 23 | Predicate structure |
| `Temporal.lean` | 22 | ValidTime and Precision types |
| `IR.lean` | 55 | DIR envelope with all 23 directive variants |
| `Shapes.lean` | 202 | Shape combinator framework + 4 stdlib shapes (functional, datatype, parentChildAgeGap, roleFit) |
| `Rules.lean` | 55 | Rule combinator + transitive closure |
| `Certificate.lean` | 42 | Certificate kinds (7) + minimal verifiers |
| `Engine.lean` | 153 | JSON dispatch loop — statement parsing, shape lookup, report encoding |
| `Main.lean` | 43 | Stdin/stdout line protocol with ready banner |
| `Theorems.lean` | 269 | 20+ kernel-checked theorems about the core data model |
| `Scope.lean` | ~80 | Scope resolution formalization — 5 theorems |
| `Correction.lean` | ~90 | Correction semantics — 8 theorems |
| `SameMeaning.lean` | ~70 | Equivalence closure — 4 theorems |
| `Hypothesis.lean` | ~65 | Hypothesis scoping and non-leakage — 5 theorems |
| `Canonicals.lean` | ~95 | Bitemporal alias resolution — 3 theorems |
| `Evidence.lean` | ~80 | Evidence chain formalization — 4 theorems |
| `Argumentation.lean` | ~85 | Abstract argumentation framework — 6 theorems |
| `Obligations.lean` | ~110 | Proof obligation lifecycle — 8 theorems |

### 5.3 The 57 kernel-checked theorems

Every theorem below is verified by the Lean 4 kernel at compile time.
If `lake build` succeeds, these properties hold for **every possible
input**, not just test cases. Zero uses of `sorry` (unfinished proof
placeholder).

**Core data model (Theorems.lean):**
- `polarity_total` — Every statement has exactly one of four polarities
- `assert_negate_distinct` — Asserted ≠ negated (paraconsistency)
- `default_visibility_asserted_only` — Default queries return only asserted statements
- `confidence_atLeast_reflexive` — Confidence ordering is reflexive
- `confidence_strong_dominates` — Strong confidence dominates all tiers
- `confidence_uncertified_is_floor` — Uncertified is the bottom tier
- `maturity_bounded` — Maturity is in [0, 4]
- `retract_preserves_identity` — Retraction never changes subject/predicate/object/context
- `retract_does_not_negate` — Retraction does not change polarity
- `snapshot_membership_is_monotone` — Snapshot membership grows with appends
- `snapshot_membership_survives_external_retraction` — Retraction elsewhere is a no-op on snapshots
- `exclude_wins_over_include` — Scope exclude always beats include
- `visible_requires_inclusion` — Visibility requires inclusion
- `identical_inputs_are_equal` — Structural equality (underpins idempotency)
- `functional_shape_no_violations_on_singletons` — Functional shape soundness
- `step_membership_one_direction` — Transitive step membership
- `roleFit_empty` — Empty input → empty role-fit report
- Role-fit worked example — kernel-checked constructive proof of fit for a concrete fixture

**Scope resolution (Scope.lean):**
- `exclude_wins_flat` — Exclude trumps include in flat resolution
- `empty_include_admits_all` — Empty include list = universal
- `not_included_not_visible` — Non-empty include rejects unlisted contexts
- `resolution_deterministic` — Same inputs → same output
- `exclude_monotone` — Adding excludes never increases visibility

**Correction semantics (Correction.lean):**
- `correct_inherits_context` — Context always inherited from original
- `correct_retracted_preserves_subject` — Retracted copy keeps subject
- `correct_retracted_preserves_predicate` — Retracted copy keeps predicate
- `correct_retracted_preserves_object` — Retracted copy keeps object
- `correct_retracted_is_retracted` — Modality set to retracted
- `correct_noop_preserves_content` — All-None correction is identity
- `correct_inherits_valid_time` — Valid time always inherited

**SameMeaning equivalence (SameMeaning.lean):**
- `no_self_alignment` — A statement cannot align with itself
- `symmetric_both_directions` — Symmetric edges are bidirectional
- `start_in_cluster` — Every node is in its own cluster
- `cluster_monotone` — Clusters grow with traversal fuel

**Hypothesis scoping (Hypothesis.lean):**
- `hypothesis_not_in_scope_not_visible` — Non-leakage
- `hypothesis_in_scope_visible` — Inclusion works
- `sibling_isolation` — Sibling hypotheses don't see each other
- `branch_preserves_base` — Branching keeps base contexts
- `branch_includes_hypothesis` — Branching includes the new hypothesis

**Bitemporal canonicals (Canonicals.lean):**
- `open_world` — Unregistered IRI resolves to itself
- `deterministic` — Same inputs → same canonical
- `no_chain_single_hop` — Resolving a canonical again yields itself

**Evidence chains (Evidence.lean):**
- `additive` — Adding a link never removes existing ones
- `retract_preserves_count` — Retraction preserves row count
- `empty_evidence_not_grounded` — No evidence → not grounded

**Argumentation (Argumentation.lean):**
- `no_attacks_non_negative` — Unattacked statements have non-negative pressure
- `no_attacks_not_in_frontier` — Unattacked statements not in contradiction frontier
- `unattacked_pressure_is_support_count` — Pressure = support count when unattacked
- `self_argument_excluded` — Self-arguments are impossible
- `retract_preserves_count` — Retraction preserves argument list length

**Proof obligations (Obligations.lean):**
- `resolved_is_terminal` — Resolved obligations cannot transition
- `rejected_is_terminal` — Rejected obligations cannot transition
- `assign_requires_open` — Assignment only from open state
- `resolve_from_open` — Resolution works from open
- `resolve_from_in_progress` — Resolution works from in-progress
- `resolve_from_resolved_fails` — Double-resolution fails
- `open_count_zero_when_all_resolved` — All resolved → zero open count

---

## 6. Live demonstration: Mistral 7B paper extraction

To validate the evidence substrate end-to-end, we downloaded the
Mistral 7B paper (arXiv:2310.06825), extracted it with `pdftotext`, and
ingested the extracted knowledge into donto through the full evidence
pipeline.

### 6.1 The paper

**Title:** Mistral 7B
**Authors:** Albert Q. Jiang, Alexandre Sablayrolles, Arthur Mensch, et al. (18 authors)
**Published:** 2023-10-10
**Key contribution:** A 7-billion-parameter language model that outperforms Llama 2 13B on all benchmarks and Llama 1 34B on reasoning/math/code, using grouped-query attention and sliding window attention.

### 6.2 What was ingested

| Category | Statements | Details |
|----------|-----------|---------|
| Paper metadata | 4 | Type, title, date, license |
| Authors | 30 | 10 authors × (type + name + authorship link) |
| Model entity | 7 | Type, parameters, name, architecture, attention mechanisms, license |
| Architecture params | 9 | dim, layers, head dim, hidden dim, heads, kv heads, window size, context length, vocab size |
| Benchmark results | 12 | MMLU 60.1%, HellaSwag 81.3%, WinoGrande 75.3%, PIQA 83.0%, ARC-e 80.0%, ARC-c 55.5%, NQ 28.8%, TriviaQA 69.9%, HumanEval 30.5%, MBPP 47.5%, MATH 13.1%, GSM8K 52.2% |
| Comparative claims | 5 | Outperforms Llama 2 13B, outperforms Llama 1 34B (partial), approaches Code-Llama 7B |
| Instruct variant | 8 | Type, base model, MT-Bench 6.84, Arena ELO 1031, outperforms Llama 2 13B Chat, guardrail stats |
| Techniques | 7 | SWA, GQA definitions + properties, rolling buffer cache, pre-fill and chunking |
| Referenced models | 5 | Llama 2 7B/13B, Llama 1 34B, Code-Llama 7B, Llama 2 13B Chat |
| **Total** | **87** | **20 unique subjects, 41 unique predicates** |

### 6.3 Evidence chain

Every statement is linked back to its source through the evidence
substrate:

```
Statement (e.g., "Mistral 7B bench:mmlu = 60.1%")
    ↑ produced_by
Extraction Run (claude-opus-4-6, v2025-04, scientific-paper-extraction)
    ↑ source_revision
Document Revision (pdftotext-24.04, sha256-hashed content)
    ↑ revision_of
Document (arxiv:2310.06825, application/pdf, https://arxiv.org/abs/2310.06825)
    ↑ extracted_by
Agent (claude-opus-extractor, type=llm, model=claude-opus-4-6)
    ↑ bound_to
Context (paper:mistral7b, kind=source, mode=permissive)
```

**174 evidence links** total (every statement linked to its extraction
run via `produced_by`).

### 6.4 Proof obligations emitted

The extractor identified three claims it couldn't fully verify:

| Obligation | Type | Priority | Detail |
|------------|------|----------|--------|
| "outperforms Llama 2 13B on all benchmarks" | `needs-source-support` | 3 | Needs per-benchmark numerical verification against Llama 2 13B's reported scores |
| "approaches Code-Llama 7B performance" | `needs-entity-disambiguation` | 2 | "Approaches" is vague — need exact code benchmark deltas (HumanEval: 30.5% vs 31.1%) |
| "outperforms Llama 1 34B" | `needs-source-support` | 2 | Only true for reasoning/math/code, not all benchmarks |

These are structured work items for downstream agents. They can be
assigned to an agent, tracked, and resolved.

### 6.5 Arguments

Two arguments wired:

1. **MMLU 60.1% supports "outperforms Llama 2 13B"** (strength 0.9) —
   Llama 2 13B scored 55.6% on MMLU, so the 60.1% provides direct
   numerical evidence for the claim.

2. **HumanEval 30.5% supports "approaches Code-Llama 7B"** (strength
   0.7) — Code-Llama 7B scored 31.1%, so 30.5% is close but not equal.

### 6.6 What the Mistral 7B knowledge graph looks like in donto

```
model:mistral-7b
  ├── rdf:type → ml:LanguageModel
  ├── schema:name → "Mistral 7B"
  ├── ml:parameterCount → "7000000000"
  ├── ml:architecture → arch:transformer
  ├── ml:usesAttention → attn:grouped-query-attention
  ├── ml:usesAttention → attn:sliding-window-attention
  ├── schema:license → "Apache 2.0"
  ├── ml:dim → "4096"
  ├── ml:nLayers → "32"
  ├── ml:nHeads → "32"
  ├── ml:nKvHeads → "8"
  ├── ml:windowSize → "4096"
  ├── ml:contextLength → "8192"
  ├── ml:vocabSize → "32000"
  ├── bench:mmlu → "60.1%"
  ├── bench:hellaswag → "81.3%"
  ├── bench:humaneval → "30.5%"
  ├── bench:gsm8k → "52.2%"
  ├── ... (8 more benchmarks)
  ├── ml:outperforms → model:llama2-13b  [argument: MMLU supports this]
  ├── ml:outperforms → model:llama1-34b  [obligation: partial claim]
  ├── ml:approachesPerformance → model:code-llama-7b  [obligation: vague]
  └── ml:usesTechnique → "Rolling Buffer Cache", "Pre-fill and Chunking"

model:mistral-7b-instruct
  ├── ml:baseModel → model:mistral-7b
  ├── bench:mt-bench → "6.84"
  ├── bench:chatbot-arena-elo → "1031"
  ├── ml:outperforms → model:llama2-13b-chat
  ├── ml:guardrailResult → "100% harmful prompt decline rate"
  ├── ml:moderationPrecision → "99.4%"
  └── ml:moderationRecall → "95.6%"

arxiv:2310.06825
  ├── rdf:type → schema:ScholarlyArticle
  ├── schema:name → "Mistral 7B"
  ├── schema:datePublished → "2023-10-10"
  ├── schema:license → "Apache 2.0"
  └── schema:author → person:albert_q__jiang, ..., person:lucile_saulnier
```

---

## 7. Full capability inventory

Everything donto can do today, organized by layer.

### 7.1 Statement layer

- **Assert** statements with subject/predicate/object (IRI or typed literal), context, polarity (asserted/negated/absent/unknown), maturity (0-4), and valid_time
- **Batch assert** multiple statements in one call
- **Retract** statements (close tx_time, never delete)
- **Correct** statements (retract + re-assert with overrides, context inherited)
- **Idempotent re-assertion** (same content returns same statement_id)
- **Pattern matching** with filters on subject, predicate, object, scope, polarity, maturity, as-of-tx, as-of-valid
- **Full-text search** over literal values with language-aware stemming (websearch syntax)
- **Retrofit ingestion** with explicit backdated valid_time and required reason

### 7.2 Context layer

- **10 context kinds** (source, snapshot, hypothesis, user, pipeline, trust, derivation, quarantine, custom, system)
- **2 modes** (permissive: any predicate; curated: registered predicates only)
- **Context hierarchy** with parent links forming a forest
- **Scope resolution** with include/exclude/descendants/ancestors/kind_filter
- **5 built-in scope presets** (anywhere, raw, curated, latest, under_hypothesis)
- **Hypothesis contexts** for counterfactual reasoning (isolated from base scope)
- **Snapshot contexts** with frozen membership (survives retraction)
- **Context environment overlays** (advisory key-value pairs: location, era, dialect, etc.)

### 7.3 Predicate layer

- **Open-world predicate registry** with canonical forms, aliases, labels, descriptions
- **Single-hop alias resolution** (no chains)
- **Bitemporal canonical drift** (same alias → different canonical at different valid_times)
- **Predicate metadata** (domain, range, inverse_of, is_symmetric, is_transitive, is_functional, cardinality)
- **Implicit registration** in permissive contexts
- **Curated rejection** of unregistered predicates

### 7.4 Reaction and endorsement layer

- **Reactions** (endorse, reject, cite, supersede) as meta-statements
- **Endorsement weights** (count(endorses) - count(rejects) per scope)
- **SameMeaning alignment** (symmetric, transitively closable equivalence)

### 7.5 Shape validation layer

- **Shape reports** as additive annotations (pass/warn/violate — never reject)
- **Built-in shapes** (functional predicate, datatype constraint)
- **Lean-authored shapes** (parent-child age gap, role fit)
- **Shape report caching** with SHA256 fingerprint
- **Per-statement shape annotations** with bitemporal lifecycle

### 7.6 Rule derivation layer

- **Built-in rules** (transitive closure, inverse emission, symmetric closure)
- **Lineage tracking** (every derived statement records its inputs)
- **Derivation report caching** with input fingerprint
- **Rule modes** (eager, batch, on_demand)

### 7.7 Certificate layer

- **7 certificate kinds** (direct assertion, substitution, transitive closure, confidence justification, shape entailment, hypothesis scoped, replay)
- **Certificate attachment** with rule IRI, inputs, body, optional signature
- **Verification recording** (verifier name, timestamp, pass/fail)

### 7.8 Evidence substrate (new)

- **Documents** with media type, language, source URL
- **Immutable revisions** with content-hash deduplication and parser version tracking
- **Standoff spans** (char offsets, tokens, sentences, pages, XPath, CSS selectors)
- **Annotation spaces** (named feature namespaces, e.g., Universal Dependencies)
- **Annotations** (feature-value pairs on spans with confidence and extraction run linkage)
- **Annotation edges** (dependency arcs, coreference links, argument structure)
- **Extraction runs** (model, version, prompt hash, temperature, seed, chunking, toolchain)
- **Evidence links** (statement ↔ document/revision/span/annotation/run/statement with link type and confidence)
- **Agent registry** (humans, LLMs, rule engines, extractors, validators, curators)
- **Agent-context bindings** (owner/contributor/reader roles)
- **Argumentation framework** (supports, rebuts, undercuts, endorses, supersedes, qualifies, potentially_same, same_referent, same_event)
- **Contradiction frontier** query (statements under active attack with net pressure)
- **Proof obligations** (needs-coref, needs-temporal-grounding, needs-source-support, needs-entity-disambiguation, etc.)
- **Obligation lifecycle** (open → in_progress → resolved/rejected/deferred)
- **Vector embeddings** (float4[], cosine similarity, brute-force nearest-neighbor search)

### 7.9 Ingestion formats

- N-Quads, Turtle, TriG, RDF/XML, JSON-LD, JSONL, CSV, property graphs
- Quarantine pipeline for shape-violating content
- Idempotent re-ingestion via content hash

### 7.10 Query surfaces

- **DontoQL** (native query language with MATCH, FILTER, SCOPE, POLARITY, MATURITY, PROJECT)
- **SPARQL 1.1 subset** (SELECT, WHERE, FILTER, GRAPH, PREFIX, LIMIT)
- **SQL functions** (donto_assert, donto_match, donto_retract, donto_correct, etc.)
- **HTTP API** (35+ routes on dontosrv)

### 7.11 Observability

- Audit log (every assert/retract/correct/retrofit is logged)
- Stats tables (per-context, per-predicate, per-maturity, per-shape, per-rule)
- Valid-time bucketing for temporal analysis

---

## 8. Testing

### 8.1 Test inventory

| Test category | Files | Tests | What's covered |
|--------------|-------|-------|----------------|
| Core invariants | 16 | 70+ | Paraconsistency, bitemporality, contexts, scopes, corrections, maturity, polarity, predicates, idempotency, concurrency |
| Alexandria extensions | 8 | 25+ | Reactions, endorsement weights, FTS, bitemporal canonicals, SameMeaning, context env, shape annotations, retrofit |
| Evidence substrate | 9 | 45 | Documents, spans, annotations, extraction runs, evidence links, agents, arguments, proof obligations, vectors |
| Comprehensive examples | 1 | 31 | Every capability end-to-end with Ada Lovelace scenario |
| Ingestion formats | 2 | 20 | N-Quads, Turtle, TriG, RDF/XML, JSON-LD, JSONL, CSV, property graphs |
| Query engines | 2 | 23 | DontoQL + SPARQL parsing and evaluation |
| dontosrv integration | 3 | 15 | Shapes, rules, certificates, Lean engine |
| **Total** | **37 binaries** | **190+** | |

All tests run against the live database with 35.5 million statements.

### 8.2 Lean verification

`lake build` compiles all 18 Lean modules and verifies all 57 theorems.
Build time: ~45 seconds. Zero `sorry` placeholders.

---

## 9. What's next

### 9.1 Near-term (documented in LEAN-FORMALIZATION-PLAN.md)

**Phase L2: Proof-carrying shapes.** Shapes currently produce reports
(violation lists). Make them produce evidence witnesses — structured
proof objects that a verifier can check without re-running the shape.
Soundness theorems for each stdlib shape.

**Phase L3: Proof-carrying derivation.** Rules currently produce
statement lists. Make them produce proof trees. Lean verifies the tree
structure. Wire `derive_request` handler in the Lean engine.

**Phase L4: Certificate verifiers in Lean.** Replace the Rust stub
verifiers with Lean verifiers that produce actual proof objects,
checkable independently of dontosrv.

### 9.2 Medium-term

- **Span-level extraction anchoring.** Every extracted claim should be
  anchored to the exact sentence in the source document, not just linked
  to the extraction run.

- **Annotation-to-statement promotion pipeline.** NER annotations →
  entity mentions → coreference resolution → merged entities → donto
  statements. The annotation and span tables are ready; the pipeline
  logic is not.

- **Source-support verification loop.** ProVe-style checker: given a
  claim and its cited source span, verify that the source actually
  supports the claim.

- **Multi-paper contradiction detection.** Ingest multiple ML papers,
  let paraconsistency hold contradictory benchmark claims, use the
  argumentation framework to identify and rank disagreements.

- **RDF-star / nanopublication export.** Curated claims exportable as
  assertion + provenance + publication info bundles.

### 9.3 Long-term

- **pgvector integration** for ANN indexing (currently brute-force
  cosine scan).
- **Lean-authored derivation rules** (currently Rust-only; protocol is
  symmetric).
- **IRI hashing** to 128-bit for storage efficiency at billion-scale.
- **Schema isolation** (`donto` schema instead of `public`).
- **Partitioning** by context kind for operational separation.

---

## 10. How to run donto

```bash
# Start Postgres
./scripts/pg-up.sh

# Apply all 33 migrations
cargo run -p donto-cli --quiet -- migrate

# Start the HTTP sidecar
cargo run -p dontosrv

# Run all tests
DONTO_TEST_DSN=postgres://donto:donto@127.0.0.1:55432/donto cargo test --workspace

# Build the Lean verification layer
cd lean && lake build

# Assert a statement via HTTP
curl -X POST localhost:7878/assert -H 'content-type: application/json' \
  -d '{"subject":"ex:alice","predicate":"ex:knows","object_iri":"ex:bob"}'

# Query via DontoQL
curl -X POST localhost:7878/dontoql -H 'content-type: application/json' \
  -d '{"query":"MATCH ?s ex:knows ?o"}'
```

---

## 11. Project metadata

- **Author:** Thomas Davis (thomasalwyndavis@gmail.com)
- **Repository:** github.com/thomasdavis/donto
- **License:** (see repository)
- **Postgres:** 16+ (Docker image: `postgres:16`)
- **Rust:** stable (Cargo workspace)
- **Lean:** 4.12.0 (no external dependencies)
- **Date of this report:** 2026-04-25
