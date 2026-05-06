# donto v1000 — Canonical PRD

**Document type:** Product requirements document, research architecture,
and agentic-coder build brief.
**Codename:** donto.
**Version:** PRD-V1000-001.
**Date:** 2026-05-07.
**Status:** Canonical. Supersedes
[`ATLAS-ZERO-FRONTIER.md`](ATLAS-ZERO-FRONTIER.md),
[`V1000-REFACTOR-PLAN.md`](V1000-REFACTOR-PLAN.md), and
[`LANGUAGE-EXTRACTION-PLAN.md`](LANGUAGE-EXTRACTION-PLAN.md) as the
single source of truth for the v1000 product. Those documents remain
on disk as historical artefacts.
**Authoring stance:** From-scratch product model. Existing standards
and formats — CLDF, RDF, SPARQL, CoNLL-U, UniMorph, TEI, LIFT, ELAN/EAF,
SHACL, PROV-O, RO-Crate, JSON-LD — are adapters, references, and
interoperability targets. They are not the native architecture.

---

## How to read this document

This is the **canonical PRD** for donto v1000. It states the product in
its own terms, then maps every requirement to the current donto
codebase with file/migration/line-number citations. Each functional
area is structured:

- **Requirement** — what v1000 must do.
- **What donto has today** — current state with citations.
- **Delta** — what changes the codebase.
- **Verdict** — `no-op` / `extend` / `build`.

This is the document agentic coders work from. The document is long;
the mapping is the work product.

---

## 0. Executive decision

donto is built as an **evidence operating system for contested
knowledge**. The product is not a linguistic database, not a knowledge
graph, not a wiki, not an archive, not a conventional ontology
platform, and not an LLM extraction app. Those are partial ancestors.
donto is the substrate that lets multiple sources, schemas,
communities, models, experts, and institutions make claims about the
same world without forcing premature consensus.

The first proving domain is **language evidence**, because language
documentation stress-tests every hard problem at once: incompatible
schemas, incomplete sources, disputed identities, multimodal evidence,
restricted cultural material, diachronic change, grammatical analysis,
formal validation, community authority, and corpus-scale annotation.
The product boundary is broader: donto must also host evidence from
medicine, law, cultural heritage, historical research, intelligence
analysis, scientific review, and future domains whose schemas do not
yet exist.

The internal model is **open-world, evidence-first,
contradiction-preserving, governance-native, bitemporal, multimodal,
and schema-plural**.

---

## 1. Product thesis

Most research systems silently assume one of:

1. There is one correct value to store.
2. There is one canonical schema to map into.
3. There is one entity-resolution answer.
4. A user either has access or does not.
5. Provenance is metadata rather than the central object.
6. Machine confidence can stand in for scholarly review.
7. Exports are static files rather than reproducible views.
8. Contradictions are quality failures instead of data.

donto rejects all eight assumptions. donto stores **evidence-backed
claims under contexts**. A claim may be asserted, denied, uncertain,
reconstructed, inferred, elicited, machine-extracted, human-reviewed,
or formally validated. Claims may contradict one another. Contradiction
is a navigable research state, not an error.

The core product question:

> Given a contested question, can donto return the relevant claims,
> the evidence behind them, the schema mappings that make them
> comparable, the identity hypotheses they depend on, the
> disagreements between them, the access policies governing them, and
> a reproducible release artefact?

If yes, the product is working.

---

## 2. Non-negotiable invariants

These are product requirements, not implementation preferences.

### I1. No claim without evidence or explicit hypothesis status

A claim must have at least one evidence anchor unless explicitly marked
`hypothesis_only`. Hypothesis-only claims are never eligible for
public release, high maturity, or downstream training without review.

**donto today:** Most claims have evidence via `donto_evidence_link`
(migration `0029_evidence_links.sql`), but the substrate does not
*enforce* this — a `donto_assert` call without an evidence link
succeeds. There is no `hypothesis_only` flag.
**Delta:** New column `donto_statement.hypothesis_only bool` (migration
`0089_hypothesis_only_flag.sql`). New write-path validator
(`packages/donto-client/src/validate.rs`) refuses to assert at maturity
≥ E2 without an evidence link unless `hypothesis_only=true`.
**Verdict:** Build (one column, one validator).

### I2. No restricted source without policy

A source cannot be ingested until its access policy is explicitly
classified. Unknown policy defaults to `restricted_pending_review`,
not public.

**donto today:** `donto_document` (migration `0023_documents.sql`)
has no policy field. Access policy as a concept does not yet exist.
**Delta:** New tables for policy capsule, attestation, audit (see §6.12,
§6.13). Sidecar middleware enforces policy presence on
`POST /sources`.
**Verdict:** Build (the largest single feature in v1000; M0 deliverable).

### I3. No destructive overwrite

Corrections, retractions, merges, splits, alignments, and policy
changes are append-only events. The system supports transaction-time
reconstruction of what was believed and visible at any prior system
time.

**donto today:** Already true for statements. `donto_retract` closes
`tx_time`; `donto_correct` creates a new statement linked to the old.
Tested in `invariants_bitemporal.rs`. CLAUDE.md non-negotiable.
**Delta:** Extend the same discipline to alignments, identity
hypotheses, policies, attestations, reviews. New event log table
`donto_event_log` (migration `0090_event_log.sql`) replaces direct
mutation on these tables; each row written via append-only event.
**Verdict:** Extend.

### I4. Contradictions are preserved

The system stores mutually incompatible claims without forcing
resolution. Contradictions produce argument edges and review
obligations, not failed writes.

**donto today:** Built in. `donto_argument` (migration
`0031_arguments.sql`) records `supports`/`rebuts`/`undercuts`/`qualifies`
edges; `donto_proof_obligation` (migration `0032_proof_obligations.sql`)
tracks open work. Tested in `invariants_paraconsistency.rs`.
**Delta:** Extend `donto_argument.relation` to include
`alternative_analysis_of`, `same_evidence_different_analysis`,
`same_claim_different_schema`, `supersedes`. Migration
`0091_argument_relations_v2.sql`.
**Verdict:** Extend.

### I5. Machine confidence is not maturity

A model may assign confidence. Maturity is earned by evidence quality,
review, cross-source support, validation, or certification. High
machine confidence cannot promote a claim by itself.

**donto today:** Confidence and maturity are already separate
(`donto_confidence` in migration `0038_confidence.sql` is overlay;
maturity is `flags & 0b11100`). However, the extraction pipeline
auto-promotes via `confidence_to_maturity` at `helpers.py:39` (0.95 →
4, 0.8 → 3, 0.6 → 2, 0.4 → 1, else 0). For v1000, this is policy.
**Delta:** Cap auto-promotion at E2 for any extraction-produced
claim; require human review action for E3+. Implement in
`activities.py` ingest activity. Domain-specific ceilings configurable
per extraction job.
**Verdict:** Extend (one config; one logic change).

### I6. Governance propagates to derivatives

Derived claims, summaries, embeddings, translations, annotations, and
exports inherit the most restrictive applicable policy of their source
anchors unless a qualified authority grants an override.

**donto today:** Governance does not exist; inheritance therefore does
not exist.
**Delta:** Build the policy capsule + access assignment system (§6.12,
§12). Inheritance computed in
`packages/donto-policy/src/inheritance.rs` as part of every write
that has source anchors.
**Verdict:** Build.

### I7. Schema mappings are typed and scoped

No two predicates, classes, labels, fields, feature names, or ontology
terms are "the same" by default. Mappings require relation type,
scope, evidence, and safety flags for query expansion, export, and
logical inference.

**donto today:** Predicate Alignment Layer
(migrations `0048_predicate_alignment.sql` through
`0055_match_alignment_integration.sql`) has six relation types:
`exact_equivalent`, `inverse_equivalent`, `sub_property_of`,
`close_match`, `decomposition`, `not_equivalent`. Confidence is
recorded; safety flags and scope are not.
**Delta:** Extend to PRD's 11 relations
(§6.10). Add three boolean safety columns:
`safe_for_query_expansion`, `safe_for_export`, `safe_for_logical_inference`.
Add `scope` column referencing a context. Migration
`0092_alignment_relations_v2.sql`. Old names alias for one release.
**Verdict:** Extend.

### I8. Identity is a hypothesis, not a foreign key

Language identity, person identity, source identity, lexeme identity,
morpheme identity, place identity, specimen identity, case identity,
and concept identity may all be contested. The system stores identity
hypotheses and lets users query under selected identity lenses.

**donto today:** `donto_identity_hypothesis` (migration
`0061_identity_hypothesis.sql`) plus `donto_identity_edge` (migration
`0060_identity_edge.sql`) plus `donto_entity_symbol` (migration
`0057_entity_symbol.sql`). Three default hypotheses (strict, likely,
exploratory) referenced by entity-resolution endpoints.
**Delta:** Add explicit `hypothesis_kind` enum to the table:
`same_as`, `different_from`, `broader_than`, `narrower_than`,
`split_candidate`, `merge_candidate`, `successor_of`, `alias_of`.
Add a query-time "identity lens" parameter to the query language
(§11). Migration `0093_identity_hypothesis_kind.sql` plus
`evaluator.rs` extension.
**Verdict:** Extend.

### I9. Adapters must report information loss

Every import and export adapter produces a `loss_report`. If a source
format cannot represent governance, contradiction, time, n-ary frames,
anchors, or review state, that limitation is explicit.

**donto today:** `quarantine.rs` exists for malformed input but no
adapter currently produces a structured loss report. CSV/JSONL/Turtle
adapters either succeed or fail; partial-information loss is silent.
**Delta:** New crate `packages/donto-loss-report/` with shared
`LossReport` type. Every adapter (existing 8 + new) returns
`(IngestReport, LossReport)`. Loss reports stored as
`donto_adapter_run` rows.
**Verdict:** Build (cross-cutting refactor; ~2 weeks).

### I10. A release is a reproducible view

A release is not merely an exported file. It is a named query plus a
policy report, source manifest, transformation manifest, checksum
manifest, and reproducibility contract.

**donto today:** No release builder. Exports are ad-hoc.
**Delta:** New crate `packages/donto-release/`. Migration
`0094_dataset_release.sql`. CLI `donto release build|inspect|reinhydrate`.
Endpoint `POST /releases`. M7 deliverable.
**Verdict:** Build.

---

## 3. Product users

### 3.1 Primary users for v1000

| User | Job |
|---|---|
| **Research analyst** | Ingest sources, inspect evidence, compare claims, resolve review obligations, publish reproducible outputs. |
| **Domain expert reviewer** | Queue of candidate claims, evidence snippets, competing analyses, policy status, fast accept/reject/qualify workflow. |
| **Community or institutional authority** | Define access rules, grant attestations, audit access, restrict exports, revoke permissions, understand how derived data is used. |
| **Data engineer / agentic coder** | Stable schemas, APIs, tests, adapter contracts, task packets, clear acceptance criteria. |
| **Computational researcher** | Queryable structured evidence across sources, schemas, versions, time, maturity levels. |

