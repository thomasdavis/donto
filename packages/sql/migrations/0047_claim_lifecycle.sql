-- Claim lifecycle: the full journey from observation to knowledge.
--
-- This migration adds functions that make the lifecycle explicit and
-- queryable. A claim's lifecycle is:
--
--   observed → extracted → typed → anchored → shape-checked →
--   source-supported → argued → certified
--
-- Each stage has a predicate that can be checked. The lifecycle
-- status is computed, not stored — it's a view over existing data.

-- Compute the lifecycle stage of a statement.
-- Returns the highest stage the statement has reached.
create or replace function donto_claim_lifecycle(p_statement_id uuid)
returns jsonb
language plpgsql stable as $$
declare
    v_stmt record;
    v_stages jsonb := '[]'::jsonb;
    v_has boolean;
begin
    select statement_id, predicate, object_lit, context,
           donto_polarity(flags) as polarity,
           donto_maturity(flags) as maturity
    into v_stmt
    from donto_statement where statement_id = p_statement_id;

    if v_stmt is null then return null; end if;

    -- Stage 1: Observed (exists in the database)
    v_stages := v_stages || jsonb_build_object(
        'stage', 'observed', 'reached', true,
        'detail', 'Statement exists in context ' || v_stmt.context);

    -- Stage 2: Extracted (has an evidence link to an extraction run)
    v_has := exists(
        select 1 from donto_evidence_link
        where statement_id = p_statement_id
          and link_type = 'produced_by'
          and target_run_id is not null
          and upper(tx_time) is null);
    v_stages := v_stages || jsonb_build_object(
        'stage', 'extracted', 'reached', v_has,
        'detail', case when v_has then 'Linked to extraction run'
                       else 'No extraction run linked' end);

    -- Stage 3: Typed (has typed literal with proper datatype, or is IRI-valued)
    v_has := v_stmt.object_lit is null  -- IRI-valued is already typed
          or (v_stmt.object_lit->>'dt') not in ('xsd:string');
    v_stages := v_stages || jsonb_build_object(
        'stage', 'typed', 'reached', v_has,
        'detail', case when v_has then 'Properly typed literal or IRI object'
                       else 'Object is untyped xsd:string' end);

    -- Stage 4: Anchored (has a span evidence link)
    v_has := exists(
        select 1 from donto_evidence_link
        where statement_id = p_statement_id
          and link_type = 'extracted_from'
          and target_span_id is not null
          and upper(tx_time) is null);
    v_stages := v_stages || jsonb_build_object(
        'stage', 'anchored', 'reached', v_has,
        'detail', case when v_has then 'Anchored to source span'
                       else 'No span anchor' end);

    -- Stage 5: Confidence-rated
    v_has := exists(
        select 1 from donto_stmt_confidence
        where statement_id = p_statement_id);
    v_stages := v_stages || jsonb_build_object(
        'stage', 'confidence_rated', 'reached', v_has,
        'detail', case when v_has then
            'Confidence: ' || (select confidence::text from donto_stmt_confidence where statement_id = p_statement_id)
            else 'No confidence score' end);

    -- Stage 6: Predicate registered
    v_has := exists(
        select 1 from donto_predicate
        where iri = v_stmt.predicate and status = 'active');
    v_stages := v_stages || jsonb_build_object(
        'stage', 'predicate_registered', 'reached', v_has,
        'detail', case when v_has then 'Predicate ' || v_stmt.predicate || ' is active'
                       else 'Predicate ' || v_stmt.predicate || ' not registered' end);

    -- Stage 7: Shape-checked (has at least one shape annotation)
    v_has := exists(
        select 1 from donto_stmt_shape_annotation
        where statement_id = p_statement_id
          and upper(tx_time) is null);
    v_stages := v_stages || jsonb_build_object(
        'stage', 'shape_checked', 'reached', v_has,
        'detail', case when v_has then
            (select 'Shape: ' || shape_iri || ' → ' || verdict
             from donto_stmt_shape_annotation
             where statement_id = p_statement_id and upper(tx_time) is null
             limit 1)
            else 'No shape validation run' end);

    -- Stage 8: Source-supported (has evidence chain to a document)
    v_has := exists(
        select 1 from donto_evidence_link el
        where el.statement_id = p_statement_id
          and (el.target_document_id is not null or el.target_span_id is not null)
          and upper(el.tx_time) is null);
    v_stages := v_stages || jsonb_build_object(
        'stage', 'source_supported', 'reached', v_has,
        'detail', case when v_has then 'Has document/span evidence'
                       else 'No source document linked' end);

    -- Stage 9: No open obligations
    v_has := not exists(
        select 1 from donto_proof_obligation
        where statement_id = p_statement_id and status = 'open');
    v_stages := v_stages || jsonb_build_object(
        'stage', 'obligations_clear', 'reached', v_has,
        'detail', case when v_has then 'All obligations resolved'
                       else (select count(*)::text || ' open obligation(s)'
                             from donto_proof_obligation
                             where statement_id = p_statement_id and status = 'open') end);

    -- Stage 10: Argued (has at least one supporting argument)
    v_has := exists(
        select 1 from donto_argument
        where target_statement_id = p_statement_id
          and relation = 'supports'
          and upper(tx_time) is null);
    v_stages := v_stages || jsonb_build_object(
        'stage', 'argued', 'reached', v_has,
        'detail', case when v_has then
            (select count(*)::text || ' supporting argument(s)'
             from donto_argument
             where target_statement_id = p_statement_id
               and relation = 'supports' and upper(tx_time) is null)
            else 'No supporting arguments' end);

    -- Stage 11: Certified
    v_has := exists(
        select 1 from donto_stmt_certificate
        where statement_id = p_statement_id and verified_ok = true);
    v_stages := v_stages || jsonb_build_object(
        'stage', 'certified', 'reached', v_has,
        'detail', case when v_has then 'Has verified certificate'
                       else 'No certificate' end);

    return jsonb_build_object(
        'statement_id', p_statement_id,
        'predicate', v_stmt.predicate,
        'maturity', v_stmt.maturity,
        'stages', v_stages,
        'stages_reached', (select count(*) from jsonb_array_elements(v_stages) e where (e.value->>'reached')::boolean),
        'stages_total', jsonb_array_length(v_stages)
    );
end;
$$;

-- Summary: lifecycle stage counts across a context
create or replace function donto_lifecycle_summary(p_context text)
returns table(stage text, reached_count bigint, total_count bigint, coverage text)
language sql stable as $$
    with stages as (
        select
            (e.value->>'stage') as stage,
            (e.value->>'reached')::boolean as reached
        from donto_statement s,
        lateral jsonb_array_elements(
            (donto_claim_lifecycle(s.statement_id))->'stages'
        ) e
        where s.context = p_context and upper(s.tx_time) is null
    )
    select
        stage,
        count(*) filter (where reached) as reached_count,
        count(*) as total_count,
        round(100.0 * count(*) filter (where reached) / count(*), 1) || '%' as coverage
    from stages
    group by stage
    order by
        case stage
            when 'observed' then 1
            when 'extracted' then 2
            when 'typed' then 3
            when 'anchored' then 4
            when 'confidence_rated' then 5
            when 'predicate_registered' then 6
            when 'shape_checked' then 7
            when 'source_supported' then 8
            when 'obligations_clear' then 9
            when 'argued' then 10
            when 'certified' then 11
        end
$$;
