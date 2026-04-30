-- Canonical shadow quads.
--
-- A materialized view of the statement ledger where every predicate has been
-- rewritten to its canonical form (and entity IRIs resolved through
-- donto_entity_alias). Shadows let read paths short-circuit alignment
-- expansion when callers want fully-canonicalized data.
--
-- Append-only: re-materializing a statement closes the prior shadow's
-- tx_time and inserts a new row. The unique partial index on (statement_id)
-- where upper(tx_time) is null guarantees at most one current shadow per
-- statement.

create table if not exists donto_canonical_shadow (
    shadow_id            uuid primary key default gen_random_uuid(),
    statement_id         uuid not null references donto_statement(statement_id),
    canonical_predicate  text not null,
    canonical_subject    text not null,
    canonical_object_iri text,
    canonical_object_lit jsonb,
    alignment_id         uuid references donto_predicate_alignment(alignment_id),
    confidence           double precision not null default 1.0,
    tx_time              tstzrange not null default tstzrange(now(), null, '[)'),
    created_at           timestamptz not null default now(),
    constraint donto_cs_object_one_of
        check ((canonical_object_iri is not null) <> (canonical_object_lit is not null)
               or (canonical_object_iri is null and canonical_object_lit is null)),
    constraint donto_cs_tx_lower_inc
        check (lower_inc(tx_time))
);

create unique index if not exists donto_cs_stmt_uniq
    on donto_canonical_shadow (statement_id) where upper(tx_time) is null;
create index if not exists donto_cs_canon_pred_idx
    on donto_canonical_shadow (canonical_predicate) where upper(tx_time) is null;
create index if not exists donto_cs_canon_subj_idx
    on donto_canonical_shadow (canonical_subject) where upper(tx_time) is null;
create index if not exists donto_cs_alignment_idx
    on donto_canonical_shadow (alignment_id) where alignment_id is not null;
create index if not exists donto_cs_tx_gist
    on donto_canonical_shadow using gist (tx_time);

-- ---------------------------------------------------------------------------
-- Functions.
-- ---------------------------------------------------------------------------

-- Materialize (or re-materialize) the canonical shadow for a single statement.
-- Picks the highest-confidence exact_equivalent or self entry from the
-- closure as the canonical predicate. Closes the prior current shadow.
create or replace function donto_materialize_shadow(
    p_statement_id uuid
) returns uuid
language plpgsql as $$
declare
    v_stmt         record;
    v_canon_pred   text;
    v_canon_subj   text;
    v_canon_obj    text;
    v_alignment_id uuid;
    v_confidence   double precision;
    v_shadow_id    uuid;
begin
    select * into v_stmt
    from donto_statement
    where statement_id = p_statement_id and upper(tx_time) is null;
    if v_stmt.statement_id is null then
        return null;
    end if;

    -- Pick the canonical predicate from the closure: prefer self (1.0),
    -- otherwise the highest-confidence exact_equivalent.
    select pc.equivalent_iri, pc.confidence
    into v_canon_pred, v_confidence
    from donto_predicate_closure pc
    where pc.predicate_iri = v_stmt.predicate
      and not pc.swap_direction
      and pc.relation in ('self', 'exact_equivalent')
    order by (pc.relation = 'self') desc, pc.confidence desc
    limit 1;

    v_canon_pred := coalesce(v_canon_pred, v_stmt.predicate);
    v_confidence := coalesce(v_confidence, 1.0);

    -- If the canonical differs from the source predicate, find the alignment
    -- edge that justified it (best-effort: any current exact_equivalent edge).
    if v_canon_pred <> v_stmt.predicate then
        select alignment_id into v_alignment_id
        from donto_predicate_alignment
        where source_iri = v_stmt.predicate
          and target_iri = v_canon_pred
          and relation = 'exact_equivalent'
          and upper(tx_time) is null
        order by confidence desc, registered_at desc
        limit 1;
    end if;

    -- Resolve entity aliases for subject and IRI object.
    v_canon_subj := donto_resolve_entity(v_stmt.subject);
    if v_stmt.object_iri is not null then
        v_canon_obj := donto_resolve_entity(v_stmt.object_iri);
    end if;

    -- Close any prior current shadow for this statement.
    update donto_canonical_shadow
    set tx_time = tstzrange(lower(tx_time), now(), '[)')
    where statement_id = p_statement_id and upper(tx_time) is null;

    -- Insert new shadow.
    insert into donto_canonical_shadow
        (statement_id, canonical_predicate, canonical_subject,
         canonical_object_iri, canonical_object_lit,
         alignment_id, confidence)
    values (p_statement_id, v_canon_pred, v_canon_subj,
            v_canon_obj, v_stmt.object_lit,
            v_alignment_id,
            case when v_canon_pred = v_stmt.predicate then 1.0
                 else v_confidence end)
    returning shadow_id into v_shadow_id;

    return v_shadow_id;
end;
$$;

-- Batch rebuild for a context, or all current statements. p_limit caps the
-- batch size for incremental rebuilds.
create or replace function donto_rebuild_shadows(
    p_context text default null,
    p_limit   int default null
) returns int
language plpgsql as $$
declare
    v_stmt  record;
    v_count int := 0;
begin
    for v_stmt in
        select statement_id from donto_statement
        where upper(tx_time) is null
          and (p_context is null or context = p_context)
        order by statement_id
        limit p_limit
    loop
        perform donto_materialize_shadow(v_stmt.statement_id);
        v_count := v_count + 1;
    end loop;
    return v_count;
end;
$$;

-- Register as a batch rule for the rule engine.
select donto_register_rule(
    'builtin:canonical_shadow',
    'builtin',
    '{"kind":"canonical_shadow_rebuild"}'::jsonb,
    'CanonicalShadowRebuild',
    'Materialize canonical shadow quads from donto_statement using the predicate closure and entity aliases.',
    null,
    'batch'
);
