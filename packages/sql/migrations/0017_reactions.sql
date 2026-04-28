-- Alexandria §3.2: reaction meta-statement pattern.
--
-- Reactions are ordinary statements whose subject is the IRI form of another
-- statement's UUID. Canonical vocabulary:
--
--   donto:endorses    — asserted agreement
--   donto:rejects     — asserted disagreement
--   donto:cites       — reference to a supporting IRI (URL or statement)
--   donto:supersedes  — the subject statement replaces the object statement
--
-- "Who reacted" = the reaction's own context. "How many endorse S" is a
-- Level-3 aggregate rule (§3.3) — this file only registers the predicates
-- and gives callers a helper for the stmt-IRI form.

-- A statement's UUID as an IRI.
create or replace function donto_stmt_iri(p_id uuid)
returns text language sql immutable as $$
    select 'donto:stmt/' || p_id::text
$$;

-- Inverse: parse a stmt-IRI back to the UUID (nullable if not a stmt IRI).
create or replace function donto_stmt_iri_to_id(p_iri text)
returns uuid language sql immutable as $$
    select case
        when p_iri like 'donto:stmt/%' then
            substring(p_iri from length('donto:stmt/') + 1)::uuid
        else null
    end
$$;

-- Register the canonical reaction predicates.
select donto_register_predicate('donto:endorses',
    'endorses',    'Author endorses the referenced statement');
select donto_register_predicate('donto:rejects',
    'rejects',     'Author rejects the referenced statement');
select donto_register_predicate('donto:cites',
    'cites',       'Author cites the referenced statement or IRI');
select donto_register_predicate('donto:supersedes',
    'supersedes',  'Subject statement supersedes the object statement');

-- Convenience: attach a reaction. `kind` is one of endorses|rejects|cites|supersedes.
-- The object is optional (endorse/reject often stand alone; cites/supersedes
-- need an object). The reaction's polarity follows the kind:
--   endorses -> asserted
--   rejects  -> negated   (the reacted-to statement is being denied)
--   cites/supersedes -> asserted
create or replace function donto_react(
    p_source_stmt uuid,      -- the statement being reacted to
    p_kind        text,      -- endorses|rejects|cites|supersedes
    p_object_iri  text default null,
    p_context     text default 'donto:anonymous',
    p_actor       text default null
) returns uuid
language plpgsql as $$
declare
    v_predicate text;
    v_polarity  text;
begin
    if not exists (select 1 from donto_statement where statement_id = p_source_stmt) then
        raise exception 'donto_react: source statement % not found', p_source_stmt;
    end if;
    case lower(p_kind)
        when 'endorses'   then v_predicate := 'donto:endorses';   v_polarity := 'asserted';
        when 'rejects'    then v_predicate := 'donto:rejects';    v_polarity := 'negated';
        when 'cites'      then v_predicate := 'donto:cites';      v_polarity := 'asserted';
        when 'supersedes' then v_predicate := 'donto:supersedes'; v_polarity := 'asserted';
        else raise exception 'donto_react: unknown kind %; want endorses|rejects|cites|supersedes', p_kind;
    end case;

    -- cites/supersedes must have an object.
    if v_predicate in ('donto:cites','donto:supersedes') and p_object_iri is null then
        raise exception 'donto_react: kind % requires p_object_iri', p_kind;
    end if;

    -- endorses/rejects are unary — the target is already the subject. We
    -- set object = subject so the statement satisfies the object_one_of
    -- check; this also makes "endorse this same thing again" idempotent.
    if v_predicate in ('donto:endorses','donto:rejects') and p_object_iri is null then
        p_object_iri := donto_stmt_iri(p_source_stmt);
    end if;

    return donto_assert(
        p_subject    := donto_stmt_iri(p_source_stmt),
        p_predicate  := v_predicate,
        p_object_iri := p_object_iri,
        p_object_lit := null,
        p_context    := p_context,
        p_polarity   := v_polarity,
        p_maturity   := 1,              -- canonical predicate => Level 1
        p_valid_lo   := null,
        p_valid_hi   := null,
        p_actor      := p_actor
    );
end;
$$;

-- Enumerate reactions to a given statement (current belief only).
-- No ORDER BY; PRD §3.10.
create or replace function donto_reactions_for(
    p_statement_id uuid
) returns table(
    reaction_id  uuid,
    kind         text,
    object_iri   text,
    context      text,
    polarity     text
) language sql stable as $$
    select
        s.statement_id,
        case s.predicate
            when 'donto:endorses'   then 'endorses'
            when 'donto:rejects'    then 'rejects'
            when 'donto:cites'      then 'cites'
            when 'donto:supersedes' then 'supersedes'
            else s.predicate
        end,
        s.object_iri,
        s.context,
        donto_polarity(s.flags)
    from donto_statement s
    where s.subject = donto_stmt_iri(p_statement_id)
      and s.predicate in ('donto:endorses','donto:rejects','donto:cites','donto:supersedes')
      and upper(s.tx_time) is null
$$;
