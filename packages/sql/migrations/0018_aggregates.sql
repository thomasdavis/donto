-- Alexandria §3.3: rule-derived aggregates.
--
-- An aggregate rule's output is a statement of shape
--   (subject, donto:weight, literal_number, scope_context)
-- with donto_stmt_lineage rows pointing at every input reaction.
-- Re-running the rule over the same inputs reproduces the aggregate;
-- running with a new input set closes the prior rows' tx_time and opens
-- fresh ones.
--
-- The PRD's DontoQL `with weights(scope=ctx)` virtual-column surface is
-- implemented here as donto_weight_of(statement_id, scope) — a
-- side-effect-free read that doesn't require materializing the aggregate.

-- Canonical predicate.
select donto_register_predicate('donto:weight',
    'weight', 'Rule-derived aggregate weight (Level-3)');

-- Register the built-in endorsement-weight rule (metadata only; dontosrv
-- and the Lean sidecar know the kind).
select donto_register_rule('builtin:endorsement_weight', 'builtin',
    '{"kind":"endorsement_weight"}'::jsonb,
    'EndorsementWeight',
    'Weight = count(endorses) - count(rejects) over reactions in scope.',
    null,
    'on_demand');

-- Compute endorsement weights for every statement that has at least one
-- reaction in the given scope, write them into `into_ctx` as Level-3
-- (rule-derived) statements of predicate donto:weight, and record
-- donto_stmt_lineage rows pointing at every input reaction.
--
-- Re-running produces the same rows (same content_hash) so the assert is
-- idempotent. If the input set changes the new weight differs and the
-- prior row is closed (retracted) first.
create or replace function donto_compute_endorsement_weights(
    p_scope    jsonb,
    p_into_ctx text,
    p_actor    text default null
) returns bigint
language plpgsql as $$
declare
    v_resolved text[];
    v_emitted  bigint := 0;
    v_inputs   uuid[];
    rec        record;
    v_new      uuid;
    v_prior    uuid;
begin
    perform donto_ensure_context(p_into_ctx, 'derivation', 'permissive');

    if p_scope is null then
        v_resolved := null;
    else
        select array_agg(context_iri) into v_resolved from donto_resolve_scope(p_scope);
    end if;

    -- For each reacted-to statement, compute the weight and collect the
    -- input reaction IDs for lineage.
    for rec in
        select
            donto_stmt_iri_to_id(s.subject) as target_id,
            sum(case
                    when s.predicate = 'donto:endorses' then 1
                    when s.predicate = 'donto:rejects'  then -1
                    else 0
                end) as weight,
            array_agg(s.statement_id) as source_ids
        from donto_statement s
        where s.predicate in ('donto:endorses','donto:rejects')
          and upper(s.tx_time) is null
          and (v_resolved is null or s.context = any(v_resolved))
          and donto_stmt_iri_to_id(s.subject) is not null
        group by donto_stmt_iri_to_id(s.subject)
    loop
        v_inputs := rec.source_ids;

        -- Close any prior OPEN weight statement for this target in this
        -- ctx whose value differs from the new value.
        for v_prior in
            select statement_id from donto_statement
            where subject    = donto_stmt_iri(rec.target_id)
              and predicate  = 'donto:weight'
              and context    = p_into_ctx
              and upper(tx_time) is null
              and (object_lit ->> 'v')::numeric is distinct from rec.weight
        loop
            perform donto_retract(v_prior, p_actor);
        end loop;

        v_new := donto_assert(
            p_subject    := donto_stmt_iri(rec.target_id),
            p_predicate  := 'donto:weight',
            p_object_iri := null,
            p_object_lit := jsonb_build_object(
                'v',  rec.weight,
                'dt', 'xsd:integer'),
            p_context    := p_into_ctx,
            p_polarity   := 'asserted',
            p_maturity   := 3,
            p_valid_lo   := null,
            p_valid_hi   := null,
            p_actor      := p_actor
        );

        -- Attach lineage to every input reaction (ignoring duplicates on
        -- re-run).
        insert into donto_stmt_lineage (statement_id, source_stmt)
        select v_new, src
        from unnest(v_inputs) as src
        on conflict do nothing;

        v_emitted := v_emitted + 1;
    end loop;

    insert into donto_derivation_report (
        rule_iri, inputs_fingerprint, scope, into_ctx, emitted_count
    ) values (
        'builtin:endorsement_weight',
        digest(coalesce(p_scope::text, '') || p_into_ctx, 'sha256'),
        coalesce(p_scope, '{}'::jsonb),
        p_into_ctx,
        v_emitted
    );

    return v_emitted;
end;
$$;

-- Ephemeral weight read: don't write, just compute on the fly. This is
-- the DontoQL `with weights(scope=ctx)` virtual-column shape.
create or replace function donto_weight_of(
    p_statement_id uuid,
    p_scope        jsonb default null
) returns bigint
language plpgsql stable as $$
declare
    v_resolved text[];
    v_w bigint;
begin
    if p_scope is null then
        v_resolved := null;
    else
        select array_agg(context_iri) into v_resolved from donto_resolve_scope(p_scope);
    end if;

    select coalesce(sum(case
                when s.predicate = 'donto:endorses' then 1
                when s.predicate = 'donto:rejects'  then -1
                else 0
            end), 0)
      into v_w
    from donto_statement s
    where s.subject    = donto_stmt_iri(p_statement_id)
      and s.predicate  in ('donto:endorses','donto:rejects')
      and upper(s.tx_time) is null
      and (v_resolved is null or s.context = any(v_resolved));

    return v_w;
end;
$$;
