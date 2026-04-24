# Donto Migration Reference

Complete reference for all 43 migrations. Each migration is an
idempotent SQL script tracked by SHA256 hash in `donto_migration`.

---

## Migration Timeline

| Phase | Migrations | Date | Summary |
|-------|-----------|------|---------|
| Phase 0: Core | 0001–0011 | 2026-04-17 | Statement atom, contexts, flags, functions, presets, predicates, snapshots, shapes, rules, certificates, observability |
| Phase 1: Fixes | 0012 | 2026-04-17 | Scope matching fix |
| Phase 2: Alexandria | 0013–0022 | 2026-04-19 | Trigram search, retrofit, shape annotations, valid-time buckets, reactions, aggregates, FTS, bitemporal canonicals, SameMeaning, context env |
| Phase 3: Evidence Substrate | 0023–0033 | 2026-04-24 | Documents, revisions, spans, annotations, annotation edges, extraction runs, evidence links, agents, arguments, proof obligations, vectors |
| Phase 3b: Claim Card | 0034 | 2026-04-24 | Claim card assembly, why-not-higher maturity blocker analysis |
| Phase 3c: Schema Gaps | 0035–0043 | 2026-04-24 | Document sections, mentions, extraction chunks, confidence, units, temporal expressions, content regions, entity aliases, candidate contexts |

---

## Phase 0: Core (0001–0011)

### 0001_core — Statement atom, contexts, lineage, audit

The foundation. Creates the three core tables that everything else
builds on.

**Tables:**
- `donto_context` — Named graphs. IRI primary key. 10 kinds (source,
  snapshot, hypothesis, user, pipeline, trust, derivation, quarantine,
  custom, system). 2 modes (permissive, curated). Parent links form a
  forest.
- `donto_statement` — The atom. UUID primary key. Subject/predicate/
  object (IRI or typed literal JSON), context FK, `tx_time` tstzrange,
  `valid_time` daterange, `flags` smallint (polarity + maturity).
  Content hash for idempotent re-ingestion. Never deleted — retraction
  closes `tx_time`.
- `donto_stmt_lineage` — Sparse overlay tracking derivation inputs.
- `donto_audit` — Audit log for assert/retract/correct/retrofit.

**Seeds:** `donto:anonymous` default context.

**Extensions:** `btree_gist`, `pgcrypto`.

### 0002_flags — Polarity and maturity packing

**Functions:**
- `donto_pack_flags(polarity, maturity)` → smallint
- `donto_polarity(flags)` → text
- `donto_maturity(flags)` → int

**Flag layout:** bits 0-1 polarity (asserted/negated/absent/unknown),
bits 2-4 maturity (0–4), bits 5-15 reserved.

### 0003_functions — Core SQL API

**Functions:**
- `donto_ensure_context(iri, kind, mode, parent)` — Idempotent context creation
- `donto_resolve_scope(scope_json)` → context IRIs — Recursive CTE with include/exclude/descendants/ancestors/kind_filter
- `donto_assert(subject, predicate, object_iri, object_lit, context, polarity, maturity, valid_lo, valid_hi, actor)` → UUID
- `donto_assert_batch(json_array, actor)` → count
- `donto_retract(statement_id, actor)` → boolean
- `donto_correct(statement_id, new_*, actor)` → UUID
- `donto_match(subject, predicate, object_*, scope, polarity, min_maturity, as_of_tx, as_of_valid)` → rows

### 0004_migrations — Migration ledger

**Tables:**
- `donto_migration` — Name + SHA256 hash + applied_at. Seeds itself with 0001–0003 on first run.

### 0005_presets — Named scope presets

**Tables:**
- `donto_scope_preset` — Name → scope JSON descriptor.

**Seeds:** `anywhere`, `raw`, `curated`, `latest`.

**Functions:**
- `donto_define_preset(name, scope, description)`
- `donto_preset_scope(name)` → jsonb
- `donto_scope_under_hypothesis(hypo_iri)` → scope jsonb
- `donto_scope_as_of(snapshot_iri)` → scope jsonb

### 0006_predicate — Predicate registry

