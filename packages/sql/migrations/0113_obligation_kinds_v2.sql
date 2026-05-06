-- v1000 / FR-011 obligation kinds extension.
--
-- v1000 PRD names nine obligation kinds:
--   needs_evidence, needs_policy, needs_review,
--   needs_identity_resolution, needs_alignment_review,
--   needs_anchor_repair, needs_contradiction_review,
--   needs_formal_validation, needs_community_authority
--
-- Migration 0032 shipped ten kinds (with hyphens):
--   needs-coref, needs-temporal-grounding, needs-source-support,
--   needs-unit-normalization, needs-entity-disambiguation,
--   needs-relation-validation, needs-human-review,
--   needs-confidence-boost, needs-context-resolution, custom
--
-- Approach: extend the CHECK constraint to allow both the v0 and v1000
-- kinds. v0 callers continue to work; new code uses the v1000 names.
-- Also extend status to v1000's six values (adding 'blocked').

alter table donto_proof_obligation
    drop constraint if exists donto_proof_obligation_obligation_type_check;

alter table donto_proof_obligation
    add constraint donto_proof_obligation_obligation_type_check
    check (obligation_type in (
        -- v0 (preserved)
        'needs-coref', 'needs-temporal-grounding', 'needs-source-support',
        'needs-unit-normalization', 'needs-entity-disambiguation',
        'needs-relation-validation', 'needs-human-review',
        'needs-confidence-boost', 'needs-context-resolution', 'custom',
        -- v1000 additions (canonical underscore naming)
        'needs_evidence',
        'needs_policy',
        'needs_review',
        'needs_identity_resolution',
        'needs_alignment_review',
        'needs_anchor_repair',
        'needs_contradiction_review',
        'needs_formal_validation',
        'needs_community_authority'
    ));

alter table donto_proof_obligation
    drop constraint if exists donto_proof_obligation_status_check;

alter table donto_proof_obligation
    add constraint donto_proof_obligation_status_check
    check (status in (
        'open', 'in_progress', 'resolved', 'rejected', 'deferred', 'blocked'
    ));

-- Helper: emit a v1000-kind obligation. Calls the existing
-- donto_emit_obligation under the hood; preserves the v1000 kind name.
create or replace function donto_emit_v1000_obligation(
    p_statement_id   uuid,
    p_obligation_type text,
    p_context        text default 'donto:anonymous',
    p_priority       smallint default 0,
    p_detail         jsonb default null,
    p_assigned_agent uuid default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    insert into donto_proof_obligation
        (statement_id, obligation_type, context, priority, detail, assigned_agent)
    values
        (p_statement_id, p_obligation_type, p_context, p_priority, p_detail, p_assigned_agent)
    returning obligation_id into v_id;
    return v_id;
end;
$$;

-- Reference: enumerate canonical v1000 kinds for clients.
create or replace view donto_v_obligation_kind_v1000 as
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
