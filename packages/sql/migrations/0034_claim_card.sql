-- Claim card: the atomic UX object of donto.
--
-- A claim card assembles everything known about a statement into one
-- composite view: the statement itself, its evidence chain, its
-- arguments, its proof obligations, its shape annotations, and its
-- path to certification.

-- Why is this statement not at a higher maturity level?
-- Returns a list of reasons blocking promotion.
create or replace function donto_why_not_higher(p_statement_id uuid)
returns table(
    current_level int,
    next_level int,
    blocker text,
    detail text
)
language plpgsql stable as $$
declare
    v_mat int;
    v_pred text;
    v_has_shape_report boolean;
    v_has_violation boolean;
    v_has_lineage boolean;
    v_has_cert boolean;
    v_has_evidence boolean;
    v_has_span_evidence boolean;
    v_open_obligations int;
    v_active_rebuttals int;
begin
    select donto_maturity(flags), predicate
    into v_mat, v_pred
    from donto_statement where statement_id = p_statement_id
      and upper(tx_time) is null;

    if v_mat is null then return; end if;

    -- Check infrastructure
    v_has_shape_report := exists(
        select 1 from donto_stmt_shape_annotation
        where statement_id = p_statement_id and upper(tx_time) is null);
    v_has_violation := exists(
        select 1 from donto_stmt_shape_annotation
        where statement_id = p_statement_id and verdict = 'violate'
          and upper(tx_time) is null);
    v_has_lineage := exists(
        select 1 from donto_stmt_lineage
        where statement_id = p_statement_id);
    v_has_cert := exists(
        select 1 from donto_stmt_certificate
        where statement_id = p_statement_id and verified_ok = true);
    v_has_evidence := exists(
        select 1 from donto_evidence_link
        where statement_id = p_statement_id and upper(tx_time) is null);
    v_has_span_evidence := exists(
        select 1 from donto_evidence_link
        where statement_id = p_statement_id and target_span_id is not null
          and upper(tx_time) is null);

    select count(*) into v_open_obligations
    from donto_proof_obligation
    where statement_id = p_statement_id and status = 'open';

    select count(*) into v_active_rebuttals
    from donto_argument
    where target_statement_id = p_statement_id
      and relation in ('rebuts', 'undercuts')
      and upper(tx_time) is null;

    -- Level 0 → 1: predicate must be registered (not implicit)
    if v_mat < 1 then
        current_level := 0; next_level := 1;
        if not exists(
            select 1 from donto_predicate
            where iri = v_pred and status = 'active'
        ) then
            blocker := 'predicate_not_registered';
            detail := 'Predicate ' || v_pred || ' is not registered (status != active)';
            return next;
        end if;
    end if;

    -- Level 1 → 2: must have at least one shape report, no open violations
    if v_mat < 2 then
        current_level := 1; next_level := 2;
        if not v_has_shape_report then
            blocker := 'no_shape_report';
            detail := 'No shape validation has been run on this statement';
            return next;
        end if;
        if v_has_violation then
            blocker := 'open_shape_violation';
            detail := 'Statement has an open shape violation';
            return next;
        end if;
    end if;

    -- Level 2 → 3: must have derivation lineage or be source-supported
    if v_mat < 3 then
        current_level := 2; next_level := 3;
        if not v_has_lineage and not v_has_evidence then
            blocker := 'no_lineage_or_evidence';
            detail := 'Statement has no derivation lineage and no evidence links';
            return next;
        end if;
        if not v_has_span_evidence then
            blocker := 'no_span_anchor';
            detail := 'Statement is not anchored to a specific source span';
            return next;
        end if;
    end if;

    -- Level 3 → 4: must have a verified certificate
    if v_mat < 4 then
        current_level := 3; next_level := 4;
        if not v_has_cert then
            blocker := 'no_certificate';
            detail := 'Statement has no verified certificate';
            return next;
        end if;
    end if;

    -- Cross-cutting: open obligations block any promotion
    if v_open_obligations > 0 then
        current_level := v_mat; next_level := v_mat + 1;
        blocker := 'open_obligations';
        detail := v_open_obligations || ' open proof obligation(s)';
        return next;
    end if;

    -- Cross-cutting: active rebuttals are a warning
    if v_active_rebuttals > 0 then
        current_level := v_mat; next_level := v_mat + 1;
        blocker := 'active_rebuttals';
        detail := v_active_rebuttals || ' active rebuttal(s) or undercut(s)';
        return next;
    end if;
