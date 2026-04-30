-- Alignment-aware query function: donto_match_aligned.
--
-- Same parameter list as donto_match (migration 0012) plus two alignment
-- knobs:
--   p_expand_predicates           — if true, also return matches reachable
--                                    via the closure index. Defaults to true.
--   p_min_alignment_confidence    — floor on closure.confidence (default 0.8).
--
-- Returns the same columns as donto_match plus:
--   matched_via             — 'direct' for the original predicate; otherwise
--                              the alignment relation ('exact_equivalent',
--                              'inverse_equivalent', 'sub_property_of',
--                              'close_match').
--   alignment_confidence    — 1.0 for direct, else from closure.
--
-- For inverse equivalents the swap is applied transparently: the SQL filters
-- with subject/object swapped, and the projection swaps them back so the
-- caller sees a well-formed (subject, predicate, object) row consistent
-- with the original predicate.

create or replace function donto_match_aligned(
    p_subject                  text default null,
    p_predicate                text default null,
    p_object_iri               text default null,
    p_object_lit               jsonb default null,
    p_scope                    jsonb default null,
    p_polarity                 text default 'asserted',
    p_min_maturity             int default 0,
    p_as_of_tx                 timestamptz default null,
    p_as_of_valid              date default null,
    p_expand_predicates        boolean default true,
    p_min_alignment_confidence double precision default 0.8
) returns table(
    statement_id         uuid,
    subject              text,
    predicate            text,
    object_iri           text,
    object_lit           jsonb,
    context              text,
    polarity             text,
    maturity             int,
    valid_lo             date,
    valid_hi             date,
    tx_lo                timestamptz,
    tx_hi                timestamptz,
    matched_via          text,
    alignment_confidence double precision
)
language plpgsql stable as $$
declare
    v_scope_provided boolean := p_scope is not null;
    v_resolved       text[];
begin
    if v_scope_provided then
        select coalesce(array_agg(context_iri), '{}'::text[])
            into v_resolved from donto_resolve_scope(p_scope);
    end if;

    -- ----- Direct match (always runs) -----
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
        upper(s.tx_time),
        'direct'::text,
        1.0::double precision
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
      and (p_as_of_valid is null or s.valid_time @> p_as_of_valid);

    -- ----- Closure expansion (only when predicate is bound and requested) -----
    if p_predicate is not null and p_expand_predicates then
        -- Non-swapping equivalents (exact_equivalent, sub_property_of, close_match).
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
            upper(s.tx_time),
            pc.relation,
            pc.confidence
        from donto_predicate_closure pc
        join donto_statement s on s.predicate = pc.equivalent_iri
        where pc.predicate_iri = p_predicate
          and pc.equivalent_iri <> p_predicate  -- skip direct (already returned)
          and not pc.swap_direction
          and pc.confidence >= p_min_alignment_confidence
          and (p_subject    is null or s.subject    = p_subject)
          and (p_object_iri is null or s.object_iri = p_object_iri)
          and (p_object_lit is null or s.object_lit = p_object_lit)
          and (not v_scope_provided or s.context = any(v_resolved))
          and (p_polarity   is null or donto_polarity(s.flags) = p_polarity)
          and donto_maturity(s.flags) >= p_min_maturity
          and (case
                  when p_as_of_tx is null then upper(s.tx_time) is null
                  else s.tx_time @> p_as_of_tx
               end)
          and (p_as_of_valid is null or s.valid_time @> p_as_of_valid);

        -- Swapping inverses: filter with s/o swapped; project with s/o swapped
        -- back so the caller sees rows oriented like the original predicate.
        return query
        select
            s.statement_id,
            s.object_iri as subject,        -- swapped back for caller
            s.predicate,
            s.subject    as object_iri,     -- swapped back for caller
            s.object_lit,
            s.context,
            donto_polarity(s.flags),
            donto_maturity(s.flags),
            lower(s.valid_time),
            upper(s.valid_time),
            lower(s.tx_time),
            upper(s.tx_time),
            pc.relation,
            pc.confidence
        from donto_predicate_closure pc
        join donto_statement s on s.predicate = pc.equivalent_iri
        where pc.predicate_iri = p_predicate
          and pc.swap_direction = true
          and pc.confidence >= p_min_alignment_confidence
          and s.object_iri is not null  -- inverse swap only applies to IRI objects
          and (p_subject    is null or s.object_iri = p_subject)    -- swapped filter
          and (p_object_iri is null or s.subject    = p_object_iri) -- swapped filter
          and (not v_scope_provided or s.context = any(v_resolved))
          and (p_polarity   is null or donto_polarity(s.flags) = p_polarity)
          and donto_maturity(s.flags) >= p_min_maturity
          and (case
                  when p_as_of_tx is null then upper(s.tx_time) is null
                  else s.tx_time @> p_as_of_tx
               end)
          and (p_as_of_valid is null or s.valid_time @> p_as_of_valid);
    end if;
end;
$$;
