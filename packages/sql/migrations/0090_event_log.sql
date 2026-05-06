-- v1000 / I3: append-only event log for non-statement objects.
--
-- donto_statement is already append-only (retraction closes tx_time;
-- correction creates a new row). This migration extends the same
-- discipline to alignments, identity hypotheses, policies,
-- attestations, and reviews, which are otherwise mutated in place.
--
-- Every state-changing operation on those object families writes a
-- row here. Current-state tables remain queryable for fast access;
-- the event log is the authoritative history.

create table if not exists donto_event_log (
    event_id        bigserial primary key,
    target_kind     text not null check (target_kind in (
        'alignment', 'identity_hypothesis', 'identity_edge',
        'policy', 'access_assignment', 'attestation',
        'review_decision', 'release', 'predicate_descriptor',
        'frame', 'frame_role', 'context'
    )),
    target_id       text not null,
    event_type      text not null check (event_type in (
        'created', 'updated', 'retracted', 'superseded', 'revoked',
        'expired', 'merged', 'split', 'approved', 'rejected',
        'qualified', 'escalated', 'deferred'
    )),
    occurred_at     timestamptz not null default now(),
    actor           text not null default 'system',
    payload         jsonb not null default '{}'::jsonb,
    prior_event_id  bigint references donto_event_log(event_id),
    request_id      text
);

create index if not exists donto_event_log_target_idx
    on donto_event_log (target_kind, target_id, occurred_at desc);
create index if not exists donto_event_log_actor_idx
    on donto_event_log (actor, occurred_at desc);
create index if not exists donto_event_log_type_idx
    on donto_event_log (event_type);
create index if not exists donto_event_log_request_idx
    on donto_event_log (request_id) where request_id is not null;

-- Emit a new event. Returns the event_id.
create or replace function donto_emit_event(
    p_target_kind text,
    p_target_id   text,
    p_event_type  text,
    p_actor       text default 'system',
    p_payload     jsonb default '{}'::jsonb,
    p_prior       bigint default null,
    p_request_id  text default null
) returns bigint
language plpgsql as $$
declare
    v_id bigint;
begin
    insert into donto_event_log
        (target_kind, target_id, event_type, actor, payload, prior_event_id, request_id)
    values
        (p_target_kind, p_target_id, p_event_type, p_actor, p_payload, p_prior, p_request_id)
    returning event_id into v_id;
    return v_id;
end;
$$;

-- Reconstruct the event chain for a target.
create or replace function donto_event_history(
    p_target_kind text,
    p_target_id   text,
    p_limit       int default 100
) returns table(
    event_id bigint, event_type text, occurred_at timestamptz,
    actor text, payload jsonb
)
language sql stable as $$
    select event_id, event_type, occurred_at, actor, payload
    from donto_event_log
    where target_kind = p_target_kind and target_id = p_target_id
    order by occurred_at desc, event_id desc
    limit p_limit
$$;
