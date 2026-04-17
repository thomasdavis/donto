-- donto Phase 0 SQL function surface.
-- Implements: donto_assert, donto_assert_batch, donto_retract, donto_correct,
-- donto_match, donto_resolve_scope. Per PRD §5, §7, §8, §12 (surface A subset).

-- ---------------------------------------------------------------------------
-- Context helpers.
-- ---------------------------------------------------------------------------

create or replace function donto_ensure_context(
    p_iri   text,
    p_kind  text default 'custom',
    p_mode  text default 'permissive',
    p_parent text default null
) returns text
language plpgsql as $$
begin
    insert into donto_context (iri, kind, mode, parent)
    values (p_iri, p_kind, p_mode, p_parent)
    on conflict (iri) do nothing;
    return p_iri;
end;
$$;

-- Resolve a scope descriptor to a concrete set of context IRIs (PRD §7).
-- Phase 0 supports: include, exclude, include_descendants, include_ancestors.
-- The `min_maturity` filter is applied at query time, not in resolution.
create or replace function donto_resolve_scope(
    p_scope jsonb
) returns table(context_iri text)
language plpgsql stable as $$
declare
    v_include             text[] := coalesce(array(select jsonb_array_elements_text(p_scope -> 'include')), '{}');
    v_exclude             text[] := coalesce(array(select jsonb_array_elements_text(p_scope -> 'exclude')), '{}');
    v_include_descendants boolean := coalesce((p_scope ->> 'include_descendants')::boolean, true);
    v_include_ancestors   boolean := coalesce((p_scope ->> 'include_ancestors')::boolean, false);
begin
    -- Empty include = visible everywhere (minus excludes).
    if array_length(v_include, 1) is null then
        return query
        select c.iri from donto_context c
        where c.iri <> all(v_exclude);
        return;
    end if;

    return query
    with recursive
    seed as (
        select unnest(v_include) as iri
    ),
    -- Descend the parent tree (parent → child).
    descend as (
        select c.iri, c.parent from donto_context c
            join seed s on c.iri = s.iri
        union all
        select c.iri, c.parent from donto_context c
            join descend d on c.parent = d.iri
            where v_include_descendants
    ),
    -- Ascend the parent tree (child → parent).
    ascend as (
        select c.iri, c.parent from donto_context c
            join seed s on c.iri = s.iri
        union all
        select c.iri, c.parent from donto_context c
            join ascend a on c.iri = a.parent
            where v_include_ancestors
    ),
    combined as (
        select iri from descend
        union
        select iri from ascend
    )
    select iri from combined
    where iri <> all(v_exclude);
end;
$$;

-- ---------------------------------------------------------------------------
-- Assert.
-- ---------------------------------------------------------------------------
-- Insert a statement. Object is either an IRI or a literal (jsonb of shape
-- {"v": ..., "dt": "<datatype iri>", "lang": "<tag-or-null>"}). Returns the
-- statement_id of the (possibly already-existing) row.
--
-- Idempotency: re-asserting an identical statement (same content_hash) in an
-- open tx_time window returns the existing id, unchanged.
create or replace function donto_assert(
    p_subject     text,
    p_predicate   text,
    p_object_iri  text,
    p_object_lit  jsonb,
    p_context     text default 'donto:anonymous',
    p_polarity    text default 'asserted',
    p_maturity    int  default 0,
    p_valid_lo    date default null,
    p_valid_hi    date default null,
    p_actor       text default null
) returns uuid
language plpgsql as $$
declare
    v_flags smallint := donto_pack_flags(p_polarity, p_maturity);
    v_valid daterange := daterange(p_valid_lo, p_valid_hi, '[)');
    v_id    uuid;
begin
    if p_context is null then
        raise exception 'donto_assert: context is required (use donto:anonymous if unknown)';
    end if;

    perform donto_ensure_context(p_context);

    insert into donto_statement (
        subject, predicate, object_iri, object_lit,
        context, valid_time, flags
    ) values (
        p_subject, p_predicate, p_object_iri, p_object_lit,
        p_context, v_valid, v_flags
    )
    on conflict (content_hash) where upper(tx_time) is null do nothing
    returning statement_id into v_id;

    if v_id is null then
        -- Existed already; fetch the open row.
        select statement_id into v_id
        from donto_statement
        where content_hash = digest(
                coalesce(p_subject,'')   || chr(31) ||
                coalesce(p_predicate,'') || chr(31) ||
                coalesce(p_object_iri,'') || chr(31) ||
                coalesce(p_object_lit::text,'') || chr(31) ||
                coalesce(p_context,'') || chr(31) ||
                (v_flags & 3)::text || chr(31) ||
                coalesce((lower(v_valid) - '2000-01-01'::date)::text, '-inf') || chr(31) ||
                coalesce((upper(v_valid) - '2000-01-01'::date)::text, '+inf'),
                'sha256')
          and upper(tx_time) is null;
    else
        insert into donto_audit (actor, action, statement_id, detail)
        values (p_actor, 'assert', v_id,
                jsonb_build_object('polarity', p_polarity, 'context', p_context));
    end if;

    return v_id;
