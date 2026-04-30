-- Event frame decomposition.
--
-- Some n-ary relations cannot be expressed as a single (s p o) triple:
-- "Marie Curie worked at the Sorbonne from 1906 to 1934 as a professor of
-- physics" has subject, organization, start, end, and role — five-ary.
--
-- The pattern is to mint a blank-node event IRI of the form
--   donto:frame/<uuid>
-- and emit role triples whose subject is that IRI:
--   <frame> rdf:type ex:EmploymentEvent
--   <frame> ex:worksAt/subject ex:marie-curie
--   <frame> ex:worksAt/object  ex:sorbonne
--   <frame> ex:startDate "1906"^^xsd:date
--   <frame> ex:endDate   "1934"^^xsd:date
--   <frame> ex:role      "professor of physics"
--
-- donto_event_frame indexes the frames; donto_decomposition_template lets a
-- predicate declare its preferred frame_type and role mapping; the
-- decomposition relation type in donto_predicate_alignment (migration 0048)
-- records the predicate-to-frame alignment.

create table if not exists donto_event_frame (
    frame_id         uuid primary key default gen_random_uuid(),
    frame_iri        text not null unique,
    frame_type       text not null,
    source_predicate text not null,
    context          text not null references donto_context(iri),
    tx_time          tstzrange not null default tstzrange(now(), null, '[)'),
    created_at       timestamptz not null default now(),
    constraint donto_ef_tx_lower_inc check (lower_inc(tx_time))
);

create index if not exists donto_ef_source_pred_idx on donto_event_frame (source_predicate);
create index if not exists donto_ef_type_idx on donto_event_frame (frame_type);
create index if not exists donto_ef_ctx_idx on donto_event_frame (context);
create index if not exists donto_ef_tx_gist on donto_event_frame using gist (tx_time);

create table if not exists donto_decomposition_template (
    template_id      uuid primary key default gen_random_uuid(),
    source_predicate text not null,
    frame_type       text not null,
    role_predicates  jsonb not null,
    registered_at    timestamptz not null default now(),
    unique (source_predicate, frame_type)
);

create index if not exists donto_dt_source_pred_idx
    on donto_decomposition_template (source_predicate);

-- ---------------------------------------------------------------------------
-- Functions.
-- ---------------------------------------------------------------------------

-- Decompose (subject, predicate, object) into an event frame plus role
-- triples in the given context. p_extra_roles is a jsonb object whose keys
-- are role-predicate IRIs and whose values are either { "iri": "..." } for
-- IRI objects or a literal jsonb (e.g., {"v":"2020","dt":"xsd:date"}) for
-- literal objects. Returns the new frame_id.
create or replace function donto_decompose_to_frame(
    p_subject     text,
    p_predicate   text,
    p_object_iri  text,
    p_context     text,
    p_frame_type  text default null,
    p_extra_roles jsonb default null,
    p_valid_lo    date default null,
    p_valid_hi    date default null,
    p_actor       text default null
) returns uuid
language plpgsql as $$
declare
    v_frame_iri text;
    v_frame_id  uuid;
    v_ft        text;
    v_tmpl      record;
    v_role      record;
begin
    -- Look up template (best match by source predicate).
    select * into v_tmpl from donto_decomposition_template
    where source_predicate = p_predicate
    limit 1;

    v_ft        := coalesce(p_frame_type, v_tmpl.frame_type, 'donto:Event');
    v_frame_iri := 'donto:frame/' || gen_random_uuid()::text;

    -- Create frame row.
    insert into donto_event_frame
        (frame_iri, frame_type, source_predicate, context)
    values (v_frame_iri, v_ft, p_predicate, p_context)
    returning frame_id into v_frame_id;

    -- Assert the frame type.
    perform donto_assert(
        v_frame_iri, 'rdf:type', v_ft, null,
        p_context, 'asserted', 0, p_valid_lo, p_valid_hi, p_actor
    );

    -- Subject role.
    perform donto_assert(
        v_frame_iri, p_predicate || '/subject', p_subject, null,
        p_context, 'asserted', 0, p_valid_lo, p_valid_hi, p_actor
    );

    -- Object role (IRI only — literal objects don't fit the role pattern).
    if p_object_iri is not null then
        perform donto_assert(
            v_frame_iri, p_predicate || '/object', p_object_iri, null,
            p_context, 'asserted', 0, p_valid_lo, p_valid_hi, p_actor
        );
    end if;

    -- Extra roles. Each key is a role predicate IRI; each value is either
    -- {"iri": "..."} for IRI objects or a literal jsonb for literal objects.
    if p_extra_roles is not null then
        for v_role in select key, value from jsonb_each(p_extra_roles) loop
            if jsonb_typeof(v_role.value) = 'object' and (v_role.value ? 'iri') then
                perform donto_assert(
                    v_frame_iri, v_role.key, v_role.value ->> 'iri', null,
                    p_context, 'asserted', 0, p_valid_lo, p_valid_hi, p_actor
                );
            else
                perform donto_assert(
                    v_frame_iri, v_role.key, null, v_role.value,
                    p_context, 'asserted', 0, p_valid_lo, p_valid_hi, p_actor
                );
            end if;
        end loop;
    end if;

    return v_frame_id;
end;
$$;
