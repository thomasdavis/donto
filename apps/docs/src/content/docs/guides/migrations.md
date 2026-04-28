---
title: Migration Reference
description: Complete reference for all 43 donto migrations
---

Complete reference for all 43 migrations. Each migration is an
idempotent SQL script tracked by SHA256 hash in `donto_migration`.

## Migration Timeline

| Phase | Migrations | Date | Summary |
|-------|-----------|------|---------|
| Phase 0: Core | 0001-0011 | 2026-04-17 | Statement atom, contexts, flags, functions, presets, predicates, snapshots, shapes, rules, certificates, observability |
| Phase 1: Fixes | 0012 | 2026-04-17 | Scope matching fix |
| Phase 2: Alexandria | 0013-0022 | 2026-04-19 | Trigram search, retrofit, shape annotations, valid-time buckets, reactions, aggregates, FTS, bitemporal canonicals, SameMeaning, context env |
| Phase 3: Evidence Substrate | 0023-0033 | 2026-04-24 | Documents, revisions, spans, annotations, annotation edges, extraction runs, evidence links, agents, arguments, proof obligations, vectors |
| Phase 3b: Claim Card | 0034 | 2026-04-24 | Claim card assembly, why-not-higher maturity blocker analysis |
| Phase 3c: Schema Gaps | 0035-0043 | 2026-04-24 | Document sections, mentions, extraction chunks, confidence, units, temporal expressions, content regions, entity aliases, candidate contexts |

## Phase 0: Core (0001-0011)

### 0001_core — Statement atom, contexts, lineage, audit

The foundation. Creates the three core tables that everything else builds on.

**Tables:**
- `donto_context` — Named graphs. IRI primary key. 10 kinds (source, snapshot, hypothesis, user, pipeline, trust, derivation, quarantine, custom, system). 2 modes (permissive, curated). Parent links form a forest.
- `donto_statement` — The atom. UUID primary key. Subject/predicate/object (IRI or typed literal JSON), context FK, `tx_time` tstzrange, `valid_time` daterange, `flags` smallint (polarity + maturity). Content hash for idempotent re-ingestion. Never deleted — retraction closes `tx_time`.
- `donto_stmt_lineage` — Sparse overlay tracking derivation inputs.
- `donto_audit` — Audit log for assert/retract/correct/retrofit.

**Seeds:** `donto:anonymous` default context.

**Extensions:** `btree_gist`, `pgcrypto`.

### 0002_flags — Polarity and maturity packing

**Functions:**
- `donto_pack_flags(polarity, maturity)` -> smallint
- `donto_polarity(flags)` -> text
- `donto_maturity(flags)` -> int

**Flag layout:** bits 0-1 polarity (asserted/negated/absent/unknown), bits 2-4 maturity (0-4), bits 5-15 reserved.

### 0003_functions — Core SQL API

**Functions:**
- `donto_ensure_context(iri, kind, mode, parent)` — Idempotent context creation
- `donto_resolve_scope(scope_json)` -> context IRIs — Recursive CTE with include/exclude/descendants/ancestors/kind_filter
- `donto_assert(subject, predicate, object_iri, object_lit, context, polarity, maturity, valid_lo, valid_hi, actor)` -> UUID
- `donto_assert_batch(json_array, actor)` -> count
- `donto_retract(statement_id, actor)` -> boolean
- `donto_correct(statement_id, new_*, actor)` -> UUID
- `donto_match(subject, predicate, object_*, scope, polarity, min_maturity, as_of_tx, as_of_valid)` -> rows

### 0004_migrations — Migration ledger

**Tables:**
- `donto_migration` — Name + SHA256 hash + applied_at. Seeds itself with 0001-0003 on first run.

### 0005_presets — Named scope presets

**Tables:**
- `donto_scope_preset` — Name -> scope JSON descriptor.

**Seeds:** `anywhere`, `raw`, `curated`, `latest`.

**Functions:**
- `donto_define_preset(name, scope, description)`
- `donto_preset_scope(name)` -> jsonb
- `donto_scope_under_hypothesis(hypo_iri)` -> scope jsonb
- `donto_scope_as_of(snapshot_iri)` -> scope jsonb

### 0006_predicate — Predicate registry

**Tables:**
- `donto_predicate` — IRI, canonical_of, label, description, domain, range, inverse_of, is_symmetric/transitive/functional, cardinality, status (active/deprecated/merged/implicit).
- `donto_datatype` — IRI + label + base. Seeded with XSD types.
- `donto_prefix` — Compact IRI prefixes. Seeded with rdf/rdfs/owl/xsd/donto.

**Functions:**
- `donto_register_predicate(...)` — Single-hop alias chains enforced
- `donto_canonical_predicate(iri)` -> canonical IRI
- `donto_implicit_register(iri)` — Auto-register in permissive contexts
- `donto_match_canonical(...)` — Query-time alias expansion

