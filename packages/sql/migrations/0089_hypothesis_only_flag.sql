-- v1000 / I1: hypothesis-only flag for claims without evidence.
--
-- Sparse overlay on donto_statement so we don't rewrite 35M+ rows.
-- A statement marked hypothesis_only is allowed to lack evidence
-- but must never reach maturity >= E2 (linked) and must never be
-- included in a public release. Application layer enforces both
-- gates; this migration provides the storage and helper functions.

create table if not exists donto_stmt_hypothesis_only (
    statement_id    uuid primary key
                    references donto_statement(statement_id) on delete cascade,
    marked_at       timestamptz not null default now(),
    marker_agent    text,
    rationale       text,
    metadata        jsonb not null default '{}'::jsonb
);

create index if not exists donto_stmt_hypothesis_only_marker_idx
    on donto_stmt_hypothesis_only (marker_agent)
    where marker_agent is not null;

-- Mark a statement as hypothesis-only (idempotent).
create or replace function donto_mark_hypothesis_only(
    p_statement_id uuid,
    p_agent        text default null,
    p_rationale    text default null
) returns void
language plpgsql as $$
begin
    insert into donto_stmt_hypothesis_only
        (statement_id, marker_agent, rationale)
    values (p_statement_id, p_agent, p_rationale)
    on conflict (statement_id) do update set
        marker_agent = excluded.marker_agent,
        rationale    = excluded.rationale,
        marked_at    = now();
end;
$$;

-- Test: is this statement marked hypothesis-only?
create or replace function donto_is_hypothesis_only(p_statement_id uuid)
returns boolean
language sql stable as $$
    select exists (
        select 1 from donto_stmt_hypothesis_only
        where statement_id = p_statement_id
    )
$$;

-- Maturity-promotion gate: returns true iff promotion is allowed.
-- Application layer should call this before raising maturity.
create or replace function donto_can_promote_maturity(
    p_statement_id  uuid,
    p_target_level  int
) returns boolean
language plpgsql stable as $$
declare
    v_hypothesis_only boolean;
begin
    select donto_is_hypothesis_only(p_statement_id) into v_hypothesis_only;
    -- Hypothesis-only claims cannot rise above E1 (parsed/candidate).
    if v_hypothesis_only and p_target_level >= 2 then
        return false;
    end if;
    return true;
end;
$$;
