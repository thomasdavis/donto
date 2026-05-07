-- Trust Kernel / §6.5 FrameRole: typed, indexed role fillers.
--
-- A frame role is one (frame, role-name, value) tuple plus optional
-- evidence anchors and ranking. Roles are indexed so cross-frame
-- queries like "which paradigm cells fill the GENDER=feminine role?"
-- are first-class.

create table if not exists donto_frame_role (
    role_id          bigserial primary key,
    frame_id         uuid not null references donto_claim_frame(frame_id) on delete cascade,
    role             text not null,
    value_kind       text not null check (value_kind in (
        'entity', 'literal', 'claim_ref', 'frame_ref', 'expression'
    )),
    value_ref        text,
    value_literal    jsonb,
    evidence_anchor_ids text[] not null default '{}',
    rank             int,
    notes            text,
    created_at       timestamptz not null default now(),
    constraint donto_frame_role_value_present
        check (value_ref is not null or value_literal is not null
               or value_kind = 'expression')
);

create index if not exists donto_frame_role_frame_idx
    on donto_frame_role (frame_id, role);
create index if not exists donto_frame_role_role_idx
    on donto_frame_role (role);
create index if not exists donto_frame_role_value_ref_idx
    on donto_frame_role (value_ref) where value_ref is not null;
create index if not exists donto_frame_role_anchors_gin
    on donto_frame_role using gin (evidence_anchor_ids);

-- Add a role to a frame.
create or replace function donto_add_frame_role(
    p_frame_id      uuid,
    p_role          text,
    p_value_kind    text,
    p_value_ref     text default null,
    p_value_literal jsonb default null,
    p_anchors       text[] default '{}',
    p_rank          int default null,
    p_notes         text default null
) returns bigint
language plpgsql as $$
declare
    v_id bigint;
begin
    insert into donto_frame_role
        (frame_id, role, value_kind, value_ref, value_literal,
         evidence_anchor_ids, rank, notes)
    values
        (p_frame_id, p_role, p_value_kind, p_value_ref, p_value_literal,
         p_anchors, p_rank, p_notes)
    returning role_id into v_id;

    perform donto_emit_event(
        'frame_role', v_id::text, 'created', 'system',
        jsonb_build_object('frame_id', p_frame_id, 'role', p_role)
    );
    return v_id;
end;
$$;

-- List all roles for a frame (ordered by rank then insertion).
create or replace function donto_frame_roles(p_frame_id uuid)
returns table(
    role_id bigint, role text, value_kind text,
    value_ref text, value_literal jsonb,
    evidence_anchor_ids text[], rank int
)
language sql stable as $$
    select role_id, role, value_kind, value_ref, value_literal,
           evidence_anchor_ids, rank
    from donto_frame_role
    where frame_id = p_frame_id
    order by coalesce(rank, 9999), role_id
$$;

-- Reverse lookup: which frames contain this entity in this role?
create or replace function donto_frames_with_role_value(
    p_role      text,
    p_value_ref text,
    p_limit     int default 100
) returns table(frame_id uuid, frame_type text, role_id bigint)
language sql stable as $$
    select fr.frame_id, f.frame_type, fr.role_id
    from donto_frame_role fr
    join donto_claim_frame f using (frame_id)
    where fr.role = p_role and fr.value_ref = p_value_ref
      and f.status = 'active'
    limit p_limit
$$;
