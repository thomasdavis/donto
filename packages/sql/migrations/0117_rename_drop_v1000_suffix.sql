-- Drop the `_v1000` suffix from SQL artifacts. The suffix was a
-- placeholder while the Trust-Kernel-class additions were being
-- shaped; the artifacts themselves don't need version naming.
--
-- The rename strategy is conservative on databases that have
-- already applied the earlier migrations:
--   * Views: drop the old name, recreate at the new name (their
--     bodies are in earlier migrations and are recreated by re-
--     running those migrations under create-or-replace, so we just
--     redefine here).
--   * Tables: ALTER TABLE ... RENAME (preserves data).
--   * Functions: drop the suffixed function, recreate at the
--     proper name with identical body.
--
-- After this migration:
--   donto_v_argument_relation_v1000        → donto_v_argument_relation
--   donto_v_alignment_relation_v1000       → donto_v_alignment_relation
--   donto_v_context_kind_v1000             → donto_v_context_kind
--   donto_v_maturity_ladder_v1000          → donto_v_maturity_ladder
--   donto_v_modality_v1000                 → donto_v_modality
--   donto_v_obligation_kind_v1000          → donto_v_obligation_kind
--   donto_v_polarity_v1000                 → donto_v_polarity
--   donto_v_statement_polarity_v1000       → donto_v_statement_polarity
--   donto_query_clause_v1000               → donto_query_clause
--   donto_register_source_v1000            → donto_register_source
--   donto_add_revision_v1000               → donto_add_revision_typed
--   donto_register_identity_hypothesis_v1000 → donto_register_clustering_hypothesis
--   donto_emit_v1000_obligation            → (dropped; callers use donto_emit_obligation directly)

-- ---------------------------------------------------------------------------
-- Views.
-- ---------------------------------------------------------------------------

drop view if exists donto_v_argument_relation_v1000;
create or replace view donto_v_argument_relation as
    select * from (values
        ('supports', 'Source provides evidence for target.'),
        ('rebuts', 'Source contradicts target conclusion.'),
        ('undercuts', 'Source attacks target reasoning.'),
        ('qualifies', 'Source limits or constrains target scope.'),
        ('explains', 'Source provides explanatory mechanism for target.'),
        ('alternative_analysis_of', 'Source proposes a different analysis of the same evidence.'),
        ('same_evidence_different_analysis', 'Source and target use overlapping evidence with incompatible analyses.'),
        ('same_claim_different_schema', 'Source and target encode the same underlying claim under different schemas.'),
        ('supersedes', 'Source replaces target.')
    ) as t(relation, description);

drop view if exists donto_v_alignment_relation_v1000;
create or replace view donto_v_alignment_relation as
    select * from (values
        ('exact_match',           'Same meaning and value space; interchangeable.'),
        ('close_match',           'Usable together for retrieval; not logical identity.'),
        ('broad_match',           'Left is broader than right.'),
        ('narrow_match',          'Left is narrower than right.'),
        ('inverse_of',            'Same relation; subject and object swapped.'),
        ('decomposes_to',         'One concept decomposes into multiple claims/values.'),
        ('has_value_mapping',     'Predicate equivalence depends on a value mapping.'),
        ('incompatible_with',     'Should not be aligned.'),
        ('derived_from',          'One schema feature was designed from another.'),
        ('local_specialization',  'Language- or project-specific refinement.'),
        ('not_equivalent',        'Explicit negative: do not align.')
    ) as t(relation, description);

drop view if exists donto_v_polarity_v1000;
create or replace view donto_v_polarity as
    select * from (values
        ('asserted',    'Source asserts the claim.'),
        ('negated',     'Source denies the claim.'),
        ('unknown',     'Source explicitly notes uncertainty.'),
        ('absent',      'Source mentions the topic without making a claim.'),
        ('conflicting', 'Derived: two stored polarities collide. View-only.')
    ) as t(polarity, description);

drop view if exists donto_v_statement_polarity_v1000;
create or replace view donto_v_statement_polarity as
    select s.statement_id, s.subject, s.predicate, s.object_iri,
           s.object_lit, s.context,
           donto_polarity(s.flags) as stored_polarity,
           case
               when exists (
                   select 1 from donto_statement s2
                   where s2.subject = s.subject
                     and s2.predicate = s.predicate
                     and (s2.object_iri = s.object_iri
                          or s2.object_lit = s.object_lit)
                     and s2.context = s.context
                     and donto_polarity(s2.flags) <> donto_polarity(s.flags)
                     and upper(s2.tx_time) is null
               ) then 'conflicting'
               else donto_polarity(s.flags)
           end as effective_polarity
    from donto_statement s
    where upper(s.tx_time) is null;