end;
$$;

-- Claim card: assemble everything about a statement in one call.
create or replace function donto_claim_card(p_statement_id uuid)
returns jsonb
language plpgsql stable as $$
declare
    v_stmt record;
    v_evidence jsonb;
    v_arguments jsonb;
    v_obligations jsonb;
    v_shapes jsonb;
    v_blockers jsonb;
    v_reactions jsonb;
begin
    -- Statement
    select statement_id, subject, predicate, object_iri, object_lit,
           context, donto_polarity(flags) as polarity,
           donto_maturity(flags) as maturity,
           lower(valid_time) as valid_lo, upper(valid_time) as valid_hi,
           lower(tx_time) as tx_lo, upper(tx_time) as tx_hi
    into v_stmt
    from donto_statement where statement_id = p_statement_id;

    if v_stmt is null then return null; end if;

    -- Evidence links
    select coalesce(jsonb_agg(jsonb_build_object(
        'link_id', link_id,
        'link_type', link_type,
        'target_document_id', target_document_id,
        'target_revision_id', target_revision_id,
        'target_span_id', target_span_id,
        'target_run_id', target_run_id,
        'target_statement_id', target_statement_id,
        'confidence', confidence
    )), '[]'::jsonb) into v_evidence
    from donto_evidence_link
    where statement_id = p_statement_id and upper(tx_time) is null;

    -- Arguments (both directions)
    select coalesce(jsonb_agg(jsonb_build_object(
        'argument_id', argument_id,
        'source', source_statement_id,
        'target', target_statement_id,
        'relation', relation,
        'strength', strength,
        'direction', case
            when source_statement_id = p_statement_id then 'outgoing'
            else 'incoming'
        end
    )), '[]'::jsonb) into v_arguments
    from donto_argument
    where (source_statement_id = p_statement_id
           or target_statement_id = p_statement_id)
      and upper(tx_time) is null;

    -- Proof obligations
    select coalesce(jsonb_agg(jsonb_build_object(
        'obligation_id', obligation_id,
        'obligation_type', obligation_type,
        'status', status,
        'priority', priority,
        'detail', detail
    )), '[]'::jsonb) into v_obligations
    from donto_proof_obligation
    where statement_id = p_statement_id;

    -- Shape annotations
    select coalesce(jsonb_agg(jsonb_build_object(
        'shape_iri', shape_iri,
        'verdict', verdict,
        'detail', detail
    )), '[]'::jsonb) into v_shapes
    from donto_stmt_shape_annotation
    where statement_id = p_statement_id and upper(tx_time) is null;

    -- Why not higher?
    select coalesce(jsonb_agg(jsonb_build_object(
        'current_level', current_level,
        'next_level', next_level,
        'blocker', blocker,
        'detail', detail
    )), '[]'::jsonb) into v_blockers
    from donto_why_not_higher(p_statement_id);

    -- Reactions
    select coalesce(jsonb_agg(jsonb_build_object(
        'reaction_id', reaction_id,
        'kind', kind,
        'context', context,
        'polarity', polarity
    )), '[]'::jsonb) into v_reactions
    from donto_reactions_for(p_statement_id);

    return jsonb_build_object(
        'statement_id', v_stmt.statement_id,
        'subject', v_stmt.subject,
        'predicate', v_stmt.predicate,
        'object_iri', v_stmt.object_iri,
        'object_lit', v_stmt.object_lit,
        'context', v_stmt.context,
        'polarity', v_stmt.polarity,
        'maturity', v_stmt.maturity,
        'valid_from', v_stmt.valid_lo,
        'valid_to', v_stmt.valid_hi,
        'asserted_at', v_stmt.tx_lo,
        'retracted_at', v_stmt.tx_hi,
        'evidence', v_evidence,
        'arguments', v_arguments,
        'obligations', v_obligations,
        'shapes', v_shapes,
        'reactions', v_reactions,
        'blockers', v_blockers
    );
end;
$$;
