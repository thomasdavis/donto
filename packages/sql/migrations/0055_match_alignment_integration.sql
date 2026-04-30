-- Wire alignment expansion into the default donto_match query path.
--
-- This is the integration migration: it replaces donto_match() so that every
-- existing caller (including the Rust evaluator's client.match_pattern()) now
-- sees alignment-expanded results without an API change. The function now
-- delegates to donto_match_aligned() with expansion ON and projects away the
-- two extra columns (matched_via, alignment_confidence) so the original
-- 12-column shape is preserved.
--
-- Callers that need strict (un-expanded) matching should use the new
-- donto_match_strict() variant added below.

create or replace function donto_match(
    p_subject      text default null,
    p_predicate    text default null,
    p_object_iri   text default null,
    p_object_lit   jsonb default null,
    p_scope        jsonb default null,
    p_polarity     text default 'asserted',
    p_min_maturity int default 0,
    p_as_of_tx     timestamptz default null,
    p_as_of_valid  date default null
) returns table(
    statement_id uuid,
    subject      text,
    predicate    text,
    object_iri   text,
    object_lit   jsonb,
    context      text,
    polarity     text,
    maturity     int,
    valid_lo     date,
    valid_hi     date,
    tx_lo        timestamptz,
    tx_hi        timestamptz
)
language plpgsql stable as $$
begin
    return query
    select m.statement_id, m.subject, m.predicate,
           m.object_iri, m.object_lit, m.context,
           m.polarity, m.maturity,
           m.valid_lo, m.valid_hi, m.tx_lo, m.tx_hi
    from donto_match_aligned(
        p_subject, p_predicate, p_object_iri, p_object_lit,
        p_scope, p_polarity, p_min_maturity,
        p_as_of_tx, p_as_of_valid,
        true,    -- expand predicates via closure
        0.8      -- default confidence floor
    ) m;
end;
$$;

-- Strict match: same parameters and result shape as donto_match, but with
-- alignment expansion disabled. For callers that need exact predicate
-- equality (debugging, rule-engine fixed points, audit traces).
create or replace function donto_match_strict(
    p_subject      text default null,
    p_predicate    text default null,
    p_object_iri   text default null,
    p_object_lit   jsonb default null,
    p_scope        jsonb default null,
    p_polarity     text default 'asserted',
    p_min_maturity int default 0,
    p_as_of_tx     timestamptz default null,
    p_as_of_valid  date default null
) returns table(
    statement_id uuid,
    subject      text,
    predicate    text,
    object_iri   text,
    object_lit   jsonb,
    context      text,
    polarity     text,
    maturity     int,
    valid_lo     date,
    valid_hi     date,
    tx_lo        timestamptz,
    tx_hi        timestamptz
)
language plpgsql stable as $$
begin
    return query
    select m.statement_id, m.subject, m.predicate,
           m.object_iri, m.object_lit, m.context,
           m.polarity, m.maturity,
           m.valid_lo, m.valid_hi, m.tx_lo, m.tx_hi
    from donto_match_aligned(
        p_subject, p_predicate, p_object_iri, p_object_lit,
        p_scope, p_polarity, p_min_maturity,
        p_as_of_tx, p_as_of_valid,
        false,   -- no expansion
        1.0
    ) m;
end;
$$;
