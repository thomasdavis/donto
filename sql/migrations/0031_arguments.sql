-- Evidence substrate §9: argumentation layer.
--
-- Structured support/attack relations between statements. Extends the
-- reaction predicates (0017) with richer argument structure including
-- strength, agent attribution, and bitemporal lifecycle. Arguments
-- are the judgment layer: claims about claims.
--
-- Relations follow argumentation framework conventions:
--   supports     — source provides evidence for target
--   rebuts       — source directly contradicts target's conclusion
--   undercuts    — source attacks the reasoning behind target
--   endorses     — source author agrees with target (weaker than supports)
--   supersedes   — source replaces target (newer/better version)
--   qualifies    — source limits/constrains target's scope
--   potentially_same — source and target may refer to same entity/event
--   same_referent    — source and target refer to same real-world entity
--   same_event       — source and target describe the same event

create table if not exists donto_argument (
    argument_id          uuid primary key default gen_random_uuid(),
    source_statement_id  uuid not null references donto_statement(statement_id),
    target_statement_id  uuid not null references donto_statement(statement_id),
    relation             text not null check (relation in (
        'supports', 'rebuts', 'undercuts',
        'endorses', 'supersedes', 'qualifies',
        'potentially_same', 'same_referent', 'same_event'
    )),
    strength             double precision,
    context              text not null references donto_context(iri),
    agent_id             uuid references donto_agent(agent_id),
    evidence             jsonb,
    tx_time              tstzrange not null default tstzrange(now(), null, '[)'),
    metadata             jsonb not null default '{}'::jsonb,
    constraint donto_argument_no_self
        check (source_statement_id <> target_statement_id),
    constraint donto_argument_tx_lower_inc
        check (lower_inc(tx_time)),
    constraint donto_argument_strength_range
        check (strength is null or (strength >= 0.0 and strength <= 1.0))
);

create unique index if not exists donto_argument_open_uniq
    on donto_argument (source_statement_id, target_statement_id, relation, context)
    where upper(tx_time) is null;

create index if not exists donto_argument_source_idx
    on donto_argument (source_statement_id);
create index if not exists donto_argument_target_idx
    on donto_argument (target_statement_id);
create index if not exists donto_argument_relation_idx
    on donto_argument (relation) where upper(tx_time) is null;
create index if not exists donto_argument_context_idx
    on donto_argument (context);
create index if not exists donto_argument_agent_idx
    on donto_argument (agent_id) where agent_id is not null;

-- Assert an argument. Idempotent: if an open argument with the same
-- (source, target, relation, context) exists, close it and open a new
-- one with updated strength/evidence.
create or replace function donto_assert_argument(
    p_source   uuid,
    p_target   uuid,
    p_relation text,
    p_context  text default 'donto:anonymous',
    p_strength double precision default null,
    p_agent_id uuid default null,
    p_evidence jsonb default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    if p_source = p_target then
        raise exception 'donto_assert_argument: source and target must differ';
    end if;
    perform donto_ensure_context(p_context);

    -- Close any prior open argument for this tuple.
    update donto_argument
    set tx_time = tstzrange(lower(tx_time), now(), '[)')
    where source_statement_id = p_source
      and target_statement_id = p_target
      and relation = p_relation
      and context = p_context
      and upper(tx_time) is null;

    insert into donto_argument
        (source_statement_id, target_statement_id, relation,
         context, strength, agent_id, evidence)
    values (p_source, p_target, p_relation,
            p_context, p_strength, p_agent_id, p_evidence)
    returning argument_id into v_id;
    return v_id;
end;
$$;

-- Retract an argument.
create or replace function donto_retract_argument(p_argument_id uuid)
returns boolean
language plpgsql as $$
declare
    v_n int;
begin
    update donto_argument
    set tx_time = tstzrange(lower(tx_time), now(), '[)')
    where argument_id = p_argument_id and upper(tx_time) is null;
    get diagnostics v_n = row_count;
    return v_n > 0;
end;
$$;

-- Current arguments about a statement (both attacking and supporting).
create or replace function donto_arguments_for(p_statement_id uuid)
returns table(
    argument_id uuid, source_statement_id uuid, target_statement_id uuid,
    relation text, strength double precision, context text, agent_id uuid
)
language sql stable as $$
    select argument_id, source_statement_id, target_statement_id,
           relation, strength, context, agent_id
    from donto_argument
    where (source_statement_id = p_statement_id
           or target_statement_id = p_statement_id)
      and upper(tx_time) is null
$$;

-- Contradiction frontier: statements with active rebuttals or undercuts.
create or replace function donto_contradiction_frontier(
    p_context text default null
) returns table(
    statement_id uuid, attack_count bigint,
    support_count bigint, net_pressure bigint
)
language sql stable as $$
    select
        t.target_statement_id as statement_id,
        count(*) filter (where t.relation in ('rebuts','undercuts')) as attack_count,
        count(*) filter (where t.relation = 'supports') as support_count,
        count(*) filter (where t.relation = 'supports')
          - count(*) filter (where t.relation in ('rebuts','undercuts')) as net_pressure
    from donto_argument t
    where upper(t.tx_time) is null
      and (p_context is null or t.context = p_context)
    group by t.target_statement_id
    having count(*) filter (where t.relation in ('rebuts','undercuts')) > 0
$$;
