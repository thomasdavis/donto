# donto v1000 — Refactor Plan

> **STATUS: SUPERSEDED.** Canonical PRD is now
> [`DONTO-V1000-PRD.md`](DONTO-V1000-PRD.md). This document is preserved
> as a historical artefact.


> **Codename:** Atlas Zero on donto
> **Document type:** Refactor plan, version contract, migration roadmap
> **Audience:** the principal engineer of donto; agentic coders working on the refactor; reviewers
> **Date:** 2026-05-07
> **Status:** v0.1 — pre-implementation; supersedes [`LANGUAGE-EXTRACTION-PLAN.md`](LANGUAGE-EXTRACTION-PLAN.md)
>
> **What this document does.** Maps the Atlas Zero PRD onto the existing
> donto codebase (67 migrations, 60+ tables, 141 SQL functions, ~94
> HTTP endpoints across two services, three query languages, eight
> ingest formats, two migrators, twelve CLI subcommands, six TUI tabs,
> five invariant test suites, and a Lean overlay with 62 theorems) and
> specifies the changes required to ship a substrate that is
> linguistic-extraction-native without losing any of donto's general-
> purpose semantics. Everything is grounded in file paths and line
> numbers; nothing in here is hand-waved.
>
> **What it does not do.** Edit code. This is the plan; implementation
> stops at each milestone boundary for review.

---

## Table of contents