**Tables:**
- `donto_predicate` — IRI, canonical_of, label, description, domain, range, inverse_of, is_symmetric/transitive/functional, cardinality, status (active/deprecated/merged/implicit).
- `donto_datatype` — IRI + label + base. Seeded with XSD types.
- `donto_prefix` — Compact IRI prefixes. Seeded with rdf/rdfs/owl/xsd/donto.

**Functions:**
- `donto_register_predicate(...)` — Single-hop alias chains enforced
- `donto_canonical_predicate(iri)` → canonical IRI
- `donto_implicit_register(iri)` — Auto-register in permissive contexts
- `donto_match_canonical(...)` — Query-time alias expansion

Patches `donto_assert` to call implicit registration in permissive mode and reject unregistered predicates in curated mode.

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

**Seeds:** `builtin:functional/<predicate>`, `builtin:datatype/<predicate>/<datatype>`.

### 0009_rule — Derivation rules

**Tables:**
- `donto_rule` — IRI, body_kind, body, output_ctx, mode (eager/batch/on_demand).
- `donto_derivation_report` — Cached derivation results.

**Seeds:** `builtin:transitive/<predicate>`, `builtin:inverse/<predicate>/<inverse>`, `builtin:symmetric/<predicate>`.

### 0010_certificate — Certificates

**Tables:**
- `donto_stmt_certificate` — statement_id, kind (7 types), rule_iri, inputs UUID[], body JSON, signature, verification state.

**Functions:**
- `donto_attach_certificate(...)` — Upsert, clears prior verification
- `donto_record_verification(...)` — Record verifier outcome

### 0011_observability — Stats tables

**Tables:**
- `donto_stats_context`, `donto_stats_predicate`, `donto_stats_maturity`, `donto_stats_shape`, `donto_stats_rule`, `donto_stats_audit` — Aggregate counters for operational visibility.

---

## Phase 1: Fixes (0012)

### 0012_match_scope_fix — Scope resolution refinement

Patches `donto_resolve_scope` to honor `kind_filter` and `exclude_kind` fields used by seeded presets. Backward compatible.

---

## Phase 2: Alexandria Extensions (0013–0022)

### 0013_search_trgm — Trigram search

Adds `pg_trgm` extension and trigram indexes on statement subjects for fuzzy search.

### 0014_retrofit — Backdated ingestion

**Tables:** `donto_retrofit` — statement_id, reason, actor, timestamp.

**Functions:** `donto_assert_retrofit(...)` — Requires explicit valid_time and reason.

**Views:** `donto_retrofit_log` — Retrofitted statements joined back to the atom.

### 0015_shape_annotations — Per-statement shape verdicts

**Tables:** `donto_stmt_shape_annotation` — Bitemporal (tx_time lifecycle). verdict in (pass, warn, violate). At most one open per (statement, shape).

**Functions:**
- `donto_attach_shape_report(stmt, shape, verdict, context, detail)` — Idempotent close-and-reopen
- `donto_has_shape_verdict(stmt, verdict, shape)` → boolean

**Views:** `donto_stmt_shape_annotation_open` — Current verdicts joined to atoms.

### 0016_valid_time_buckets — Temporal aggregation

**Functions:** `donto_valid_time_buckets(interval, epoch, predicate, subject, scope)` — Time-binned statement counts.

### 0017_reactions — Meta-statements

Registers `donto:endorses`, `donto:rejects`, `donto:cites`, `donto:supersedes` predicates.

**Functions:**
- `donto_stmt_iri(uuid)` / `donto_stmt_iri_to_id(iri)` — UUID↔IRI conversion
- `donto_react(source_stmt, kind, object, context, actor)` → UUID
- `donto_reactions_for(stmt)` → rows

### 0018_aggregates — Endorsement weights

Registers `donto:weight` predicate and `builtin:endorsement_weight` rule.

**Functions:**
- `donto_compute_endorsement_weights(scope, into_ctx, actor)` → count
- `donto_weight_of(stmt, scope)` → int

### 0019_fts — Full-text search

**Functions:**
- `donto_lang_to_regconfig(lang)` → regconfig — BCP-47 to PG text search config
- `donto_stmt_lit_tsv(object_lit)` → tsvector — Composite expression
- `donto_match_text(query, lang, scope, predicate, polarity, maturity)` → rows with score