drop view if exists donto_v_modality_v1000;
create or replace view donto_v_modality as
    select * from (values
        ('descriptive',          'Source describes how the world is.'),
        ('prescriptive',         'Source prescribes how the world should be.'),
        ('reconstructed',        'Claim was reconstructed (e.g., proto-language).'),
        ('inferred',             'Claim was inferred analytically.'),
        ('elicited',             'Claim was elicited from a speaker / informant.'),
        ('corpus_observed',      'Claim is a corpus observation.'),
        ('typological_summary',  'Claim summarizes a feature across a language.'),
        ('experimental_result',  'Claim is an experimental result.'),
        ('clinical_observation', 'Claim is a clinical observation.'),
        ('legal_holding',        'Claim is a legal holding or precedent.'),
        ('archival_metadata',    'Claim is metadata from an archive record.'),
        ('oral_history',         'Claim is from oral testimony.'),
        ('community_protocol',   'Claim is a community protocol or rule.'),
        ('model_output',         'Claim was produced by a machine model.'),
        ('other',                'Other modality not in the standard list.')
    ) as t(modality, description);

drop view if exists donto_v_maturity_ladder_v1000;
create or replace view donto_v_maturity_ladder as
    select 0 as stored, 'E0' as level, 'Raw' as name,
           donto_maturity_description(0) as description
    union all select 1, 'E1', 'Candidate',          donto_maturity_description(1)
    union all select 2, 'E2', 'Evidence-supported', donto_maturity_description(2)
    union all select 3, 'E3', 'Reviewed',           donto_maturity_description(3)
    union all select 5, 'E4', 'Corroborated',       donto_maturity_description(5)
    union all select 4, 'E5', 'Certified',          donto_maturity_description(4);

drop view if exists donto_v_context_kind_v1000;
create or replace view donto_v_context_kind as
    select * from (values
        ('source',                  'A registered source object.'),
        ('source_version',          'An immutable source version (revision).'),
        ('dataset_release',         'A versioned dataset release.'),
        ('project',                 'A project workspace.'),
        ('hypothesis',              'A scholarly hypothesis context.'),
        ('identity_lens',           'An identity-resolution lens.'),
        ('schema_lens',             'A schema-alignment lens.'),
        ('review_lens',             'A reviewer view scope.'),
        ('community_policy_scope',  'A community/institutional policy scope.'),
        ('language_or_variety',     'A language variety scope.'),
        ('corpus',                  'A corpus.'),
        ('experiment',              'An experimental scope.'),
        ('jurisdiction',            'A legal jurisdiction.'),
        ('clinical_cohort',         'A clinical cohort.'),
        ('historical_period',       'A historical period.'),
        ('user_workspace',          'An individual user workspace.'),
        ('release_view',            'A release-view scope.'),
        ('derived',                 'A derivation context.'),
        ('snapshot',                'A snapshot context.'),
        ('user',                    'A user context.'),
        ('custom',                  'A custom context kind.')
    ) as t(kind, description);

drop view if exists donto_v_obligation_kind_v1000;
create or replace view donto_v_obligation_kind as
    select * from (values
        ('needs_evidence',              'Claim lacks an evidence anchor.'),
        ('needs_policy',                'Source or claim has no access policy assigned.'),
        ('needs_review',                'Claim awaits reviewer decision.'),
        ('needs_identity_resolution',   'Entity identity is contested or ambiguous.'),
        ('needs_alignment_review',      'Schema alignment proposal awaits review.'),
        ('needs_anchor_repair',         'Anchor locator is invalid or low-confidence.'),
        ('needs_contradiction_review',  'Claim is in active contradiction with another.'),
        ('needs_formal_validation',     'Claim awaits Lean shape or formal-validation pass.'),
        ('needs_community_authority',   'Source needs community-authority decision.')
    ) as t(obligation_kind, description);

-- ---------------------------------------------------------------------------
-- Tables.
-- ---------------------------------------------------------------------------

alter table if exists donto_query_clause_v1000
    rename to donto_query_clause;

-- The helper function `donto_query_clauses` queries the renamed table; redefine
-- it to use the canonical name.
create or replace function donto_query_clauses(p_kind text default null)
returns table(clause_name text, clause_kind text, description text)
language sql stable as $$
    select clause_name, clause_kind, description
    from donto_query_clause
    where deprecated_in is null
      and (p_kind is null or clause_kind = p_kind)
    order by clause_kind, clause_name
$$;

-- ---------------------------------------------------------------------------
-- Functions.
-- ---------------------------------------------------------------------------

