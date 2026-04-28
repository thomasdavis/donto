-- Fix: donto_match treated an *unresolvable* scope (one that names contexts
-- that don't exist) as "no scope" because array_agg over zero rows returns
-- NULL. Coalesce to an empty array so the IN-ANY check rejects everything.
--
-- Affected by: migrator dry-run tests (PRD §19) where ensure_context is
-- skipped, so the requested scope iri exists nowhere in donto_context.

create or replace function donto_match(
    p_subject    text default null,
    p_predicate  text default null,
    p_object_iri text default null,
    p_object_lit jsonb default null,
    p_scope      jsonb default null,
    p_polarity   text default 'asserted',
    p_min_maturity int default 0,
    p_as_of_tx   timestamptz default null,
    p_as_of_valid date default null
) returns table(
    statement_id uuid,
    subject text,
    predicate text,
    object_iri text,
    object_lit jsonb,
    context text,
    polarity text,
    maturity int,
    valid_lo date,
    valid_hi date,
    tx_lo timestamptz,
    tx_hi timestamptz
)
language plpgsql stable as $$
declare
    v_scope_provided boolean := p_scope is not null;
    v_resolved text[];
begin
    if v_scope_provided then
        select coalesce(array_agg(context_iri), '{}'::text[])
            into v_resolved from donto_resolve_scope(p_scope);
    end if;

    return query
    select
        s.statement_id,
        s.subject,
        s.predicate,
        s.object_iri,
        s.object_lit,
        s.context,
        donto_polarity(s.flags),
        donto_maturity(s.flags),
        lower(s.valid_time),
        upper(s.valid_time),
        lower(s.tx_time),
        upper(s.tx_time)
    from donto_statement s
    where (p_subject    is null or s.subject    = p_subject)
      and (p_predicate  is null or s.predicate  = p_predicate)
      and (p_object_iri is null or s.object_iri = p_object_iri)
      and (p_object_lit is null or s.object_lit = p_object_lit)
      and (not v_scope_provided or s.context = any(v_resolved))
      and (p_polarity   is null or donto_polarity(s.flags) = p_polarity)
      and donto_maturity(s.flags) >= p_min_maturity
      and (case
              when p_as_of_tx is null then upper(s.tx_time) is null
              else s.tx_time @> p_as_of_tx
           end)
      and (p_as_of_valid is null
           or (s.valid_time @> p_as_of_valid));
end;
$$;
