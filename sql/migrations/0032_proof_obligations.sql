-- Evidence substrate §10: proof obligations.
--
-- When extraction cannot resolve coreference, temporal grounding,
-- source support, or normalization, it emits a candidate statement
-- plus an obligation. Obligations are structured work items for
-- agents: they track what needs to be done to promote a raw
-- extraction to a higher maturity level.
--
-- Obligations turn extraction failure into structured work rather
-- than silent data loss.

create table if not exists donto_proof_obligation (
    obligation_id    uuid primary key default gen_random_uuid(),
    statement_id     uuid references donto_statement(statement_id),
    obligation_type  text not null check (obligation_type in (
        'needs-coref', 'needs-temporal-grounding', 'needs-source-support',
        'needs-unit-normalization', 'needs-entity-disambiguation',
        'needs-relation-validation', 'needs-human-review',
        'needs-confidence-boost', 'needs-context-resolution', 'custom'
    )),
    status           text not null default 'open'
                     check (status in (
                         'open', 'in_progress', 'resolved', 'rejected', 'deferred'
                     )),
    priority         smallint not null default 0,
    context          text not null references donto_context(iri),
    assigned_agent   uuid references donto_agent(agent_id),
    resolved_by      uuid references donto_agent(agent_id),
    detail           jsonb,
    created_at       timestamptz not null default now(),
    resolved_at      timestamptz,
    metadata         jsonb not null default '{}'::jsonb
);

create index if not exists donto_proof_obligation_stmt_idx
    on donto_proof_obligation (statement_id)
    where statement_id is not null;
create index if not exists donto_proof_obligation_open_idx
    on donto_proof_obligation (status, priority desc)
    where status = 'open';
create index if not exists donto_proof_obligation_type_idx
    on donto_proof_obligation (obligation_type);
create index if not exists donto_proof_obligation_assigned_idx
    on donto_proof_obligation (assigned_agent)
    where assigned_agent is not null;
create index if not exists donto_proof_obligation_context_idx
    on donto_proof_obligation (context);

-- Emit a proof obligation for a statement.
create or replace function donto_emit_obligation(
    p_statement_id    uuid,
    p_obligation_type text,
    p_context         text default 'donto:anonymous',
    p_priority        smallint default 0,
    p_detail          jsonb default null,
    p_assigned_agent  uuid default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    perform donto_ensure_context(p_context);
    insert into donto_proof_obligation
        (statement_id, obligation_type, context, priority, detail, assigned_agent)
    values (p_statement_id, p_obligation_type, p_context, p_priority, p_detail, p_assigned_agent)
    returning obligation_id into v_id;
    return v_id;
end;
$$;

-- Resolve an obligation.
create or replace function donto_resolve_obligation(
    p_obligation_id uuid,
    p_resolved_by   uuid default null,
    p_status        text default 'resolved'
) returns boolean
language plpgsql as $$
declare
    v_n int;
begin
    update donto_proof_obligation
    set status      = p_status,
        resolved_by = p_resolved_by,
        resolved_at = now()
    where obligation_id = p_obligation_id
      and status in ('open', 'in_progress');
    get diagnostics v_n = row_count;
    return v_n > 0;
end;
$$;

-- Assign an obligation to an agent.
create or replace function donto_assign_obligation(
    p_obligation_id uuid,
    p_agent_id      uuid
) returns void
language plpgsql as $$
begin
    update donto_proof_obligation
    set assigned_agent = p_agent_id,
        status = 'in_progress'
    where obligation_id = p_obligation_id
      and status = 'open';
end;
$$;

-- Open obligations, optionally filtered by type and/or context.
create or replace function donto_open_obligations(
    p_obligation_type text default null,
    p_context         text default null,
    p_limit           int default 100
) returns table(
    obligation_id uuid, statement_id uuid,
    obligation_type text, priority smallint,
    context text, assigned_agent uuid,
    detail jsonb, created_at timestamptz
)
language sql stable as $$
    select obligation_id, statement_id,
           obligation_type, priority,
           context, assigned_agent,
           detail, created_at
    from donto_proof_obligation
    where status = 'open'
      and (p_obligation_type is null or obligation_type = p_obligation_type)
      and (p_context is null or context = p_context)
    order by priority desc, created_at
    limit p_limit
$$;

-- Summary: obligation counts by type and status.
create or replace function donto_obligation_summary(
    p_context text default null
) returns table(obligation_type text, status text, cnt bigint)
language sql stable as $$
    select obligation_type, status, count(*)
    from donto_proof_obligation
    where (p_context is null or context = p_context)
    group by obligation_type, status
$$;
