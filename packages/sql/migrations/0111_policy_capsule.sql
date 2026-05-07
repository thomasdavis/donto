-- Trust Kernel / §6.12 PolicyCapsule — the Trust Kernel.
--
-- The largest single feature of v1000 and the M0 deliverable. A policy
-- capsule governs source access, derived data, export, model use, and
-- release eligibility with fifteen distinct allowed actions:
--
--   read_metadata, read_content, quote, view_anchor_location,
--   derive_claims, derive_embeddings, translate, summarize,
--   export_claims, export_sources, export_anchors,
--   train_model, publish_release, share_with_third_party,
--   federated_query
--
-- Inheritance defaults to max_restriction. Unknown policy is restricted.
-- Authority refs are agent or organisation IRIs. Revocation is recorded
-- via the event log (migration 0090).

create table if not exists donto_policy_capsule (
    policy_id           uuid primary key default gen_random_uuid(),
    policy_iri          text not null unique,
    policy_kind         text not null check (policy_kind in (
        'public',
        'open_metadata_restricted_content',
        'community_restricted',
        'embargoed',
        'licensed',
        'private',
        'regulated',
        'sealed',
        'unknown_restricted'
    )),
    authority_refs      text[] not null default '{}',
    allowed_actions     jsonb not null default jsonb_build_object(
        'read_metadata',         false,
        'read_content',          false,
        'quote',                 false,
        'view_anchor_location',  false,
        'derive_claims',         false,
        'derive_embeddings',     false,
        'translate',             false,
        'summarize',             false,
        'export_claims',         false,
        'export_sources',        false,
        'export_anchors',        false,
        'train_model',           false,
        'publish_release',       false,
        'share_with_third_party',false,
        'federated_query',       false
    ),
    inheritance_rule    text not null default 'max_restriction'
                        check (inheritance_rule in (
                            'max_restriction', 'source_policy', 'authority_override_only'
                        )),
    expiry              timestamptz,
    revocation_status   text not null default 'active'
                        check (revocation_status in (
                            'active', 'revoked', 'expired', 'superseded'
                        )),
    human_readable_summary text,
    labels              jsonb not null default '{}'::jsonb,
                        -- TK / BC labels: {"tk": [...], "bc": [...]}
    created_at          timestamptz not null default now(),
    created_by          text not null default 'system',
    metadata            jsonb not null default '{}'::jsonb
);

create index if not exists donto_policy_kind_idx
    on donto_policy_capsule (policy_kind);
create index if not exists donto_policy_revocation_idx
    on donto_policy_capsule (revocation_status) where revocation_status <> 'active';
create index if not exists donto_policy_authority_gin
    on donto_policy_capsule using gin (authority_refs);

-- Default policies. These are referenced by code paths that need a
-- canonical policy_iri without proprietary configuration.
insert into donto_policy_capsule
    (policy_iri, policy_kind, allowed_actions, inheritance_rule,
     human_readable_summary, created_by)
values
    ('policy:default/public', 'public',
     jsonb_build_object(
        'read_metadata', true, 'read_content', true, 'quote', true,
        'view_anchor_location', true, 'derive_claims', true,
        'derive_embeddings', true, 'translate', true, 'summarize', true,
        'export_claims', true, 'export_sources', true, 'export_anchors', true,
        'train_model', false, 'publish_release', true,
        'share_with_third_party', true, 'federated_query', true),
     'max_restriction',
     'Public domain or openly licensed material; train_model still requires explicit grant.',
     'system'),
    ('policy:default/restricted_pending_review', 'unknown_restricted',
     jsonb_build_object(
        'read_metadata', true, 'read_content', false, 'quote', false,
        'view_anchor_location', false, 'derive_claims', false,
        'derive_embeddings', false, 'translate', false, 'summarize', false,
        'export_claims', false, 'export_sources', false, 'export_anchors', false,
        'train_model', false, 'publish_release', false,
        'share_with_third_party', false, 'federated_query', false),
     'max_restriction',
     'Default policy for any source whose policy is not yet classified.',
     'system'),
    ('policy:default/community_restricted', 'community_restricted',
     jsonb_build_object(
        'read_metadata', true, 'read_content', false, 'quote', false,
        'view_anchor_location', false, 'derive_claims', false,
        'derive_embeddings', false, 'translate', false, 'summarize', false,
        'export_claims', false, 'export_sources', false, 'export_anchors', false,
        'train_model', false, 'publish_release', false,
        'share_with_third_party', false, 'federated_query', false),
     'max_restriction',
     'Material under community authority; access by community attestation only.',
     'system'),
    ('policy:default/private_research', 'private',
     jsonb_build_object(
        'read_metadata', true, 'read_content', true, 'quote', true,
        'view_anchor_location', true, 'derive_claims', true,
        'derive_embeddings', true, 'translate', true, 'summarize', true,
        'export_claims', false, 'export_sources', false, 'export_anchors', false,
        'train_model', false, 'publish_release', false,
        'share_with_third_party', false, 'federated_query', false),
     'max_restriction',
     'Internal project use only; no export, training, or publication.',
     'system')
