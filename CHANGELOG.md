# Changelog

All notable changes to donto are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### v1000 foundation (PRD-V1000-001)

donto's transition into an evidence operating system for contested
knowledge. See `docs/DONTO-V1000-PRD.md` for the canonical PRD.

#### Schema additions (28 migrations, 0089–0116)

- **Trust Kernel (M0):** `donto_policy_capsule` with 15 typed actions
  (read_metadata, read_content, quote, view_anchor_location,
  derive_claims, derive_embeddings, translate, summarize, export_*,
  train_model, publish_release, share_with_third_party,
  federated_query); `donto_access_assignment` for target→policy
  binding with max-restriction inheritance; `donto_attestation` for
  caller authorization with revocation, expiry, and audit. Default
  policies: `policy:default/public`,
  `policy:default/restricted_pending_review`,
  `policy:default/community_restricted`,
  `policy:default/private_research`. Functions:
  `donto_assign_policy`, `donto_effective_actions`,
  `donto_action_allowed`, `donto_issue_attestation`,
  `donto_revoke_attestation`, `donto_authorise`.
- **Hypothesis-only flag (I1):** `donto_stmt_hypothesis_only` overlay
  + `donto_can_promote_maturity` gate.
- **Append-only event log (I3):** `donto_event_log` for non-statement
  objects (alignments, identities, policies, attestations, reviews,
  releases, frames).
- **Argument relations v2 (I4):** extended `donto_argument` with
  `alternative_analysis_of`, `same_evidence_different_analysis`,
  `same_claim_different_schema`, `explains`, `supersedes`. Added
  `review_state`, `evidence_anchor_ids`.
- **Alignment relations v2 (I7):** extended
  `donto_predicate_alignment` to 11 relations (including
  `broad_match`, `has_value_mapping`, `derived_from`,
  `local_specialization`); added `safe_for_query_expansion`,
  `safe_for_export`, `safe_for_logical_inference`, `scope`,
  `review_status`, `evidence_anchor_ids`. New
  `donto_alignment_value_mapping` for has-value-mapping payloads.
- **Identity hypothesis v2 (I8):** new `donto_identity_proposal`
  table with kinds `same_as | different_from | broader_than |
  narrower_than | split_candidate | merge_candidate | successor_of |
  alias_of`. Existing `donto_identity_hypothesis` extended with
  `method`, `authority`, `provenance_proposal_id`.
- **Release builder (I10):** `donto_dataset_release` with policy
  report, source manifest, transformation manifest, checksum
  manifest, citation metadata, reproducibility status.
  `donto_release_artifact` for per-format outputs.
- **Source object extension (§6.1):** `donto_document` extended with
  `source_kind`, `creators`, `source_date` (EDTF), `registered_by`,
  `policy_id`, `content_address`, `native_format`, `adapter_used`,
  `status`. New `donto_register_source_v1000` enforces policy
  presence (PRD I2 — no source without policy).
- **Source version extension (§6.2):** `donto_document_revision`
  extended with `version_kind`, `quality_metrics`,
  `derived_from_versions`, `created_by`. New
  `donto_revision_lineage` walker.
- **Anchor kind registry (§6.3):** `donto_anchor_kind` with seed
  vocabulary of 13 kinds (whole_source, char_span, page_box,
  image_box, media_time, table_cell, csv_row, json_pointer,
  xml_xpath, html_css, token_range, annotation_id, archive_field).
  Per-kind locator validators.
- **Polarity v2 (§6.4):** view `donto_v_statement_polarity_v1000`
  surfaces derived `conflicting` polarity without storing a fifth
  bit value.
- **Statement modality (§7.4):** `donto_stmt_modality` overlay with
  14 modality values.
- **Extraction levels (§7.3):** `donto_stmt_extraction_level`
  overlay with 10 levels and per-level
  `donto_max_auto_maturity` ceiling.
- **Multi-value confidence (§7.2):** `donto_stmt_confidence`
  extended with `calibrated_confidence`, `human_confidence`,
  `source_reliability_weight`, plus `donto_confidence_lens`
  resolver.
- **Maturity ladder E0–E5 (§7.1):** rename helpers and a new tier
  E4 "Corroborated" between Reviewed and Certified, all stored in
  the existing 3-bit field. Backwards-compatible.
- **Multi-context membership (§6.4):** `donto_statement_context`
  junction for additional contexts beyond `donto_statement.context`.
- **Claim kind overlay (§6.4):** `donto_stmt_claim_kind` with
  values atomic | frame_summary | absence | identity | alignment |
  policy | review | validation.
- **Claim frames + roles (§6.5):** `donto_claim_frame` and
  `donto_frame_role` for n-ary structured analyses with indexed
  role values.
- **Multi-parent contexts (§6.6):** `donto_context_parent` junction
  with role-typed parents (inherit | lens | governance | review |
  release).
- **Entity extension (§6.7):** `donto_entity_symbol` extended with
  `entity_kind`, `external_ids`, `identity_status`, `policy_id`. New
  `donto_entity_label` for multilingual labels.
- **Predicate minting (§6.9):** `donto_predicate_descriptor`
  extended with `minting_status`, `nearest_existing_at_mint`,
  `source_schema`, `definition`. `donto_mint_predicate_candidate`
  refuses without descriptors and nearest-neighbour record;
  `donto_predicate_is_approved` gates production use.
- **Review decisions (FR-012):** `donto_review_decision` with 9
  decision types (accept, reject, qualify, request_evidence, merge,
  split, escalate, mark_sensitive, defer) and rationale-required
  constraint. `donto_review_queue` view.
- **Obligation kinds v2 (FR-011):** extended
  `donto_proof_obligation` with v1000 kinds (`needs_evidence`,
  `needs_policy`, `needs_review`, `needs_identity_resolution`,
  `needs_alignment_review`, `needs_anchor_repair`,
  `needs_contradiction_review`, `needs_formal_validation`,
  `needs_community_authority`) and a `blocked` status. v0 kinds
  preserved.
- **Frame type registry (FR-005):** `donto_frame_type` with seed
  vocabulary covering the 18 PRD §13.4 language-pilot frame types
  plus 6 cross-domain frames (medicine, law, science, governance).
- **Query language v2 metadata (FR-015):** `donto_query_clause_v1000`
  records the v1000 clause vocabulary including new clauses
  (MODALITY, EXTRACTION_LEVEL, IDENTITY_LENS, SCHEMA_LENS,
  REVIEW_STATE, POLICY_ALLOWS, AS_OF, etc.) for parser/evaluator
  reference.

#### Documentation

- New canonical PRD: `docs/DONTO-V1000-PRD.md`. Replaces the
  three planning docs (`docs/LANGUAGE-EXTRACTION-PLAN.md`,
  `docs/V1000-REFACTOR-PLAN.md`,
  `docs/ATLAS-ZERO-FRONTIER.md`) which remain on disk as historical
  artefacts.

#### Notes

- All 28 migrations are idempotent. Existing data is unaffected;
  new columns default safely.
- M0 (Trust Kernel) lands the schema and SQL functions for
  policy/attestation/audit. Sidecar middleware that enforces these
  at the HTTP layer is the next milestone.
- The fresh research PRD is preserved at
  `docs/DONTO-V1000-PRD.md` and supersedes earlier planning.

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
