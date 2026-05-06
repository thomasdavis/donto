-- v1000 / §6.9 predicate minting workflow.
--
-- The PRD requires that a new predicate cannot be approved without a
-- definition, examples, domain/range hints, and nearest-neighbor
-- comparison against existing predicates.
--
-- Migration 0049 introduced donto_predicate_descriptor which already
-- carries label, gloss, subject_type, object_type, domain, examples,
-- and embedding. v1000 adds a minting-status enum plus a record of
-- the nearest-neighbor decision at mint time.

alter table donto_predicate_descriptor
    add column if not exists minting_status text not null default 'candidate'
        check (minting_status in (
            'candidate', 'approved', 'deprecated', 'merged'
        ));

alter table donto_predicate_descriptor
    add column if not exists minting_decision_at timestamptz;

alter table donto_predicate_descriptor
    add column if not exists minting_decision_by text;

alter table donto_predicate_descriptor
    add column if not exists nearest_existing_at_mint jsonb not null default '[]'::jsonb;

alter table donto_predicate_descriptor
    add column if not exists source_schema text;

alter table donto_predicate_descriptor
    add column if not exists definition text;

create index if not exists donto_pd_minting_status_idx
    on donto_predicate_descriptor (minting_status)
    where minting_status <> 'approved';
create index if not exists donto_pd_source_schema_idx
    on donto_predicate_descriptor (source_schema) where source_schema is not null;

-- Mint a candidate predicate. Refuses if any required field is missing.
-- The nearest-neighbor record is supplied by the caller (the embeddings
-- search runs in application code; this function validates and stores).
create or replace function donto_mint_predicate_candidate(
    p_iri             text,
    p_label           text,
    p_definition      text,
    p_subject_type    text,
    p_object_type     text,
    p_domain          text,
    p_examples        jsonb,                              -- array of {subject, object}
    p_nearest_at_mint jsonb,                              -- array of {predicate_id, similarity}
    p_source_schema   text default 'donto-native',
    p_minting_decision_by text default null,
    p_embedding_model text default null,
    p_embedding       float4[] default null
) returns text
language plpgsql as $$
declare
    v_iri  text;
begin
    if p_label is null or length(trim(p_label)) = 0 then
        raise exception 'donto_mint_predicate_candidate: label required';
    end if;
    if p_definition is null or length(trim(p_definition)) = 0 then
        raise exception 'donto_mint_predicate_candidate: definition required';
    end if;
    if p_subject_type is null or p_object_type is null then
        raise exception 'donto_mint_predicate_candidate: subject_type and object_type required';
    end if;
    if p_examples is null or jsonb_array_length(p_examples) = 0 then
        raise exception 'donto_mint_predicate_candidate: at least one example required';
    end if;
    if p_nearest_at_mint is null then
        raise exception 'donto_mint_predicate_candidate: nearest-neighbor record required';
    end if;

    perform donto_implicit_register(p_iri);

    insert into donto_predicate_descriptor
        (iri, label, gloss, subject_type, object_type, domain,
         example_subject, example_object,
         embedding_model, embedding, metadata,
         minting_status, source_schema, definition,
         nearest_existing_at_mint, minting_decision_by)
    values
        (p_iri, p_label, p_definition, p_subject_type, p_object_type, p_domain,
         (p_examples->0->>'subject'),
         (p_examples->0->>'object'),
         p_embedding_model, p_embedding,
         jsonb_build_object('all_examples', p_examples),
         'candidate', p_source_schema, p_definition,
         p_nearest_at_mint, p_minting_decision_by)
    on conflict (iri) do update set
        label                    = excluded.label,
        gloss                    = excluded.gloss,
        subject_type             = excluded.subject_type,
        object_type              = excluded.object_type,
        domain                   = excluded.domain,
        definition               = excluded.definition,
        source_schema            = excluded.source_schema,
        nearest_existing_at_mint = excluded.nearest_existing_at_mint,
        updated_at               = now()
    returning iri into v_iri;

    perform donto_emit_event(
        'predicate_descriptor', v_iri, 'created',
        coalesce(p_minting_decision_by, 'system'),
        jsonb_build_object('label', p_label, 'minting_status', 'candidate')
    );
    return v_iri;
end;
$$;

-- Approve a candidate.
create or replace function donto_approve_predicate(
    p_iri              text,
    p_minting_decision_by text default 'system'
) returns boolean
language plpgsql as $$
declare
    v_n int;
begin
    update donto_predicate_descriptor
    set minting_status        = 'approved',
        minting_decision_at   = now(),
        minting_decision_by   = p_minting_decision_by
    where iri = p_iri and minting_status <> 'approved';
    get diagnostics v_n = row_count;

    if v_n > 0 then
        perform donto_emit_event(
            'predicate_descriptor', p_iri, 'approved',
            p_minting_decision_by,
            jsonb_build_object('previous_status', 'candidate')
        );
    end if;
    return v_n > 0;
end;
$$;

-- Deprecate a predicate.
create or replace function donto_deprecate_predicate(
    p_iri      text,
    p_actor    text default 'system',
    p_reason   text default null
) returns boolean
language plpgsql as $$
declare
    v_n int;
begin
    update donto_predicate_descriptor
    set minting_status      = 'deprecated',
        minting_decision_at = now(),
        minting_decision_by = p_actor,
        metadata            = metadata || jsonb_build_object('deprecation_reason', p_reason)
    where iri = p_iri and minting_status <> 'deprecated';
    get diagnostics v_n = row_count;

    if v_n > 0 then
        perform donto_emit_event(
            'predicate_descriptor', p_iri, 'updated',
            p_actor,
            jsonb_build_object('status', 'deprecated', 'reason', p_reason)
        );
    end if;
    return v_n > 0;
end;
$$;

-- Pre-approval gate for production use of a candidate. Returns false
-- if the predicate is candidate or deprecated.
create or replace function donto_predicate_is_approved(p_iri text)
returns boolean
language sql stable as $$
    select coalesce(
        (select minting_status = 'approved'
         from donto_predicate_descriptor where iri = p_iri),
        false
    )
$$;
