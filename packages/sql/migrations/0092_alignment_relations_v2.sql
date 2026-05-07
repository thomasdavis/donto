-- v1000 / I7: schema mappings are typed and scoped.
--
-- Migration 0048 shipped six alignment relations:
--   exact_equivalent, inverse_equivalent, sub_property_of,
--   close_match, decomposition, not_equivalent
--
-- v1000 PRD §6.10 specifies eleven:
--   exact_equivalent, close_match, broad_match, narrow_match,
--   inverse_of, decomposes_to, has_value_mapping, incompatible_with,
--   derived_from, local_specialization, not_equivalent
--
-- Atlas of changes:
--   * keep all six existing names as valid (backwards-compatible)
--   * add narrow_match (semantic alias for sub_property_of, kept distinct)
--   * add broad_match, has_value_mapping, derived_from, local_specialization
--   * add three boolean safety flags
--   * add scope column referencing donto_context
--   * add review_status column for the alignment review queue
--   * add evidence_anchor_ids for argument-style anchoring
--   * add donto_alignment_value_mapping for has_value_mapping payloads
--
-- Old code paths writing exact_equivalent / sub_property_of / etc.
-- continue to work. New code paths should prefer the v1000 names.

alter table donto_predicate_alignment
    drop constraint if exists donto_predicate_alignment_relation_check;

alter table donto_predicate_alignment
    add constraint donto_predicate_alignment_relation_check
    check (relation in (
        -- v0 (preserved)
        'exact_equivalent',
        'inverse_equivalent',
        'sub_property_of',
        'close_match',
        'decomposition',
        'not_equivalent',
        -- v1000 additions / aliases
        'exact_match',                -- alias of exact_equivalent
        'inverse_of',                 -- alias of inverse_equivalent
        'narrow_match',               -- alias of sub_property_of
        'decomposes_to',              -- alias of decomposition
        'incompatible_with',          -- alias of not_equivalent
        'broad_match',                -- new
        'has_value_mapping',          -- new
        'derived_from',               -- new
        'local_specialization'        -- new
    ));

-- v1000 safety flags. Defaults are conservative:
--   * safe_for_query_expansion = true  (existing behavior)
--   * safe_for_export          = false (caller must opt in)
--   * safe_for_logical_inference = false (caller must opt in)
alter table donto_predicate_alignment
    add column if not exists safe_for_query_expansion boolean not null default true;
alter table donto_predicate_alignment
    add column if not exists safe_for_export boolean not null default false;
alter table donto_predicate_alignment
    add column if not exists safe_for_logical_inference boolean not null default false;

-- Scope: alignment may be valid only within a context subtree.
alter table donto_predicate_alignment
    add column if not exists scope text references donto_context(iri);

create index if not exists donto_pa_scope_idx
    on donto_predicate_alignment (scope) where scope is not null;

-- Review status separate from tx_time lifecycle.
alter table donto_predicate_alignment
    add column if not exists review_status text not null default 'candidate'
    check (review_status in ('candidate', 'accepted', 'rejected', 'superseded'));

create index if not exists donto_pa_review_status_idx
    on donto_predicate_alignment (review_status)
    where review_status <> 'accepted';

-- Per-alignment evidence anchors.
alter table donto_predicate_alignment
    add column if not exists evidence_anchor_ids text[] not null default '{}';

create index if not exists donto_pa_evidence_anchors_gin
    on donto_predicate_alignment using gin (evidence_anchor_ids);

-- ---------------------------------------------------------------------------
-- Value mapping table for has_value_mapping alignments.
--
-- When two predicates have aligned but non-identical value spaces
-- (e.g., wals:Feature98 codes 1..4 vs grambank:GBxxx binary), the
-- alignment row is one record but the value mapping is many rows.
-- ---------------------------------------------------------------------------

create table if not exists donto_alignment_value_mapping (
    mapping_id      bigserial primary key,
    alignment_id    uuid not null references donto_predicate_alignment(alignment_id) on delete cascade,
    left_value      text not null,
    right_value     text not null,
    confidence      double precision not null default 1.0
                    check (confidence >= 0 and confidence <= 1),
    notes           text,
    created_at      timestamptz not null default now(),
    constraint donto_avm_unique unique (alignment_id, left_value, right_value)
);

create index if not exists donto_avm_alignment_idx
    on donto_alignment_value_mapping (alignment_id);

-- Helper: register a value mapping for an alignment.
create or replace function donto_register_value_mapping(
    p_alignment_id uuid,
    p_left_value   text,
    p_right_value  text,
    p_confidence   double precision default 1.0,
    p_notes        text default null
) returns bigint
language plpgsql as $$
declare
    v_id bigint;
begin
    insert into donto_alignment_value_mapping
        (alignment_id, left_value, right_value, confidence, notes)
    values
        (p_alignment_id, p_left_value, p_right_value, p_confidence, p_notes)
    on conflict (alignment_id, left_value, right_value) do update set
        confidence = excluded.confidence,
        notes      = excluded.notes
    returning mapping_id into v_id;
    return v_id;
end;
$$;

-- Canonical v1000 relation list (for clients selecting valid values).
create or replace view donto_v_alignment_relation_v1000 as
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