### 0007_snapshot — Frozen membership snapshots

**Tables:**
- `donto_snapshot` — IRI, base_scope, captured_tx_time, member_count.
- `donto_snapshot_member` — (snapshot_iri, statement_id).

**Functions:**
- `donto_snapshot_create(iri, scope, note)` — Freeze current visible statements
- `donto_match_in_snapshot(...)` — Query against frozen membership
- `donto_snapshot_drop(iri)`

### 0008_shape — Shape catalog and reports

**Tables:**
- `donto_shape` — IRI, severity (info/warning/violation), body_kind (builtin/lean/dir), body JSON.
- `donto_shape_report` — Cached evaluation results with scope fingerprint.
- `donto_stmt_shape_reports` — Per-statement report attachment.

### 0009_rule — Derivation rules

**Tables:**
- `donto_rule` — IRI, body_kind, body, output_ctx, mode (eager/batch/on_demand).
- `donto_derivation_report` — Cached derivation results.

### 0010_certificate — Certificates

**Tables:**
- `donto_stmt_certificate` — statement_id, kind (7 types), rule_iri, inputs UUID[], body JSON, signature, verification state.

**Functions:**
- `donto_attach_certificate(...)` — Upsert, clears prior verification
- `donto_record_verification(...)` — Record verifier outcome

### 0011_observability — Stats tables

**Tables:**
- `donto_stats_context`, `donto_stats_predicate`, `donto_stats_maturity`, `donto_stats_shape`, `donto_stats_rule`, `donto_stats_audit` — Aggregate counters for operational visibility.

## Phase 1: Fixes (0012)

### 0012_match_scope_fix — Scope resolution refinement

Patches `donto_resolve_scope` to honor `kind_filter` and `exclude_kind` fields used by seeded presets. Backward compatible.

## Phase 2: Alexandria Extensions (0013-0022)

### 0013_search_trgm — Trigram search

Adds `pg_trgm` extension and trigram indexes on statement subjects for fuzzy search.

### 0014_retrofit — Backdated ingestion

**Tables:** `donto_retrofit` — statement_id, reason, actor, timestamp.

**Functions:** `donto_assert_retrofit(...)` — Requires explicit valid_time and reason.

### 0015_shape_annotations — Per-statement shape verdicts

**Tables:** `donto_stmt_shape_annotation` — Bitemporal (tx_time lifecycle). verdict in (pass, warn, violate). At most one open per (statement, shape).

**Functions:**
- `donto_attach_shape_report(stmt, shape, verdict, context, detail)` — Idempotent close-and-reopen
- `donto_has_shape_verdict(stmt, verdict, shape)` -> boolean

### 0016_valid_time_buckets — Temporal aggregation

**Functions:** `donto_valid_time_buckets(interval, epoch, predicate, subject, scope)` — Time-binned statement counts.

### 0017_reactions — Meta-statements

Registers `donto:endorses`, `donto:rejects`, `donto:cites`, `donto:supersedes` predicates.

**Functions:**
- `donto_stmt_iri(uuid)` / `donto_stmt_iri_to_id(iri)` — UUID<->IRI conversion
- `donto_react(source_stmt, kind, object, context, actor)` -> UUID
- `donto_reactions_for(stmt)` -> rows

### 0018_aggregates — Endorsement weights

**Functions:**
- `donto_compute_endorsement_weights(scope, into_ctx, actor)` -> count
- `donto_weight_of(stmt, scope)` -> int

### 0019_fts — Full-text search

**Functions:**
- `donto_lang_to_regconfig(lang)` -> regconfig
- `donto_stmt_lit_tsv(object_lit)` -> tsvector
- `donto_match_text(query, lang, scope, predicate, polarity, maturity)` -> rows with score

No index created by default (would lock large tables). Operators run `CREATE INDEX CONCURRENTLY` when ready.

### 0020_bitemporal_canonicals — Time-dependent aliases

**Tables:** `donto_predicate_alias` — (alias, canonical, valid_time daterange).

**Functions:**
- `donto_register_alias_at(alias, canonical, valid_lo, valid_hi, actor)`
- `donto_canonical_predicate_at(iri, as_of_date)` -> canonical

### 0021_same_meaning — Parallel-literal alignment

**Functions:**
- `donto_align_meaning(stmt_a, stmt_b, context, actor)` — Emits both directions
- `donto_meaning_cluster(stmt, scope)` -> statement_ids — Recursive transitive closure

### 0022_context_env — Environment overlays

**Tables:** `donto_context_env` — (context, key, value jsonb). Advisory only.

**Functions:**
- `donto_context_env_set/get/delete(context, key, ...)`
- `donto_contexts_with_env(required_pairs)` -> context IRIs