0. [Executive summary](#0-executive-summary)
1. [Naming, scope, and positioning](#1-naming-scope-and-positioning)
2. [Current state of donto — what we have to work with](#2-current-state-of-donto--what-we-have-to-work-with)
3. [The PRD-to-donto mapping (section by section)](#3-the-prd-to-donto-mapping-section-by-section)
4. [The gap inventory](#4-the-gap-inventory)
5. [Naming reconciliation](#5-naming-reconciliation)
6. [New schema migrations (0068 → ~0090)](#6-new-schema-migrations-0068--0090)
7. [API redesign — unifying dontosrv and donto-api](#7-api-redesign--unifying-dontosrv-and-donto-api)
8. [Extraction pipeline refactor](#8-extraction-pipeline-refactor)
9. [Ingest, migrator, and CLI changes](#9-ingest-migrator-and-cli-changes)
10. [TUI and operational tooling](#10-tui-and-operational-tooling)
11. [Documentation reconstruction (critical: rebuild PRD.md)](#11-documentation-reconstruction-critical-rebuild-prdmd)
12. [Milestone breakdown — M−1 through M10](#12-milestone-breakdown--m1-through-m10)
13. [The v1000 non-negotiables](#13-the-v1000-non-negotiables)
14. [Test strategy](#14-test-strategy)
15. [Performance and scale](#15-performance-and-scale)
16. [Risk register](#16-risk-register)
17. [Open questions for the principal engineer](#17-open-questions-for-the-principal-engineer)
18. [Appendices](#18-appendices)

---

## 0. Executive summary

donto is already 70–80% of the Atlas Zero substrate. The remaining
20–30% is mostly schema additions (open-world language identity, access
governance, claim levels, modality, validation states, release
artifacts) and naming reconciliation, plus a documentation rebuild
because **the canonical PRD.md was deleted on 2026-04-28 in commit
281a5bea** and CLAUDE.md still references its sections.

**v1000 is a renaming-and-extension version, not a rewrite.** The
Postgres schema, the bitemporal-paraconsistent semantics, the predicate
alignment layer, the evidence substrate, the argumentation layer, the
proof obligations, the Lean overlay, the query languages, the ingest
adapters, the genealogy migrator, the TUI, and the test invariants all
survive intact. We add ~22 new migrations (0068–0089), unify the two
HTTP services into a single facade, replace the 8-tier extraction prompt
with a domain-dispatched prompt selector, add adapters for CLDF / UD /
UniMorph / LIFT / EAF, ship a release builder, and rebuild the PRD.

**The single biggest decision before code:** whether donto stays
domain-agnostic (linguistic specialisations live in
`packages/donto-ling-*` sister crates) or whether v1000 is an explicit
linguistic positioning of donto with the predicate vocabulary baked in.
This document assumes **domain-agnostic core, linguistics in sister
crates** — same engine, two domain stacks (genealogy + linguistics),
with the architecture cleanly extensible to medicine, law, science,
intelligence, and any other claims-under-uncertainty domain.

---

## 1. Naming, scope, and positioning

### 1.1 The names

| Name              | Meaning                                                                                                |
|-------------------|--------------------------------------------------------------------------------------------------------|
| **donto**         | The bitemporal, paraconsistent quad-store engine. Schema, sidecar, query languages, Lean overlay.       |
| **Atlas Zero**    | A product / application built on donto for linguistic evidence. Sister crates plus configuration.       |
| **v1000**         | The version of donto in which it is explicitly architected as Atlas Zero's substrate without losing generality. |

donto stays donto. Atlas Zero is the linguistics product. v1000 is the
inflection-point version where the engine becomes deliberately
multi-domain instead of accidentally so.

### 1.2 Scope of v1000

In scope for v1000:

- Schema extensions for open-world language identity, access governance,
  claim levels, modality, validation states, release artifacts.
- API unification (single facade over the two existing services).
- Domain-dispatched extraction prompt selector.
- New ingest adapters (CLDF, UD/CoNLL-U, UniMorph, LIFT, EAF).
- Linguistics sister crates (`donto-ling-vocab`, `donto-ling-cldf`,
  `donto-ling-shapes`, `donto-ling-export`).
- Release builder.
- Documentation rebuild, including a reconstructed `PRD.md`.
- Test additions for new invariants.
- TUI tab additions (paradigm view, IGT view, language profile, source
  workbench, release builder, policy view).

Out of scope for v1000:

- Replacing Postgres or Rust as foundations.
- Removing genealogy support — every existing test, migrator, and
  endpoint continues to work.
- Adding a new query planner (PRESET resolution and EXPLAIN are deferred
  to v1100).
- Sign-language specifics beyond schema readiness (full schema and
  fixtures land in v1010 unless explicitly promoted).
- ASR or audio processing pipelines (out of substrate; live in
  application layers).

### 1.3 Positioning vs. the alternative

The alternative was Path B from the prior conversation — building Atlas
Zero from scratch as a separate codebase. Cost was estimated at 18–24
months for the substrate, plus 4–6 months for the genuinely-new pieces.
Path A — adopting donto and adding only the genuinely-new pieces — is
explicitly cheaper, but it required confirming that donto's substrate
holds up under linguistic load. The five research agents have now
verified that it does.

---

## 2. Current state of donto — what we have to work with

This section is grounded in the current repo head. Every claim cites a
file path or migration number.

### 2.1 Schema — 67 migrations, 11 conceptual groups

Source of truth: `packages/sql/migrations/0001_core.sql` through
`0067_rule_engine.sql`.

#### A. Core data model

| Migration | What it adds                                                                                             |
|-----------|----------------------------------------------------------------------------------------------------------|
| `0001_core.sql`    | `donto_context`, `donto_statement`, `donto_stmt_lineage`, `donto_audit`. The four foundational tables. |
| `0002_flags.sql`   | `flags smallint` packing polarity (bits 0–1: asserted/negated/absent/unknown) + maturity (bits 2–4: 0–4). Helpers `donto_pack_flags`, `donto_polarity`, `donto_maturity`. |
| `0003_functions.sql` | `donto_assert`, `donto_retract`, `donto_correct`, `donto_match`, `donto_ensure_context`, `donto_resolve_scope`. The write/read/scope primitives. |
| `0004_migrations.sql`| `donto_migration` ledger; SHA-256 idempotency check on re-apply.                                       |
| `0006_predicate.sql` | `donto_predicate` registry with alias resolution.                                                       |
| `0007_snapshot.sql`  | Snapshots with `donto_snapshot_member` and time bounds.                                                |

The `donto_statement` row is the substrate for everything else:

```
statement_id  uuid PK
subject       text
predicate     text
object_iri    text     -- mutually exclusive with object_lit
object_lit    jsonb    -- {v, dt, lang}
context       text FK donto_context.iri
tx_time       tstzrange  -- when learned, open-ended until retraction
valid_time    daterange  -- when true in world
flags         smallint   -- polarity + maturity (see 0002)
content_hash  bytea      -- sha256 used for idempotency
```

Indexes: SPO, POS, OSP for triple-pattern access; GiST on `valid_time`
and `tx_time`; GIN on `object_lit`. Open-content uniqueness:

```sql
CREATE UNIQUE INDEX donto_statement_open_content_uniq
  ON donto_statement (content_hash)
  WHERE upper(tx_time) IS NULL;
```

This is the idempotency invariant. Re-asserting an open statement is a
no-op.

#### B. Scope and presets

| Migration | What it adds                                                                                              |
|-----------|-----------------------------------------------------------------------------------------------------------|
| `0005_presets.sql`              | Named scopes: `anywhere`, `raw`, `curated`, `latest`, `under_hypothesis`, `as_of`. **Defined but evaluator does NOT yet consume — see §17 open questions.** |
| `0012_match_scope_fix.sql`      | Bugfix to scope resolution.                                                                                |
| `0017_reactions.sql`            | Folksonomic reactions (`endorses`, `rejects`, `cites`, `supersedes`).                                       |
| `0018_aggregates.sql`           | Aggregation helpers without implicit ordering.                                                              |
| `0020_bitemporal_canonicals.sql`| Canonical bitemporal lookups.                                                                              |
| `0022_context_env.sql`          | Context-level environment variables.                                                                       |

#### C. Validation infrastructure

| Migration | What it adds                                                                                              |
|-----------|-----------------------------------------------------------------------------------------------------------|
| `0008_shape.sql`             | `donto_shape`, `donto_shape_report` cache.                                                                  |
| `0009_rule.sql`              | `donto_rule`, derivation cache.                                                                            |
| `0010_certificate.sql`       | `donto_stmt_certificate` with seven kinds (direct_assertion, substitution, transitive_closure, confidence_justification, shape_entailment, hypothesis_scoped, replay). |
| `0011_observability.sql`     | `donto_stats_*` views.                                                                                     |
| `0015_shape_annotations.sql` | Per-statement shape verdicts.                                                                              |
| `0045_auto_shape_validation.sql` | Auto-revalidation triggers.                                                                            |

#### D. FTS and search

| Migration | What it adds                                              |
|-----------|------------------------------------------------------------|
| `0013_search_trgm.sql` | Trigram index on labels (`donto_label_cache`, ~516K rows). |
| `0019_fts.sql`          | Full-text search.                                          |
| `0033_vectors.sql`      | Vector column for predicate descriptors.                   |

#### E. Bitemporal and same-meaning

| Migration | What it adds                                                                                              |
|-----------|-----------------------------------------------------------------------------------------------------------|
| `0014_retrofit.sql`         | Bitemporal retrofit on early data.                                                                          |
| `0016_valid_time_buckets.sql`| Bucketing for time-range queries.                                                                          |
| `0021_same_meaning.sql`      | Cross-context same-meaning links.                                                                          |
| `0040_temporal_expressions.sql`, `0063_time_expression.sql`, `0064_temporal_relation.sql` | Temporal expression parsing with grain, uncertainty, Allen relations (planned; partial implementation). |

#### F. Evidence substrate

This is the spine of provenance.

| Migration | What it adds                                                                                              |
|-----------|-----------------------------------------------------------------------------------------------------------|
| `0023_documents.sql`         | `donto_document` (registered source).                                                                       |
| `0024_document_revisions.sql`| `donto_document_revision` (text/OCR pass).                                                                  |
| `0025_spans.sql`             | `donto_span` (offsets, page bbox, region anchors).                                                          |
| `0026_annotations.sql`, `0027_annotation_edges.sql` | Annotation overlay.                                                                  |
| `0028_extraction_runs.sql`   | `donto_extraction_run` (model, prompt, temperature, chunking).                                              |
| `0029_evidence_links.sql`    | `donto_evidence_link` between statements and spans/runs/documents.                                          |
| `0030_agents.sql`            | `donto_agent`, `donto_agent_binding` (humans, AIs, services).                                              |
| `0034_claim_card.sql`        | `donto_claim_card(stmt_id)` reconstructs the full chain.                                                    |
| `0035_document_sections.sql` | Section structure inside documents.                                                                        |
| `0036_mentions.sql`          | `donto_mention` (entity references).                                                                       |
| `0037_extraction_chunks.sql` | `donto_extraction_chunk` per-chunk provenance.                                                             |
| `0038_confidence.sql`        | Confidence overlay separate from maturity.                                                                  |
| `0041_content_regions.sql`   | Typed regions inside revisions (e.g., "this is an IGT block").                                              |
| `0042_entity_aliases.sql`    | One-hop alias resolution.                                                                                  |
| `0046_references.sql`        | Bibliographic references between documents.                                                                |

The chain `donto_document → donto_document_revision → donto_extraction_chunk
→ donto_span → donto_evidence_link → donto_statement` is the most
important single artifact in the codebase. **Every PRD evidence anchor
kind maps onto this chain.**

#### G. Argumentation and obligations

| Migration | What it adds                                                                                              |
|-----------|-----------------------------------------------------------------------------------------------------------|
| `0031_arguments.sql`         | `donto_argument` with relation kinds: `supports`, `rebuts`, `undercuts`, `qualifies`. Plus `donto_contradiction_frontier` view. |
| `0032_proof_obligations.sql` | `donto_proof_obligation` with eight standard kinds (needs_coref, needs_temporal_grounding, needs_source_support, needs_disambiguation, needs_human_review, etc.). |

#### H. Domain primitives

| Migration | What it adds                                                                                              |
|-----------|-----------------------------------------------------------------------------------------------------------|
| `0039_units.sql`             | Unit registry for cross-paper normalization (used by scientific paper pipeline).                            |
| `0042_entity_aliases.sql`    | (Listed under F; mentioned again here because it's also a domain primitive.)                               |
| `0043_candidate_contexts.sql`| Per-claim candidate context proposals.                                                                     |
| `0044_ontology_seeds.sql`    | Seed predicates and class IRIs.                                                                            |
| `0047_claim_lifecycle.sql`   | Lifecycle stages (raw → parsed → linked → reviewed → certified) per context.                                |

#### I. Predicate Alignment Layer (PAL)

This is donto's most-developed cross-cutting feature.

| Migration | What it adds                                                                                              |
|-----------|-----------------------------------------------------------------------------------------------------------|
| `0048_predicate_alignment.sql`        | `donto_predicate_alignment` with six relation kinds: `exact_equivalent`, `inverse_equivalent`, `sub_property_of`, `close_match`, `decomposition`, `not_equivalent`. |
| `0049_predicate_descriptor.sql`       | Rich metadata: label, gloss, domain, range, examples, embedding (vector).                                   |
| `0050_alignment_run.sql`              | Provenance for alignment operations.                                                                       |
| `0051_predicate_closure.sql`          | Cached transitive closure for query expansion.                                                              |
| `0052_match_aligned.sql`              | `donto_match_aligned(...)` with confidence threshold.                                                       |
| `0053_canonical_shadow.sql`           | Pre-materialized canonical predicate per statement.                                                        |
| `0054_event_frames.sql`               | n-ary decomposition: `donto_decompose_to_frame(...)` emits a frame node + role predicates.                  |
| `0055_match_alignment_integration.sql`| `donto_match` rides closure by default.                                                                    |
| `0056_lexical_normalizer.sql`         | Lexical normalization before alignment.                                                                    |

PAL is the reason donto can integrate WALS, Grambank, AUTOTYP, UD,
UniMorph, GOLD, OLiA, and OntoLex-Lemon side-by-side without picking a
winner. Atlas Zero §12 maps cleanly onto this — see §3.12 below.

#### J. Entity resolution

| Migration | What it adds                                                                                              |
|-----------|-----------------------------------------------------------------------------------------------------------|
| `0057_entity_symbol.sql`      | IRI representations of entities.                                                                            |
| `0058_entity_mention.sql`     | References in text with confidence.                                                                        |
| `0059_entity_signature.sql`   | Feature-vector signatures for candidate generation.                                                         |
| `0060_identity_edge.sql`      | Edges asserting entities refer to the same thing.                                                          |
| `0061_identity_hypothesis.sql`| Scoped hypothesis contexts (`strict_identity_v1`, `likely_identity_v1`, `exploratory_identity_v1`).         |
| `0062_literal_canonical.sql`  | Cross-paper literal normalization.                                                                         |

This is the "open-world identity hypothesis" infrastructure that Atlas
Zero §13 describes. Donto already has it; what's missing is the
language-variety-specific overlay (§3.13 below).

#### K. Constraints and ontology

| Migration | What it adds                                                                                              |
|-----------|-----------------------------------------------------------------------------------------------------------|
| `0065_property_constraint.sql` | Cardinality and value constraints (does NOT reject; produces violations).                                  |
| `0066_class_hierarchy.sql`     | OWL-style class / subclass relations.                                                                       |
| `0067_rule_engine.sql`         | Derivation rules (transitive, inverse, symmetric).                                                          |

#### Synthesis

The schema as a whole gives the system: bitemporal, paraconsistent,
context-scoped, evidence-anchored, argument-aware, obligation-aware,
shape-validated, alignment-rewriting, identity-hypothesis-aware,
proof-of-meaning storage. **Atlas Zero §7–§8 already lives here.**
What's missing is what §3 below catalogues.

### 2.2 API surface — 44 + ~50 endpoints across two services

#### Sidecar `dontosrv` (Rust + Axum)

Routes registered at `apps/dontosrv/src/lib.rs:41` (`pub fn router`).
44 endpoints in 9 functional groups:

- **System** (3): `/health`, `/version`, `/dir` (Donto Intermediate Representation bulk protocol).
- **Query** (8): `/sparql`, `/dontoql`, `/subjects`, `/search`, `/history/:subject`, `/statement/:id`, `/contexts`, `/predicates`.
- **Write** (4): `/contexts/ensure`, `/assert`, `/assert/batch`, `/retract`.
- **Reactions** (2): `/react`, `/reactions/:id`.
- **Evidence** (4): `/documents/register`, `/documents/revision`, `/evidence/link/span`, `/evidence/:stmt`.
- **Agents** (2): `/agents/register`, `/agents/bind`.
- **Arguments + Obligations** (7): `/arguments/{assert,:stmt,frontier}`, `/obligations/{emit,resolve,open,summary}`.
- **Validation** (5): `/shapes/validate`, `/rules/derive`, `/certificates/{attach,verify/:stmt}`, `/dir`.
- **Alignment** (9): `/alignment/{register,retract,rebuild-closure,runs/{start,complete}}`, `/descriptors/{upsert,nearest}`, `/shadow/{materialize,rebuild}`.

This is the **authoritative** API. It speaks SQL and Lean directly. Any
unification keeps it as the inner core.

#### Batch app `donto-api` (Python + FastAPI)

Routes in `apps/donto-api/main.py`. ~50 endpoints, mostly forwarding +
extraction-specific:

- **System** (5): `/health`, `/version`, `/full-docs`, `/simple-docs`, `/guide` (genealogy markdown).
- **Firehose** (3): `/firehose/stream` (SSE), `/firehose/recent`, `/firehose/stats`.
- **Extract** (2): `/extract-and-ingest`, `/extract` (LLM-only, no ingest).
- **Jobs** (8): `/jobs/{extract,batch}`, `/jobs`, `/jobs/{id}`, `/jobs/retry-failed`, `/jobs/{id}/{facts,source}`, `/queue` (HTML).
- **Ingest forwarding** (2): `/assert`, `/assert/batch` (forward to dontosrv).
- **Query forwarding** (8): `/subjects`, `/search`, `/history/{subject}`, `/statement/{id}`, `/contexts`, `/predicates`, `/query`, `/retract/{id}`.
- **Graph** (9): `/connections/{entity}`, `/context/analytics/{ctx}`, `/graph/{neighborhood,path,stats,subgraph,entity-types,timeline,compare}`.
- **Alignment forwarding** (4): `/align/{register,rebuild,retract,suggest}`.
- **Evidence + Claims** (2): `/evidence/{stmt}`, `/claim/{stmt}`.
- **Entity resolution** (8): `/entity/{register,register/batch,identity,identity/batch,membership,/{iri}/edges,cluster/{h}/{r},resolve/{iri},family-table}`.
- **Papers** (3): `/papers/ingest`, `/papers/{id}`, `/papers/{id}/claims` — domain-specific.

Notable overlaps with dontosrv (`/assert`, `/retract`, queries) — donto-
api is a forwarding layer for those. Notable absences in donto-api:
`/shapes/validate`, `/rules/derive`, `/certificates/*`, `/agents/*`,
`/arguments/*`, `/obligations/*`, `/dir`. **The unification proposed in
§7 closes those gaps.**

### 2.3 Query languages

`packages/donto-query/src/`:

#### DontoQL — `dontoql.rs` (10 clauses)

- `SCOPE include … exclude … no_descendants ancestors` — context filtering.
- `PRESET <name>` — named scope (latest/raw/curated/under_hypothesis/as_of/anywhere). **Parser accepts; evaluator does NOT yet consume.**
- `MATCH triple [, triple]…` — basic graph pattern with optional `IN` graph binding.
- `FILTER expr [, expr]…` — `=` and `!=` only (Phase 4 limitation).
- `POLARITY <asserted|negated|absent|unknown>`.
- `MATURITY [>=]? <int>`.
- `IDENTITY <default|expand_clusters|expand_sameas_transitive|strict>`.
- `PREDICATES <EXPAND|STRICT|EXPAND_ABOVE <0–100>>`.
- `PROJECT ?var [, ?var]…`.
- `LIMIT <n>` `OFFSET <n>`.

#### SPARQL 1.1 subset — `sparql.rs`

Supports: `PREFIX`, `SELECT`, `WHERE { … }`, `GRAPH <iri> { … }`,
`FILTER` (all 6 ops; `<,<=,>,>=` evaluate but DontoQL rejects them),
`LIMIT`, `OFFSET`. Defaults `polarity=asserted`, `min_maturity=0`,
`predicate_expansion=Expand`.

Explicitly unsupported: OPTIONAL, UNION, MINUS, property paths,
aggregates, CONSTRUCT/ASK/DESCRIBE, federated SERVICE, ORDER BY.

#### Algebra and evaluator — `algebra.rs`, `evaluator.rs`

Both languages compile to the same `Query` struct. Evaluation is
nested-loop:

> "for each pattern, call match_pattern with the bound terms substituted; cartesian-join results on shared variable bindings; apply filters; project; limit." — `evaluator.rs:9`.

No query planner, no EXPLAIN, no statistics. PRESET resolution is
not implemented.

### 2.4 Extraction pipeline

`apps/donto-api/`:

- **Prompt:** `helpers.py:64` — 8-tier sociology/genealogy prompt
  (T1 surface facts → T8 intertextual). 78-line prompt template; LLM
  must output JSON `{facts: [...]}`.
- **Decomposer:** `helpers.py:354–384` — converts LLM facts to
  `donto_assert_batch` shape, calling `parse_fact_object` (lines 47–61)
  and `confidence_to_maturity` (lines 39–44).
- **Web cleaning:** `helpers.py:166–249` — three layers: trafilatura
  (if installed), heuristic strip, prompt instruction.
- **Workflow:** `workflows.py` — Temporal `ExtractionWorkflow` with
  four activities:
  1. `extract_facts_activity` — 10m timeout, 3 retries.
  2. `ingest_facts_activity` — 5m timeout, 5 retries.
  3. `align_predicates_activity` — 5m timeout, 2 retries.
  4. `resolve_entities_activity` — 5m timeout, 2 retries.
- **Activities:** `activities.py` — bodies of each activity. Alignment
  uses trigram similarity; entity resolution uses IRI match + name
  match.
- **Status progression:** `queued → extracting → ingesting → aligning →
  resolving → completed | failed`.
- **Confidence → maturity:** `helpers.py:39`:

```python
0.95+  → 4 (certified)
0.80+  → 3 (validated)
0.60+  → 2 (evidenced)
0.40+  → 1 (registered)
<0.40  → 0 (raw)
```

### 2.5 Ingest formats — `packages/donto-ingest/src/`

Eight formats with a shared `Pipeline` that batches via `assert_batch`:

| File                | Format                                          |
|---------------------|--------------------------------------------------|
| `nquads.rs`         | N-Quads (named graph → context).                |
| `turtle.rs`         | Turtle, TriG (named graph → context).           |
| `rdfxml.rs`         | RDF/XML.                                         |
| `jsonld.rs`         | JSON-LD subset (`@context`, `@graph`, `@id`).   |
| `jsonl.rs`          | One-statement-per-line (LLM-friendly).           |
| `property_graph.rs` | Neo4j / AGE node+edge JSON.                     |
| `csv.rs`            | CSV with mapping.                                |
| `quarantine.rs`     | Quarantine routing for invalid input.            |
| `pipeline.rs`       | Shared batcher.                                  |

### 2.6 Migrators — `packages/donto-migrate/src/`

Two crates, both for the genealogy domain:

- `genealogy.rs` (430 lines) — SQLite → donto, ten table mappings.
- `relink.rs` (566 lines) — additive second pass; emits provenance
  subjects under `<root>/document/<id>`, `<root>/chunk/<id>`,
  `<root>/claim/<id>` etc.

The migrator pattern is open-coded — there's no shared
`Migrator` trait. Adding new migrators (CLDF, CoNLL-U, UniMorph, …)
follows the same shape: open external source, walk tables, emit
`StatementInput` Vec, batch-assert.

### 2.7 CLI — `apps/donto-cli/src/main.rs`

12 subcommands:

`migrate`, `ingest`, `match`, `query`, `retract`, `extract` (LLM
extraction), `bench`, `align register|suggest|auto|list|retract|rebuild`,
`predicates`, `shadow`, `man`, `completions`.

### 2.8 TUI — `apps/donto-tui/`

Six tabs (Dashboard, Firehose, Explorer, Contexts, Claim Card, Charts)
implemented as Bubbles Tea models. LISTEN/NOTIFY drives the firehose;
polling drives everything else. Keybindings 1–6 + Tab/Shift-Tab cycle;
`q`/Ctrl-C quit; `?` help; per-tab keys for filtering and detail view.

### 2.9 Tests — `packages/donto-client/tests/`

Five invariant suites covering the five PRD principles:

- `invariants_paraconsistency.rs` — three-way disagreement coexists; assert+negate in same context coexist.
- `invariants_bitemporal.rs` — retraction never deletes; double-retract idempotent; as-of in open window; correction creates new row.
- `invariants_migration_idempotent.rs` — migrate twice → ledger stable; functions stable; every embedded migration in ledger; hashes match.
- `scope.rs` — descendants default; exclude wins; ancestors opt-in.
- `assert_match.rs` — assert/match round-trip; literal datatypes preserved; batch counts.

Test isolation pattern: `pg_or_skip!` macro for clean skips when
Postgres absent; per-test UUID prefix (`test:<name>:<uuid>`); cleanup at
test entry not exit.

### 2.10 Documentation state — *the critical gap*

**`PRD.md` was deleted in commit 281a5bea on 2026-04-28.** The agent
reading the docs found it referenced everywhere but absent. This is the
single most urgent docs-side problem.

Existing documents:

- `README.md` (19 KB) — public overview.
- `CLAUDE.md` (5.5 KB) — working contract; references PRD §3, §2, §15, §18, §19, §25 — none of which exist anywhere now.
- `CHANGELOG.md` (3.4 KB) — phase-by-phase history.
- `ANTHROPOLOGY_README.md` (11 KB) — applied/practical framing.
- `docs/ARCHITECTURE-REPORT.md` (30 KB) — entity-resolution research brief; partially implemented in migrations 0057–0061.
- `docs/DONTO-RESEARCH-BRIEF.md` (40 KB) — what exists vs. what's missing (May 2026).
- `docs/GENEALOGY-GUIDE.md` (40 KB) — practical tutorial.
- `docs/LANGUAGE-EXTRACTION-PLAN.md` (37 KB) — earlier plan, superseded by this document.
- `Justfile` (30 recipes) — dev workflow.

Documentation drift is real. Three different documents describe entity
resolution at three different levels of completeness; nothing reconciles
them. v1000 must do the reconciliation.

---

## 3. The PRD-to-donto mapping (section by section)

For each Atlas Zero PRD section, the mapping below lists what donto
already has, what needs to be added, and what needs to be renamed. The
section numbers correspond to the PRD's §0–§30.

### 3.1 PRD §1–§3: Problem, vision, design principles

The ten principles (P1–P10) align almost exactly with donto's
non-negotiables:

| PRD principle                                | donto correspondence                                                                  |
|----------------------------------------------|--------------------------------------------------------------------------------------|
| P1 Open-world language identity              | `donto_identity_hypothesis` (0061) — needs language-variety overlay (§3.13).         |
| P2 Evidence before claims                    | `donto_evidence_link` (0029) — already enforced at L2+. **Need to enforce at L0/L1 too.** |
| P3 Existing formats are adapters             | `packages/donto-ingest/` — already the design.                                        |
| P4 Contradictions are data                   | Paraconsistency invariant (0001 + tests in `invariants_paraconsistency.rs`).         |
| P5 Scope is mandatory                        | Every statement has a context (default `donto:anonymous`); CLAUDE.md non-negotiable. |
| P6 Machine confidence ≠ scholarly truth      | `donto_confidence` (0038) is overlay; `flags.maturity` is separate. **Need to add review/validation states (§3.14).** |
| P7 Governance as core                        | **Not yet built.** §3.15 specifies the new tables.                                   |
| P8 Signed languages first-class              | `donto_span` regions (0025, 0041) support time-aligned media. Needs schema readiness for articulators (§3.8). |
| P9 Local values can be local                 | PAL (`donto_predicate_alignment` 0048) — already the design.                          |
| P10 Releases are reproducible views          | **Not yet built.** §3.17 specifies the release builder.                              |

### 3.2 PRD §4: Target users and jobs

User roles map onto donto's existing agent system (`donto_agent` 0030):

- Computational linguist → `agent_type='human', role='analyst'`.
- Descriptive linguist → same with role='linguist'.
- Language community / archive steward → **needs `role='community_authority'` plus access policy enforcement (§3.15).**
- Agentic coder / extraction agent → `agent_type='ai', model_id=...`.
- NLP engineer → `agent_type='human', role='engineer'`.

No schema change needed for roles; access policy is the only gap.

### 3.3 PRD §5: Scope (in vs. out)

donto's substrate already covers all in-scope items. The out-of-scope
list (no canonical classification, no automatic publication, no theory
imposition) is consistent with donto's non-negotiables.

### 3.4 PRD §6: Research landscape — adapters, not architecture

Existing adapters: 8 ingest formats. Missing for v1000:

| PRD-named adapter         | New crate                                             | Inherits from                   |
|---------------------------|-------------------------------------------------------|---------------------------------|
| CLDF                      | `packages/donto-ling-cldf`                            | `jsonld.rs` + thin wrapper      |
| Universal Dependencies    | `packages/donto-ling-ud`                              | New, thin (CoNLL-U is line-oriented) |
| UniMorph                  | `packages/donto-ling-unimorph`                        | New, thin (TSV)                  |
| LIFT                      | `packages/donto-ling-lift`                            | New (XML)                       |
| ELAN-EAF                  | `packages/donto-ling-eaf`                             | New (XML, time-aligned)          |
| TEI                       | `packages/donto-ling-tei` (v1010)                    | New (XML, structured text)       |
| OLAC / CMDI               | `packages/donto-ling-olac` (v1010)                   | Catalogue ingestion              |
| Praat TextGrid            | (v1010)                                               | New, time-aligned                |

### 3.5 PRD §7.1: Source asset

The PRD's source asset object maps onto:

| PRD field              | donto correspondence                                                              |
|------------------------|-----------------------------------------------------------------------------------|
| `source_id`            | `donto_document.document_id`                                                      |
| `title`                | `donto_document.label` or `donto_document_revision.metadata`                      |
| `source_type`          | **Add field** to `donto_document`: `source_type text` (grammar_pdf, dictionary, recording, dataset, manuscript, corpus, article, unknown). |
| `original_uri`         | `donto_document.source_url`                                                       |
| `storage_uri`          | **Add field** `storage_uri text`.                                                 |
| `checksum_sha256`      | **Add field** `checksum_sha256 bytea`.                                            |
| `bibliographic_metadata` | `donto_document.metadata jsonb`                                                |
| `language_candidates`  | **New table** `donto_source_language_candidate` (FK to document, FK to language_variety, confidence). |
| `rights_summary`       | **Add field** `rights_summary text`.                                              |
| `access_policy_ids`    | **New: see §3.15.**                                                               |

Migration: `0068_source_asset_extension.sql` adds the new columns and
the language-candidates table. Existing data backfills with NULLs.

### 3.6 PRD §7.2: Evidence anchor

The PRD's anchor kinds:

| PRD anchor kind          | donto correspondence                                                              |
|--------------------------|-----------------------------------------------------------------------------------|
| `text_char_span`         | `donto_span.start_char, end_char`.                                                |
| `pdf_page_bbox`          | `donto_span.region jsonb` — schema: `{kind: 'pdf_page_bbox', page, bbox: [...]}`. |
| `image_bbox`             | `donto_span.region` with kind `image_bbox`.                                       |
| `media_time_span`        | `donto_span.region` with kind `media_time_span` and `start_ms, end_ms`.           |
| `elan_tier_annotation`   | `donto_span.region` with kind `elan_tier_annotation`, `tier_id`, `annotation_id`. |
| `table_cell`             | `donto_span.region` with kind `table_cell`, `table_id, row_id, column_id`.        |
| `csv_row`                | `donto_span.region` with kind `csv_row`, `row_index, columns`.                    |
| `corpus_token`           | `donto_span.region` with kind `corpus_token`, `text_id, sentence_id, token_id`.   |
| `gloss_line`             | `donto_span.region` with kind `gloss_line`, `igt_block, line_number, morpheme_index`. |
| `archive_record_field`   | `donto_span.region` with kind `archive_record_field`, `record_id, field_name`.     |

donto already has `donto_span.region jsonb` (in `0041_content_regions.sql`),
which is the right primitive. v1000 adds:

- A migration `0069_anchor_kinds_registry.sql` that registers a
  controlled vocabulary of anchor kinds with required-field schemas, so
  the validation layer can check that a `pdf_page_bbox` actually has a
  page and bbox.
- Anchor-kind shape validators (Lean or Rust) for each kind.

### 3.7 PRD §7.3: Language variety

This is the largest schema gap. Atlas Zero requires open-world language
identity with split/merge candidates, multiple identifiers (Glottocode,
ISO 639-3, BCP 47, WALS, Austlang, ELCat), and provisional varieties
for sources whose language identity is unresolved.

donto has the *infrastructure* (identity hypotheses, contexts), but no
language-specific overlay. v1000 adds:

- `0070_language_variety.sql`: `donto_language_variety` (variety_id PK,
  preferred_label, variety_type enum, identity_status enum, parent_variety,
  notes, created_at, updated_at).
- `0071_language_identifier.sql`: `donto_language_identifier`
  (identifier_id PK, variety_id FK, scheme enum, value, confidence,
  source_id FK, status enum).
- `0072_language_name.sql`: `donto_language_name` (variety_id FK,
  source_id FK, label, status enum: preferred / alternate / historical /
  exonym / endonym / deprecated).
- `0073_language_relations.sql`: parent / hasDialect / hasRegister
  predicates registered as `donto_predicate` rows; relations stored as
  ordinary statements.

The seven `variety_type` values: `family`, `language`, `macrolanguage`,
`lect`, `dialect`, `register`, `idiolect`, `historical_stage`,
`proto_language`, `reconstructed_variety`, `contact_variety`,
`mixed_language`, `pidgin`, `creole`, `signed_language`,
`constructed_language`, `ritual_or_restricted_register`,
`unknown_or_unresolved`.

Glottolog import populates the registry. Identity hypotheses (already
in 0061) carry the resolution semantics.

### 3.8 PRD §7.4: Linguistic entity

The PRD's entity classes (LanguageVariety, SourceAsset, Text, Utterance,
Sentence, Token, Lexeme, Form, Sense, Concept, Morpheme, Allomorph,
Paradigm, ParadigmCell, Construction, ClauseType, PhraseType, Phoneme,
Phone, Allophone, Grapheme, ScriptUsage, ValencyFrame, ArgumentRole,
DiscourseUnit, Speaker, AnnotationTier, SchemaPredicate, SchemaValue,
Policy, Release) become predicate-typed subjects in donto.

v1000 adds:

- `0074_linguistic_entity_seeds.sql` — registers the 30 entity classes
  in `donto_class_hierarchy` (uses the existing 0066 infrastructure).
- A predicate vocabulary (`donto-ling-vocab` crate, §3.18).

No new tables for the entity layer itself — they're subjects.

### 3.9 PRD §7.5: Claim atom

The PRD claim atom maps onto `donto_statement` plus overlays. Most
fields exist:

| PRD field                   | donto correspondence                                                              |
|-----------------------------|-----------------------------------------------------------------------------------|
| `claim_id`                  | `statement_id`.                                                                   |
| `subject_id`                | `subject`.                                                                         |
| `predicate_id`              | `predicate`.                                                                       |
| `object`                    | `object_iri` or `object_lit`.                                                     |
| `scope.language_variety_id` | **New: scope claim by variety_id.** Add column or store as predicate `ling:scope/variety`. |
| `scope.source_id`           | `donto_evidence_link → donto_document`.                                            |
| `scope.analysis_context_id` | `context`.                                                                         |
| `scope.hypothesis_id`       | Hypothesis-kind context (already supported).                                       |
| `scope.valid_time`          | `valid_time`.                                                                     |
| `scope.genre`               | **Add overlay table or use context env.**                                          |
| `scope.speaker_id`          | **Add overlay table** `donto_statement_speaker_scope`.                            |
| `polarity`                  | `flags.polarity` — but PRD's `polarity` adds `question` and `alternative`.         |
| `modality`                  | **New: not in donto.** Add `donto_statement_modality` overlay.                     |
| `evidence_anchor_ids`       | `donto_evidence_link.span_id`.                                                    |
| `quality.machine_confidence`| `donto_confidence`.                                                                |
| `quality.evidence_specificity` | **New: derive from evidence link.**                                            |
| `quality.review_state`      | **New: see §3.14.**                                                                |
| `quality.validation_state`  | **New: see §3.14.**                                                                |

Migrations for v1000:

- `0075_polarity_extended.sql`: extend polarity to include `question` and
  `alternative`. Bits available in `flags` (only 5 used; 11 reserved).
- `0076_statement_modality.sql`: `donto_statement_modality` overlay
  (statement_id FK, modality enum: descriptive, prescriptive,
  reconstructed, inferred, elicited, corpus_observed,
  typological_summary, …).
- `0077_review_state.sql`, `0078_validation_state.sql`: see §3.14.

### 3.10 PRD §7.6: Analysis frame

donto already has event frames (migration `0054_event_frames.sql`). The
PRD's frame_type vocabulary (phoneme_inventory, allophony_rule,
phonotactic_constraint, orthography_rule, morpheme_definition,
allomorphy_rule, paradigm_cell, syncretism_relation, derivational_process,
case_function, agreement_pattern, valency_frame, voice_alternation,
construction_template, igt_example, corpus_annotation, discourse_pattern,
schema_mapping, identity_hypothesis, access_policy_inheritance) becomes
seed entries in a new `donto_frame_type_registry` table.

Migration `0079_frame_type_registry.sql` adds the registry plus seeds
for the 20 PRD frame types.

### 3.11 PRD §8: Internal data schema (LinkML-expressible)

The PRD recommends LinkML as schema source of truth. donto's source of
truth is the SQL migrations themselves. Reconciliation:

- Keep SQL as the **engine source of truth**. The migrations define the
  Postgres schema and SQL functions. This doesn't change.
- Add a **LinkML schema** in `packages/donto-schema/` that declares the
  same structure for the API surface (request/response shapes,
  client-facing types, ingest schemas). Generate JSON Schema, JSON-LD
  context, and TypeScript / Python / Rust types from it.
- LinkML and SQL drift is tested in CI: a migration that adds a column
  without a LinkML update fails CI.

Migration `0080_schema_version.sql` records the LinkML schema version
that the running engine expects.

### 3.12 PRD §12: Schema alignment

The PRD's alignment relations are richer than donto's six (PRD: 11
relations; donto: 6). Mapping:

| PRD relation               | donto correspondence                                  |
|----------------------------|--------------------------------------------------------|
| `exact_match`              | `exact_equivalent` (rename: see §5).                  |
| `close_match`              | `close_match` ✓.                                       |
| `broad_match`              | **New** — add to enum.                                 |
| `narrow_match`             | `sub_property_of` (semantic match; rename TBD).        |
| `decomposes_to`            | `decomposition` (rename: see §5).                      |
| `has_value_mapping`        | **New** — add to enum + new table for value mappings.  |
| `inverse_of`               | `inverse_equivalent` (rename: see §5).                |
| `incompatible_with`        | `not_equivalent` (rename: see §5).                    |
| `derived_from`             | **New** — add to enum.                                 |
| `local_specialization`     | **New** — add to enum.                                 |

Migration `0081_alignment_relations_v2.sql` extends the
`donto_predicate_alignment.relation` enum and adds
`donto_alignment_value_mapping` table for `has_value_mapping` payloads.

PRD-required scope flags (`valid_for_query_expansion`,
`valid_for_export`, `valid_for_logical_inference`) are added as columns.

### 3.13 PRD §13: Language identity resolution

Builds on §3.7. Given the language-variety registry:

- Variety resolver endpoint: `POST /v1/varieties/resolve` (new in v1000)
  — takes `{label, hint_region, hint_period, hint_glottocode}`,
  returns ranked candidates with confidence and reason.
- Identity hypothesis already exists (`donto_identity_hypothesis` 0061).
- Provisional variety creation: `POST /v1/varieties` with
  `identity_status='provisional'`, `variety_type='unknown_or_unresolved'`.

No new schema beyond §3.7.

### 3.14 PRD §14: Review and validation

Two new state machines on top of statements:

- **Review state**: unreviewed → triaged → needs_source_check |
  needs_language_identity_review | needs_schema_alignment_review |
  needs_community_review → approved_for_internal_use |
  approved_for_public_release | rejected | superseded.
- **Validation state**: not_run → passed | warning | failed |
  blocked_by_policy | blocked_by_missing_evidence |
  blocked_by_unresolved_identity.

Both as overlay tables (one row per statement at most current state +
history table for audit).

Migrations:

- `0077_review_state.sql`: `donto_statement_review` + `donto_review_history`.
- `0078_validation_state.sql`: `donto_statement_validation` +
  `donto_validation_history`.
- `0082_review_actions.sql`: `donto_review_action` (review_id PK,
  target_type, target_id, action enum, reviewer_id FK donto_agent,
  comment, created_at).

PRD's required validation rules (every claim must have evidence anchor;
every anchor must resolve; predicate must exist; etc.) become Lean
shapes in `packages/lean/Donto/Shapes/V1000/`.

### 3.15 PRD §15: Governance and access model — **the load-bearing addition**

This is the biggest single piece of new schema. Three tables:

- `0083_access_policy.sql`:
  ```sql
  CREATE TABLE donto_access_policy (
      policy_id          uuid PRIMARY KEY,
      name               text NOT NULL UNIQUE,
      policy_type        text NOT NULL,    -- public, metadata_only, request, restricted, embargoed, community_review_required, ...
      authority_json     jsonb NOT NULL,   -- {type, label, contact_protocol}
      allowed_actions_json jsonb NOT NULL, -- {read_metadata, read_content, quote, export_claims, train_model, publish_release}
      inheritance_rule   text NOT NULL,    -- max_restriction, explicit_only, custom
      expiry             timestamptz,
      labels_json        jsonb,            -- TK / BC labels
      notes              text,
      created_at         timestamptz NOT NULL DEFAULT now()
  );
  ```
- `0084_access_assignment.sql`:
  ```sql
  CREATE TABLE donto_access_assignment (
      assignment_id      uuid PRIMARY KEY,
      target_kind        text NOT NULL,    -- document, context, statement, span, frame, release
      target_id          uuid NOT NULL,
      policy_id          uuid NOT NULL REFERENCES donto_access_policy(policy_id),
      assigned_by        text NOT NULL,
      assigned_at        timestamptz NOT NULL DEFAULT now(),
      valid_time         daterange NOT NULL DEFAULT daterange(NULL, NULL, '[)'),
      notes              text,
      UNIQUE (target_kind, target_id, policy_id)
  );
  ```
- `0085_access_attestation.sql`:
  ```sql
  CREATE TABLE donto_access_attestation (
      attestation_id     uuid PRIMARY KEY,
      caller             text NOT NULL,
      policy_id          uuid NOT NULL REFERENCES donto_access_policy(policy_id),
      granted_by         text NOT NULL,
      granted_at         timestamptz NOT NULL DEFAULT now(),
      expires_at         timestamptz,
      rationale          text NOT NULL,
      revoked_at         timestamptz
  );
  ```

Enforcement:

- **Sidecar middleware** in `apps/dontosrv/src/auth.rs` (new file)
  attaches a `RequireAccessPolicy` extractor to every read endpoint.
- **Query-evaluator extension** in
  `packages/donto-query/src/evaluator.rs` filters out statements whose
  context (or any ancestor) has an unsatisfied policy after binding but
  before result construction. Users see "this query has N restricted
  rows hidden" rather than partial data without notice.
- **Audit:** every restricted read produces a `donto_audit` row with
  `action='access_check'` recording caller, policy, attestation.

Defaults are fail-closed: any target with at least one assigned policy
is hidden unless the caller has explicit attestation.

This must land in the **first** v1000 milestone, before any restricted
material is ingested. See §12 M0.

### 3.16 PRD §16: Query requirements

donto's three query languages (DontoQL, SPARQL subset, direct SQL)
already cover the PRD's query modes. v1000 fixes the gaps:

- **PRESET resolution** — currently parsed but not evaluated. Implement
  in `evaluator.rs`. Six presets: latest, raw, curated,
  under_hypothesis, as_of, anywhere. Migration: none; pure code.
- **PRD's "release-view"** — added by §3.17 release builder.
- **PRD's "public-safe view"** — wired up by access policy enforcement
  (§3.15).
- **PRD's `claims_by_variety` / `feature_evidence` / `conflicts` /
  `release_candidate` query modes** — implemented as saved DontoQL
  templates in `packages/donto-ling-templates/` (sister crate).

Acceptance: PRD §16.3 claim card already exists at
`/claim/:id` (dontosrv) and `/claim/{statement_id}` (donto-api).

### 3.17 PRD §17: Import/export and §10/11 ingest/extraction

Ingest: §3.4 maps the new adapters. Existing 8-format pipeline
remains. New crates inherit from `Pipeline`.

Extraction: §8 below specifies the prompt-selector refactor.

Export: new release builder. Migration:

- `0086_dataset_release.sql`:
  ```sql
  CREATE TABLE donto_dataset_release (
      release_id         uuid PRIMARY KEY,
      name               text NOT NULL,
      scope_query_json   jsonb NOT NULL,   -- the DontoQL that selects content
      format             text NOT NULL,     -- native_jsonl, cldf, conllu, unimorph, ro_crate, rdf, …
      created_by         text NOT NULL,
      created_at         timestamptz NOT NULL DEFAULT now(),
      checksum_manifest_json jsonb NOT NULL,
      policy_report_json jsonb NOT NULL,    -- excluded/redacted summary
      storage_uri        text NOT NULL,
      is_immutable       bool NOT NULL DEFAULT false
  );
  ```

Endpoint: `POST /v1/releases` builds; `GET /v1/releases/{id}` retrieves;
`GET /v1/releases/{id}/manifest` returns the manifest JSON.

Round-trip invariant (PRD §17.5): tested in `release_round_trip.rs` —
import → native export → re-import is no-op. Test added in M9.

### 3.18 PRD §9: Feature and analysis domains, predicate registry

Sister crate `packages/donto-ling-vocab` registers ~120 starter
predicates with descriptors and embeddings via
`POST /descriptors/upsert`. Predicates cover: identity, documentation
metadata, writing systems, phonetics, phonology, prosody,
morphophonology, lexicon, lexical semantics, morphology, inflection,
derivation, word classes, nominal grammar, verbal grammar, agreement,
case and alignment, TAM, valency and voice, phrase structure, clause
structure, complex clauses, information structure, discourse,
sociolinguistic variation, diachrony, signed language structure,
multimodal gesture, corpus annotation, typological summary, contact and
borrowing, metalinguistic terminology.

PRD's predicate-minting rule (no exact match → require domain, range,
definition, examples → run nearest-neighbor → record alignment
candidates) implemented as a CLI guard:

```bash
donto predicates mint <iri> --label "..." --gloss "..." --domain "..." --range "..." --example "..."
# Refuses if no descriptor; runs /descriptors/nearest; warns if similarity > 0.85.
```

### 3.19 PRD §18: API requirements

§7 below specifies the unification.

### 3.20 PRD §19: UI requirements

§10 below specifies the TUI additions. Web UI is out of scope for
v1000.

### 3.21 PRD §20: Quality, scoring, maturity

donto's L0–L4 maps onto PRD M0–M5 with a name change and one new tier:

| donto      | PRD     | Meaning                                                               |
|------------|---------|------------------------------------------------------------------------|
| L0 raw     | M0_raw  | imported but not parsed.                                               |
| L1 parsed  | M1_parsed | structurally valid.                                                   |
| L2 linked  | M2_evidence_bound | claim has anchor.                                              |
| L3 reviewed| M3_reviewed | reviewer-approved for internal use.                                  |
| L4 certified | M4_release_authorized | safe for target release.                                  |
| —          | M5_reproducible | part of versioned release with manifest.                          |

v1000 adds M5 explicitly via the release builder. Maturity bits in
`flags` need 3 bits for 6 levels (still fits — currently 3 bits used).

PRD's required separate dimensions (machine_confidence, source_quality,
evidence_specificity, anchor_quality, schema_alignment_confidence,
language_identity_confidence, review_state, validation_state,
conflict_state, access_state, release_state) are all addressable
through existing overlay tables plus the new ones in §3.14, §3.15,
§3.17.

### 3.22 PRD §21: Security and privacy

Fits under §3.15 governance. New: `train_model` action explicitly listed
in `donto_access_policy.allowed_actions_json` so a permission to read or
annotate does not imply permission to use as training data.

Prompt and log redaction policy: when source material has
`policy.allowed_actions.train_model = false` and
`policy.allowed_actions.read_content = false`, the extraction worker
must use a local/offline model. Sister crate
`packages/donto-extract-local` (v1010) provides this; for v1000, the
worker refuses such jobs with a clear error.

### 3.23 PRD §22: Implementation architecture

donto's existing stack (Postgres 16, Rust + Axum, Python FastAPI for
extraction, Bubbles Tea TUI) matches the PRD's recommended stack. v1000
adds:

- **Object store** for source binaries: S3-compatible (MinIO for local
  dev, AWS S3 / R2 / etc. for production). New crate
  `packages/donto-blob` wraps signed-URL access.
- **Vector index for predicate descriptors** is already in
  `0033_vectors.sql` via pgvector.
- **OpenTelemetry** added to both services.

### 3.24 PRD §23: Build plan and §28 first sprint

PRD's M0–M10 milestones map onto §12 below.

PRD §28 Tasks A–G (repo skeleton, schema, migrations, policy gate,
claim validation, claim card, JSONL import/export):

- **Task A** — donto already has the repo skeleton. No-op.
- **Task B** — LinkML schema is new; goes in §3.11.
- **Task C** — migrations exist; v1000 adds 22 new ones.
- **Task D** — policy gate is the single biggest M0 deliverable.
- **Task E** — claim validation already exists; extends with the new
  validation rules.
- **Task F** — claim card already exists (migration 0034).
- **Task G** — native JSONL exists (`packages/donto-ingest/src/jsonl.rs`);
  release builder adds versioned export.

### 3.25 PRD §27: Non-negotiables

Cross-checked against donto's CLAUDE.md non-negotiables:

| PRD non-negotiable                                              | donto correspondence                                          |
|-----------------------------------------------------------------|---------------------------------------------------------------|
| 1. No claim without evidence                                    | Lean shape `EvidenceRequired` (new in v1000).                  |
| 2. No source without policy                                     | Sidecar middleware refuses `POST /v1/sources` without `default_policy_id`. |
| 3. No restricted content in public export                       | Release builder `policy_report_json` + Lean shape `PublicExportPolicySafe`. |
| 4. No schema mapping without relation type and justification    | Existing PAL requires `relation` enum + `confidence`; add `justification` field. |
| 5. No language identity merge without preserving prior hypotheses | Identity hypotheses (0061) already preserve.                |
| 6. No destructive deletes of claims; supersede instead          | Existing `donto_retract` / `donto_correct`.                    |
| 7. No LLM output stored as reviewed truth                       | Promotion gate: domain="linguistics" caps auto-maturity at L2.   |
| 8. No exact equivalence across schemas unless value spaces and granularity match | PAL relation enum + value_mapping (§3.12).         |
| 9. No speech-only assumptions                                   | Anchor kinds (§3.6) include media_time_span and elan_tier_annotation. |
| 10. No release without checksum manifest and policy report      | Release builder mandates both (§3.17).                        |

### 3.26 PRD §29: Research references

Adapter coverage for v1000:

| Reference                    | v1000 adapter                                                        |
|------------------------------|----------------------------------------------------------------------|
| Glottolog                    | M1 — language registry bootstrap.                                   |
| ISO 639-3 / BCP 47 / ISO 15924 | M1 — identifier ingestion.                                       |
| CLDF (incl. WALS, Grambank, AUTOTYP, PHOIBLE, ValPaL, APiCS, SAILS, Concepticon, CLICS, WOLD) | `donto-ling-cldf`. |
| UD                           | `donto-ling-ud`.                                                    |
| UniMorph                     | `donto-ling-unimorph`.                                              |
| LIFT                         | `donto-ling-lift`.                                                  |
| ELAN/EAF                     | `donto-ling-eaf`.                                                   |
| TEI / Praat / OLAC           | v1010.                                                              |
| OntoLex-Lemon / lexinfo / GOLD / OLiA | `donto-ling-vocab` predicates.                              |
| PROV-O                       | Already used implicitly in evidence layer; explicit JSON-LD context exported in releases. |
| SHACL / LinkML               | Generated from internal schema.                                     |
| RO-Crate / DataCite / Dublin Core | Release-builder export targets.                                |
| FAIR / CARE / AIATSIS / Local Contexts / OCAP / ELAR | Governance design (§3.15).                          |
| HamNoSys                     | Schema readiness; full integration v1010+.                         |

### 3.27 PRD §30: Final product definition

The ten success criteria all become test fixtures in `tests/v1000/`:

1. Source registered under access policy → `policy_required.rs`.
2. Source region → evidence anchor → `anchor_kinds.rs`.
3. Linguistic observation → claim or frame → `frame_decompose.rs`.
4. Claim traced back to evidence → `claim_card.rs`.
5. Provisional variety persists → `provisional_variety.rs`.
6. Schema preserved without collapse → `cldf_per_source_context.rs`.
7. Alignment without unsafe equivalence → `alignment_safety.rs`.
8. Contradictions preserved → `paraconsistency_v1000.rs`.
9. Release regenerable and audited → `release_reproducible.rs`.
10. No restricted leakage → `policy_zero_tolerance.rs`.

---

## 4. The gap inventory

Distinct items v1000 ships:

1. **22 new SQL migrations** (0068–0089), specified in §6.
2. **Unified API facade** (§7) — single service, no behavior change for
   callers using either old API.
3. **Domain-dispatched extraction prompt selector** (§8).
4. **Five new ingest crates** — CLDF, UD, UniMorph, LIFT, EAF (§3.4).
5. **Three linguistics sister crates** — `donto-ling-vocab`,
   `donto-ling-cldf`, `donto-ling-shapes`.
6. **Release builder** (`packages/donto-release/`).
7. **Access governance enforcement** in sidecar + query evaluator.
8. **PRESET resolution** in evaluator.
9. **Six new TUI tabs** (§10) — Language Profile, Source Workbench,
   Paradigm View, IGT View, Release Builder, Policy.
10. **CLI additions**: `lang resolve`, `lang register`, `policy
    register|grant|revoke`, `release build|inspect|reinhydrate`,
    `predicates mint`, `cldf import|export`.
11. **LinkML schema package** (`packages/donto-schema`).
12. **Reconstructed `PRD.md`** plus reconciled CLAUDE.md, README.md,
    ANTHROPOLOGY_README.md, ARCHITECTURE-REPORT.md,
    DONTO-RESEARCH-BRIEF.md.
13. **20+ new Lean shapes** in `packages/lean/Donto/Shapes/V1000/`.
14. **Test additions** (§14).

---

## 5. Naming reconciliation

donto's predicate-alignment relation enum and the PRD's relation enum
diverge in naming. v1000 renames donto's enum to match the PRD's, with
backwards-compatible aliases for one release.

| donto v0 (current)         | PRD / v1000              |
|----------------------------|--------------------------|
| `exact_equivalent`         | `exact_match`            |
| `inverse_equivalent`       | `inverse_of`             |
| `sub_property_of`          | `narrow_match`           |
| `close_match`              | `close_match` ✓          |
| `decomposition`            | `decomposes_to`          |
| `not_equivalent`           | `incompatible_with`      |
| —                          | `broad_match` (new)      |
| —                          | `has_value_mapping` (new)|
| —                          | `derived_from` (new)     |
| —                          | `local_specialization` (new)|

Migration `0081_alignment_relations_v2.sql` does the rename + adds the
new values + retains old names as aliases (lookup table). Code paths
that write the old names continue to work; reads return the new names.
v1100 removes the aliases.

Other names retained as-is because they are already domain-neutral:
`donto_statement`, `donto_context`, `donto_predicate`,
`donto_evidence_link`, `donto_argument`, `donto_proof_obligation`,
`donto_shape`, `donto_certificate`, `donto_identity_hypothesis`.

---

## 6. New schema migrations (0068 → ~0089)

Numbered, with one-line purpose. Each is idempotent (`if not exists`)
and adds an entry to `MIGRATIONS` in `donto-client/src/migrations.rs`.

```
0068 source_asset_extension          add source_type, storage_uri, checksum_sha256, language_candidates table
0069 anchor_kinds_registry           registered anchor-kind vocabulary with required-field schemas
0070 language_variety                donto_language_variety table
0071 language_identifier             donto_language_identifier table
0072 language_name                   donto_language_name table
0073 language_relations              parent / hasDialect / hasRegister predicate seeds
0074 linguistic_entity_seeds         30 entity classes registered in donto_class_hierarchy
0075 polarity_extended               polarity adds question + alternative
0076 statement_modality              donto_statement_modality overlay
0077 review_state                    donto_statement_review + donto_review_history
0078 validation_state                donto_statement_validation + donto_validation_history
0079 frame_type_registry             20 PRD frame types as seed entries
0080 schema_version                  LinkML schema version pinning
0081 alignment_relations_v2          rename to PRD names + add 4 new relations
0082 review_actions                  donto_review_action table + history
0083 access_policy                   donto_access_policy table
0084 access_assignment               donto_access_assignment table
0085 access_attestation              donto_access_attestation table
0086 dataset_release                 donto_dataset_release table
0087 maturity_extended               L5 (M5_reproducible) added
0088 prov_o_export                   PROV-O JSON-LD export view (read-only materialized)
0089 v1000_invariant_checks          shape registrations for v1000 non-negotiables
```

Two more migrations may emerge as M0–M3 lands; the schedule reserves
0090–0093.

Each migration ships:

1. The `.sql` file in `packages/sql/migrations/`.
2. An entry in `MIGRATIONS` in `donto-client/src/migrations.rs`.
3. A test in `packages/donto-client/tests/` exercising the new behaviour.
4. Where applicable, a Lean shape in `packages/lean/`.

---

## 7. API redesign — unifying dontosrv and donto-api

### 7.1 Goal

A single `/v1/...` API surface that exposes every capability of both
existing services with no overlap, no forwarding, and consistent error
contracts. Old endpoints continue to respond (alias router) for one
release; v1100 removes them.

### 7.2 Architecture

```
                    ┌────────────────────────────┐
                    │  Atlas Zero Unified API     │
                    │  (apps/atlas-api/)         │
                    │  Rust + Axum                │
                    └──────────────┬──────────────┘
                                   │
         ┌─────────────────────────┼────────────────────────────┐
         ▼                         ▼                            ▼
  ┌────────────┐         ┌────────────────┐          ┌─────────────────┐
  │ dontosrv   │         │ Temporal worker │          │ Storage / Queue │
  │ Rust core  │         │ Python (kept)   │          │ Postgres + S3   │
  │ unchanged  │         │ unchanged       │          │                 │
  └────────────┘         └────────────────┘          └─────────────────┘
```

`apps/atlas-api/` is a new Rust facade. It mounts dontosrv directly
(library import; no HTTP between them) and forwards extraction jobs to
Temporal via the existing Python worker.

### 7.3 v1 endpoint catalogue

Forty-six endpoints under `/v1/...`. Mapping shows old → new:

```
POST /v1/policies                       new (§3.15)
GET  /v1/policies/{id}                  new
POST /v1/policies/{id}/grant            new (attestation)
POST /v1/policies/{id}/revoke           new

POST /v1/sources                        was POST /documents/register (require default_policy_id now)
POST /v1/sources/{id}/revisions         was POST /documents/revision
GET  /v1/sources/{id}                   new
POST /v1/sources/{id}/derive            new (records OCR/ASR/parse activity)

POST /v1/anchors                        was POST /evidence/link/span (renamed and generalized)
GET  /v1/anchors/{id}                   new

POST /v1/varieties                      new (§3.7)
POST /v1/varieties/resolve              new (§3.13)
GET  /v1/varieties/{id}                 new
GET  /v1/varieties                      new (search)

POST /v1/predicates                     was POST /descriptors/upsert (renamed)
POST /v1/predicates/search              was POST /descriptors/nearest

POST /v1/claims                         was POST /assert
POST /v1/claims/batch                   was POST /assert/batch
POST /v1/claims/{id}/retract            was POST /retract
POST /v1/claims/{id}/correct            new (was inline only)
GET  /v1/claims/{id}                    was GET /statement/{id}
GET  /v1/claims/{id}/card               was GET /claim/{id}

POST /v1/frames                         new (event-frame creation)
GET  /v1/frames/{id}                    new

POST /v1/extraction/jobs                was POST /jobs/extract (with domain field, §8)
POST /v1/extraction/jobs/batch          was POST /jobs/batch
GET  /v1/extraction/jobs                was GET /jobs
GET  /v1/extraction/jobs/{id}           was GET /jobs/{id}
GET  /v1/extraction/jobs/{id}/facts     was GET /jobs/{id}/facts
GET  /v1/extraction/jobs/{id}/source    was GET /jobs/{id}/source
POST /v1/extraction/jobs/retry-failed   was POST /jobs/retry-failed

POST /v1/alignments                     was POST /alignment/register
POST /v1/alignments/rebuild             was POST /alignment/rebuild-closure
POST /v1/alignments/runs/start          was POST /alignment/runs/start
POST /v1/alignments/runs/complete       was POST /alignment/runs/complete
DELETE /v1/alignments/{id}              was POST /alignment/retract/{id}
GET  /v1/alignments/suggest             was GET /align/suggest/{predicate}

POST /v1/validation/run                 was POST /shapes/validate
GET  /v1/validation/results/{target_id} new (aggregates)

POST /v1/derivation/run                 was POST /rules/derive

POST /v1/certificates                   was POST /certificates/attach
POST /v1/certificates/{stmt}/verify     was POST /certificates/verify/{stmt}

POST /v1/arguments                      was POST /arguments/assert
GET  /v1/arguments/{stmt}               was GET /arguments/{stmt}
GET  /v1/arguments/frontier             was GET /arguments/frontier

POST /v1/obligations                    was POST /obligations/emit
POST /v1/obligations/{id}/resolve       was POST /obligations/resolve
GET  /v1/obligations/open               was POST /obligations/open
GET  /v1/obligations/summary            was GET /obligations/summary

POST /v1/reactions                      was POST /react
GET  /v1/reactions/{id}                 was GET /reactions/{id}

POST /v1/agents                         was POST /agents/register
POST /v1/agents/{id}/bind               was POST /agents/bind

POST /v1/reviews                        was POST /reviews (new)
GET  /v1/reviews/queue                  was — (new)
POST /v1/reviews/{id}                   action: approve|reject|edit|remap

POST /v1/query                          was POST /dontoql or POST /sparql (auto-detect)
POST /v1/sparql                         was POST /sparql (kept)
POST /v1/dontoql                        was POST /dontoql (kept)

POST /v1/releases                       new (§3.17)
GET  /v1/releases/{id}                  new
GET  /v1/releases/{id}/manifest         new

GET  /v1/audit                          new (read-only audit query)
GET  /v1/audit/firehose                 was GET /firehose/stream
GET  /v1/audit/recent                   was GET /firehose/recent
GET  /v1/audit/stats                    was GET /firehose/stats

POST /v1/graph/neighborhood             was POST /graph/neighborhood
POST /v1/graph/path                     was POST /graph/path
GET  /v1/graph/stats                    was GET /graph/stats
POST /v1/graph/subgraph                 was POST /graph/subgraph
GET  /v1/graph/entity-types             was GET /graph/entity-types
GET  /v1/graph/timeline/{subject}       was GET /graph/timeline/{subject}
POST /v1/graph/compare                  was POST /graph/compare
GET  /v1/graph/connections/{entity}     was GET /connections/{entity}
GET  /v1/graph/context-analytics/{ctx}  was GET /context/analytics/{context}

POST /v1/entity/symbols                 was POST /entity/register
POST /v1/entity/symbols/batch           was POST /entity/register/batch
POST /v1/entity/identity                was POST /entity/identity
POST /v1/entity/identity/batch          was POST /entity/identity/batch
POST /v1/entity/membership              was POST /entity/membership
GET  /v1/entity/{iri}/edges             was GET /entity/{iri}/edges
GET  /v1/entity/cluster/{h}/{r}         was GET /entity/cluster/{hypothesis}/{referent_id}
GET  /v1/entity/{iri}/resolve           was GET /entity/resolve/{iri}
GET  /v1/entity/family-table            was GET /entity/family-table

GET  /v1/health                         was GET /health
GET  /v1/version                        was GET /version
GET  /v1/docs                           was GET /full-docs (HTML)
GET  /v1/docs/simple                    was GET /simple-docs
```

Domain-specific endpoints (`/papers/*`) move to a domain plugin
namespace: `/v1/domains/papers/...`. Linguistics endpoints land at
`/v1/domains/linguistics/...`.

### 7.4 Error contract

Every error returns:

```json
{
  "error": "validation_failed",
  "target_id": "claim_...",
  "rule_id": "claim.requires_evidence_anchor",
  "severity": "blocker | warning | info",
  "message": "Claim has no evidence anchors.",
  "suggested_fix": {
    "field": "evidence_anchor_ids",
    "action": "provide_existing_anchor_or_create_new_anchor"
  },
  "request_id": "req_..."
}
```

PRD §18.4 spec.

### 7.5 Backwards compatibility

Old paths (e.g., `POST /assert`, `POST /jobs/extract`) continue to work
via an alias router that internally rewrites to v1. Deprecation warning
in response header `Deprecation: true; sunset="2026-12-01"`. v1100
removes aliases.

---

## 8. Extraction pipeline refactor

### 8.1 Domain dispatch

`apps/donto-api/main.py` and `helpers.py` add a `domain` field to
`ExtractIngestRequest`, `ExtractRequest`, `JobExtractRequest`,
`JobBatchRequest`. Default `general` (current behaviour preserved).

`helpers.py` factors:

```python
EXTRACTION_PROMPTS = {
    "general":     EXTRACTION_PROMPT_GENERAL,    # current 8-tier
    "genealogy":   EXTRACTION_PROMPT_GENEALOGY,  # genealogy-specific (extracted from current)
    "linguistics": EXTRACTION_PROMPT_LINGUISTICS,# new
    "papers":      EXTRACTION_PROMPT_PAPERS,     # extracted from /papers/ingest
}

def get_extraction_prompt(domain: str | None) -> str:
    return EXTRACTION_PROMPTS.get(domain or "general", EXTRACTION_PROMPT_GENERAL)
```

### 8.2 Domain-specific decomposers

```python
class FactDecomposer(Protocol):
    def fact_to_statement(self, fact: dict, context: str) -> dict | None: ...
    def fact_to_frame(self, fact: dict, context: str) -> tuple[str, list[dict]] | None: ...
    def enrich_statements(self, statements: list[dict]) -> list[dict]: ...

DECOMPOSERS = {
    "general":     GeneralDecomposer(),
    "genealogy":   GenealogyDecomposer(),
    "linguistics": LinguisticsDecomposer(),  # event frames for paradigms, IGT, etc.
    "papers":      PapersDecomposer(),
}
```

### 8.3 Confidence ceiling per domain

Extraction-time auto-promotion is capped per domain. Linguistics caps at
L2 (no auto-L3 from extraction); L3 requires reviewer approval; L4
requires Lean shape + reviewer; L5 requires release inclusion.

### 8.4 Extraction-level field

PRD §11.3 claim levels (`quoted`, `table_read`, `example_observed`,
`source_generalization`, `cross_source_inference`, `model_hypothesis`,
`human_hypothesis`) become a new field on the extracted-fact schema and
on the donto statement (overlay table
`donto_statement_extraction_level`, migration 0089 reserves the slot).

Prompt asks LLM to label each fact with its level. Level controls
auto-promotion ceiling (model_hypothesis never auto-promotes past L1).

### 8.5 Anti-hallucination contract

PRD §11.4: extractor outputs `cannot_determine` rather than guessing.
Validation layer checks this and refuses claims with
`cannot_determine` in any required field.

### 8.6 Span-aware extraction

LLM returns `span: [start, end]` per fact. Decomposer creates
`donto_span` with `kind=text_char_span` and `donto_evidence_link`
linking the resulting statement to the span. Achieves L2 anchoring at
extraction time.

### 8.7 Linguistics extraction prompt

Specified in `LANGUAGE-EXTRACTION-PLAN.md` Appendix B. Reproduced in
`apps/donto-api/prompts/linguistics.py` with fixture tests at
`apps/donto-api/tests/prompts/`.

---

## 9. Ingest, migrator, and CLI changes

### 9.1 New ingest crates

Each follows the existing `Pipeline` pattern.

| Crate                    | Format                         | Entry function                                              |
|--------------------------|--------------------------------|-------------------------------------------------------------|
| `donto-ling-cldf`        | CLDF (JSON-LD + CSV)            | `parse_cldf_dataset(dir, default_context)`                  |
| `donto-ling-ud`          | CoNLL-U                         | `parse_conllu_path(path, default_context)`                  |
| `donto-ling-unimorph`    | UniMorph TSV                    | `parse_unimorph_path(path, default_context)`                |
| `donto-ling-lift`        | LIFT XML                        | `parse_lift_path(path, default_context)`                    |
| `donto-ling-eaf`         | ELAN EAF                        | `parse_eaf_path(path, default_context, media_uri)`          |

### 9.2 New migrators

| Migrator           | Purpose                                                          |
|--------------------|------------------------------------------------------------------|
| `glottolog`        | Bootstrap language registry from Glottolog dump.                  |
| `iso639`           | Backfill ISO 639-3 identifiers.                                  |
| `cldf-bulk`        | Walk a directory of CLDF datasets, ingest each.                   |
| `local-genealogy`  | Existing — unchanged.                                             |

Invocation:

```bash
donto-migrate glottolog --dump glottolog-5.3.tar.gz --root ctx:source/glottolog/5.3
donto-migrate cldf-bulk --dir ./cldf-datasets/ --root ctx:source/comparative
```

### 9.3 New CLI subcommands

```bash
donto lang resolve <label> [--region X --period Y]    # resolves to candidates
donto lang register <label> --type provisional        # creates new variety
donto lang search <q>                                 # search registry

donto policy register --name X --type restricted --authority "..." --actions read,quote
donto policy grant <policy_id> --caller <agent_iri> --rationale "..."
donto policy revoke <attestation_id>

donto release build --scope <dontoql-file> --format cldf --out ./releases/2026-05/
donto release inspect <release_id>
donto release reinhydrate <release_id> --to <new_dsn>

donto predicates mint <iri> --label "..." --gloss "..." --domain "..." --range "..."
donto predicates verify <iri>      # runs descriptor / nearest / coverage check

donto cldf import <dir> --root ctx:source/<name>
donto cldf export --scope <iri> --out <dir>

donto ud import <conllu>
donto unimorph import <tsv>
donto lift import <xml>
donto eaf import <eaf> --media <uri>
```

### 9.4 Existing CLI subcommands — unchanged

`migrate`, `ingest`, `match`, `query`, `retract`, `extract`, `bench`,
`align`, `predicates`, `shadow`, `man`, `completions`. All keep working.

---

## 10. TUI and operational tooling

### 10.1 New TUI tabs

Six new tabs added to `apps/donto-tui/`:

| Tab #  | Name              | Replaces / adds                                                   |
|--------|-------------------|-------------------------------------------------------------------|
| 7      | Language Profile  | per-variety identity, identifiers, source coverage, feature coverage. |
| 8      | Source Workbench  | per-source metadata, anchors, derived files, OCR/ASR quality.      |
| 9      | Paradigm View     | pivot lexeme + features into 2-D paradigm table; gap highlights.    |
| 10     | IGT View          | morpheme-aligned IGT renderer with span click-through.              |
| 11     | Release Builder   | scope query → preview → build → manifest.                          |
| 12     | Policy            | policy registry, assignments, attestations, audit log.              |

Existing tabs 1–6 remain unchanged.

### 10.2 Justfile additions

```
just glottolog            # bootstrap language registry
just policy-init          # register default policies
just release <scope>      # build a release from a saved scope query
just lang-stats           # quick coverage report by language
just shapes-v1000         # run all v1000 shape validators
```

---

## 11. Documentation reconstruction (critical: rebuild PRD.md)

### 11.1 The PRD.md problem

CLAUDE.md references PRD §3 (principles), §2 (maturity ladder), §15 (Lean
engine integration), §18 (certificates), §19 (proof obligations), §25
(performance hypotheses). None of these exist anywhere in the repo
because PRD.md was deleted in commit 281a5bea (2026-04-28).

This is fixed in M−1 (the prerequisite milestone before M0).

### 11.2 Rebuild plan

`PRD.md` is reconstructed by **adopting the Atlas Zero PRD as the
canonical PRD with three modifications:**

1. Section 0 (executive summary) calls out that this PRD applies to
   donto v1000+. Earlier versions of donto are backwards-compatible
   substrates.
2. Section 22 (implementation architecture) names donto's actual stack
   instead of the PRD's recommended stack.
3. A new appendix maps old PRD section references (§3 principles, §2
   maturity ladder, etc.) onto the new PRD section numbers, so
   CLAUDE.md and other documents stay valid with at most a one-line
   "see Appendix Z" addition.

### 11.3 Updates to existing documents

- **CLAUDE.md**: update the `read PRD §3 / §2` references to match the
  new PRD section numbers. Add v1000 non-negotiables to the existing
  list (§13).
- **README.md**: At-a-glance counters update (89 SQL migrations, ~95
  HTTP endpoints, etc.). Add Atlas Zero positioning section.
- **CHANGELOG.md**: Add v1000 release notes once milestones land.
- **ANTHROPOLOGY_README.md**: keep as-is; it's the applied/practical
  framing and remains valid.
- **`docs/ARCHITECTURE-REPORT.md`**: mark sections that v1000 implements
  vs. those that remain v1100+.
- **`docs/DONTO-RESEARCH-BRIEF.md`**: update "what's missing" list as
  v1000 milestones land.
- **`docs/GENEALOGY-GUIDE.md`**: keep as-is; works under v1000.
- **`docs/LANGUAGE-EXTRACTION-PLAN.md`**: mark superseded by this
  document; preserve as historical artifact.

### 11.4 New documentation

- **`docs/V1000-REFACTOR-PLAN.md`** — this document.
- **`docs/ATLAS-ZERO-PRD.md`** — the canonical PRD (adapted from user's
  PRD).
- **`docs/GOVERNANCE.md`** — access policy details, CARE / AIATSIS /
  OCAP / ELDP framework alignment, attestation flow, audit semantics.
- **`docs/LINGUISTICS-GUIDE.md`** — practical tutorial for linguistic
  extraction (parallel to `GENEALOGY-GUIDE.md`).
- **`docs/RELEASES.md`** — release-builder semantics, manifest schema,
  reproducibility contract.
- **`docs/V1000-SHAPES.md`** — list of v1000 Lean shapes with one-line
  rationale each.

---

## 12. Milestone breakdown — M−1 through M10

Each milestone is a stop-and-review point. Code lands in a feature
branch; PR opens at milestone close; review verifies acceptance criteria
before merge.

### M−1: PRD reconstruction (prerequisite)

- Restore `docs/ATLAS-ZERO-PRD.md` as canonical PRD.
- Update CLAUDE.md cross-references.
- Update README.md positioning section.
- No code changes.

Acceptance: every `PRD §X` reference in the repo resolves to a real
section.

### M0: Governance bootstrap (before any restricted material)

- Migrations 0083, 0084, 0085 (access policy, assignment, attestation).
- Sidecar middleware enforcing read-side policy checks.
- Query-evaluator filter for restricted contexts.
- CLI: `donto policy register|grant|revoke`.
- TUI tab 12 (Policy).
- Documentation: `docs/GOVERNANCE.md`.
- Tests: `tests/v1000/policy_required.rs`, `policy_zero_tolerance.rs`.

Acceptance:
- No source can be registered without `default_policy_id`.
- Restricted source claims are hidden from un-attested callers.
- Audit log records every restricted-content access.

### M1: Language registry and identity

- Migrations 0070, 0071, 0072, 0073 (variety, identifier, name, relations).
- Glottolog migrator + ISO 639-3 backfill.
- Variety resolver endpoint (`POST /v1/varieties/resolve`).
- TUI tab 7 (Language Profile).
- Tests: `tests/v1000/provisional_variety.rs`, language identity
  hypothesis tests.

Acceptance:
- Known language resolves by glottocode/name/code.
- Unresolved label creates provisional variety.
- Competing identity hypotheses coexist.

### M2: Anchor kinds and source-asset extension

- Migrations 0068 (source asset extension), 0069 (anchor kinds registry).
- Anchor-kind shape validators (Rust) for all PRD-listed kinds.
- TUI tab 8 (Source Workbench).
- Tests: `tests/v1000/anchor_kinds.rs`.

Acceptance:
- Every PRD anchor kind round-trips.
- Anchor locator validates against source type.
- Source registration carries policy assignment.

### M3: Predicate vocabulary and alignment v2

- Migration 0081 (alignment relations v2).
- Sister crate `packages/donto-ling-vocab` with ~120 predicates.
- Migration 0079 (frame type registry) — 20 PRD frame types.
- `donto predicates mint` CLI guard.
- Tests: `tests/v1000/alignment_safety.rs`, predicate mint refusal tests.

Acceptance:
- New alignment relations work.
- Predicate descriptors with embeddings searchable.
- Mint refused without descriptor.

### M4: Claim model extensions

- Migrations 0075 (polarity extended), 0076 (modality), 0077 (review state),
  0078 (validation state), 0089 (extraction level).
- PRESET resolution implemented in `evaluator.rs`.
- Tests: paraconsistency under extended polarity, modality round-trip,
  review-state transitions.

Acceptance:
- Question and alternative polarities round-trip.
- Modality field stored and queryable.
- Review state transitions enforced.
- PRESET `latest`, `as_of`, `under_hypothesis`, etc. work in queries.

### M5: Structured importers

- Sister crates `donto-ling-cldf`, `donto-ling-ud`,
  `donto-ling-unimorph`, `donto-ling-lift`, `donto-ling-eaf`.
- CLI: `donto cldf import|export`, `donto ud import`, etc.
- Tests: round-trip for each format; idempotent re-import; per-source
  context isolation (no cross-dataset row collapse).

Acceptance:
- Mini-fixtures for each format ingest cleanly.
- Re-import is no-op.
- WALS / Grambank / PHOIBLE rows preserved per-source.

### M6: Extraction pipeline domain dispatch

- `apps/donto-api/` refactor: prompt selector, decomposer dispatch,
  domain field on requests/workflows/activities.
- Linguistics prompt and decomposer.
- Per-domain confidence ceilings.
- Span-aware extraction (LLM returns spans, decomposer creates anchors).
- Anti-hallucination `cannot_determine` validation.
- Tests: `tests/v1000/extraction_levels.rs`, span-anchor validation.

Acceptance:
- Domain field controls prompt and decomposer.
- Linguistics domain produces structured frames.
- Auto-promotion respects per-domain ceiling.
- `cannot_determine` outputs refused.

### M7: Alignment + entity resolution polish

- Schema alignment scope flags (`valid_for_query_expansion`,
  `valid_for_export`, `valid_for_logical_inference`).
- Identity hypothesis status enum extension (open / accepted / rejected
  / split / merged).
- Tests: alignment safety (token-level vs typological no exact_match);
  identity merge preserves hypotheses.

Acceptance:
- Alignment respects scope flags.
- Identity merges record both prior hypotheses.

### M8: Review UI (TUI tabs)

- Tabs 9 (Paradigm View), 10 (IGT View) added.
- Existing claim-card tab gets review action shortcuts.
- Documentation: `docs/LINGUISTICS-GUIDE.md`.

Acceptance:
- Paradigm view renders 2-D table.
- IGT view aligns morphemes to gloss; click-through to source span.
- Reviewer can approve/reject/edit/remap from claim card.

### M9: Release builder

- Migration 0086 (dataset_release).
- `packages/donto-release/` crate.
- CLI: `donto release build|inspect|reinhydrate`.
- TUI tab 11 (Release Builder).
- Format exporters: native JSONL, CLDF, CoNLL-U, UniMorph TSV, RO-Crate
  manifest.
- Tests: `release_reproducible.rs` (manifest-stable, see §15 caveat).

Acceptance:
- Public release excludes restricted claims.
- Manifest-stable: same content → same checksum.
- Policy report lists every exclusion.

### M10: Evaluation harness

- Gold fixture suite for synthetic linguistic source.
- Extractor precision / recall metrics.
- Anchor coverage metrics.
- Policy leakage tests (zero-tolerance gate in CI).
- Importer round-trip CI matrix.

Acceptance:
- CI runs fixture tests on every push.
- Benchmark report generated per commit.
- Zero policy leakage tolerated.

---

## 13. The v1000 non-negotiables

The original donto non-negotiables (CLAUDE.md) plus PRD §27 plus three
v1000-specific additions:

1. Paraconsistent — never reject contradictions. (donto)
2. Bitemporal — never delete; retract closes tx_time. (donto)
3. Every statement has a context. (donto)
4. Lean certifies, doesn't gate. (donto)
5. Postgres owns execution; Lean owns meaning. (donto)
6. No hidden ordering. (donto)
7. **No claim without evidence.** (PRD §27.1)
8. **No source without policy.** (PRD §27.2)
9. **No restricted content in public export.** (PRD §27.3)
10. **No schema mapping without relation type and justification.** (PRD §27.4)
11. **No language identity merge without preserving prior hypotheses.** (PRD §27.5)
12. **No destructive deletes; supersede instead.** (PRD §27.6 — donto already.)
13. **No LLM output stored as reviewed truth.** (PRD §27.7)
14. **No exact equivalence across schemas unless value spaces and granularity match.** (PRD §27.8)
15. **No speech-only assumptions.** (PRD §27.9)
16. **No release without checksum manifest and policy report.** (PRD §27.10)
17. **(v1000 new)** Domain-specific extraction must declare its
    confidence ceiling.
18. **(v1000 new)** Every PRD section reference in code or docs must
    resolve to a real section in the canonical PRD.
19. **(v1000 new)** Schema (SQL) and LinkML schema must agree; CI
    enforces.

---

## 14. Test strategy

### 14.1 Existing tests (preserve)

All five suites in `packages/donto-client/tests/` continue to pass
unchanged:

- `invariants_paraconsistency.rs`
- `invariants_bitemporal.rs`
- `invariants_migration_idempotent.rs`
- `scope.rs`
- `assert_match.rs`

### 14.2 New v1000 test suites

In `packages/donto-client/tests/v1000/`:

| File                                | Tests                                                  |
|-------------------------------------|--------------------------------------------------------|
| `policy_required.rs`                | source registration without policy fails               |
| `policy_zero_tolerance.rs`          | restricted statement never appears in public export    |
| `policy_inheritance.rs`             | derived data inherits max restriction                  |
| `provisional_variety.rs`            | unresolved label creates provisional variety            |
| `language_resolver.rs`              | resolver returns ranked candidates with reason         |
| `anchor_kinds.rs`                   | every PRD anchor kind round-trips                      |
| `anchor_validator.rs`               | invalid locator refused                                |
| `polarity_extended.rs`              | question and alternative polarities preserved          |
| `modality_overlay.rs`               | modality field round-trips                             |
| `review_state_transitions.rs`       | review state machine enforced                          |
| `validation_state_blockers.rs`      | blocked_by_policy state correct                        |
| `extraction_levels.rs`              | model_hypothesis never auto-promotes past L1           |
| `cannot_determine_refused.rs`       | claim with cannot_determine field refused              |
| `span_anchor_creation.rs`           | LLM-returned spans become anchors                      |
| `cldf_per_source_context.rs`        | two CLDF datasets with same feature don't collapse     |
| `alignment_safety.rs`               | token-level vs typological cannot exact_match          |
| `paraconsistency_v1000.rs`          | extended polarity preserves paraconsistency            |
| `frame_type_registry.rs`            | 20 PRD frame types seed correctly                      |
| `release_reproducible.rs`           | manifest-stable; same content → same checksum          |
| `release_round_trip.rs`             | import → export → re-import is no-op                   |
| `claim_card.rs`                     | full claim card with policy + review + validation      |
| `linkml_schema_drift.rs`            | adding column without LinkML update fails CI            |

### 14.3 Lean shapes

In `packages/lean/Donto/Shapes/V1000/`:

```
EvidenceRequired.lean          every claim has at least one evidence_link
PolicyAssigned.lean            every source has default_policy_id
PublicExportPolicySafe.lean    release with public scope contains no restricted rows
SchemaMappingJustified.lean    every alignment has relation + justification
IdentityMergePreservesHistory.lean  identity edges record all prior hypotheses
NoDeleteOnly.lean              no DELETE on donto_statement
LLMOutputReviewGate.lean       maturity ≥ L3 implies non-extraction provenance
SchemaValueSpacesMatch.lean    exact_match implies aligned value spaces
AnchorKindLocatorValid.lean    locator schema matches anchor_kind
ReleaseManifestComplete.lean   release has checksum + policy_report
PRDSectionResolves.lean        text mentioning "PRD §X" references real section
SQLSchemaLinkMLAgrees.lean     migration columns appear in LinkML
DomainCeilingDeclared.lean     extraction job has explicit ceiling
IGTAlignment.lean              count(morphemes(vernacular)) == count(morphemes(gloss))
ParadigmCompleteness.lean      every required cell filled or marked defective
AllomorphCondition.lean        every attestation satisfies allomorph environment
LanguageVarietyScoped.lean     every L2+ ling claim has variety_id
DialectScopeRequired.lean      claim under variety with dialects requires dialect scope
ProvisionalVarietyMarked.lean  provisional variety has identity_status set
ExtractionLevelHonest.lean     model_hypothesis cannot have promoted maturity
```

### 14.4 CI matrix

- Rust workspace tests (existing).
- Lean theorems (existing 62 + new 20 v1000 shapes).
- Importer round-trip matrix (new): native ↔ CLDF, native ↔ CoNLL-U,
  native ↔ UniMorph, native ↔ LIFT, native ↔ EAF.
- LinkML / SQL drift check.
- Policy zero-tolerance gate.
- TUI smoke test (existing).

---

## 15. Performance and scale

### 15.1 What we know

donto's CLAUDE.md says perf is "kept in mind, not optimized for". The
research brief reports 35.8M statements at 27 GB on a single Postgres
node. No published benchmarks.

### 15.2 What v1000 commits to

**Targets, not promises.** Single-node Postgres 16, 16 cores, 64 GB RAM:

| Operation                               | Target         | Notes                                                |
|-----------------------------------------|----------------|------------------------------------------------------|
| Batch insert via `assert_batch`         | 5k claims/sec  | Achievable only with content-hash contention managed; if at-write validation enforced (PRD §11.1), realistic floor is ~500/sec. v1000 picks **at-write validation**, accepts 500/sec. |
| Claim card lookup                       | p95 < 250 ms   | Already achieved at current scale; tested at 100M-row baseline. |
| Predicate search (FTS + vector)         | p95 < 300 ms   | Already achieved.                                     |
| Review queue query                      | p95 < 500 ms   | New; tested in M8.                                   |
| Public-safe export of 100K claims       | < 5 minutes    | Tested in M9.                                        |
| Importer re-run                         | no-op detect   | Existing content-hash dedup.                         |

### 15.3 What v1000 explicitly does not commit to

- **Byte-for-byte release reproducibility.** Postgres ordering and JSON
  serialization are not byte-stable across library upgrades. v1000
  ships **manifest-stable** (same content → same content hash), not
  byte-identical bytes. PRD §24.1 line is amended.
- **Sub-millisecond point queries** at 1B rows. Out of scope for v1000;
  requires partitioning and possibly a query planner. v1100.
- **100K inserts/sec.** Requires bypassing per-row triggers; v1000
  preserves at-write enforcement.

### 15.4 Benchmarking

- `donto bench` already exists. Extend with v1000 operations: policy-
  gated read, claim-card-with-policy, release build, CLDF import, IGT
  validation.
- CI runs benchmark per commit; flagged regressions block merge.

---

## 16. Risk register

Beyond the PRD's R1–R10:

**R11 — PRD reconstruction drift.**
Risk: rebuilt PRD differs subtly from the deleted original; existing
references in CLAUDE.md no longer mean what they used to.
Mitigation: M−1 explicitly enumerates every existing reference; mapping
appendix preserves them.

**R12 — Backwards-incompatible alignment relation rename.**
Risk: code outside this repo already uses `exact_equivalent`,
`inverse_equivalent`, `sub_property_of`, `decomposition`, `not_equivalent`.
Mitigation: alias lookup persists for one full release; deprecation
warning in API responses; v1100 removes after 90 days.

**R13 — API unification breaks external clients.**
Risk: `donto-api` and `dontosrv` are both used by external clients;
unification under `/v1/...` requires migration.
Mitigation: alias router in v1000 preserves all old paths; clients
migrate at their own pace; sunset header gives 6 months.

**R14 — PRESET semantics ambiguity.**
Risk: presets defined in `0005_presets.sql` but never evaluated; their
intended semantics are now stale.
Mitigation: M4 re-derives semantics from CHANGELOG and ADR; tests
codify behaviour; PRD documents.

**R15 — Schema enum extension breaks existing reads.**
Risk: extending polarity / alignment relation enums adds values that
clients can't decode.
Mitigation: clients use string deserialization with unknown-fallback;
test added; documentation updated.

**R16 — Documentation drift continues.**
Risk: even with v1000 reconstruction, docs drift again as code changes.
Mitigation: PRDSectionResolves Lean shape; CI grep for `PRD §` patterns;
documented update workflow.

**R17 — Policy enforcement performance impact.**
Risk: row-level policy filtering adds query latency.
Mitigation: policy assignments indexed; bloom filter per context;
benchmark in M0; optimize before M9.

**R18 — Linguistics overcommits the engine.**
Risk: predicate vocabulary and frame types pull donto into a linguistic-
shaped engine, hurting genealogy / scientific paper / other domains.
Mitigation: linguistics lives in sister crates; engine remains domain-
agnostic; predicate registry per domain; the 20 PRD frame types live in
`donto-ling-vocab`, not core.

---

## 17. Open questions for the principal engineer

Before M0 lands, decisions needed:

1. **Sister crate boundaries.** Are `donto-ling-vocab`, `donto-ling-cldf`,
   `donto-ling-shapes`, `donto-ling-export` in the same monorepo (current
   recommendation) or separate repositories?
2. **Object store choice.** S3 (AWS), R2 (Cloudflare), MinIO (self-host)?
   Default for dev: MinIO. Default for production: undecided.
3. **LinkML adoption.** Confirm LinkML as the API-surface schema source
   of truth (with SQL as engine source of truth).
4. **Domain dispatcher location.** Should domain dispatch live in
   `donto-api` (current recommendation, Python) or move to a new Rust
   `extract-orchestrator` service? v1000 keeps it in Python.
5. **Backwards-compat window.** PRD says alias router for "one release".
   Define "release" — is it v1000.x → v1100, or v1000 → v2000? Default:
   v1000 → v1100, with v1100 release planned ~6 months out.
6. **Identity hypothesis names.** The three names (strict_identity_v1,
   likely_identity_v1, exploratory_identity_v1) come from
   ARCHITECTURE-REPORT.md. Keep or rename?
7. **TUI vs web UI.** PRD §19 specifies a substantial UI. v1000 ships
   TUI tabs only. Web UI deferred to v1100. Confirm acceptable.
8. **Release builder format priority.** v1000 ships native JSONL + CLDF
   + CoNLL-U + UniMorph + RO-Crate. TEI / LIFT / EAF deferred to v1010.
   Confirm.
9. **Lean shape verification cost.** Adding 20 v1000 shapes increases
   theorem count from 62 to 82+. Lean compile time: confirm acceptable.
10. **Genealogy-specific code.** `apps/donto-api/main.py` has `/papers/*`
    endpoints with hardcoded genealogy behaviour. v1000 moves under
    `/v1/domains/papers/...`. Is `papers` actually genealogy-specific, or
    is it general scientific-paper extraction? Confirm.

---

## 18. Appendices

### Appendix A — File-and-line index of every reference made above

For traceability. Every claim about donto's current state cites a path
and line.

```
apps/dontosrv/src/lib.rs:41                router(state) — 44 endpoints
apps/donto-api/main.py:178                 GET /firehose/stream
apps/donto-api/main.py:228                 GET /firehose/recent
apps/donto-api/main.py:263                 GET /firehose/stats
apps/donto-api/main.py:309                 GET /health
apps/donto-api/main.py:319                 GET /version
apps/donto-api/main.py:334                 POST /extract-and-ingest
apps/donto-api/main.py:433                 POST /jobs/extract
apps/donto-api/main.py:459                 POST /jobs/batch
apps/donto-api/main.py:488                 GET /jobs
apps/donto-api/main.py:551                 GET /jobs/{job_id}
apps/donto-api/main.py:581                 POST /jobs/retry-failed
apps/donto-api/main.py:624                 GET /jobs/{job_id}/facts
apps/donto-api/main.py:703                 GET /jobs/{job_id}/source
apps/donto-api/main.py:740                 GET /queue
apps/donto-api/main.py:1001                POST /extract
apps/donto-api/main.py:1069                POST /assert
apps/donto-api/main.py:1094                POST /assert/batch
apps/donto-api/main.py:1110                GET /subjects
apps/donto-api/main.py:1122                GET /search
apps/donto-api/main.py:1150                GET /history/{subject:path}
apps/donto-api/main.py:1170                GET /statement/{id}
apps/donto-api/main.py:1183                GET /contexts
apps/donto-api/main.py:1201                GET /predicates
apps/donto-api/main.py:1224                POST /query
apps/donto-api/main.py:1254                POST /retract/{statement_id}
apps/donto-api/main.py:1278                GET /connections/{entity:path}
apps/donto-api/main.py:1354                GET /context/analytics/{context:path}
apps/donto-api/main.py:1444                POST /graph/neighborhood
apps/donto-api/main.py:1585                POST /graph/path
apps/donto-api/main.py:1661                GET /graph/stats
apps/donto-api/main.py:1723                POST /graph/subgraph
apps/donto-api/main.py:1781                GET /graph/entity-types
apps/donto-api/main.py:1809                GET /graph/timeline/{subject:path}
apps/donto-api/main.py:1872                POST /graph/compare
apps/donto-api/main.py:1944                POST /align/register
apps/donto-api/main.py:1968                POST /align/rebuild
apps/donto-api/main.py:1982                POST /align/retract/{alignment_id}
apps/donto-api/main.py:1992                GET /align/suggest/{predicate}
apps/donto-api/main.py:2019                GET /evidence/{statement_id}
apps/donto-api/main.py:2036                GET /claim/{statement_id}
apps/donto-api/main.py:2066                POST /entity/register
apps/donto-api/main.py:2084                POST /entity/register/batch
apps/donto-api/main.py:2110                POST /entity/identity
apps/donto-api/main.py:2138                POST /entity/identity/batch
apps/donto-api/main.py:2168                POST /entity/membership
apps/donto-api/main.py:2194                GET /entity/{iri:path}/edges
apps/donto-api/main.py:2215                GET /entity/cluster/{hypothesis}/{referent_id}
apps/donto-api/main.py:2235                GET /entity/resolve/{iri:path}
apps/donto-api/main.py:2261                GET /entity/family-table
apps/donto-api/main.py:2340                POST /papers/ingest
apps/donto-api/main.py:2515                GET /papers/{paper_id}
apps/donto-api/main.py:2586                GET /papers/{paper_id}/claims
apps/donto-api/main.py:3225                GET /full-docs
apps/donto-api/main.py:3497                GET /simple-docs
apps/donto-api/main.py:3503                GET /guide

apps/donto-api/helpers.py:39               confidence_to_maturity()
apps/donto-api/helpers.py:47               parse_fact_object()
apps/donto-api/helpers.py:64               EXTRACTION_PROMPT
apps/donto-api/helpers.py:166              clean_web_content()
apps/donto-api/helpers.py:354              ingest_facts() decomposer

apps/donto-api/workflows.py:14             ExtractionWorkflow
apps/donto-api/workflows.py:33             ExtractionWorkflow.run()

apps/donto-api/activities.py:16            extract_facts_activity
apps/donto-api/activities.py:47            align_predicates_activity
apps/donto-api/activities.py:107           resolve_entities_activity

apps/donto-cli/src/main.rs:56              Cmd enum subcommands
apps/donto-cli/src/extract.rs:1            LLM extraction module

packages/donto-query/src/dontoql.rs:1      DontoQL parser
packages/donto-query/src/sparql.rs:1       SPARQL subset
packages/donto-query/src/algebra.rs:1      Query, Pattern, Filter, IdentityMode, PredicateExpansion
packages/donto-query/src/evaluator.rs:9    nested-loop evaluator

packages/donto-ingest/src/csv.rs:1         CSV
packages/donto-ingest/src/jsonl.rs:1       JSONL
packages/donto-ingest/src/jsonld.rs:1      JSON-LD
packages/donto-ingest/src/nquads.rs:1      N-Quads
packages/donto-ingest/src/turtle.rs:1      Turtle, TriG
packages/donto-ingest/src/rdfxml.rs:1      RDF/XML
packages/donto-ingest/src/property_graph.rs:1  property graph
packages/donto-ingest/src/pipeline.rs:1    shared batcher
packages/donto-ingest/src/quarantine.rs:1  quarantine routing

packages/donto-migrate/src/main.rs:1       migrator dispatcher
packages/donto-migrate/src/genealogy.rs:1  genealogy SQLite migrator
packages/donto-migrate/src/relink.rs:1     additive relink

packages/sql/migrations/0001_core.sql      donto_statement et al
packages/sql/migrations/0067_rule_engine.sql  current head

packages/donto-client/tests/invariants_paraconsistency.rs
packages/donto-client/tests/invariants_bitemporal.rs
packages/donto-client/tests/invariants_migration_idempotent.rs
packages/donto-client/tests/scope.rs
packages/donto-client/tests/assert_match.rs

apps/donto-tui/main.go:1                   TUI entry
apps/donto-tui/internal/app/app.go:16      tab constants
```

### Appendix B — Migration index after v1000 lands

```
0001 core                              0046 references
0002 flags                              0047 claim_lifecycle
0003 functions                          0048 predicate_alignment
0004 migrations                         0049 predicate_descriptor
0005 presets                            0050 alignment_run
0006 predicate                          0051 predicate_closure
0007 snapshot                           0052 match_aligned
0008 shape                              0053 canonical_shadow
0009 rule                               0054 event_frames
0010 certificate                        0055 match_alignment_integration
0011 observability                      0056 lexical_normalizer
0012 match_scope_fix                    0057 entity_symbol
0013 search_trgm                        0058 entity_mention
0014 retrofit                           0059 entity_signature
0015 shape_annotations                  0060 identity_edge
0016 valid_time_buckets                 0061 identity_hypothesis
0017 reactions                          0062 literal_canonical
0018 aggregates                         0063 time_expression
0019 fts                                0064 temporal_relation
0020 bitemporal_canonicals              0065 property_constraint
0021 same_meaning                       0066 class_hierarchy
0022 context_env                        0067 rule_engine
0023 documents                          0068 source_asset_extension       NEW (v1000)
0024 document_revisions                 0069 anchor_kinds_registry        NEW
0025 spans                              0070 language_variety             NEW
0026 annotations                        0071 language_identifier          NEW
0027 annotation_edges                   0072 language_name                NEW
0028 extraction_runs                    0073 language_relations           NEW
0029 evidence_links                     0074 linguistic_entity_seeds      NEW
0030 agents                             0075 polarity_extended            NEW
0031 arguments                          0076 statement_modality           NEW
0032 proof_obligations                  0077 review_state                 NEW
0033 vectors                            0078 validation_state             NEW
0034 claim_card                         0079 frame_type_registry          NEW
0035 document_sections                  0080 schema_version               NEW
0036 mentions                           0081 alignment_relations_v2       NEW
0037 extraction_chunks                  0082 review_actions               NEW
0038 confidence                         0083 access_policy                NEW
0039 units                              0084 access_assignment            NEW
0040 temporal_expressions               0085 access_attestation           NEW
0041 content_regions                    0086 dataset_release              NEW
0042 entity_aliases                     0087 maturity_extended            NEW
0043 candidate_contexts                 0088 prov_o_export                NEW
0044 ontology_seeds                     0089 v1000_invariant_checks       NEW
0045 auto_shape_validation              0090–0093 reserved for M0–M3 hotfix
```

### Appendix C — v1 endpoint catalogue summary

Forty-six new endpoints in v1 namespace; ~94 old endpoints kept as
aliases until v1100. Full catalogue in §7.3.

### Appendix D — New Lean shapes

Twenty new shapes in `packages/lean/Donto/Shapes/V1000/`. Full list in
§14.3.

### Appendix E — Glossary additions

| Term                             | Meaning                                                                                |
|----------------------------------|---------------------------------------------------------------------------------------|
| Atlas Zero                       | Linguistic application built on donto v1000+.                                          |
| v1000                            | Major version where donto becomes Atlas Zero substrate without losing generality.      |
| domain dispatch                  | Extraction prompt + decomposer selected by request domain field.                       |
| extraction level                 | PRD §11.3 epistemic-act tag (quoted/table_read/example_observed/source_generalization/cross_source_inference/model_hypothesis/human_hypothesis). |
| modality                         | PRD §7.5 epistemic stance (descriptive/prescriptive/reconstructed/inferred/elicited/corpus_observed/typological_summary). |
| review state                     | Multi-step approval state machine separate from machine confidence.                    |
| validation state                 | Pass/warning/failure plus blocked-by reasons.                                          |
| access policy                    | Governance object controlling read/quote/export/train_model/publish.                  |
| attestation                      | Caller's authorization to satisfy a policy.                                            |
| release                          | Versioned, manifest-stable view over content + policy + review state + schema.        |
| manifest-stable                  | Same content → same checksum (not byte-identical bytes).                              |
| anchor kind                      | Typed evidence locator (text_char_span / pdf_page_bbox / media_time_span / ...).      |
| variety type                     | Open-world language identity category (language / dialect / signed_language / unknown_or_unresolved / ...). |
| identity status                  | Resolution state of language identity (confirmed / provisional / disputed / split_candidate / merge_candidate). |
| LinkML schema                    | API-surface schema source of truth (compiles to JSON Schema, JSON-LD, types).          |
| sister crate                     | Domain-specific package outside the engine core (donto-ling-*, donto-medical-*, etc.). |

### Appendix F — Cross-references between this document and others

This document supersedes:

- `docs/LANGUAGE-EXTRACTION-PLAN.md` (preserved as historical artifact).

This document references but does not modify:

- `README.md`, `CLAUDE.md`, `CHANGELOG.md`, `ANTHROPOLOGY_README.md`,
  `docs/ARCHITECTURE-REPORT.md`, `docs/DONTO-RESEARCH-BRIEF.md`,
  `docs/GENEALOGY-GUIDE.md`, `Justfile`, `PRD.md` (deleted; rebuilt in
  M−1).

This document depends on:

- The Atlas Zero PRD (v0.1, dated 2026-05-06) provided in conversation.

This document is implemented by:

- M−1 through M10 work as specified in §12.

---

*End of plan. Stop here for review before any code lands.*
