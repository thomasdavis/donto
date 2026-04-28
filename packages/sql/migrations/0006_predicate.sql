-- Phase 3: Predicate registry (PRD §9).
--
-- Open-world: any predicate IRI may appear on a statement. The registry maps
-- IRIs to canonical forms, attaches optional metadata, and enables read-time
-- alias resolution. In permissive contexts, unregistered predicates are
-- recorded with status='implicit' on first use.

create table if not exists donto_predicate (
    iri                 text primary key,
    canonical_of        text references donto_predicate(iri),
    label               text,
    description         text,
    domain              text,                  -- shape iri (Phase 5)
    range_iri           text,                  -- shape iri for IRI-valued objects
    range_datatype      text,                  -- datatype iri for literal-valued objects
    inverse_of          text references donto_predicate(iri),
    is_symmetric        boolean not null default false,
    is_transitive       boolean not null default false,
    is_functional       boolean not null default false,
    is_inverse_functional boolean not null default false,
    card_min            int,
    card_max            int,
    registered_by       text,
    registered_at       timestamptz not null default now(),
    status              text not null default 'active'
                        check (status in ('active','deprecated','merged','implicit')),
    constraint donto_predicate_alias_no_chain
        check (canonical_of is null or canonical_of <> iri)
);

create index if not exists donto_predicate_canonical_idx
    on donto_predicate (canonical_of) where canonical_of is not null;
create index if not exists donto_predicate_status_idx
    on donto_predicate (status);

-- Datatype side table — minimal Phase 3. Used by curated-mode validation
-- in later phases. For now it is a free-form catalog.
create table if not exists donto_datatype (
    iri          text primary key,
    label        text,
    description  text,
    base         text     -- base datatype IRI (e.g., xsd:decimal for xsd:integer)
);

-- Common XSD datatypes seeded for convenience.
insert into donto_datatype (iri, label) values
    ('xsd:string',  'string'),
    ('xsd:integer', 'integer'),
    ('xsd:decimal', 'decimal'),
    ('xsd:boolean', 'boolean'),
    ('xsd:date',    'date'),
    ('xsd:dateTime','dateTime'),
    ('rdf:langString','language-tagged string')
on conflict (iri) do nothing;

-- Prefix table for compact IRIs.
create table if not exists donto_prefix (
    prefix text primary key,
    iri    text not null
);
insert into donto_prefix (prefix, iri) values
    ('rdf',   'http://www.w3.org/1999/02/22-rdf-syntax-ns#'),
    ('rdfs',  'http://www.w3.org/2000/01/rdf-schema#'),
    ('owl',   'http://www.w3.org/2002/07/owl#'),
    ('xsd',   'http://www.w3.org/2001/XMLSchema#'),
    ('donto', 'urn:donto:')
on conflict (prefix) do nothing;

-- ---------------------------------------------------------------------------
-- Registration helpers.
-- ---------------------------------------------------------------------------

create or replace function donto_register_predicate(
    p_iri text,
    p_label text default null,
    p_description text default null,
    p_canonical_of text default null,
    p_inverse_of text default null,
    p_domain text default null,
    p_range_iri text default null,
    p_range_datatype text default null
) returns text language plpgsql as $$
begin
    -- Reject alias chains (PRD §9: single-hop only).
    if p_canonical_of is not null then
        if exists (select 1 from donto_predicate where iri = p_canonical_of and canonical_of is not null) then
            raise exception 'donto_register_predicate: cannot alias to %; it is itself an alias', p_canonical_of;
        end if;
    end if;

    insert into donto_predicate (
        iri, label, description, canonical_of, inverse_of,
        domain, range_iri, range_datatype, status
    ) values (
        p_iri, p_label, p_description, p_canonical_of, p_inverse_of,
        p_domain, p_range_iri, p_range_datatype, 'active'
    )
    on conflict (iri) do update set
        label          = coalesce(excluded.label, donto_predicate.label),
        description    = coalesce(excluded.description, donto_predicate.description),
        canonical_of   = coalesce(excluded.canonical_of, donto_predicate.canonical_of),
        inverse_of     = coalesce(excluded.inverse_of, donto_predicate.inverse_of),
        domain         = coalesce(excluded.domain, donto_predicate.domain),
        range_iri      = coalesce(excluded.range_iri, donto_predicate.range_iri),
        range_datatype = coalesce(excluded.range_datatype, donto_predicate.range_datatype),
        status         = case when donto_predicate.status = 'implicit' then 'active'
                              else donto_predicate.status end;
    return p_iri;
end;
$$;

-- Look up the canonical IRI for a predicate (one-hop, never chained).
create or replace function donto_canonical_predicate(p_iri text)
returns text language sql stable as $$
    select coalesce(canonical_of, iri) from donto_predicate where iri = p_iri
    union all
    select p_iri  -- pass-through if not registered
    limit 1
$$;

-- Implicit registration on first use, called by donto_assert when the context
-- is permissive. (Curated contexts should reject unregistered predicates;
-- we wire that in Phase 5.)
create or replace function donto_implicit_register(p_iri text)
returns void language plpgsql as $$
begin
    insert into donto_predicate (iri, status) values (p_iri, 'implicit')
    on conflict (iri) do nothing;
end;
$$;

-- Patch donto_assert to call implicit registration.
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
    v_mode  text;
begin
    if p_context is null then
        raise exception 'donto_assert: context is required';
    end if;

    perform donto_ensure_context(p_context);

    select mode into v_mode from donto_context where iri = p_context;
    if v_mode = 'permissive' then
        perform donto_implicit_register(p_predicate);
    elsif v_mode = 'curated' then
        if not exists (select 1 from donto_predicate where iri = p_predicate and status = 'active') then
            raise exception 'donto_assert: predicate % not registered (curated context %)', p_predicate, p_context;
        end if;
    end if;

    insert into donto_statement (
        subject, predicate, object_iri, object_lit, context, valid_time, flags
    ) values (
        p_subject, p_predicate, p_object_iri, p_object_lit, p_context, v_valid, v_flags
    )
    on conflict (content_hash) where upper(tx_time) is null do nothing
    returning statement_id into v_id;

    if v_id is null then
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

-- Read-time alias resolution. Wraps donto_match: if predicate is a known
-- alias, also match its canonical (and any siblings).
create or replace function donto_match_canonical(
    p_subject text default null,
    p_predicate text default null,
    p_object_iri text default null,
    p_scope jsonb default null,
    p_polarity text default 'asserted',
    p_min_maturity int default 0
) returns table(
    statement_id uuid, subject text, predicate text,
    object_iri text, object_lit jsonb, context text,
    polarity text, maturity int,
    valid_lo date, valid_hi date,
    tx_lo timestamptz, tx_hi timestamptz
)
language plpgsql stable as $$
declare
    v_canonical text;
    v_predicates text[];
begin
    if p_predicate is null then
        return query select * from donto_match(p_subject, null, p_object_iri, null,
                                               p_scope, p_polarity, p_min_maturity, null, null);
        return;
    end if;
    v_canonical := donto_canonical_predicate(p_predicate);
    -- All predicates whose canonical is v_canonical (alias siblings + self).
    select array_agg(iri) into v_predicates from (
        select v_canonical as iri
        union
        select iri from donto_predicate where canonical_of = v_canonical
    ) x;

    return query
    select * from donto_match(p_subject, null, p_object_iri, null,
                              p_scope, p_polarity, p_min_maturity, null, null) m
    where m.predicate = any(v_predicates);
end;
$$;
