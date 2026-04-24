-- Evidence substrate §1: immutable document objects.
--
-- Documents are the raw material from which statements are extracted.
-- They are immutable once created; edits create new revisions (0024).
-- The document IRI is the stable identifier; revisions track content
-- over time.

create table if not exists donto_document (
    document_id   uuid primary key default gen_random_uuid(),
    iri           text not null unique,
    media_type    text not null default 'text/plain',
    label         text,
    source_url    text,
    language      text,          -- BCP 47 primary subtag
    metadata      jsonb not null default '{}'::jsonb,
    created_at    timestamptz not null default now()
);

create index if not exists donto_document_media_type_idx
    on donto_document (media_type);
create index if not exists donto_document_language_idx
    on donto_document (language) where language is not null;
create index if not exists donto_document_created_idx
    on donto_document (created_at);

-- Idempotent ensure, analogous to donto_ensure_context.
create or replace function donto_ensure_document(
    p_iri        text,
    p_media_type text default 'text/plain',
    p_label      text default null,
    p_source_url text default null,
    p_language   text default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    select document_id into v_id from donto_document where iri = p_iri;
    if v_id is not null then
        return v_id;
    end if;
    insert into donto_document (iri, media_type, label, source_url, language)
    values (p_iri, p_media_type, p_label, p_source_url, p_language)
    on conflict (iri) do nothing
    returning document_id into v_id;
    if v_id is null then
        select document_id into v_id from donto_document where iri = p_iri;
    end if;
    return v_id;
end;
$$;

-- Register a document with full metadata.
create or replace function donto_register_document(
    p_iri        text,
    p_media_type text default 'text/plain',
    p_label      text default null,
    p_source_url text default null,
    p_language   text default null,
    p_metadata   jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    insert into donto_document (iri, media_type, label, source_url, language, metadata)
    values (p_iri, p_media_type, p_label, p_source_url, p_language, p_metadata)
    on conflict (iri) do update set
        media_type = excluded.media_type,
        label      = coalesce(excluded.label, donto_document.label),
        source_url = coalesce(excluded.source_url, donto_document.source_url),
        language   = coalesce(excluded.language, donto_document.language),
        metadata   = donto_document.metadata || excluded.metadata
    returning document_id into v_id;
    return v_id;
end;
$$;