### 3.2 Later users

| User | Job |
|---|---|
| **Publisher / repository operator** | Citable release artefacts, DOI-compatible metadata, provenance, checksums, policy-compliant exports. |
| **Federated instance operator** | Signed cross-instance trust, selective replication, policy-preserving query, release verification. |
| **Model evaluator** | Extraction-run provenance, reviewer decisions, calibration metrics, bias/audit reports. |

---

## 4. Scope

### 4.1 v1000 in scope

1. Source registration and versioned evidence storage.
2. Policy-first ingest.
3. Evidence anchors over text, image, tabular, structured, and media sources.
4. Candidate claim extraction and manual claim entry.
5. Context-scoped claim storage with polarity, modality, extraction
   level, confidence, maturity, and time.
6. N-ary frames for claims that cannot be represented as simple
   subject–predicate–object.
7. Predicate and schema alignment with typed relation semantics.
8. Identity hypotheses with merge/split/possible-same/different-from
   states.
9. Argumentation graph for support, rebuttal, undercutting,
   qualification, alternative analyses.
10. Review queue and maturity promotion rules.
11. Access policy inheritance, attestations, audit log, export
    enforcement.
12. Query API and reviewer UI.
13. Release builder with reproducibility manifests.
14. Linguistic pilot domain demonstrating the hardest evidence class.

### 4.2 v1000 out of scope

1. Replacing existing linguistic tools, archives, medical ontologies,
   legal databases, or publication systems.
2. Training a foundation model.
3. Claiming full automation of scholarly judgment.
4. Open-internet federation.
5. Guaranteeing perfect entity resolution.
6. Forcing a canonical ontology.
7. Storing restricted source content in public cloud contexts by
   default.
8. Treating any imported dataset as truth merely because it is
   structured.
9. Treating old interchange standards as internal architecture.
10. Public marketing claims of Indigenous data-governance compliance
    without independent review.

---

## 5. Design posture: old formats are adapters

| Existing pattern | Use in donto | Native replacement |
|---|---|---|
| RDF triples / quads | Export/import option | Evidence claim records plus frames and contexts |
| CLDF | Linguistic interchange adapter | Source-scoped claim and frame model |
| CoNLL-U / UD | Corpus adapter | Token/annotation frames with alternative analyses |
| UniMorph TSV | Paradigm adapter | Inflection-frame records and paradigm-state objects |
| TEI | Text/archive adapter | Source version + anchor graph + structured annotations |
| LIFT / FLEx | Lexical adapter | Lexeme/sense/form frames with evidence and policy |
| ELAN/EAF | Media annotation adapter | Time-aligned anchors and multimodal annotation frames |
| SKOS | Alignment/export vocabulary | Typed alignment edges with safety flags |
| SHACL | Validation/export inspiration | Native validation obligations plus optional SHACL view |
| PROV-O | Provenance export inspiration | Native source/run/anchor/agent lineage model |
| RO-Crate | Release packaging | Native reproducible-release manifest exported as RO-Crate |
| Verifiable Credentials | Attestation inspiration | Native attestation model with VC-compatible export |

Adapter rule: **ingest everything, canonicalize nothing prematurely,
record all loss.**

---

## 6. Native domain model — 14 first-class object families

Each family below is specified as the PRD requires it, then mapped to
current donto.

### 6.1 SourceObject

**Spec.** A document, dataset, database release, recording, image,
table, API payload, archive record, manuscript, webpage, or born-digital
object.

```jsonc
{
  "source_id": "src_...",
  "source_kind": "pdf|image|audio|video|dataset|table|api|webpage|manuscript|database_release|other",
  "title": "string",
  "creators": ["agent_id"],
  "source_date": "edtf_or_null",
  "registered_at": "timestamp",
  "registered_by": "agent_id",
  "policy_id": "policy_...",
  "content_address": "sha256_or_external_uri",
  "bibliographic_metadata": {},
  "native_format": "string",
  "adapter_used": "adapter_id|null",
  "status": "registered|ingested|quarantined|retired"
}
```

**donto today.** `donto_document` (migration `0023_documents.sql`) has
`document_id`, `iri`, `media_type`, `label`, `source_url`, `language`.
**Delta.** Add columns: `source_kind`, `creators jsonb`, `source_date
jsonb (EDTF)`, `registered_by`, `policy_id`, `content_address`,
`bibliographic_metadata jsonb`, `native_format`, `adapter_used`,
`status`. Migration `0095_source_object_extension.sql`.
**Verdict.** Extend.

### 6.2 SourceVersion

**Spec.** Immutable representation of a source at a point in system
time: OCR text, parsed rows, transcript, normalised PDF text, image
tiles, table cells.

**donto today.** `donto_document_revision` (migration
`0024_document_revisions.sql`) has revision_id, document_id, body,
parser_version. Lineage exists via foreign key.
**Delta.** Add `version_kind` enum (raw / ocr / transcript / parsed /
normalized / translated / redacted), `content_hash`, `quality_metrics
jsonb`, `derived_from_versions text[]`. Migration
`0096_source_version_extension.sql`.
**Verdict.** Extend.

### 6.3 EvidenceAnchor

**Spec.** Locator inside a source version. Anchor kinds:

```text
char_span | page_box | image_box | media_time | table_cell | csv_row
| json_pointer | xml_xpath | html_css | token_range | annotation_id
| archive_field | whole_source
```

```jsonc
{
  "anchor_id": "anc_...",
  "version_id": "ver_...",
  "anchor_kind": "char_span",
  "locator": {"start": 10392, "end": 10514},
  "locator_schema_version": "anchor-schema-1",
  "confidence": 0.98,
  "policy_id": "policy_..."
}
```

**donto today.** `donto_span` (migration `0025_spans.sql`) has
char-offsets and a `region jsonb` for everything else.
`donto_content_regions` (migration `0041_content_regions.sql`)
provides typed regions. Linking via
`donto_evidence_link` (migration `0029_evidence_links.sql`).
**Delta.** Register the 13 anchor kinds in a controlled vocabulary
(migration `0097_anchor_kind_registry.sql`). Add a per-kind locator
schema with validators (`packages/donto-anchor/src/`). Add `policy_id`
column. Validators integrated into the write path so an invalid
locator rejects.
**Verdict.** Extend (registry + validators, no new table).

### 6.4 ClaimRecord

**Spec.** Central unit of structured knowledge.

```jsonc
{
  "claim_id": "clm_...",
  "claim_kind": "atomic|frame_summary|absence|identity|alignment|policy|review|validation",
  "subject_ref": "entity_or_literal_ref|null",
  "predicate_ref": "predicate_or_local_term|null",
  "object_ref": "entity_or_literal_ref|null",
  "frame_id": "frame_id|null",
  "contexts": ["ctx_..."],
  "polarity": "asserted|negated|unknown|absent|conflicting",
  "modality": "descriptive|prescriptive|reconstructed|inferred|elicited|corpus_observed|typological_summary|legal_holding|clinical_observation|experimental_result|other",
  "extraction_level": "quoted|table_read|example_observed|source_generalization|cross_source_inference|model_hypothesis|human_hypothesis|manual_entry",
  "confidence": {"machine": 0.82, "calibrated": null, "human": null},
  "maturity": "E0|E1|E2|E3|E4|E5",
  "valid_time": "edtf_interval_or_null",
  "transaction_time": {"opened_at": "...", "closed_at": null},
  "evidence_anchors": ["anc_..."],
  "created_by": "agent_or_run_id",
  "policy_id": "policy_...",
  "status": "active|superseded|retracted|quarantined"
}
```

**donto today.** `donto_statement` (migration `0001_core.sql`) covers
subject / predicate / object / context / `valid_time` / `tx_time` /
`flags` (polarity 2 bits + maturity 3 bits) / `content_hash`. Single
context per row; the rest of the dimensions live in overlays.
**Delta.**

- Polarity extension: add `unknown` and `conflicting` (already in flag
  layout under different names). Migration `0098_polarity_v2.sql`.
- Modality: new overlay `donto_statement_modality(statement_id,
  modality)` (migration `0099_statement_modality.sql`).
- Extraction level: new overlay
  `donto_statement_extraction_level(statement_id, level)` (migration
  `0100_extraction_level.sql`).
- Confidence with three values: extend `donto_confidence` (migration
  `0101_confidence_multivalue.sql`).
- Maturity rename L0→E0 .. L4→E4, add E5: migration
  `0102_maturity_e_naming.sql` extends `flags` maturity bits to 4 bits
  (was 3) — backward-compatible because we use lower 3 bits unchanged.
- Multiple contexts per claim: new junction table
  `donto_statement_context(statement_id, context, role)` (migration
  `0103_multi_context.sql`). Existing single-context column remains as
  `primary_context`.
- `claim_kind`: new column on `donto_statement` (migration
  `0104_claim_kind.sql`).
- `status`: derivable from existing tx_time / superseded_by; expose as
  view `donto_v_claim_status`.

**Verdict.** Extend (six small migrations; preserves all existing
data).

### 6.5 ClaimFrame

**Spec.** N-ary analyses and complex structured events.

```jsonc
{
  "frame_id": "frm_...",
  "frame_type": "paradigm_cell|allomorphy_rule|construction|argument_structure|interlinear_example|diagnosis|legal_precedent|experiment_result|identity_hypothesis|schema_mapping|other",
  "roles": [
    {"role": "agent", "value": "entity_ref", "anchors": ["anc_..."]},
    {"role": "patient", "value": "entity_ref", "anchors": ["anc_..."]}
  ],
  "constraints": [],
  "frame_schema_version": "frame-schema-1",
  "policy_id": "policy_..."
}
```

**donto today.** Event frames exist (migration `0054_event_frames.sql`)
via `donto_decompose_to_frame()`. Roles are stored as predicates on
the frame node — queryable but not indexed as a roles structure.
**Delta.** New tables `donto_claim_frame(frame_id, frame_type,
frame_schema_version, policy_id)` and `donto_frame_role(frame_id,
role, value_ref, anchor_ids)` with role index. Migrations
`0105_claim_frame.sql`, `0106_frame_role.sql`. The 18 frame types in
§13.4 register as seed rows. Existing event-frame statements expose
through a compatibility view.
**Verdict.** Build (two new tables; co-exist with the event-frame
pattern).

### 6.6 ContextScope

**Spec.** Defines the scope in which a claim is made or queried. 16
context kinds:

```text
source | source_version | dataset_release | project | hypothesis
| identity_lens | schema_lens | review_lens | community_policy_scope
| language_or_variety | corpus | experiment | jurisdiction
| clinical_cohort | historical_period | user_workspace | release_view
```

```jsonc
{
  "context_id": "ctx_...",
  "context_kind": "source|hypothesis|schema_lens|...",
  "label": "string",
  "parent_contexts": ["ctx_..."],
  "policy_id": "policy_...",
  "created_at": "...",
  "created_by": "agent_id",
  "closed_at": null
}
```

