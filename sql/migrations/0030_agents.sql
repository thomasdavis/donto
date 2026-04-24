-- Evidence substrate §8: agent registry and workspaces.
--
-- Agents are the actors that produce and consume statements. They may
-- be humans, LLMs, rule engines, extractors, validators, or curators.
-- Agent-context bindings define workspace ownership: which contexts
-- an agent controls.

create table if not exists donto_agent (
    agent_id     uuid primary key default gen_random_uuid(),
    iri          text not null unique,
    label        text,
    agent_type   text not null check (agent_type in (
        'human', 'llm', 'rule_engine', 'extractor',
        'validator', 'curator', 'system', 'custom'
    )),
    model_id     text,
    metadata     jsonb not null default '{}'::jsonb,
    created_at   timestamptz not null default now()
);

create index if not exists donto_agent_type_idx
    on donto_agent (agent_type);
create index if not exists donto_agent_model_idx
    on donto_agent (model_id) where model_id is not null;

-- Bind agents to contexts. Roles define the access level:
--   owner       — full control, can create sub-contexts
--   contributor — can assert/retract within the context
--   reader      — read-only access (advisory, not enforced in Phase 0)
create table if not exists donto_agent_context (
    agent_id   uuid not null references donto_agent(agent_id),
    context    text not null references donto_context(iri),
    role       text not null default 'owner'
               check (role in ('owner','contributor','reader')),
    bound_at   timestamptz not null default now(),
    primary key (agent_id, context)
);

create index if not exists donto_agent_context_ctx_idx
    on donto_agent_context (context);

-- Idempotent agent ensure.
create or replace function donto_ensure_agent(
    p_iri        text,
    p_agent_type text default 'custom',
    p_label      text default null,
    p_model_id   text default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    select agent_id into v_id from donto_agent where iri = p_iri;
    if v_id is not null then return v_id; end if;
    insert into donto_agent (iri, agent_type, label, model_id)
    values (p_iri, p_agent_type, p_label, p_model_id)
    on conflict (iri) do nothing
    returning agent_id into v_id;
    if v_id is null then
        select agent_id into v_id from donto_agent where iri = p_iri;
    end if;
    return v_id;
end;
$$;

-- Bind an agent to a context.
create or replace function donto_bind_agent_context(
    p_agent_id uuid,
    p_context  text,
    p_role     text default 'owner'
) returns void
language plpgsql as $$
begin
    perform donto_ensure_context(p_context);
    insert into donto_agent_context (agent_id, context, role)
    values (p_agent_id, p_context, p_role)
    on conflict (agent_id, context) do update
        set role = excluded.role, bound_at = now();
end;
$$;

-- List contexts an agent has access to, with roles.
create or replace function donto_agent_contexts(p_agent_id uuid)
returns table(context text, role text)
language sql stable as $$
    select context, role from donto_agent_context
    where agent_id = p_agent_id
$$;

-- Agents bound to a context.
create or replace function donto_context_agents(p_context text)
returns table(agent_id uuid, iri text, agent_type text, role text)
language sql stable as $$
    select a.agent_id, a.iri, a.agent_type, ac.role
    from donto_agent_context ac
    join donto_agent a using (agent_id)
    where ac.context = p_context
$$;
