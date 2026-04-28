-- Evidence substrate: candidate context kind and promotion.
--
-- Candidate claims live below Level 0 — things the extractor thinks
-- *might* be claims but hasn't committed to asserting. Promotion
-- copies the candidate into a target context and retracts the
-- original. The candidate stays in history.

-- Add 'candidate' as a context kind
alter table donto_context drop constraint if exists donto_context_kind_check;
alter table donto_context add constraint donto_context_kind_check
    check (kind in (
        'source','snapshot','hypothesis','user','pipeline',
        'trust','derivation','quarantine','custom','system',
        'candidate'
    ));

-- Promote a candidate statement to a target context.
-- Retracts the candidate and asserts a new statement in the target.
-- Returns the new statement_id. Lineage is tracked.
create or replace function donto_promote_candidate(
    p_statement_id   uuid,
    p_target_context text,
    p_actor          text default null
) returns uuid
language plpgsql as $$
declare
    v_old donto_statement;
    v_new uuid;
begin
    select * into v_old
    from donto_statement
    where statement_id = p_statement_id and upper(tx_time) is null;

    if v_old.statement_id is null then
        raise exception 'donto_promote_candidate: no open statement %', p_statement_id;
    end if;

    -- Verify the source is a candidate context
    if not exists (
        select 1 from donto_context
        where iri = v_old.context and kind = 'candidate'
    ) then
        raise exception 'donto_promote_candidate: statement % is not in a candidate context', p_statement_id;
    end if;

    perform donto_ensure_context(p_target_context);

    -- Assert in target context
    v_new := donto_assert(
        p_subject    := v_old.subject,
        p_predicate  := v_old.predicate,
        p_object_iri := v_old.object_iri,
        p_object_lit := v_old.object_lit,
        p_context    := p_target_context,
        p_polarity   := donto_polarity(v_old.flags),
        p_maturity   := donto_maturity(v_old.flags),
        p_valid_lo   := lower(v_old.valid_time),
        p_valid_hi   := upper(v_old.valid_time),
        p_actor      := p_actor
    );

    -- Track lineage
    insert into donto_stmt_lineage (statement_id, source_stmt)
    values (v_new, p_statement_id)
    on conflict do nothing;

    -- Retract the candidate
    perform donto_retract(p_statement_id, p_actor);

    insert into donto_audit (actor, action, statement_id, detail)
    values (p_actor, 'promote_candidate', v_new,
            jsonb_build_object('from_candidate', p_statement_id,
                               'from_context', v_old.context,
                               'to_context', p_target_context));

    return v_new;
end;
$$;

-- Bulk promote all candidates from a context that meet a confidence threshold
create or replace function donto_promote_candidates_above(
    p_candidate_context text,
    p_target_context    text,
    p_min_confidence    double precision default 0.5,
    p_actor             text default null
) returns bigint
language plpgsql as $$
declare
    v_count bigint := 0;
    v_stmt record;
begin
    for v_stmt in
        select s.statement_id
        from donto_statement s
        join donto_stmt_confidence c using (statement_id)
        where s.context = p_candidate_context
          and upper(s.tx_time) is null
          and c.confidence >= p_min_confidence
    loop
        perform donto_promote_candidate(v_stmt.statement_id, p_target_context, p_actor);
        v_count := v_count + 1;
    end loop;
    return v_count;
end;
$$;