No index created by default (would lock 35M-row table). Operators run `CREATE INDEX CONCURRENTLY` when ready.

### 0020_bitemporal_canonicals — Time-dependent aliases

**Tables:** `donto_predicate_alias` — (alias, canonical, valid_time daterange). GiST index on valid_time.

**Functions:**
- `donto_register_alias_at(alias, canonical, valid_lo, valid_hi, actor)`
- `donto_canonical_predicate_at(iri, as_of_date)` → canonical — Narrowest interval wins, then timeless fallback, then self

### 0021_same_meaning — Parallel-literal alignment

Registers `donto:SameMeaning` predicate (symmetric).

**Functions:**
- `donto_align_meaning(stmt_a, stmt_b, context, actor)` — Emits both directions
- `donto_meaning_cluster(stmt, scope)` → statement_ids — Recursive transitive closure

### 0022_context_env — Environment overlays

**Tables:** `donto_context_env` — (context, key, value jsonb). Advisory only.

**Functions:**
- `donto_context_env_set/get/delete(context, key, ...)`
- `donto_contexts_with_env(required_pairs)` → context IRIs

---

## Phase 3: Evidence Substrate (0023–0033)

### 0023_documents — Document objects

**Tables:** `donto_document` — IRI (unique), media_type, label, source_url, language.

**Functions:**
- `donto_ensure_document(iri, media_type, label, source_url, language)` → UUID — Idempotent
- `donto_register_document(...)` → UUID — Upserts metadata

### 0024_document_revisions — Immutable content snapshots

**Tables:** `donto_document_revision` — document FK, revision_number (auto-increment), body text, body_bytes, content_hash (SHA256), parser_version.

**Functions:**
- `donto_add_revision(doc_id, body, body_bytes, parser_version, metadata)` → UUID — Deduplicates by content hash
- `donto_latest_revision(doc_id)` → UUID

### 0025_spans — Standoff annotations

**Tables:** `donto_span` — revision FK, span_type (char_offset/token/sentence/paragraph/page/line/region/xpath/css/custom), start_offset, end_offset, selector jsonb, surface_text.

**Functions:**
- `donto_create_char_span(revision, start, end, surface)` → UUID
- `donto_spans_overlapping(revision, start, end)` → rows

### 0026_annotations — Feature-value pairs on spans

**Tables:**
- `donto_annotation_space` — IRI (unique), label, feature_ns, version.
- `donto_annotation` — span FK, space FK, feature, value, value_detail jsonb, confidence, run FK.

**Functions:**
- `donto_ensure_annotation_space(iri, label, ns, version)` → UUID
- `donto_annotate_span(span, space, feature, value, detail, confidence, run)` → UUID
- `donto_annotations_for_span(span, space?, feature?)` → rows

### 0027_annotation_edges — Structural relations

**Tables:** `donto_annotation_edge` — source/target annotation FKs, space FK, relation. No self-links.

**Functions:**
- `donto_link_annotations(source, target, space, relation, metadata)` → UUID
- `donto_edges_from(annotation)` / `donto_edges_to(annotation)` → rows

### 0028_extraction_runs — Provenance

**Tables:** `donto_extraction_run` — model_id, model_version, prompt_hash, prompt_template, chunking_strategy, temperature, seed, toolchain jsonb, source_revision FK, context FK, status (running/completed/failed/partial), timestamps, emit counts.

Adds FK from `donto_annotation.run_id` → `donto_extraction_run`.

**Functions:**
- `donto_start_extraction(model, version, revision, context, template, temp, seed, chunking, toolchain, metadata)` → UUID
- `donto_complete_extraction(run, status, stmts, annotations)`

### 0029_evidence_links — Statement ↔ evidence

**Tables:** `donto_evidence_link` — statement FK, link_type (extracted_from/supported_by/contradicted_by/derived_from/cited_in/anchored_at/produced_by), polymorphic target (exactly one of: document/revision/span/annotation/run/statement), confidence, context, tx_time (bitemporal).

**Functions:**
- `donto_link_evidence_span(stmt, span, type, confidence, context)` → UUID
- `donto_link_evidence_run(stmt, run, type, context)` → UUID
- `donto_link_evidence_statement(stmt, target_stmt, type, confidence, context)` → UUID
- `donto_retract_evidence_link(link)` → boolean
- `donto_evidence_for(stmt)` → rows

