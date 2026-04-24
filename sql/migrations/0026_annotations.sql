-- Evidence substrate §4: annotation spaces and annotations.
--
-- Annotation spaces are named feature namespaces (e.g. Universal
-- Dependencies POS tags, NER labels, morphological features). They
-- provide stable vocabularies for machine-generated observations.
--
-- Annotations attach feature-value pairs to spans within a space.
-- They are the observation layer: raw machine output that has not
-- yet been promoted to statements.

create table if not exists donto_annotation_space (
    space_id     uuid primary key default gen_random_uuid(),
    iri          text not null unique,
    label        text,
    feature_ns   text,
    version      text,
    metadata     jsonb not null default '{}'::jsonb,
    created_at   timestamptz not null default now()
);

create or replace function donto_ensure_annotation_space(
    p_iri        text,
    p_label      text default null,
    p_feature_ns text default null,
    p_version    text default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    select space_id into v_id from donto_annotation_space where iri = p_iri;
    if v_id is not null then return v_id; end if;
    insert into donto_annotation_space (iri, label, feature_ns, version)
    values (p_iri, p_label, p_feature_ns, p_version)
    on conflict (iri) do nothing
    returning space_id into v_id;
    if v_id is null then
        select space_id into v_id from donto_annotation_space where iri = p_iri;
    end if;
    return v_id;
end;
$$;

create table if not exists donto_annotation (
    annotation_id  uuid primary key default gen_random_uuid(),
    span_id        uuid not null references donto_span(span_id),
    space_id       uuid not null references donto_annotation_space(space_id),
    feature        text not null,
    value          text,
    value_detail   jsonb,
    confidence     double precision,
    run_id         uuid,
    metadata       jsonb not null default '{}'::jsonb,
    created_at     timestamptz not null default now()
);

create index if not exists donto_annotation_span_idx
    on donto_annotation (span_id);
create index if not exists donto_annotation_space_idx
    on donto_annotation (space_id);
create index if not exists donto_annotation_feature_idx
    on donto_annotation (space_id, feature);
create index if not exists donto_annotation_value_idx
    on donto_annotation (space_id, feature, value)
    where value is not null;
create index if not exists donto_annotation_run_idx
    on donto_annotation (run_id) where run_id is not null;
create index if not exists donto_annotation_confidence_idx
    on donto_annotation (confidence)
    where confidence is not null;

-- Batch-insert annotations for an extraction run.
create or replace function donto_annotate_span(
    p_span_id    uuid,
    p_space_id   uuid,
    p_feature    text,
    p_value      text default null,
    p_detail     jsonb default null,
    p_confidence double precision default null,
    p_run_id     uuid default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    insert into donto_annotation
        (span_id, space_id, feature, value, value_detail, confidence, run_id)
    values (p_span_id, p_space_id, p_feature, p_value, p_detail, p_confidence, p_run_id)
    returning annotation_id into v_id;
    return v_id;
end;
$$;

-- Annotations for a span, optionally filtered by space and/or feature.
create or replace function donto_annotations_for_span(
    p_span_id  uuid,
    p_space_id uuid default null,
    p_feature  text default null
) returns table(
    annotation_id uuid, space_id uuid, feature text,
    value text, value_detail jsonb, confidence double precision
)
language sql stable as $$
    select annotation_id, space_id, feature, value, value_detail, confidence
    from donto_annotation
    where span_id = p_span_id
      and (p_space_id is null or space_id = p_space_id)
      and (p_feature  is null or feature  = p_feature)
$$;