on conflict (policy_iri) do nothing;

-- Now that the policy table exists, wire the soft-references from
-- earlier migrations (0095, 0105, 0108) to actual FKs.
-- Note: the FKs are added with NOT VALID semantics so existing data
-- with NULL policy_id keeps working. New writes are validated.
alter table donto_document
    drop constraint if exists donto_document_policy_fk;
alter table donto_document
    add constraint donto_document_policy_fk
    foreign key (policy_id) references donto_policy_capsule(policy_iri)
    on delete restrict not valid;

alter table donto_claim_frame
    drop constraint if exists donto_claim_frame_policy_fk;
alter table donto_claim_frame
    add constraint donto_claim_frame_policy_fk
    foreign key (policy_id) references donto_policy_capsule(policy_iri)
    on delete restrict not valid;

alter table donto_entity_symbol
    drop constraint if exists donto_entity_symbol_policy_fk;
alter table donto_entity_symbol
    add constraint donto_entity_symbol_policy_fk
    foreign key (policy_id) references donto_policy_capsule(policy_iri)
    on delete restrict not valid;

-- Access assignment: which policy applies to which target.
-- A target may have multiple assignments (e.g., source-level + project-level);
-- the inheritance_rule determines which wins.
create table if not exists donto_access_assignment (
    assignment_id   uuid primary key default gen_random_uuid(),
    target_kind     text not null check (target_kind in (
        'document', 'document_revision', 'span', 'context',
        'statement', 'frame', 'release', 'entity', 'predicate'
    )),
    target_id       text not null,
    policy_iri      text not null references donto_policy_capsule(policy_iri),
    assigned_by     text not null,
    assigned_at     timestamptz not null default now(),
    valid_time      daterange not null default daterange(null, null, '[)'),
    notes           text,
    constraint donto_access_assignment_uniq
        unique (target_kind, target_id, policy_iri)
);

create index if not exists donto_access_assignment_target_idx
    on donto_access_assignment (target_kind, target_id);
create index if not exists donto_access_assignment_policy_idx
    on donto_access_assignment (policy_iri);

-- Resolve effective policy actions for a target.
-- Combines all policies assigned to the target under max_restriction:
-- an action is allowed iff every policy permits it.
create or replace function donto_effective_actions(
    p_target_kind text,
    p_target_id   text
) returns jsonb
language plpgsql stable as $$
declare
    v_keys text[] := array[
        'read_metadata', 'read_content', 'quote', 'view_anchor_location',
        'derive_claims', 'derive_embeddings', 'translate', 'summarize',
        'export_claims', 'export_sources', 'export_anchors',
        'train_model', 'publish_release', 'share_with_third_party',
        'federated_query'
    ];
    v_result jsonb := '{}';
    v_key    text;
    v_allowed boolean;
    v_count   int;
begin
    select count(*) into v_count
    from donto_access_assignment
    where target_kind = p_target_kind and target_id = p_target_id;

    if v_count = 0 then
        -- No policy assigned → fall through to default-restricted.
        return (select allowed_actions
                from donto_policy_capsule
                where policy_iri = 'policy:default/restricted_pending_review');
    end if;

    foreach v_key in array v_keys loop
        select bool_and(coalesce((p.allowed_actions->>v_key)::boolean, false))
        into v_allowed
        from donto_access_assignment a
        join donto_policy_capsule p on p.policy_iri = a.policy_iri
        where a.target_kind = p_target_kind
          and a.target_id = p_target_id
          and p.revocation_status = 'active'
          and (p.expiry is null or p.expiry > now());
        v_result := v_result || jsonb_build_object(v_key, coalesce(v_allowed, false));
    end loop;

    return v_result;
end;
$$;

-- Quick allow/deny check.
create or replace function donto_action_allowed(
    p_target_kind text,
    p_target_id   text,
    p_action      text
) returns boolean
language sql stable as $$
    select coalesce((donto_effective_actions(p_target_kind, p_target_id) ->> p_action)::boolean, false)
$$;

-- Assign a policy.
create or replace function donto_assign_policy(
    p_target_kind text,
    p_target_id   text,
    p_policy_iri  text,
    p_assigned_by text default 'system',
    p_notes       text default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    insert into donto_access_assignment
        (target_kind, target_id, policy_iri, assigned_by, notes)
    values
        (p_target_kind, p_target_id, p_policy_iri, p_assigned_by, p_notes)
    on conflict (target_kind, target_id, policy_iri) do update set
        assigned_by = excluded.assigned_by,
        notes       = coalesce(excluded.notes, donto_access_assignment.notes)
    returning assignment_id into v_id;

    perform donto_emit_event(
        'access_assignment', v_id::text, 'created',
        p_assigned_by,
        jsonb_build_object(
            'target_kind', p_target_kind,
            'target_id',   p_target_id,
            'policy_iri',  p_policy_iri
        )
    );
    return v_id;
end;
$$;