### 0030_agents — Agent registry

**Tables:**
- `donto_agent` — IRI (unique), agent_type (human/llm/rule_engine/extractor/validator/curator/system/custom), model_id.
- `donto_agent_context` — (agent, context) with role (owner/contributor/reader).

**Functions:**
- `donto_ensure_agent(iri, type, label, model)` → UUID
- `donto_bind_agent_context(agent, context, role)`
- `donto_agent_contexts(agent)` / `donto_context_agents(context)` → rows

### 0031_arguments — Argumentation framework

**Tables:** `donto_argument` — source/target statement FKs, relation (supports/rebuts/undercuts/endorses/supersedes/qualifies/potentially_same/same_referent/same_event), strength [0,1], context FK, agent FK, tx_time (bitemporal). At most one open per (source, target, relation, context).

**Functions:**
- `donto_assert_argument(source, target, relation, context, strength, agent, evidence)` → UUID — Idempotent close-and-reopen
- `donto_retract_argument(argument)` → boolean
- `donto_arguments_for(stmt)` → rows (both directions)
- `donto_contradiction_frontier(context?)` → (stmt, attack_count, support_count, net_pressure)

### 0032_proof_obligations — Extraction work items

**Tables:** `donto_proof_obligation` — statement FK, obligation_type (10 types), status (open/in_progress/resolved/rejected/deferred), priority, assigned_agent FK, resolved_by FK, detail jsonb.

**Functions:**
- `donto_emit_obligation(stmt, type, context, priority, detail, agent)` → UUID
- `donto_resolve_obligation(obl, resolved_by, status)` → boolean
- `donto_assign_obligation(obl, agent)`
- `donto_open_obligations(type?, context?, limit)` → rows
- `donto_obligation_summary(context?)` → (type, status, count)

### 0033_vectors — Embedding layer

**Tables:** `donto_vector` — subject_type (statement/document/revision/span/annotation), subject_id, model_id, model_version, dimensions, embedding float4[]. Unique on (type, id, model).

**Functions:**
- `donto_store_vector(type, id, model, version, embedding)` → UUID — Upserts
- `donto_cosine_similarity(a, b)` → double precision — Null on dimension mismatch
- `donto_nearest_vectors(type, model, query, limit)` → (id, similarity) — Brute-force scan

---

## Phase 3b: Claim Card (0034)

### 0034_claim_card — Epistemic state assembly

**Functions:**
- `donto_why_not_higher(stmt)` → (current_level, next_level, blocker, detail) — Explains what blocks maturity promotion. Checks: predicate registration, shape reports, shape violations, derivation lineage, evidence links, span anchors, certificates, open obligations, active rebuttals.
- `donto_claim_card(stmt)` → jsonb — Assembles the full epistemic state: statement fields, evidence links, arguments, obligations, shape annotations, reactions, and maturity blockers in one composite JSON object.

---

## Phase 3c: Schema Gaps (0035–0043)

### 0035_document_sections — Hierarchical document structure

**Tables:**
- `donto_document_section` — revision FK, parent_section FK (forest), level (1=h1, 2=h2...), title, ordinal, span FK.
- `donto_table` — revision FK, section FK, label ("Table 2"), caption, row/col counts, span FK.
- `donto_table_cell` — table FK, row_idx, col_idx, is_header, row_header, col_header, value, value_numeric, span FK. Unique on (table, row, col).

**Functions:**
- `donto_add_section(revision, title, level, parent, ordinal, span)` → UUID
- `donto_add_table(revision, label, caption, rows, cols, section, span)` → UUID
- `donto_add_table_cell(table, row, col, value, row_header, col_header, is_header, numeric, span)` → UUID — Upserts
- `donto_table_cells(table)` → rows ordered by (row, col)

### 0036_mentions — Entity mentions and coreference

**Tables:**
- `donto_mention` — span FK, mention_type (entity/event/relation/attribute/temporal/quantity/citation/custom), entity_iri (resolved or null), candidate_iris text[], confidence, run FK.
- `donto_coref_cluster` — revision FK, resolved_iri, confidence, run FK.
- `donto_coref_member` — (cluster, mention) with is_representative flag.