drop function if exists donto_register_source_v1000(
    text, text, text, text, text, text, text, jsonb, jsonb, text, text, text, text, jsonb
);
create or replace function donto_register_source(
    p_iri              text,
    p_source_kind      text,
    p_policy_id        text,
    p_media_type       text default 'text/plain',
    p_label            text default null,
    p_source_url       text default null,
    p_language         text default null,
    p_creators         jsonb default '[]'::jsonb,
    p_source_date      jsonb default null,
    p_content_address  text default null,
    p_native_format    text default null,
    p_adapter_used     text default null,
    p_registered_by    text default null,
    p_metadata         jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    if p_policy_id is null or length(trim(p_policy_id)) = 0 then
        raise exception 'donto_register_source: policy_id is required';
    end if;

    insert into donto_document
        (iri, media_type, label, source_url, language, metadata,
         source_kind, creators, source_date, registered_by,
         policy_id, content_address, native_format, adapter_used, status)
    values
        (p_iri, p_media_type, p_label, p_source_url, p_language, p_metadata,
         p_source_kind, p_creators, p_source_date, p_registered_by,
         p_policy_id, p_content_address, p_native_format, p_adapter_used,
         'registered')
    on conflict (iri) do update set
        media_type = excluded.media_type,
        label      = coalesce(excluded.label, donto_document.label),
        source_url = coalesce(excluded.source_url, donto_document.source_url),
        language   = coalesce(excluded.language, donto_document.language),
        metadata   = donto_document.metadata || excluded.metadata,
        source_kind = coalesce(excluded.source_kind, donto_document.source_kind),
        creators   = case
                        when excluded.creators = '[]'::jsonb then donto_document.creators
                        else excluded.creators
                     end,
        source_date = coalesce(excluded.source_date, donto_document.source_date),
        registered_by = coalesce(excluded.registered_by, donto_document.registered_by),
        policy_id  = coalesce(excluded.policy_id, donto_document.policy_id),
        content_address = coalesce(excluded.content_address, donto_document.content_address),
        native_format = coalesce(excluded.native_format, donto_document.native_format),
        adapter_used = coalesce(excluded.adapter_used, donto_document.adapter_used)
    returning document_id into v_id;

    return v_id;
end;
$$;

drop function if exists donto_add_revision_v1000(
    uuid, text, text, bytea, text, jsonb, uuid[], text, jsonb
);
create or replace function donto_add_revision_typed(
    p_document_id          uuid,
    p_version_kind         text,
    p_body                 text default null,
    p_body_bytes           bytea default null,
    p_parser_version       text default null,
    p_quality_metrics      jsonb default '{}'::jsonb,
    p_derived_from         uuid[] default '{}',
    p_created_by           text default null,
    p_metadata             jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_hash bytea;
    v_next int;
    v_id   uuid;
begin
    if p_body is null and p_body_bytes is null then
        raise exception 'donto_add_revision_typed: body or body_bytes required';
    end if;

    v_hash := digest(coalesce(p_body, '') || coalesce(encode(p_body_bytes, 'hex'), ''), 'sha256');

    select revision_id into v_id
    from donto_document_revision
    where document_id = p_document_id and content_hash = v_hash;
    if v_id is not null then
        update donto_document_revision
        set version_kind         = p_version_kind,
            quality_metrics      = p_quality_metrics,
            derived_from_versions = p_derived_from,
            created_by           = coalesce(p_created_by, created_by),
            metadata             = metadata || p_metadata
        where revision_id = v_id;
        return v_id;
    end if;

    select coalesce(max(revision_number), 0) + 1 into v_next
    from donto_document_revision where document_id = p_document_id;

    insert into donto_document_revision
        (document_id, revision_number, body, body_bytes, content_hash,
         parser_version, metadata, version_kind, quality_metrics,
         derived_from_versions, created_by)
    values
        (p_document_id, v_next, p_body, p_body_bytes, v_hash,
         p_parser_version, p_metadata, p_version_kind,
         p_quality_metrics, p_derived_from, p_created_by)
    returning revision_id into v_id;
    return v_id;
end;
$$;

drop function if exists donto_register_identity_hypothesis_v1000(
    text, text, double precision, double precision, text, text, uuid, jsonb
);
create or replace function donto_register_clustering_hypothesis(
    p_name              text,
    p_description       text default null,
    p_threshold_same    double precision default 0.85,
    p_threshold_distinct double precision default 0.05,
    p_method            text default 'rule',
    p_authority         text default null,
    p_provenance_proposal_id uuid default null,
    p_policy_json       jsonb default '{}'::jsonb
) returns bigint
language plpgsql as $$
declare
    v_id bigint;
begin
    insert into donto_identity_hypothesis
        (name, description, threshold_same, threshold_distinct,
         policy_json, method, authority, provenance_proposal_id)
    values
        (p_name, p_description, p_threshold_same, p_threshold_distinct,
         p_policy_json, p_method, p_authority, p_provenance_proposal_id)
    on conflict (name) do update set
        description     = coalesce(excluded.description, donto_identity_hypothesis.description),
        method          = excluded.method,
        authority       = coalesce(excluded.authority, donto_identity_hypothesis.authority)
    returning hypothesis_id into v_id;

    perform donto_emit_event(
        'identity_hypothesis', v_id::text, 'created',
        coalesce(p_authority, 'system'),
        jsonb_build_object('name', p_name, 'method', p_method)
    );
    return v_id;
end;
$$;

-- The trust-kernel obligation wrapper was a thin shim over the existing
-- donto_emit_obligation; callers can use that directly with the v1000 kind
-- names. Drop the wrapper to avoid duplicate API surface.
drop function if exists donto_emit_v1000_obligation(
    uuid, text, text, smallint, jsonb, uuid
);
