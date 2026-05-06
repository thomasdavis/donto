-- v1000 / §6.4 ClaimRecord multi-context support.
--
-- A claim may belong to multiple context scopes (e.g., a source context
-- AND a hypothesis context AND a project context). Migration 0001 has a
-- single context column on donto_statement. Rather than rewrite that,
-- we add a junction table donto_statement_context for additional
-- contexts. The original column remains the "primary" context.
--
-- Reads honoring multi-context membership use the new view
-- donto_v_statement_contexts which UNIONs primary + junction rows.

create table if not exists donto_statement_context (
    statement_id    uuid not null references donto_statement(statement_id) on delete cascade,
    context         text not null references donto_context(iri),
    role            text not null default 'secondary'
                    check (role in ('secondary', 'derivation', 'hypothesis_lens',
                                    'identity_lens', 'schema_lens', 'review_lens',
                                    'release_lens', 'project_lens')),
    added_at        timestamptz not null default now(),
    added_by        text,
    primary key (statement_id, context, role)
);

create index if not exists donto_stmt_ctx_context_idx
    on donto_statement_context (context);
create index if not exists donto_stmt_ctx_role_idx
    on donto_statement_context (role);

-- Add a context membership.
create or replace function donto_add_statement_context(
    p_statement_id uuid,
    p_context      text,
    p_role         text default 'secondary',
    p_added_by     text default null
) returns void
language plpgsql as $$
begin
    perform donto_ensure_context(p_context);
    insert into donto_statement_context
        (statement_id, context, role, added_by)
    values
        (p_statement_id, p_context, p_role, p_added_by)
    on conflict (statement_id, context, role) do nothing;
end;
$$;

-- Remove a context membership (does not touch the primary context).
create or replace function donto_remove_statement_context(
    p_statement_id uuid,
    p_context      text,
    p_role         text default 'secondary'
) returns boolean
language plpgsql as $$
declare
    v_n int;
begin
    delete from donto_statement_context
    where statement_id = p_statement_id
      and context = p_context
      and role = p_role;
    get diagnostics v_n = row_count;
    return v_n > 0;
end;
$$;

-- Unified view of a statement's contexts (primary + secondary).
create or replace view donto_v_statement_contexts as
    select s.statement_id, s.context, 'primary'::text as role,
           null::timestamptz as added_at, null::text as added_by
    from donto_statement s
    union all
    select sc.statement_id, sc.context, sc.role, sc.added_at, sc.added_by
    from donto_statement_context sc;

-- Reverse lookup: list all statements in a context (primary or secondary).
create or replace function donto_statements_in_context(
    p_context text,
    p_limit   int default 1000
) returns table(statement_id uuid, role text)
language sql stable as $$
    select statement_id, role from donto_v_statement_contexts
    where context = p_context
    limit p_limit
$$;