**donto today.** `donto_context` (migration `0001_core.sql`) has iri,
kind, parent, label, metadata, mode, created_at, closed_at. Existing
kinds: `source`, `hypothesis`, `derived`, `snapshot`, `user`, `custom`.
**Delta.** Extend kinds enum to the 16. Add `created_by` FK to
`donto_agent`. Allow multiple parents (migration
`0107_context_multi_parent.sql` introduces
`donto_context_parent(context, parent)`).
**Verdict.** Extend.

### 6.7 EntityRecord

**Spec.** Any referent: language variety, person, lexeme, morpheme,
source, place, concept, legal case, medical condition, dataset, event,
artefact, etc.

```jsonc
{
  "entity_id": "ent_...",
  "entity_kind": "language_variety|person|lexeme|morpheme|place|concept|artifact|case|condition|gene|event|organization|other",
  "labels": [{"text": "...", "language": "bcp47_or_null", "script": "iso15924_or_null"}],
  "external_ids": [{"registry": "...", "id": "...", "confidence": 1.0}],
  "created_at": "...",
  "identity_status": "provisional|stable|deprecated|split|merged|contested",
  "policy_id": "policy_..."
}
```

**donto today.** `donto_entity_symbol` (migration
`0057_entity_symbol.sql`) plus `donto_entity_alias` (migration
`0042_entity_aliases.sql`). Subjects appear in `donto_statement.subject`
as IRIs.
**Delta.** Build a richer `donto_entity` table that extends
`donto_entity_symbol` with `entity_kind`, `labels jsonb` (multilingual),
`external_ids jsonb`, `identity_status`, `policy_id`. Migration
`0108_entity_extension.sql`.
**Verdict.** Extend.

### 6.8 IdentityHypothesis

**Spec.** Identity resolution explicitly modeled.

```jsonc
{
  "identity_hypothesis_id": "idh_...",
  "hypothesis_kind": "same_as|different_from|broader_than|narrower_than|split_candidate|merge_candidate|successor_of|alias_of",
  "entity_refs": ["ent_A", "ent_B"],
  "confidence": 0.73,
  "method": "human|rule|model|registry_match|cross_source_evidence",
  "evidence_anchors": ["anc_..."],
  "context_id": "ctx_identity_lens_...",
  "status": "candidate|accepted|rejected|superseded"
}
```

**donto today.** `donto_identity_hypothesis` (migration
`0061_identity_hypothesis.sql`) plus `donto_identity_edge` (migration
`0060_identity_edge.sql`). Three default hypothesis contexts.
**Delta.** Add `hypothesis_kind`, `method`, `status` columns. Allow
`entity_refs` to be multi-entity (current edge model is binary).
Migration `0109_identity_hypothesis_v2.sql`.
**Verdict.** Extend.

### 6.9 PredicateRecord

**Spec.** Predicates are first-class objects, not strings.

```jsonc
{
  "predicate_id": "prd_...",
  "label": "string",
  "definition": "string",
  "domain_hints": ["entity_kind"],
  "range_hints": ["entity_kind|literal_type"],
  "examples": [],
  "source_schema": "donto-native|wals|grambank|ud|custom|...",
  "created_by": "agent_id",
  "minting_status": "candidate|approved|deprecated|merged",
  "nearest_existing_at_mint": [{"predicate_id": "...", "similarity": 0.82}]
}
```

**donto today.** `donto_predicate` (migration `0006_predicate.sql`)
plus `donto_predicate_descriptor` (migration
`0049_predicate_descriptor.sql`) which already has label, gloss,
domain, range, examples, embedding.
**Delta.** Add `minting_status` column and `nearest_existing_at_mint
jsonb` capture. Add `source_schema` column. Migration
`0110_predicate_minting.sql`. CLI `donto predicates mint` that
refuses without descriptor + nearest-neighbour search.
**Verdict.** Extend.

### 6.10 AlignmentEdge

**Spec.** Eleven relations + three safety flags + scope.

```jsonc
{
  "alignment_id": "aln_...",
  "left_ref": "...",
  "right_ref": "...",
  "relation": "exact_equivalent|close_match|broad_match|narrow_match|inverse_of|decomposes_to|has_value_mapping|incompatible_with|derived_from|local_specialization|not_equivalent",
  "scope": "ctx_...",
  "safe_for_query_expansion": true,
  "safe_for_export": false,
  "safe_for_logical_inference": false,
  "confidence": 0.91,
  "evidence_anchors": ["anc_..."],
  "review_status": "candidate|accepted|rejected|superseded"
}
```

**donto today.** `donto_predicate_alignment` (migration
`0048_predicate_alignment.sql`) has 6 relations and confidence. No
scope, no safety flags, no review status. Closure built via
`donto_predicate_closure` (migration `0051_predicate_closure.sql`).
**Delta.** Migration `0092_alignment_relations_v2.sql` (already
called out in §I7) extends relations to 11, adds the three safety
booleans, adds `scope`, adds `review_status`, adds
`evidence_anchors text[]`.
**Verdict.** Extend.

### 6.11 ArgumentEdge

**Spec.** Connects claims as evidence, disagreement, or qualification.

```jsonc
{
  "argument_id": "arg_...",
  "from_claim_id": "clm_A",
  "to_claim_id": "clm_B",
  "argument_kind": "supports|rebuts|undercuts|qualifies|explains|alternative_analysis_of|same_evidence_different_analysis|same_claim_different_schema|supersedes",
  "strength": 0.64,
  "evidence_anchors": ["anc_..."],
  "created_by": "agent_or_run_id"
}
```

**donto today.** `donto_argument` (migration `0031_arguments.sql`)
has 4 relations: `supports`, `rebuts`, `undercuts`, `qualifies`. Has
`strength`. No evidence anchors per argument; no `explains` /
`alternative_analysis_of` / `same_evidence_different_analysis` /
`same_claim_different_schema` / `supersedes`.
**Delta.** Migration `0091_argument_relations_v2.sql` (already in §I4)
extends the relation enum to 9 kinds and adds `evidence_anchors`.
**Verdict.** Extend.

### 6.12 PolicyCapsule

**Spec.** Governs source access, derived data, export, model use,
release eligibility. 14 allowed-action types.

```jsonc
{
  "policy_id": "pol_...",
  "policy_kind": "public|open_metadata_restricted_content|community_restricted|embargoed|licensed|private|regulated|sealed|unknown_restricted",
  "authority_refs": ["agent_or_org_id"],
  "allowed_actions": {
    "read_metadata": true, "read_content": false, "quote": false,
    "view_anchor_location": false, "derive_claims": true,
    "derive_embeddings": false, "translate": false, "summarize": false,
    "export_claims": false, "export_sources": false, "export_anchors": false,
    "train_model": false, "publish_release": false,
    "share_with_third_party": false, "federated_query": false
  },
  "inheritance_rule": "max_restriction|source_policy|authority_override_only",
  "expiry": null,
  "revocation_status": "active|revoked|expired|superseded",
  "human_readable_summary": "string"
}
```

**donto today.** Does not exist.
**Delta.** Build. Migration `0111_policy_capsule.sql` introduces
`donto_policy_capsule`. M0 deliverable.
**Verdict.** Build.

### 6.13 Attestation

**Spec.** Proof that an actor may perform an action under a policy.

```jsonc
{
  "attestation_id": "att_...",
  "holder_agent_id": "agent_...",
  "issuer_agent_id": "agent_...",
  "policy_id": "pol_...",
  "actions": ["read_content", "derive_claims"],
  "purpose": "review|community_curation|private_research|publication|model_training|audit",
  "issued_at": "...",
  "expires_at": "...|null",
  "revoked_at": null,
  "credential_ref": "vc_or_local_signature_ref"
}
```

**donto today.** Does not exist.
**Delta.** Build. Migration `0112_attestation.sql`. M0 deliverable.
W3C Verifiable Credentials compatibility deferred to v1010.
**Verdict.** Build.

### 6.14 ReleaseManifest

**Spec.** Citable, reproducible view.

```jsonc
{
  "release_id": "rel_...",
  "release_name": "string",
  "query_spec": {},
  "policy_report": {},
  "source_manifest": [],
  "transformation_manifest": [],
  "checksums": [],
  "created_at": "...",
  "created_by": "agent_id",
  "output_formats": ["donto-jsonl", "ro-crate", "cldf", "rdf", "csv", "conllu"],
  "reproducibility_status": "reproducible|policy_dependent|non_reproducible",
  "citation_metadata": {}
}
```

**donto today.** Does not exist.
**Delta.** Build. Migration `0094_dataset_release.sql` (in §I10). M7
deliverable.
**Verdict.** Build.

---

## 7. Epistemic model

### 7.1 Maturity ladder — E0 through E5

| Level | Name | Meaning | Promotion requirements |
|---|---|---|---|
| E0 | Raw | Source or extraction artefact exists, not trusted as a claim. | Source registered, policy classified. |
| E1 | Candidate | Model/rule/human proposed. | Evidence anchor or `hypothesis_only`. |
| E2 | Evidence-supported | Grounded in span/row/timecode, basic validation passes. | Anchor validation, source-policy inheritance, no malformed terms. |
| E3 | Reviewed | Domain reviewer accepted, rejected, or qualified. | Human or authorised reviewer decision. |
| E4 | Corroborated | Cross-source support or survives contradiction review. | Multiple independent anchors or accepted argument analysis. |
| E5 | Certified | Passes formal or highly structured validation. | Machine-checkable certificate, formal shape, or domain proof artefact. |

Promotion is monotonic per claim event. A claim may be superseded or
retracted; maturity history remains queryable.

**donto today.** L0–L4 (5 levels). Mapping:

```
L0 raw       → E0 Raw
L1 parsed    → E1 Candidate
L2 linked    → E2 Evidence-supported
L3 reviewed  → E3 Reviewed
L4 certified → E5 Certified
(no donto equivalent) → E4 Corroborated  ← new tier
```

**Delta.** Migration `0102_maturity_e_naming.sql` widens
maturity bits from 3 to 4 (room for 0–15) and renames levels in the
helper functions (`donto_maturity` returns "E0".."E5"). E4 is new and
fires when cross-source corroboration is detected by an obligation
resolver. Verdict: Extend.

### 7.2 Confidence model

donto stores at least four values:

1. `machine_confidence` — model/rule reported.
2. `calibrated_confidence` — empirical, calibrated against reviewer
   decisions.
3. `human_confidence` — reviewer reported.
4. `source_reliability_weight` — source/method reliability if defined.

Queries may apply a weighting lens; default does not collapse to a
scalar.

**donto today.** Single confidence score in `donto_confidence`.
**Delta.** Migration `0101_confidence_multivalue.sql` adds three
columns to `donto_confidence` plus `confidence_lens text` for the
weighting selection. Verdict: Extend.

### 7.3 Extraction levels

```text
quoted | table_read | example_observed | source_generalization
| cross_source_inference | model_hypothesis | human_hypothesis
| manual_entry | registry_import | adapter_import
```