## Phase 3: Evidence Substrate (0023-0033)

### 0023_documents — Document objects

**Tables:** `donto_document` — IRI (unique), media_type, label, source_url, language.

### 0024_document_revisions — Immutable content snapshots

**Tables:** `donto_document_revision` — document FK, revision_number (auto-increment), body text, body_bytes, content_hash (SHA256), parser_version.

### 0025_spans — Standoff annotations

**Tables:** `donto_span` — revision FK, span_type (char_offset/token/sentence/paragraph/page/line/region/xpath/css/custom), start_offset, end_offset, selector jsonb, surface_text.

### 0026_annotations — Feature-value pairs on spans

**Tables:**
- `donto_annotation_space` — IRI (unique), label, feature_ns, version.
- `donto_annotation` — span FK, space FK, feature, value, value_detail jsonb, confidence, run FK.

### 0027_annotation_edges — Structural relations

**Tables:** `donto_annotation_edge` — source/target annotation FKs, space FK, relation. No self-links.

### 0028_extraction_runs — Provenance

**Tables:** `donto_extraction_run` — model_id, model_version, prompt_hash, prompt_template, chunking_strategy, temperature, seed, toolchain jsonb, source_revision FK, context FK, status, timestamps, emit counts.

### 0029_evidence_links — Statement-evidence binding

**Tables:** `donto_evidence_link` — statement FK, link_type (7 types), polymorphic target, confidence, context, tx_time (bitemporal).

### 0030_agents — Agent registry

**Tables:**
- `donto_agent` — IRI (unique), agent_type (human/llm/rule_engine/extractor/validator/curator/system/custom).
- `donto_agent_context` — (agent, context) with role (owner/contributor/reader).

### 0031_arguments — Argumentation framework

**Tables:** `donto_argument` — source/target statement FKs, relation (9 types), strength [0,1], context FK, agent FK, tx_time (bitemporal).

### 0032_proof_obligations — Extraction work items

**Tables:** `donto_proof_obligation` — statement FK, obligation_type (10 types), status (open/in_progress/resolved/rejected/deferred), priority, assigned_agent FK.

### 0033_vectors — Embedding layer

**Tables:** `donto_vector` — subject_type, subject_id, model_id, model_version, dimensions, embedding float4[].

## Phase 3b: Claim Card (0034)

### 0034_claim_card — Epistemic state assembly

**Functions:**
- `donto_why_not_higher(stmt)` -> (current_level, next_level, blocker, detail) — Explains what blocks maturity promotion.
- `donto_claim_card(stmt)` -> jsonb — Assembles the full epistemic state in one composite JSON object.

## Phase 3c: Schema Gaps (0035-0043)

### 0035_document_sections — Hierarchical document structure

**Tables:** `donto_document_section`, `donto_table`, `donto_table_cell`.

### 0036_mentions — Entity mentions and coreference

**Tables:** `donto_mention`, `donto_coref_cluster`, `donto_coref_member`.

### 0037_extraction_chunks — Per-chunk tracking

**Tables:** `donto_extraction_chunk` — run FK, revision FK, chunk_index, start/end offsets, token_count, prompt_hash, response_hash, latency_ms, status.

### 0038_confidence — Statement-level confidence

**Tables:** `donto_stmt_confidence` — statement FK (PK), confidence [0,1], confidence_source, run FK.

### 0039_units — Unit registry and conversion

**Tables:** `donto_unit` — IRI (PK), label, symbol, dimension, si_base FK, si_factor. Seeded with 26 common units across 7 dimensions.

### 0040_temporal_expressions — Parsed temporal expressions

**Tables:** `donto_temporal_expression` — span FK, raw_text, resolved_from/to dates, resolution, reference_date, confidence, run FK.

### 0041_content_regions — Non-textual content

**Tables:** `donto_content_region` — revision FK, region_type, label, caption, content_hash, content_bytes, alt_text, span FK, section FK.

### 0042_entity_aliases — Cross-system identity

**Tables:** `donto_entity_alias` — (alias_iri, canonical_iri) PK, system, confidence, registered_by.

### 0043_candidate_contexts — Pre-assertion staging

Adds `candidate` to the context kind check constraint. Functions for promoting candidates above a confidence threshold.

## Summary

| Metric | Value |
|--------|-------|
| Total migrations | 43 |
| Total tables | 46 |
| Total SQL functions | 58 (in migrations 0023-0043 alone) |
| Seeded data | Default context, 5 scope presets, 4 reaction predicates, 3 rule types, 2 shape types, 7 XSD datatypes, 5 prefixes, 26 units |
| Constraint types used | CHECK, UNIQUE, FK, PK, partial unique indexes |
| Index types used | btree, GIN, GiST, trigram |
| Extension dependencies | btree_gist, pgcrypto, pg_trgm |
