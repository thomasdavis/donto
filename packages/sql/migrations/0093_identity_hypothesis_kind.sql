-- Trust Kernel / I8: identity is a hypothesis, not a foreign key.
--
-- Migration 0061 introduced donto_identity_hypothesis as a clustering
-- solution (strict / likely / exploratory). PRD §6.8 specifies a
-- richer per-hypothesis structure with hypothesis_kind, method, and
-- status enums distinct from the clustering-solution status.
--
-- Rather than overload the existing table (which represents a *solution*
-- across many edges), this migration adds a peer table `donto_identity_proposal`
-- that represents an individual identity-relation proposal between
-- two or more entities. The existing donto_identity_hypothesis remains
-- the clustering-solution table; donto_identity_proposal is the
-- per-relation table the PRD §6.8 schema describes.

create table if not exists donto_identity_proposal (
    proposal_id        uuid primary key default gen_random_uuid(),
    hypothesis_kind    text not null check (hypothesis_kind in (
        'same_as', 'different_from',
        'broader_than', 'narrower_than',
        'split_candidate', 'merge_candidate',
        'successor_of', 'alias_of'
    )),
    entity_refs        text[] not null check (cardinality(entity_refs) >= 2),
    confidence         double precision not null default 0.5
                       check (confidence >= 0 and confidence <= 1),
    method             text not null default 'human' check (method in (
        'human', 'rule', 'model', 'registry_match', 'cross_source_evidence'
    )),
    evidence_anchor_ids text[] not null default '{}',
    context_id         text references donto_context(iri),
    status             text not null default 'candidate' check (status in (
        'candidate', 'accepted', 'rejected', 'superseded'
    )),
    created_at         timestamptz not null default now(),
    created_by         text,
    metadata           jsonb not null default '{}'::jsonb
);

create index if not exists donto_idproposal_kind_idx
    on donto_identity_proposal (hypothesis_kind);
create index if not exists donto_idproposal_status_idx
    on donto_identity_proposal (status) where status <> 'accepted';
create index if not exists donto_idproposal_entities_gin
    on donto_identity_proposal using gin (entity_refs);
create index if not exists donto_idproposal_context_idx
    on donto_identity_proposal (context_id) where context_id is not null;
create index if not exists donto_idproposal_anchors_gin
    on donto_identity_proposal using gin (evidence_anchor_ids);

-- Register an identity proposal.
create or replace function donto_register_identity_proposal(
    p_kind          text,
    p_entity_refs   text[],
    p_confidence    double precision default 0.5,
    p_method        text default 'human',
    p_anchors       text[] default '{}',
    p_context_id    text default null,
    p_created_by    text default null,
    p_metadata      jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    insert into donto_identity_proposal
        (hypothesis_kind, entity_refs, confidence, method,
         evidence_anchor_ids, context_id, created_by, metadata)
    values
        (p_kind, p_entity_refs, p_confidence, p_method,
         p_anchors, p_context_id, p_created_by, p_metadata)
    returning proposal_id into v_id;

    perform donto_emit_event(
        'identity_hypothesis', v_id::text, 'created',
        coalesce(p_created_by, 'system'),
        jsonb_build_object('kind', p_kind, 'method', p_method)
    );
    return v_id;
end;
$$;

-- Update proposal status (accept / reject / supersede).
create or replace function donto_set_identity_proposal_status(
    p_proposal_id uuid,
    p_status      text,
    p_actor       text default 'system',
    p_notes       text default null
) returns void
language plpgsql as $$
begin
    update donto_identity_proposal
    set status = p_status,
        metadata = metadata || jsonb_build_object(
            'status_history',
            coalesce(metadata->'status_history', '[]'::jsonb) ||
            jsonb_build_array(jsonb_build_object(
                'status', p_status,
                'actor', p_actor,
                'at', now(),
                'notes', p_notes
            ))
        )
    where proposal_id = p_proposal_id;

    perform donto_emit_event(
        'identity_hypothesis', p_proposal_id::text,
        case p_status
            when 'accepted' then 'approved'
            when 'rejected' then 'rejected'
            when 'superseded' then 'superseded'
            else 'updated'
        end,
        p_actor,
        jsonb_build_object('status', p_status, 'notes', p_notes)
    );
end;
$$;