**donto today.** Implicit in extraction-pipeline tier (T1–T8 in
`helpers.py:64`), but not stored as a property of the resulting claim.
**Delta.** Migration `0100_extraction_level.sql` (already in §6.4)
adds the per-claim level. Auto-promotion gating: claims with
`extraction_level=model_hypothesis` are capped at E1; `manual_entry` is
capped at E2 unless reviewed; `quoted`/`table_read` may auto-reach E2.
Verdict: Extend.

### 7.4 Modality

```text
descriptive | prescriptive | reconstructed | inferred | elicited
| corpus_observed | typological_summary | experimental_result
| clinical_observation | legal_holding | archival_metadata
| oral_history | community_protocol | model_output
```

**donto today.** Does not exist.
**Delta.** Migration `0099_statement_modality.sql` (already in §6.4).
Verdict: Build (one overlay table).

---

## 8. Functional requirements

Every FR includes a current-state mapping.

### FR-001 Source registration

POST `/sources` rejects missing policy. Duplicate content hashes are
idempotent.
**donto today.** `POST /documents/register` (sidecar
`apps/dontosrv/src/lib.rs:65`) accepts `iri`, `media_type`, `label`,
`source_url`, `language`. No policy field.
**Delta.** Add `default_policy_id` (required); migrate field name to
`policy_id`. M0.
**Verdict.** Extend.

### FR-002 Source versioning

OCR, transcript, parsed rows, normalized text, translation, and
redaction are separate versions; lineage queryable; idempotent re-run.
**donto today.** `POST /documents/revision` (line 66 of `lib.rs`)
already supports versions; lineage via FK; content-hash dedup.
**Delta.** Add `version_kind`, `quality_metrics`, `derived_from_versions`.
**Verdict.** Extend.

### FR-003 Anchor creation and validation

Supported anchors v1000: `whole_source`, `char_span`, `page_box`,
`table_cell`, `csv_row`, `json_pointer`, `xml_xpath`, `media_time`,
`token_range`, `annotation_id`. Invalid locator → reject or quarantine.
**donto today.** `POST /evidence/link/span` (line 67) creates evidence
links to spans. Spans support char offsets and `region jsonb`.
**Delta.** Anchor-kind registry + validators per kind. M2.
**Verdict.** Extend.

### FR-004 Claim write path

Claims without evidence allowed only with `hypothesis_only=true` and
maturity ≤ E1. Claim writes compute inherited policy. Append-only.
**donto today.** Append-only: yes. Inherited policy: not implemented.
Hypothesis-only flag: not implemented.
**Delta.** Hypothesis-only column (§I1). Policy inheritance computation
in `packages/donto-policy/`. M2.
**Verdict.** Extend.

### FR-005 Frame write path

Frames represent at least: identity hypothesis, schema mapping,
interlinear example, paradigm cell, allomorphy rule, legal precedent,
experimental result, clinical observation, source-citation chain.
Frame roles indexed.
**donto today.** Event frames work via
`donto_decompose_to_frame()`. Roles are predicates; no role index.
**Delta.** New `donto_claim_frame` and `donto_frame_role` tables (§6.5).
**Verdict.** Build.

### FR-006 Context scopes

Supported v1000 kinds listed in §6.6. Inheritance queryable. Access
policy may inherit through context trees.
**donto today.** Context tree exists; six kinds supported; inheritance
via parent FK.
**Delta.** Extend kinds enum, add multi-parent support.
**Verdict.** Extend.

### FR-007 Predicate registry

New predicate approval requires label, definition, domain/range hints,
examples, source schema, nearest-neighbor comparison. Candidate
predicates can be used in E1 claims but not released publicly without
approval.
**donto today.** Descriptor exists; minting workflow does not exist.
**Delta.** CLI `donto predicates mint` with required descriptor. CI
guard rejects un-descriptored predicates. M3.
**Verdict.** Extend.

### FR-008 Alignment registry

Eleven relation types (§6.10). Each declares safety for query
expansion / export / logical inference. Query expansion runs in
`STRICT`, `EXPAND_SAFE`, or `EXPAND_EXPERIMENTAL` mode.
**donto today.** Six relations, no safety flags, expansion runs by
default.
**Delta.** Migration `0092_alignment_relations_v2.sql`. Query language
extended to recognise the three modes. M3.
**Verdict.** Extend.

### FR-009 Identity hypotheses

Users query under selected identity lens. Merge/split candidates never
destroy original entity IDs. Identity hypotheses can be supported,
rebutted, reviewed.
**donto today.** Three default lenses; entity IDs preserved across
merges; rebutting is via argument edges (already supported).
**Delta.** Add `hypothesis_kind` and `method` columns; expose lens as
query parameter.
**Verdict.** Extend.

### FR-010 Argumentation graph

Nine argument kinds (§6.11). Contradiction frontier view exposed.
Argument edges may have evidence anchors and review states.
**donto today.** Four argument kinds; contradiction frontier exists
(`/arguments/frontier`); no evidence anchors per argument; no review
state on arguments.
**Delta.** Migration `0091_argument_relations_v2.sql`.
**Verdict.** Extend.

### FR-011 Proof obligations

Nine kinds: `needs_evidence`, `needs_policy`, `needs_review`,
`needs_identity_resolution`, `needs_alignment_review`,
`needs_anchor_repair`, `needs_contradiction_review`,
`needs_formal_validation`, `needs_community_authority`.
**donto today.** Eight kinds (`donto_proof_obligation` migration
`0032_proof_obligations.sql`). Missing: `needs_policy`,
`needs_alignment_review`, `needs_anchor_repair`,
`needs_community_authority`. (Some current kinds get renamed.)
**Delta.** Migration `0113_obligation_kinds_v2.sql` extends enum.
**Verdict.** Extend.

### FR-012 Review workflow

Reviewer decisions: accept, reject, qualify, merge, split,
request-more-evidence, escalate-to-authority, mark-sensitive. Citable
and auditable. Feed calibration metrics; do not silently alter
historical claims.
**donto today.** Reactions exist (`donto_react`, migration
`0017_reactions.sql`) but they are folksonomic, not structured review.
No review-decision table.
**Delta.** New table `donto_review_decision` (migration
`0114_review_decision.sql`). M4.
**Verdict.** Build.

### FR-013 Policy enforcement

Unknown policy → restricted. Policy checks occur before content
retrieval, extraction, embedding generation, export, release. Derived
records inherit restrictive policy. Aggregations don't leak restricted
counts unless allowed. Every restricted action audit-logged.
**donto today.** No policy enforcement.
**Delta.** Sidecar middleware + query-evaluator filter (§12). M0.
**Verdict.** Build.

### FR-014 Attestation management

Attestations scoped by holder, issuer, policy, action, purpose,
expiry. Revocation immediate for new reads/exports. Exportable to W3C
VC.
**donto today.** Does not exist.
**Delta.** Migration `0112_attestation.sql` + service code. M0.
**Verdict.** Build.

### FR-015 Query API

Native query API supports claim search, evidence drill-down,
contradiction frontier, identity-lens queries, schema-expanded queries,
as-of transaction time. Queries return policy-filtered results by
default.
**donto today.** DontoQL + SPARQL subset support most dimensions.
PRESET clauses parse but evaluator does not implement them. No
identity-lens, schema-lens, or policy-filtered semantics.
**Delta.** Implement PRESET resolution in `evaluator.rs`. Add
identity-lens and schema-lens parameters. Apply policy filter as
post-binding step. M3 + M4.
**Verdict.** Extend.

### FR-016 Release builder

Releases include query spec, policy report, source manifest,
transformation manifest, checksums, loss report, citation metadata.
Re-running unchanged release produces identical content checksums.
Public release blocks if policy report contains unresolved restrictions.
**donto today.** Does not exist.
**Delta.** Build. M7.
**Verdict.** Build.

### FR-017 Adapter framework

