-- Trust Kernel / FR-012 review workflow.
--
-- Reactions (migration 0017) are folksonomic, not structured review.
-- This release introduces donto_review_decision: structured, citable,
-- per-target reviewer decisions with rationale.
--
-- A review decision targets one of:
--   claim | alignment | identity | policy | release | anchor | source
-- and applies one of nine decisions:
--   accept | reject | qualify | request_evidence | merge | split
--   | escalate | mark_sensitive | defer

create table if not exists donto_review_decision (
    review_id        uuid primary key default gen_random_uuid(),
    target_type      text not null check (target_type in (
        'claim', 'alignment', 'identity', 'policy', 'release',
        'anchor', 'source', 'frame', 'predicate', 'attestation'
    )),
    target_id        text not null,
    decision         text not null check (decision in (
        'accept', 'reject', 'qualify', 'request_evidence',
        'merge', 'split', 'escalate', 'mark_sensitive', 'defer'
    )),
    reviewer_id      text not null,
    review_context   text references donto_context(iri),
    rationale        text not null,
    confidence       double precision
                     check (confidence is null or
                            (confidence >= 0 and confidence <= 1)),
    related_decision_id uuid references donto_review_decision(review_id),
        -- For decisions that supersede or relate to a prior review
    policy_iri       text references donto_policy_capsule(policy_iri),
    created_at       timestamptz not null default now(),
    metadata         jsonb not null default '{}'::jsonb,
    constraint donto_review_decision_rationale_present
        check (length(trim(rationale)) > 0)
);

create index if not exists donto_review_target_idx
    on donto_review_decision (target_type, target_id);
create index if not exists donto_review_reviewer_idx
    on donto_review_decision (reviewer_id, created_at desc);
create index if not exists donto_review_decision_idx
    on donto_review_decision (decision);
create index if not exists donto_review_context_idx
    on donto_review_decision (review_context) where review_context is not null;
create index if not exists donto_review_related_idx
    on donto_review_decision (related_decision_id) where related_decision_id is not null;

-- Record a review decision.
create or replace function donto_record_review(
    p_target_type    text,
    p_target_id      text,
    p_decision       text,
    p_reviewer_id    text,
    p_rationale      text,
    p_review_context text default null,
    p_confidence     double precision default null,
    p_related        uuid default null,
    p_policy_iri     text default null,
    p_metadata       jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    if p_review_context is not null then
        perform donto_ensure_context(p_review_context);
    end if;

    insert into donto_review_decision
        (target_type, target_id, decision, reviewer_id,
         review_context, rationale, confidence,
         related_decision_id, policy_iri, metadata)
    values
        (p_target_type, p_target_id, p_decision, p_reviewer_id,
         p_review_context, p_rationale, p_confidence,
         p_related, p_policy_iri, p_metadata)
    returning review_id into v_id;

    perform donto_emit_event(
        'review_decision', v_id::text,
        case p_decision
            when 'accept' then 'approved'
            when 'reject' then 'rejected'
            when 'qualify' then 'qualified'
            when 'escalate' then 'escalated'
            when 'defer' then 'deferred'
            else 'created'
        end,
        p_reviewer_id,
        jsonb_build_object(
            'target_type', p_target_type,
            'target_id', p_target_id,
            'decision', p_decision
        )
    );
    return v_id;
end;
$$;

-- Most-recent decision for a target.
create or replace function donto_latest_review(
    p_target_type text,
    p_target_id   text
) returns uuid
language sql stable as $$
    select review_id from donto_review_decision
    where target_type = p_target_type and target_id = p_target_id
    order by created_at desc
    limit 1
$$;

-- Review queue: open targets without an accept/reject decision yet.
-- This is intentionally simple; production queues will overlay
-- policy + maturity + obligation filters.
create or replace function donto_review_queue(
    p_target_type text default null,
    p_limit       int default 100
) returns table(
    target_type text, target_id text,
    last_decision text, last_review_at timestamptz, review_count bigint
)
language sql stable as $$
    select target_type, target_id,
           (array_agg(decision order by created_at desc))[1] as last_decision,
           max(created_at) as last_review_at,
           count(*) as review_count
    from donto_review_decision
    where p_target_type is null or target_type = p_target_type
    group by target_type, target_id
    order by max(created_at) desc
    limit p_limit
$$;