**Functions:**
- `donto_create_mention(span, type, entity, candidates, confidence, run)` → UUID
- `donto_create_coref_cluster(revision, mention_ids[], resolved_iri, confidence, run)` → UUID — First mention is representative
- `donto_mentions_in_revision(revision, type?)` → rows

### 0037_extraction_chunks — Per-chunk tracking

**Tables:** `donto_extraction_chunk` — run FK, revision FK, chunk_index (unique per run), start/end offsets, token_count, prompt_hash, response_hash, latency_ms, status (pending/running/completed/failed).

**Functions:**
- `donto_add_extraction_chunk(run, revision, index, start, end, tokens, prompt_hash, latency)` → UUID — Upserts
- `donto_extraction_chunks(run)` → rows ordered by chunk_index

### 0038_confidence — Statement-level confidence

**Tables:** `donto_stmt_confidence` — statement FK (PK), confidence [0,1], confidence_source (extraction/human/model/aggregated/rule/calibrated/custom), run FK.

**Functions:**
- `donto_set_confidence(stmt, confidence, source, run)` — Upserts
- `donto_get_confidence(stmt)` → double precision or null
- `donto_low_confidence_statements(context?, threshold, limit)` → rows

### 0039_units — Unit registry and conversion

**Tables:** `donto_unit` — IRI (PK), label, symbol, dimension, si_base FK, si_factor.

**Seeds:** 26 common units across 7 dimensions: ratio (accuracy, percent, F1), time (second through attosecond, year/day/hour), length (meter, nanometer, angstrom), temperature (kelvin, celsius), energy (eV, joule), currency (USD, EUR), mass (kg, g, mg).

**Functions:**
- `donto_convert_unit(value, from_unit, to_unit)` → double precision — Returns null on dimension mismatch or missing unit
- `donto_normalize_percent(raw_text)` → double precision — Handles "60.1%", "60.1 percent", "0.601"

### 0040_temporal_expressions — Parsed temporal expressions

**Tables:** `donto_temporal_expression` — span FK, raw_text, resolved_from/to dates, resolution (exact/day/month/year/decade/century/relative/vague/approximate), reference_date, confidence, run FK.

**Functions:**
- `donto_add_temporal_expression(span, raw, from, to, resolution, ref_date, confidence, run)` → UUID
- `donto_temporal_expressions_in_range(from, to)` → rows

### 0041_content_regions — Non-textual content

**Tables:** `donto_content_region` — revision FK, region_type (image/chart/diagram/code_block/formula/video/audio/map/screenshot/custom), label, caption, content_hash, content_bytes, alt_text, span FK, section FK.

**Functions:**
- `donto_add_content_region(revision, type, label, caption, alt_text, span, section)` → UUID

### 0042_entity_aliases — Cross-system identity

**Tables:** `donto_entity_alias` — (alias_iri, canonical_iri) PK, system, confidence, registered_by. No self-aliases.

**Functions:**
- `donto_register_entity_alias(alias, canonical, system, confidence, actor)` — Idempotent, keeps highest confidence
- `donto_resolve_entity(iri)` → canonical IRI — One-hop, highest confidence, self if unregistered
- `donto_entity_aliases(iri)` → rows — Both directions

### 0043_candidate_contexts — Pre-assertion staging

Adds `candidate` to the context kind check constraint.

**Functions:**
- `donto_promote_candidate(stmt, target_context, actor)` → UUID — Verifies source is candidate context, asserts in target, tracks lineage, retracts candidate
- `donto_promote_candidates_above(candidate_context, target_context, min_confidence, actor)` → count — Bulk promotion based on confidence threshold

---

## Summary

| Metric | Value |
|--------|-------|
| Total migrations | 43 |
| Total tables | 46 |
| Total SQL functions | 58 (in migrations 0023–0043 alone) |
| Seeded data | Default context, 5 scope presets, 4 reaction predicates, 3 rule types, 2 shape types, 7 XSD datatypes, 5 prefixes, 26 units |
| Constraint types used | CHECK, UNIQUE, FK, PK, partial unique indexes |
| Index types used | btree, GIN, GiST, trigram |
| Extension dependencies | btree_gist, pgcrypto, pg_trgm |