Every adapter declares supported source kinds, output object families,
loss-report fields, validation checks, round-trip expectations.
Adapter failure → quarantine, not silent loss.
**donto today.** Eight ingest formats exist with shared `Pipeline`.
Quarantine routing exists. No structured loss reports.
**Delta.** New `LossReport` type; every adapter returns one. Adapter
contract (FR-017's JSON object) declared in code.
**Verdict.** Extend.

### FR-018 Extraction runs

Each run records model name, version, provider, prompt hash, toolchain
version, adapter version, chunking, temperature/seed, input source
versions, output claims, cost/latency.
**donto today.** `donto_extraction_run` (migration
`0028_extraction_runs.sql`) plus `donto_extraction_chunk` (migration
`0037_extraction_chunks.sql`). Most fields exist.
**Delta.** Add `policy_check_id` reference and explicit
`prompt_hash`.
**Verdict.** Extend.

### FR-019 Validation overlay

Validation failures create annotations and obligations. Some
write-path validations are hard gates: malformed anchors, missing
policy, broken source-version reference. Domain validations promote
claims only when defined by policy and review rules.
**donto today.** Shape validation (migration `0008_shape.sql`)
produces annotations. No hard gates.
**Delta.** Add hard-gate validators to write path. Specify in
`packages/donto-validate/`.
**Verdict.** Extend.

### FR-020 Observability and audit

Audit log covers writes, policy decisions, restricted reads, exports,
release builds, reviewer decisions, attestation checks. Dashboards
show source ingest, claim volume, maturity distribution, contradiction
frontier, policy status, review backlog, extraction acceptance,
adapter failures, release reproducibility.
**donto today.** `donto_audit` (migration `0001_core.sql`) covers
writes; firehose SSE; observability views. No policy-decision audit;
no release audit.
**Delta.** Extend audit to cover all governance actions; new dashboards.
**Verdict.** Extend.

---

## 9. Non-functional requirements

| ID | Requirement | donto status |
|---|---|---|
| NFR-001 Security | Restricted reads require policy + attestation; external model calls pass policy check; encryptable at rest; tamper-evident audit. | M0 builds. |
| NFR-002 Privacy and governance | Distinct read/quote/derive/export/train/publish permissions; policy inheritance to embeddings and summaries; community/institution-specific vocabularies. | M0 builds. |
| NFR-003 Scalability | 10M claims commodity; 100M anchors with partitioning path; 1B target for v2000; P95 < 2s for indexed queries at 10M. | Current: ~35M statements at 27GB on a single Postgres node. v1000 maintains. |
| NFR-004 Reliability | Idempotent ingestion/extraction; quarantine path; checksum-stable manifests. | Existing idempotency invariant; release manifest is M7. |
| NFR-005 Interoperability | Native donto-jsonl first; v1000 ships RO-Crate + one domain export; RDF/JSON-LD/CLDF/CoNLL-U as adapters. | Native JSONL exists; RO-Crate is M7; CLDF is M6. |
| NFR-006 Explainability | Every claim view shows source, anchor, context, policy, run, confidence, maturity, modality, review state, related disagreements, lens. | `donto_claim_card()` (migration `0034`) covers most; extend for policy and modality. |
| NFR-007 Testability | Product invariants automated; policy leakage in CI; adapter round-trip with loss reports. | Five invariant suites exist; extend for v1000. |

---

## 10. Architecture

### 10.1 Component map

```text
donto v1000
├── Trust Kernel
│   ├── PolicyCapsule service
│   ├── Attestation service
│   ├── Access-check middleware
│   └── Audit ledger
│
├── Evidence Kernel
│   ├── Source registry          (donto_document, extended)
│   ├── Source version store     (donto_document_revision, extended)
│   ├── Anchor registry          (donto_span + donto_content_regions)
│   ├── Object/blob pointer store (NEW: donto-blob/)
│   └── Adapter quarantine       (donto-ingest/quarantine.rs)
│
├── Claim Kernel
│   ├── Claim records            (donto_statement)
│   ├── Claim frames             (NEW: donto_claim_frame)
│   ├── Context scopes           (donto_context, extended)
│   ├── Transaction-time history (built in)
│   ├── Valid-time expressions   (donto_time_expression, extended)
│   └── Epistemic metadata       (overlays per dimension)
│
├── Schema Kernel
│   ├── Predicate registry       (donto_predicate + donto_predicate_descriptor)
│   ├── Alignment registry       (donto_predicate_alignment, extended)
│   ├── Closure builder          (donto_predicate_closure)
│   ├── Value mappings           (NEW: donto_alignment_value_mapping)
│   └── Predicate-minting workflow (NEW: donto-mint)
│
├── Identity Kernel
│   ├── Entity registry          (donto_entity_symbol + donto_entity, NEW)
│   ├── Identity hypotheses      (donto_identity_hypothesis, extended)
│   ├── Lens resolver            (NEW: donto-lens)
│   └── Split/merge workflows    (extends donto_identity_edge)
│
├── Argument Kernel
│   ├── Argument edges           (donto_argument, extended)
│   ├── Contradiction frontier   (existing view)
│   ├── Obligation engine        (donto_proof_obligation, extended)
│   └── Review-state propagation (NEW: donto-review)
│
├── Extraction Kernel
│   ├── Adapter runners          (donto-ingest crates)
│   ├── Chunkers                 (donto-api workers)
│   ├── LLM/rule extractors      (donto-api activities + new domain dispatch)
│   ├── Run provenance           (donto_extraction_run + chunks)
│   ├── Candidate writer         (extended ingest_facts())
│   └── Calibration dataset      (NEW: M8)
│
├── Review Workbench
│   ├── Claim review             (TUI tab + web later)
│   ├── Alignment review         (TUI tab)
│   ├── Identity review          (TUI tab)
│   ├── Policy review            (TUI tab)
│   └── Release review           (TUI tab)
│
├── Query Engine
│   ├── Native query API         (donto-query, extended)
│   ├── Lens resolution          (NEW)
│   ├── Policy-filtered execution (NEW)
│   ├── Schema expansion         (existing PAL closure)
│   └── Evidence drill-down      (donto_claim_card)
│
└── Release Builder
    ├── Query capture            (NEW)
    ├── Policy report            (NEW)
    ├── Manifest generation      (NEW)
    ├── Checksums                (NEW)
    ├── Loss reports             (NEW)
    └── Adapter exports          (extends donto-ingest)
```

### 10.2 Storage posture

- **Postgres** (existing): source metadata, anchors, claims, frames,
  policies, attestations, alignments, identities, arguments,
  obligations, audit events.
- **Object storage** (new in v1000): PDFs, images, audio, video,
  large datasets. New crate `packages/donto-blob/` wraps signed-URL
  access. Default dev: MinIO. Production: choose S3-compatible
  (decided per deployment).
- **Search index** (existing): trigram on labels; FTS on bodies; both
  via Postgres.
- **Vector index** (existing): pgvector for predicate descriptors,
  span embeddings, candidate generation.
- **Queue/worker** (existing): Temporal for extraction, validation,
  release jobs.

The storage layer is event-sourced for destructive operations — already
true for `donto_statement`; extended in v1000 to alignments, identities,
policies, attestations, reviews via `donto_event_log`.

### 10.3 API posture

Three layers:

1. **Operational REST** for ingestion, review, policy, release, admin.
2. **Native Query API** for evidence-oriented search and graph
   traversal.
3. **Adapter APIs** for CLDF, RDF, CoNLL-U, RO-Crate, JSONL, domain-
   specific exports.

SPARQL is supported as an adapter, not as the only query interface.
External standards cannot express the full native policy / evidence /
maturity model without extensions.

---

## 11. Native query language requirements

### 11.1 Query dimensions

Every query may specify:

```text
subject/entity constraints
predicate/schema constraints
object/value constraints
context scopes
identity lens
schema lens
valid time
transaction time
policy visibility
maturity range
modality
extraction level
review status
argument state
release eligibility
anchor/source filters
```

### 11.2 Examples

Contested-claim search:

```text
FIND claims
WHERE subject IN entity("language:*")
  AND predicate EXPANDS_FROM concept("case_marking") USING schema_lens("linguistics-core")
  AND maturity >= E2
  AND modality IN [descriptive, typological_summary, corpus_observed]
  AND policy ALLOWS read_metadata
RETURN claim, source, anchors, review_state, disagreements
ORDER BY contradiction_pressure DESC
LIMIT 100
```

Release-safe claims:

```text
FIND claims
WHERE context UNDER ctx("project:language-pilot")
  AND maturity >= E3
  AND policy ALLOWS publish_release
  AND status = active
RETURN release_record
WITH evidence = redacted_if_required
```

As-of reconstruction:

```text
FIND claims
WHERE subject = entity("ent:...")
  AND transaction_time AS_OF "2026-01-01T00:00:00Z"
RETURN claim, maturity, policy, review_state
```

**donto today.** DontoQL has 10 clauses (SCOPE, PRESET, MATCH, FILTER,
POLARITY, MATURITY, IDENTITY, PREDICATES, PROJECT, LIMIT/OFFSET).
SPARQL subset has PREFIX, SELECT, WHERE, GRAPH, FILTER, LIMIT, OFFSET.
PRESET parses but does not evaluate.

**Delta.** v1000 query language extensions:
- Add `MODALITY` clause.
- Add `EXTRACTION_LEVEL` clause.
- Add `IDENTITY_LENS` clause (replaces / extends current `IDENTITY`).
- Add `SCHEMA_LENS` clause.
- Add `POLICY ALLOWS <action>` filter.
- Add `EXPANDS_FROM concept(...) USING schema_lens(...)` syntax.
- Add `TRANSACTION_TIME AS_OF <ts>` clause.
- Add `ORDER BY contradiction_pressure DESC` (one named ordering).
- Add `WITH evidence = redacted_if_required` post-clause.
- Implement PRESET resolution finally.

Implementation: extend `dontoql.rs` parser; extend `algebra.rs` Query
struct; extend `evaluator.rs` to honour new dimensions. Migration
`0115_query_v2_metadata.sql` records the new clause vocabulary.

**Verdict.** Extend.

---

## 12. Governance and access-control requirements

### 12.1 Permission actions

```text
read_metadata | read_content | quote | view_anchor_location
| derive_claims | derive_embeddings | translate | summarize
| export_claims | export_sources | export_anchors
| train_model | publish_release | share_with_third_party
| federated_query
```

15 distinct actions. Permission to read does not imply permission to
quote, derive embeddings, train a model, or publish.

### 12.2 Policy inheritance

Derived objects using multiple anchors inherit the most restrictive
policy unless all relevant authorities approve a different policy.

```text
public grammar page + restricted recording timecode -> derived claim is restricted
```

### 12.3 Governance review gates

Required checks:

1. Source ingestion.
2. External extraction call.
3. Embedding generation.
4. Public display.
5. Claim export.
6. Release generation.
7. Federation.
8. Training-set inclusion.

### 12.4 Authorities

```text
individual researcher | project PI | archive | publisher
| community organization | cultural authority | ethics board
| legal authority | data steward | institutional review board
```

Multiple authorities and unresolved authority questions allowed.
Unresolved authority creates a `needs_community_authority` obligation.

### 12.5 Governance product surfaces

v1000 UI:

- Policy registry.
- Attestation registry.
- Source policy assignment view.
- Derived-data policy graph.
- Restricted access audit view.
- Release policy report.
- Authority escalation queue.

**donto today.** None of this exists.
**Delta.** Build whole stack. M0 (kernel) + M4 (UI surfaces).
**Verdict.** Build.

---

## 13. Language-evidence pilot domain

The language pilot is the first stress test, not the product boundary.

### 13.1 Pilot goals

1. Represent every known or future language variety as an open-world
   entity, not a closed list.
2. Ingest registry, typological, corpus, lexical, phonological,
   textual, media evidence.
3. Preserve incompatible analyses from multiple sources.
4. Align overlapping grammatical schemas without forcing a canonical
   ontology.
5. Support restricted/community-governed language documentation.
6. Produce a citable release whose restrictions are enforceable.

### 13.2 Native language entity model

A language variety is an `EntityRecord` with kind `language_variety`.
External identifiers from Glottolog, ISO 639, BCP 47, ISO 15924, WALS,
or local project IDs are *external_ids*; none is authoritative for
every use case. Unknown, extinct, ancient, reconstructed, constructed,
signed, mixed, revived, secret, ceremonial, or future varieties are
represented by provisional entities.

### 13.3 Source categories

v1000 supports at minimum:

1. Registry datasets.
2. Comparative typological databases.
3. Reference grammars.
4. Dictionaries and lexica.
5. Corpus/treebank data.
6. Interlinear glossed text.
7. Phonological inventories.
8. Field recordings with time-aligned annotation.
9. Manuscript/archival source descriptions.
10. Community/governance metadata.

### 13.4 Frame types — language pilot

```text
phoneme_inventory | phoneme_attestation | allophone_rule
| phonotactic_constraint | morpheme_inventory | allomorphy_rule
| paradigm_cell | lexeme_entry | sense_mapping | interlinear_example
| construction_template | valency_frame | argument_marking_pattern
| clause_type | corpus_token_annotation | dependency_edge
| translation_alignment | dialect_variant | language_identity_hypothesis
```

These register as seed rows in `donto_frame_type_registry`
(migration `0116_frame_type_registry.sql`) at M6.

### 13.5 Adapters

| Adapter | Scope | Status |
|---|---|---|
| CLDF importer/exporter | Comparative datasets | Build (M5) |
| Glottolog/registry importer | Language registry bootstrap | Build (M1) |
| WALS / Grambank / PHOIBLE / ValPaL / AUTOTYP / APiCS / SAILS | Via CLDF or source-specific | Build (M5) |
| CoNLL-U | Treebank | Build (M5) |
| UniMorph | Paradigms | Build (M5) |
| LIFT | Lexicon (FieldWorks ecosystem) | Build (M5) |
| ELAN/EAF | Time-aligned media | Build (M5) |
| Praat TextGrid | Phonetic annotation | v1010 |
| TEI dictionary/text | Textual archives | v1010 |
| Generic PDF/grammar | Grammar PDFs | Build (M5; existing PDF chunker as base) |

### 13.6 Pilot success test

Researcher can:

1. Register a language variety (or provisional variety).
2. Ingest registry/comparative data.
3. Ingest one grammar or corpus source.
4. See candidate claims with anchors.
5. See schema alignment suggestions.
6. Review claims and promote maturity.
7. Query across source schemas.
8. See disagreements preserved.
9. Enforce source governance.
10. Publish a policy-compliant release.

---

## 14. Adapter requirements

### 14.1 Adapter contract

```jsonc
{
  "adapter_id": "adapter.cldf.v1",
  "input_formats": ["metadata.json", "csv"],
  "output_objects": ["SourceObject", "SourceVersion", "EvidenceAnchor", "ClaimRecord", "ClaimFrame", "EntityRecord", "PredicateRecord"],
  "policy_requirements": ["source_policy_required"],
  "loss_report_schema": {},
  "round_trip_expectation": "lossless|lossy_declared|import_only|export_only",
  "validation_checks": []
}
```

### 14.2 Loss-report examples

- Export to CSV loses argument graph.
- Export to CoNLL-U loses governance policy except as comments.
- Export to CLDF may lose claim-level contradiction graph unless
  extension tables are used.
- Export to RDF may lose native maturity semantics unless custom
  vocabulary is included.
- Public release redacts anchor locations.

### 14.3 Initial adapter backlog

| Priority | Adapter | Direction | Rationale |
|---|---|---|---|
| P0 | Native donto JSONL | import/export | Debuggable internal interchange |
| P0 | Generic text/PDF | import | Most evidence starts as documents |
| P0 | CSV/TSV | import/export | Minimal structured-data path |
| P1 | CLDF | import/export | Core language pilot interoperability |
| P1 | CoNLL-U | import/export | Token/corpus interoperability |
| P1 | RO-Crate | export | Citable research package |
| P1 | LIFT | import/export | Lexical/fieldwork interoperability |
| P2 | EAF / ELAN | import/export | Time-aligned media |
| P2 | TEI | import/export | Textual/archival |
| P2 | RDF/JSON-LD | import/export | Linked-data bridge |
| P3 | FHIR | later | Medical |
| P3 | Legal citation formats | later | Law |

---

## 15. Review workbench

### 15.1 Required views

1. **Claim card** — claim, evidence, source, anchor, context, policy,
   maturity, confidence, modality, extraction level, related arguments.
2. **Evidence viewer** — source text/image/audio/table at anchor
   location.
3. **Contradiction frontier** — claims under active rebuttal or
   incompatible values.
4. **Predicate alignment queue** — candidate mappings and nearest
   neighbours.
5. **Identity resolution queue** — merge/split/possible-same candidates.
6. **Policy queue** — sources or claims missing policy clarity.
7. **Release readiness view** — blockers before publication.
8. **Reviewer calibration view** — acceptance rates by extractor,
   adapter, source type, domain, reviewer.

### 15.2 Review-decision schema

```jsonc
{
  "review_id": "rev_...",
  "target_type": "claim|alignment|identity|policy|release|anchor|source",
  "target_id": "string",
  "decision": "accept|reject|qualify|request_evidence|merge|split|escalate|mark_sensitive|defer",
  "reviewer_id": "agent_...",
  "review_context": "ctx_...",
  "rationale": "string",
  "confidence": 0.88,
  "created_at": "...",
  "policy_id": "policy_..."
}
```

### 15.3 Reviewer incentives as product design

The reviewer bottleneck is product risk, not operations. v1000 makes
review valuable by:

- Making review decisions citable in releases.
- Showing reviewers downstream impact.
- Tracking reviewer workload and burnout metrics.
- Supporting community-curation workflows.
- Separating expert review from policy-authority review.

---

## 16. Extraction system

### 16.1 Posture

The extractor produces **candidate claims**. Never truth.

The extractor must:

1. Cite anchors for every claim.
2. Record uncertainty and alternative analyses.
3. Emit proof obligations when evidence is weak.
4. Avoid minting new predicates unless needed.
5. Use schema lenses where appropriate.
6. Respect source policy before sending data to any model.
7. Record full run provenance.

### 16.2 Extraction-run object

```jsonc
{
  "run_id": "run_...",
  "run_kind": "adapter_parse|llm_extract|rule_extract|ocr|transcription|translation|embedding|validation",
  "model": {"provider": "string|null", "name": "string|null", "version": "string|null"},
  "prompt_hash": "sha256|null",
  "adapter_id": "string|null",
  "input_version_ids": ["ver_..."],
  "output_claim_ids": ["clm_..."],
  "output_anchor_ids": ["anc_..."],
  "policy_check_id": "chk_...",
  "cost": {"input_tokens": 0, "output_tokens": 0, "usd": 0.0},
  "quality": {},
  "created_at": "..."
}
```

### 16.3 Candidate JSON

```jsonc
{
  "candidates": [
    {
      "candidate_type": "claim|frame|identity|alignment|obligation",
      "subject": "entity_or_literal_or_new_entity",
      "predicate": "predicate_or_new_predicate",
      "object": "entity_or_literal_or_new_entity",
      "frame": null,
      "modality": "descriptive",
      "extraction_level": "quoted",
      "confidence": 0.77,
      "evidence": [{"anchor_kind": "char_span", "locator": {"start": 100, "end": 220}}],
      "uncertainty": "string|null",
      "alternative_analyses": []
    }
  ]
}
```

### 16.4 Acceptance rules

- Candidate with no anchor → quarantine or hypothesis-only.
- Candidate with disallowed policy action → blocked.
- Candidate with novel predicate → predicate-candidate workflow.
- Candidate contradicting existing claim → create argument/obligation,
  do not reject.
- Candidate using uncertain identity → create identity hypothesis.

**donto today.** `donto-api` extraction pipeline (Temporal workflow:
`extract → ingest → align → resolve`). 8-tier prompt at
`helpers.py:64`. Confidence-to-maturity at `helpers.py:39`. No
modality, no extraction level, no policy gate, no candidate-frame
output, no quarantine for malformed candidates.

**Delta.** Substantial:

- Replace 8-tier prompt with domain-dispatched prompt selector
  (`linguistic`, `genealogy`, `medical`, `legal`, `general`).
- Each prompt asks for `modality` and `extraction_level` per fact.
- Decomposer dispatch per domain (`apps/donto-api/decomposers/`).
- Policy-check before external model call (refuse if source policy's
  `allowed_actions.train_model = false` and external model is
  involved).
- Quarantine for malformed candidates (currently silently dropped).
- Maturity ceiling per `extraction_level`.
- Candidate frames (not just atomic claims) supported.

**Verdict.** Extend (substantial work; M5 — extraction kernel).

---

## 17. Release system

### 17.1 Release contract

A release consists of:

1. Release query.
2. Claim set.
3. Evidence manifest.
4. Source manifest.
5. Policy report.
6. Transformation report.
7. Adapter loss report.
8. Checksum manifest.
9. Citation metadata.
10. Reproduction instructions.
11. Optional export packages.

### 17.2 Release blockers

A public release fails if:

- Any included claim has policy disallowing publication.
- Any included source has unresolved policy.
- Any included claim references restricted anchor locations without
  redaction.
- Any included claim is below release maturity threshold.
- Any adapter loss report contains unaccepted critical loss.
- Any required review has not occurred.

### 17.3 Release formats

Native: `donto-release.jsonl` plus `manifest.json`.

Optional exports:

- RO-Crate package.
- CLDF package for language datasets.
- CoNLL-U for corpus releases.
- CSV/TSV for tabular subsets.
- RDF/JSON-LD for linked-data consumers.
- Human-readable HTML report.

**donto today.** None.
**Delta.** Build. M7.
**Verdict.** Build.

---

## 18. Product milestones — M0 through M9

The fresh PRD's milestone sequence is the canonical sequence. v1000
ships M0 → M7 in the next ~12 months; M8 and M9 are research /
hardening phases extending into v1100.

### M0 — Trust Kernel

Goal: no data enters without policy; every sensitive action
audit-visible.

Deliverables:
- PolicyCapsule schema + service.
- Attestation schema + service.
- Access-check middleware in sidecar + query evaluator.
- Audit ledger (extends `donto_audit`).
- Policy admin UI (TUI tab).
- Restricted-read test suite.

Acceptance:
- Missing policy blocks source ingest.
- Unknown policy → restricted.
- Derived claim inherits source policy.
- Public export blocks restricted claims.
- Audit log records all restricted operations.

**Mapped tables:** `donto_policy_capsule`, `donto_access_assignment`,
`donto_attestation`, `donto_event_log` (for governance events). Five
new migrations (`0111`–`0114` plus an event-log migration).

### M1 — Evidence Kernel

Goal: source registration, immutable versions, typed anchors.

Deliverables:
- SourceObject service (extends `donto_document`).
- SourceVersion service (extends `donto_document_revision`).
- EvidenceAnchor validators per anchor kind.
- Object-storage pointer integration (`donto-blob/`).
- Generic text/PDF/CSV adapters with loss reports.

Acceptance:
- Can register source, add OCR version, create char-span and page-box
  anchors.
- Invalid anchor locator fails.
- Version lineage queryable.
- Re-ingestion idempotent.

### M2 — Claim Kernel

Goal: claims and frames with epistemic metadata.

Deliverables:
- ClaimRecord with modality, extraction_level, multi-context, claim_kind.
- ClaimFrame with role index.
- ContextScope with extended kinds and multi-parent.
- Transaction-time history (already there).
- Valid-time expression support (extends `donto_time_expression`).

Acceptance:
- Atomic claims and n-ary frames write.
- As-of queries return prior state.
- Claims without anchors limited to hypothesis-only E1.

### M3 — Schema and Identity Kernel

Goal: predicate alignment + identity hypotheses as product primitives.

Deliverables:
- Predicate registry with minting workflow (refuse without descriptor).
- Predicate nearest-neighbour search.
- AlignmentEdge with 11 relations + safety flags + scope.
- Closure builder with safety-aware expansion.
- EntityRecord with extended kinds.
- IdentityHypothesis with kind enum.
- Identity-lens query parameter in DontoQL.

Acceptance:
- Query expansion respects safety flags.
- Identity lens changes results without deleting original IDs.
- Candidate predicate requires descriptor + nearest-neighbour check.

### M4 — Argument and Review Kernel

Goal: disagreement, obligations, review operational.

Deliverables:
- ArgumentEdge with 9 kinds and per-edge anchors.
- Contradiction-frontier view (extends existing).
- Obligation kinds extended to 9.
- Review decision API with 9 decision types.
- Claim card UI.

Acceptance:
- Contradictory claims appear in frontier.
- Reviewer can accept/reject/qualify with rationale.
- Maturity promotion obeys review rules.

### M5 — Extraction Kernel

Goal: extraction pipeline produces evidence-anchored candidates.

Deliverables:
- Extension of `donto-api` Temporal workflow with domain dispatch.
- New extraction prompt per domain (genealogy, linguistic, papers,
  medical-stub, legal-stub, general).
- Per-domain decomposer.
- Candidate schema validation (refuses malformed).
- Quarantine path for invalid candidates.
- Policy check before external model call.

Acceptance:
- Every candidate has anchor or hypothesis-only flag.
- Policy blocks external calls for restricted sources.
- Run provenance complete.
- Acceptance/rejection metrics collected.

### M6 — Language pilot

Goal: prove the hardest domain.

Deliverables:
- Language entity profile (TUI tab).
- Glottolog + ISO 639 importer.
- CLDF importer/exporter (`donto-ling-cldf`).
- CoNLL-U importer (`donto-ling-ud`).
- UniMorph importer (`donto-ling-unimorph`).
- LIFT importer (`donto-ling-lift`).
- EAF importer minimal (`donto-ling-eaf`).
- 18 language-specific frame types registered.
- Language pilot review screens.

Acceptance:
- Ingest registry + comparative + one grammar/corpus source.
- Cross-schema linguistic claims queryable.
- Disagreements visible and navigable.
- Release-safe language subset producible.

### M7 — Release Builder

Goal: citable, reproducible releases.

Deliverables:
- ReleaseManifest service.
- Query capture.
- Checksum manifest.
- Policy report.
- Loss report.
- Native JSONL export.
- RO-Crate export.
- CLDF release export.

Acceptance:
- Release blocks policy violations.
- Re-running unchanged release reproduces checksums (manifest-stable;
  see §15 of `V1000-REFACTOR-PLAN.md` superseded note for caveat).
- Release package includes citation metadata.

### M8 — Scale and Calibration

Goal: prepare for serious production use.

Deliverables:
- Partitioning strategy.
- Query EXPLAIN tooling.
- Calibration dashboards.
- Reviewer-acceptance metrics.
- Adapter-failure analytics.
- Predicate audit workflow (monthly merge/deprecate).

Acceptance:
- 10M-claim benchmark meets latency target.
- Reviewer acceptance rates calibrate extractor confidence.
- Predicate audit proposes merge/deprecate candidates.

### M9 — Federation Research Spike

Goal: determine whether federated donto is feasible before promising
it.

Deliverables:
- Federation threat model.
- Cross-instance attestation prototype.
- Remote-query redaction prototype.
- Signed-release verification.
- Research memo comparing VC/DID/Solid/SPARQL-federation/DataCite-style.

Acceptance:
- Two toy instances exchange policy-filtered release metadata.
- Cross-instance restricted content cannot leak through counts or
  errors.
- Product decision recorded: proceed, defer, or reject federation.

---

## 19. Agentic-coder task packets

Ten packets, one per kernel/service. Each is a sprint-sized unit.

### Packet A — Define core schema migrations

Create migrations / models for:

```text
donto_policy_capsule
donto_access_assignment
donto_attestation
donto_event_log
(extensions to donto_document)
(extensions to donto_document_revision)
(extensions to donto_statement: hypothesis_only, claim_kind, primary_context)
donto_statement_modality
donto_statement_extraction_level
donto_statement_context (junction)
(extensions to donto_confidence)
donto_claim_frame
donto_frame_role
donto_review_decision
(extensions to donto_argument)
(extensions to donto_proof_obligation)
(extensions to donto_predicate_alignment)
(extensions to donto_predicate_descriptor)
(extensions to donto_entity_symbol → donto_entity)
(extensions to donto_identity_hypothesis)
donto_dataset_release
donto_anchor_kind_registry
donto_frame_type_registry
donto_alignment_value_mapping
```

Acceptance:
- Cannot create source without policy.
- Cannot create active claim without anchor unless `hypothesis_only=true`.
- Cannot delete/overwrite historical claim event.
- Derived claim inherits source policy.

### Packet B — Policy enforcement middleware

Single policy-check path used by all reads, extractions, exports,
embeddings, releases.

Acceptance:
- Endpoint tests for allowed/denied actions.
- Aggregation-leakage tests.
- Attestation expiry/revocation tests.
- Derived-policy inheritance tests.

### Packet C — Source and anchor services

Source registration, versioning, anchor creation, anchor validators.

Acceptance:
- Char span in range passes.
- Char span out of range fails.
- Page box validates normalised coordinates.
- Media time validates start/end and source duration.
- Table cell validates row/column when parsed table available.

### Packet D — Claim and frame writer

Claim write API, frame write API, context assignment, transaction-time
history, policy propagation.

Acceptance:
- Atomic claim write with anchor succeeds.
- Frame write with indexed roles succeeds.
- Correction creates new event and closes prior current view.
- As-of query returns prior state.

### Packet E — Predicate and alignment workbench

Predicate registry, nearest-neighbour lookup, candidate minting,
alignment creation, closure rebuild, query expansion.

Acceptance:
- New predicate without descriptor rejected.
- Alignment unsafe for logical inference does not appear in inference
  expansion.
- Strict mode: only exact predicate.
- Safe mode: includes safe alignments.

### Packet F — Identity-lens service

Entity records, identity hypotheses, merge/split candidates, query-time
lens resolution.

Acceptance:
- Same query under strict vs exploratory lens returns different
  groupings.
- Rejected hypothesis no longer participates in accepted lens.
- Original entity IDs queryable after accepted merge.

### Packet G — Argument frontier and obligations

Argument edges, contradiction detection, obligation engine, review
state.

Acceptance:
- Mutually exclusive categorical values produce frontier entry.
- Claim with low OCR anchor produces `needs_anchor_repair`.
- Rebuttal edge appears on both claim cards.

### Packet H — Extraction pipeline

Extraction-run object, adapter runner, chunk queue, model interface,
candidate-schema validation, quarantine.

Acceptance:
- Restricted source blocks external model call.
- Malformed candidate goes to quarantine.
- Every generated claim links to run ID and anchor.
- Prompt/model metadata preserved.

### Packet I — Language pilot adapters

Minimal CLDF, CoNLL-U, UniMorph, generic grammar-text paths.

Acceptance:
- CLDF ValueTable rows become source-scoped claim records.
- CoNLL-U tokens become token frames/annotations.
- UniMorph rows become inflection frames.
- Round-trip exports produce declared loss reports.

### Packet J — Release builder

Release-query capture, policy report, checksums, native JSONL export,
RO-Crate export, citation metadata.

Acceptance:
- Policy violation blocks public release.
- Identical release re-run has identical checksums.
- Release manifest lists all source versions and transformations.

---

## 20. API sketch

### Source APIs

```http
POST   /v1/sources
POST   /v1/sources/{source_id}/versions
GET    /v1/sources/{source_id}
GET    /v1/sources/{source_id}/versions
POST   /v1/anchors
GET    /v1/anchors/{anchor_id}
```

### Claim APIs

```http
POST   /v1/claims
POST   /v1/claims/batch
GET    /v1/claims/{claim_id}
POST   /v1/frames
GET    /v1/frames/{frame_id}
POST   /v1/claims/{claim_id}/correct
POST   /v1/claims/{claim_id}/retract
```

### Policy APIs

```http
POST   /v1/policies
GET    /v1/policies/{policy_id}
POST   /v1/attestations
POST   /v1/attestations/{attestation_id}/revoke
POST   /v1/policy/check
GET    /v1/audit/restricted
```

### Schema/identity APIs

```http
POST   /v1/predicates
GET    /v1/predicates/nearest
POST   /v1/alignments
POST   /v1/alignments/rebuild-closure
POST   /v1/entities
POST   /v1/identity-hypotheses
GET    /v1/identity-lenses/{lens_id}/resolve/{entity_id}
```

### Review APIs

```http
GET    /v1/review/queue
POST   /v1/review/decisions
GET    /v1/frontier/contradictions
GET    /v1/obligations
POST   /v1/obligations/{id}/resolve
```

### Extraction APIs

```http
POST   /v1/extraction/runs
POST   /v1/extraction/jobs
GET    /v1/extraction/jobs/{job_id}
GET    /v1/extraction/runs/{run_id}/candidates
```

### Release APIs

```http
POST   /v1/releases/dry-run
POST   /v1/releases
GET    /v1/releases/{release_id}
GET    /v1/releases/{release_id}/manifest
GET    /v1/releases/{release_id}/download
```

Old endpoints from current `dontosrv` (`/assert`, `/retract`, `/sparql`,
`/dontoql`, `/claim/:id`, etc.) and current `donto-api` (`/jobs/extract`,
`/firehose/stream`, `/papers/ingest`, etc.) continue to work via alias
router for one release window. v1100 removes aliases.

---

## 21. Testing strategy

### 21.1 Invariant tests in CI

- No source without policy.
- No claim without evidence except `hypothesis_only`.
- No public export of restricted claim.
- Derived policy at least as restrictive as source policy.
- Corrections preserve transaction-time history.
- Contradictory claims coexist.
- Query expansion respects alignment safety flags.
- Identity lens never deletes original entity.

### 21.2 Adapter tests

For each adapter:
- Golden fixture import.
- Malformed fixture quarantine.
- Idempotent re-import.
- Export loss report.
- Round-trip where promised.

### 21.3 Policy red-team tests

Simulate leakage attempts through:
- Counts.
- Search snippets.
- Anchor locations.
- Error messages.
- Embedding nearest neighbours.
- Release manifests.
- Reviewer queues.
- Logs.

### 21.4 Extraction quality tests

- Candidate precision by source type.
- Anchor accuracy.
- Predicate reuse rate.
- Novel-predicate false-positive rate.
- Review-acceptance rate.
- Calibration error.

### 21.5 Scale tests

Benchmarks at 1M, 10M, 100M synthetic, 1B design simulation.

Measured operations:
- Claim lookup.
- Evidence drill-down.
- Contradiction-frontier rebuild.
- Alignment-closure rebuild.
- Identity-lens resolution.
- Release build.

**Existing donto invariant tests** (preserve):
`invariants_paraconsistency.rs`, `invariants_bitemporal.rs`,
`invariants_migration_idempotent.rs`, `scope.rs`, `assert_match.rs`.

---

## 22. Metrics

### Product metrics

- Sources ingested.
- % of sources with explicit policy.
- Claim count by maturity.
- % of claims with valid anchors.
- Review throughput.
- Claim accept/reject/qualify rate.
- Contradiction-frontier size.
- Median time candidate → reviewed.
- Release count and reproducibility rate.

### Governance metrics

- Restricted-read attempts allowed/denied.
- Policy violations blocked.
- Sources pending authority review.
- Derived records with inherited restrictions.
- Release blockers by policy category.
- Audit completeness.

### Extraction metrics

- Cost per accepted claim.
- Accepted claims per source type.
- Anchor error rate.
- Predicate novelty rate.
- Predicate merge/deprecation rate.
- Calibration error by extractor and domain.

### Research-value metrics

- Questions answered that previously required manual cross-source
  reconciliation.
- Cross-schema query success rate.
- Disagreements discovered.
- Releases cited.
- External datasets imported with useful loss reports.

---

## 23. Risks and mitigations

| ID | Risk | Mitigation |
|---|---|---|
| R1 | Reviewer bottleneck | Prioritise high-value obligations; active learning; citable decisions; reviewer-load tracking; calibration. |
| R2 | Governance misuse | Steering group before compliance claims; community/institution-specific policy modules; fail-closed default; independent audits; plain-language summaries. |
| R3 | Predicate explosion | Candidate predicate queue; descriptor requirement; nearest-neighbour check; monthly vocabulary audit; merge/deprecate workflow. |
| R4 | False schema alignment | Typed alignment relations; safety flags for query/export/inference; alignment review queue; loss reports on exports. |
| R5 | Entity-resolution overconfidence | Identity lenses; confidence thresholds; competing hypotheses; split/merge audit; strict lens by default. |
| R6 | Policy leakage through derived data | Derived-policy propagation; restricted aggregation checks; embedding policy gate; public-release blocker; red-team leakage tests. |
| R7 | External model dependency | Provider abstraction; local/offline model path; run provenance; policy blocks external calls where necessary. |
| R8 | Adapter trap | Native model first; adapter loss reports; core tests not written in adapter terms; every adapter replaceable. |
| R9 | Performance collapse | Materialised safe closures; indexed policy-visibility sets; query EXPLAIN; partitioning; release precomputation. |
| R10 | Market/category confusion | Lead with evidence workflows and use cases; demonstrate contradictions and governance, not graph diagrams; publish high-quality pilot releases. |

---

## 24. Research backlog

These tasks should be assigned to deep-research agents or domain
experts before v2000 commitments.

| ID | Topic |
|---|---|
| RB1 | Paraconsistency at production scale |
| RB2 | Technical Indigenous/community data governance |
| RB3 | Cross-dataset linguistic querying |
| RB4 | UD and alternative analyses |
| RB5 | Entity resolution for sparse scholarly entities |
| RB6 | Healthcare vocabulary alignment as prior art |
| RB7 | Temporal uncertainty modelling |
| RB8 | Federated evidence systems |
| RB9 | Restricted-data computation (DP, TEE, federated analytics) |
| RB10 | Release durability (DOI, RO-Crate, Software Heritage, content-addressed) |

Detailed prompts in `ATLAS-ZERO-FRONTIER.md` §8 (preserved as historical
reference; replace "Atlas Zero" with "donto" when re-using).

---

## 25. External standards and references

donto uses external standards as references and adapters; the native
architecture is not constrained by them.

### Linguistic / language-data references

CLDF; Glottolog; WALS; Grambank; AUTOTYP; PHOIBLE; ValPaL; APiCS;
SAILS; Universal Dependencies; UniMorph; OntoLex-Lemon; LexInfo;
ELAN/EAF; LIFT; TEI; Praat TextGrid.

### Governance and release references

FAIR Principles; CARE Principles; Local Contexts TK and BC Labels;
AIATSIS-style ethics; RO-Crate; DataCite Metadata Schema; W3C
Verifiable Credentials; W3C DID Core.

### Provenance, validation, schema references

PROV-O; SHACL; LinkML; SKOS; RDF/JSON-LD; Allen interval algebra;
TimeML; EDTF.

---

## 26. v1000 definition of done

donto v1000 is done when all of the following hold:

1. A source cannot be ingested without policy classification.
2. A restricted source cannot leak through extraction, embedding,
   query, export, or release.
3. A researcher can ingest a source and see evidence-anchored
   candidate claims.
4. Claims store context, polarity, modality, extraction level,
   confidence, maturity, valid time, transaction time, evidence
   anchors, policy, and creator/run provenance.
5. Contradictory claims coexist and appear in the contradiction
   frontier.
6. Predicate alignment works with typed safety flags.
7. Identity hypotheses can be queried under separate lenses.
8. Review decisions promote or block maturity according to rules.
9. The language pilot imports at least one registry/comparative source
   and one primary source.
10. Release builder produces a reproducible native release package
    with policy report and checksums.
11. Adapter exports produce loss reports.
12. The UI supports claim cards, evidence viewing, review queue,
    policy administration, contradiction frontier, release readiness.
13. CI covers all non-negotiable invariants.

---

## 27. Product north-star

donto succeeds if a researcher can ask a contested question and
receive an answer that says:

1. Here are the claims.
2. Here is the evidence for each claim.
3. Here is who or what produced each claim.
4. Here is which source, schema, context, time, and identity lens each
   claim depends on.
5. Here is where the sources disagree.
6. Here is which claims are reviewed, corroborated, or certified.
7. Here is what cannot be shown because of governance policy.
8. Here is the reproducible release artefact.

donto is not a machine that decides what is true. donto is a system
that makes contested knowledge inspectable, governable, reviewable,
and reproducible.

---

## Appendix A. Implementation data dictionary

### Tables (donto v1000 superset)

```text
donto_context                       (existing, extended kinds)
donto_context_parent                (NEW: multi-parent support)
donto_statement                     (existing, extended)
donto_stmt_lineage                  (existing)
donto_audit                         (existing, extended)
donto_event_log                     (NEW: append-only events for non-statement objects)

donto_document                      (existing → SourceObject, extended)
donto_document_revision             (existing → SourceVersion, extended)
donto_span                          (existing → EvidenceAnchor, extended)
donto_content_regions               (existing, extended)
donto_anchor_kind_registry          (NEW: 13 kinds with locator schemas)
donto_evidence_link                 (existing)

donto_claim_frame                   (NEW)
donto_frame_role                    (NEW)
donto_frame_type_registry           (NEW)

donto_statement_modality            (NEW: overlay)
donto_statement_extraction_level    (NEW: overlay)
donto_statement_context             (NEW: junction for multi-context)
donto_confidence                    (existing, extended to multivalue)

donto_predicate                     (existing)
donto_predicate_descriptor          (existing, extended for minting status)
donto_predicate_alignment           (existing, extended to v2)
donto_alignment_value_mapping       (NEW)
donto_predicate_closure             (existing)
donto_canonical_shadow              (existing)

donto_entity                        (NEW: extends donto_entity_symbol)
donto_entity_mention                (existing)
donto_entity_signature              (existing)
donto_identity_edge                 (existing)
donto_identity_hypothesis           (existing, extended)

donto_argument                      (existing, extended to v2)
donto_proof_obligation              (existing, extended kinds)

donto_review_decision               (NEW)
donto_review_history                (NEW: append-only)

donto_policy_capsule                (NEW)
donto_access_assignment             (NEW)
donto_attestation                   (NEW)

donto_extraction_run                (existing, extended)
donto_extraction_chunk              (existing)

donto_dataset_release               (NEW)
donto_release_artifact              (NEW)

donto_adapter_run                   (NEW: per-run loss reports)
donto_loss_report                   (NEW: parsed loss-report payload)

donto_agent                         (existing)
donto_agent_binding                 (existing)
```

### Required indexes (additions)

```text
statement(primary_context, predicate)
statement(modality)
statement(extraction_level)
statement(claim_kind)
statement(hypothesis_only) WHERE hypothesis_only = true
context(context_kind)
context(policy_id)
anchor(version_id, anchor_kind)
alignment(left_ref, relation, scope)
alignment(right_ref, relation, scope)
alignment(safe_for_query_expansion, safe_for_export, safe_for_logical_inference)
identity_hypothesis(hypothesis_kind, status)
argument(argument_kind, from_claim_id)
review_decision(target_type, target_id)
attestation(holder_agent_id, policy_id, expires_at)
access_assignment(target_kind, target_id)
audit_event(actor_id, action, created_at)
event_log(target_kind, target_id, occurred_at)
```

### Required current-state views

```text
donto_v_current_claim
donto_v_visible_claim_for_agent(agent_id)
donto_v_claim_card                  (extends existing donto_claim_card)
donto_v_contradiction_frontier
donto_v_release_eligible_claim
donto_v_policy_inheritance_graph
donto_v_review_queue
donto_v_identity_lens_resolution
donto_v_schema_expansion_safe
```

---

## Appendix B. First-sprint checklist

The first sprint does not start with LLM extraction. It establishes
the trust and evidence substrate.

Day-one build order:

1. Schema migrations for policies, sources, versions, anchors, claims,
   audit events.
2. Enforce policy-required source registration.
3. Anchor validators for char spans and whole-source anchors.
4. Claim writer with evidence requirement.
5. Claim card endpoint.
6. Restricted/public policy test fixtures.
7. One generic text-source adapter.
8. One native JSONL release export.
9. CI invariant tests.
10. Only then add model extraction.

This sequence corresponds to M0 → M1 → start of M2.

---

## Appendix C. External references snapshot

Checked during this PRD's drafting on 2026-05-06.

- CLDF 1.3 — https://github.com/cldf/cldf
- Glottolog 5.3 — https://glottolog.org/
- Universal Dependencies — https://universaldependencies.org/download.html
- RO-Crate 1.2 — https://www.researchobject.org/ro-crate/specification/1.2/index.html
- WALS Online — https://wals.info/
- Grambank — https://grambank.clld.org/
- CARE Principles — https://datascience.codata.org/articles/10.5334/dsj-2020-043
- Local Contexts Labels — https://localcontexts.org/labels/about-the-labels/
- PROV-O — https://www.w3.org/TR/prov-o/
- SHACL 1.2 Core — https://www.w3.org/TR/shacl12-core/
- LinkML — https://linkml.io/
- W3C Verifiable Credentials Data Model 2.0 — https://www.w3.org/TR/vc-data-model-2.0/
- W3C DID Core — https://www.w3.org/TR/did-core/
- DataCite Metadata Schema 4.6 — https://schema.datacite.org/meta/kernel-4.6/
- FAIR Principles — https://www.nature.com/articles/sdata201618

---

## Appendix D. Migration index — donto v1000 schema additions

```
0089 hypothesis_only_flag
0090 event_log
0091 argument_relations_v2
0092 alignment_relations_v2
0093 identity_hypothesis_kind
0094 dataset_release
0095 source_object_extension
0096 source_version_extension
0097 anchor_kind_registry
0098 polarity_v2
0099 statement_modality
0100 extraction_level
0101 confidence_multivalue
0102 maturity_e_naming
0103 multi_context
0104 claim_kind
0105 claim_frame
0106 frame_role
0107 context_multi_parent
0108 entity_extension
0109 identity_hypothesis_v2
0110 predicate_minting
0111 policy_capsule
0112 attestation
0113 obligation_kinds_v2
0114 review_decision
0115 query_v2_metadata
0116 frame_type_registry
```

28 new migrations across 10 milestones. Each is idempotent
(`if not exists`) and entered into the migration ledger
(`donto-client/src/migrations.rs::MIGRATIONS`).

---

*End of canonical PRD. Open work begins at M0 (Trust Kernel). All
prior planning documents in `docs/` are historical; this is the
single reference of record.*
