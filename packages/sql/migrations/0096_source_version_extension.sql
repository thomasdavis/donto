-- v1000 / §6.2 SourceVersion: extend donto_document_revision toward the
-- PRD shape. PRD-named fields not yet present:
--   version_kind, quality_metrics, derived_from_versions
--
-- (content_hash already exists. created_by is a new column.)

alter table donto_document_revision
    add column if not exists version_kind text not null default 'raw'
        check (version_kind in (
            'raw', 'ocr', 'transcript', 'parsed',
            'normalized', 'translated', 'redacted'
        ));

alter table donto_document_revision
    add column if not exists quality_metrics jsonb not null default '{}'::jsonb;

alter table donto_document_revision
    add column if not exists derived_from_versions uuid[] not null default '{}';

alter table donto_document_revision
    add column if not exists created_by text;

create index if not exists donto_revision_version_kind_idx
    on donto_document_revision (version_kind);
create index if not exists donto_revision_derived_from_gin
    on donto_document_revision using gin (derived_from_versions);

-- v1000 add-revision entrypoint. Adds version_kind, quality_metrics,
-- derived_from. Existing donto_add_revision continues to work.
create or replace function donto_add_revision_v1000(
    p_document_id          uuid,
    p_version_kind         text,
    p_body                 text default null,
    p_body_bytes           bytea default null,
    p_parser_version       text default null,
    p_quality_metrics      jsonb default '{}'::jsonb,
    p_derived_from         uuid[] default '{}',
    p_created_by           text default null,
    p_metadata             jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_hash bytea;
    v_next int;
    v_id   uuid;
begin
    if p_body is null and p_body_bytes is null then
        raise exception 'donto_add_revision_v1000: body or body_bytes required';
    end if;

    v_hash := digest(coalesce(p_body, '') || coalesce(encode(p_body_bytes, 'hex'), ''), 'sha256');

    select revision_id into v_id
    from donto_document_revision
    where document_id = p_document_id and content_hash = v_hash;
    if v_id is not null then
        -- Idempotent: re-call with same content updates metadata only.
        update donto_document_revision
        set version_kind         = p_version_kind,
            quality_metrics      = p_quality_metrics,
            derived_from_versions = p_derived_from,
            created_by           = coalesce(p_created_by, created_by),
            metadata             = metadata || p_metadata
        where revision_id = v_id;
        return v_id;
    end if;

    select coalesce(max(revision_number), 0) + 1 into v_next
    from donto_document_revision where document_id = p_document_id;

    insert into donto_document_revision
        (document_id, revision_number, body, body_bytes, content_hash,
         parser_version, metadata, version_kind, quality_metrics,
         derived_from_versions, created_by)
    values
        (p_document_id, v_next, p_body, p_body_bytes, v_hash,
         p_parser_version, p_metadata, p_version_kind,
         p_quality_metrics, p_derived_from, p_created_by)
    returning revision_id into v_id;
    return v_id;
end;
$$;

-- Lineage walker: list all ancestors of a revision.
create or replace function donto_revision_lineage(p_revision_id uuid)
returns table(revision_id uuid, version_kind text, depth int)
language sql stable as $$
    with recursive lineage as (
        select revision_id, version_kind, 0 as depth, derived_from_versions
        from donto_document_revision
        where revision_id = p_revision_id
        union all
        select r.revision_id, r.version_kind, l.depth + 1, r.derived_from_versions
        from donto_document_revision r
        join lineage l on r.revision_id = any(l.derived_from_versions)
    )
    select revision_id, version_kind, depth from lineage
$$;