end;
$$;

-- ---------------------------------------------------------------------------
-- Batch assert. Accepts a jsonb array of statement objects.
-- ---------------------------------------------------------------------------
create or replace function donto_assert_batch(
    p_statements jsonb,
    p_actor      text default null
) returns int
language plpgsql as $$
declare
    v_stmt    jsonb;
    v_count   int := 0;
begin
    if jsonb_typeof(p_statements) <> 'array' then
        raise exception 'donto_assert_batch: expected JSON array, got %', jsonb_typeof(p_statements);
    end if;

    for v_stmt in select * from jsonb_array_elements(p_statements) loop
        perform donto_assert(
            p_subject    := v_stmt ->> 'subject',
            p_predicate  := v_stmt ->> 'predicate',
            p_object_iri := v_stmt ->> 'object_iri',
            p_object_lit := v_stmt -> 'object_lit',
            p_context    := coalesce(v_stmt ->> 'context', 'donto:anonymous'),
            p_polarity   := coalesce(v_stmt ->> 'polarity', 'asserted'),
            p_maturity   := coalesce((v_stmt ->> 'maturity')::int, 0),
            p_valid_lo   := nullif(v_stmt ->> 'valid_lo','')::date,
            p_valid_hi   := nullif(v_stmt ->> 'valid_hi','')::date,
            p_actor      := p_actor
        );
        v_count := v_count + 1;
    end loop;

    return v_count;
end;
$$;

-- ---------------------------------------------------------------------------
-- Retract: close transaction-time. Never deletes.
-- ---------------------------------------------------------------------------
create or replace function donto_retract(
    p_statement_id uuid,
    p_actor        text default null
) returns boolean
language plpgsql as $$
declare
    v_now timestamptz := now();
    v_updated int;
begin
    update donto_statement
       set tx_time = tstzrange(lower(tx_time), v_now, '[)')
     where statement_id = p_statement_id
       and upper(tx_time) is null;

    get diagnostics v_updated = row_count;

    if v_updated > 0 then
        insert into donto_audit (actor, action, statement_id)
        values (p_actor, 'retract', p_statement_id);
        return true;
    end if;

    return false;
end;
$$;

-- ---------------------------------------------------------------------------
-- Correct: retract the prior open statement and assert a replacement that
-- shares (subject, predicate, object*, context, valid_time) but differs in
-- some other field. The new row is returned.
-- ---------------------------------------------------------------------------
create or replace function donto_correct(
    p_statement_id uuid,
    p_new_subject  text default null,
    p_new_predicate text default null,
    p_new_object_iri text default null,
    p_new_object_lit jsonb default null,
    p_new_polarity text default null,
    p_actor        text default null
) returns uuid
language plpgsql as $$
declare
    v_old donto_statement;
    v_new uuid;
begin
    select * into v_old from donto_statement where statement_id = p_statement_id and upper(tx_time) is null;
    if v_old.statement_id is null then
        raise exception 'donto_correct: no open statement % found', p_statement_id;
    end if;

    perform donto_retract(p_statement_id, p_actor);

    v_new := donto_assert(
        p_subject    := coalesce(p_new_subject, v_old.subject),
        p_predicate  := coalesce(p_new_predicate, v_old.predicate),
        p_object_iri := coalesce(p_new_object_iri, v_old.object_iri),
        p_object_lit := coalesce(p_new_object_lit, v_old.object_lit),
        p_context    := v_old.context,
        p_polarity   := coalesce(p_new_polarity, donto_polarity(v_old.flags)),
        p_maturity   := donto_maturity(v_old.flags),
        p_valid_lo   := lower(v_old.valid_time),
        p_valid_hi   := upper(v_old.valid_time),
        p_actor      := p_actor
    );

    insert into donto_audit (actor, action, statement_id, detail)
    values (p_actor, 'correct', v_new,
            jsonb_build_object('replaces', p_statement_id));

    return v_new;
end;
$$;

-- ---------------------------------------------------------------------------
-- Match: pattern query with subject/predicate/object filters and scope.
-- Phase 0 surface is intentionally narrow; full DontoQL/SPARQL come in Phase 4.
-- ---------------------------------------------------------------------------
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
    v_resolved text[];
begin
    if p_scope is null then
        v_resolved := null;
    else
        select array_agg(context_iri) into v_resolved from donto_resolve_scope(p_scope);
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
      and (v_resolved   is null or s.context = any(v_resolved))
      and (p_polarity   is null or donto_polarity(s.flags) = p_polarity)
      and donto_maturity(s.flags) >= p_min_maturity
      and (case
              when p_as_of_tx is null then upper(s.tx_time) is null    -- "current belief" default
              else s.tx_time @> p_as_of_tx                              -- explicit time travel
           end)
      and (p_as_of_valid is null
           or (s.valid_time @> p_as_of_valid));
end;
$$;
