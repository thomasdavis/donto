-- Trust Kernel / §6.5 ClaimFrame: n-ary analyses with indexed roles.
--
-- Migration 0054 (event_frames) introduced a frame pattern based on
-- frame nodes plus role predicates. That works for ad-hoc decomposition
-- but doesn't index roles for cross-frame queries. PRD §6.5 wants
-- structured frames with typed role indexing.
--
-- We add donto_claim_frame (the frame header) here. Roles land in 0106.

create table if not exists donto_claim_frame (
    frame_id              uuid primary key default gen_random_uuid(),
    frame_type            text not null,
    frame_schema_version  text not null default 'frame-schema-1',
    primary_context       text not null references donto_context(iri),
    policy_id             text,                              -- FK in 0111
    label                 text,
    constraints           jsonb not null default '[]'::jsonb,
    created_at            timestamptz not null default now(),
    created_by            text,
    status                text not null default 'active'
                          check (status in ('draft', 'active', 'superseded', 'retracted')),
    metadata              jsonb not null default '{}'::jsonb
);

create index if not exists donto_claim_frame_type_idx
    on donto_claim_frame (frame_type);
create index if not exists donto_claim_frame_context_idx
    on donto_claim_frame (primary_context);
create index if not exists donto_claim_frame_status_idx
    on donto_claim_frame (status) where status <> 'active';
create index if not exists donto_claim_frame_policy_idx
    on donto_claim_frame (policy_id) where policy_id is not null;

-- Create a frame.
create or replace function donto_create_claim_frame(
    p_frame_type      text,
    p_primary_context text,
    p_label           text default null,
    p_policy_id       text default null,
    p_constraints     jsonb default '[]'::jsonb,
    p_created_by      text default null,
    p_metadata        jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    perform donto_ensure_context(p_primary_context);

    insert into donto_claim_frame
        (frame_type, primary_context, label, policy_id, constraints,
         created_by, metadata)
    values
        (p_frame_type, p_primary_context, p_label, p_policy_id, p_constraints,
         p_created_by, p_metadata)
    returning frame_id into v_id;

    perform donto_emit_event(
        'frame', v_id::text, 'created',
        coalesce(p_created_by, 'system'),
        jsonb_build_object('frame_type', p_frame_type)
    );
    return v_id;
end;
$$;

create or replace function donto_set_frame_status(
    p_frame_id uuid,
    p_status   text,
    p_actor    text default 'system'
) returns void
language plpgsql as $$
begin
    update donto_claim_frame
    set status = p_status
    where frame_id = p_frame_id;

    perform donto_emit_event(
        'frame', p_frame_id::text,
        case p_status
            when 'retracted' then 'retracted'
            when 'superseded' then 'superseded'
            else 'updated'
        end,
        p_actor,
        jsonb_build_object('status', p_status)
    );
end;
$$;
