-- Trust Kernel / §6.1 SourceObject: extend donto_document toward the PRD shape.
--
-- The PRD names these fields that don't yet exist on donto_document:
--   source_kind, creators, source_date, registered_by, policy_id,
--   content_address, native_format, adapter_used, status,
--   bibliographic_metadata (subset of metadata jsonb).
--
-- Rather than rewrite donto_document, we add nullable columns. Existing
-- callers (donto_ensure_document, donto_register_document) keep working;
-- new callers can populate the extension fields.
--
-- policy_id is added but its FK is wired in 0111 (policy capsule) so this
-- migration doesn't depend on M0 having shipped.

alter table donto_document
    add column if not exists source_kind text
        check (source_kind is null or source_kind in (
            'pdf', 'image', 'audio', 'video', 'dataset', 'table',
            'api', 'webpage', 'manuscript', 'database_release',
            'archive_record', 'other'
        ));

alter table donto_document
    add column if not exists creators jsonb not null default '[]'::jsonb;

-- EDTF-shaped (Extended Date Time Format) value, kept as jsonb for
-- precision/uncertainty support. Examples:
--   {"value": "1860"}, {"value": "1860..1862"}, {"value": "circa 1860"}
alter table donto_document
    add column if not exists source_date jsonb;

alter table donto_document
    add column if not exists registered_by text;

-- policy_id: typed as text; FK constraint added by migration 0111.
alter table donto_document
    add column if not exists policy_id text;

-- content_address: sha256 hex or external URI (e.g., s3://, ipfs://).
alter table donto_document
    add column if not exists content_address text;

alter table donto_document
    add column if not exists native_format text;

alter table donto_document
    add column if not exists adapter_used text;

alter table donto_document
    add column if not exists status text not null default 'registered'
        check (status in (
            'registered', 'ingested', 'quarantined', 'retired'
        ));

create index if not exists donto_document_source_kind_idx
    on donto_document (source_kind) where source_kind is not null;
create index if not exists donto_document_status_idx
    on donto_document (status);
create index if not exists donto_document_policy_idx
    on donto_document (policy_id) where policy_id is not null;
create index if not exists donto_document_content_address_idx
    on donto_document (content_address) where content_address is not null;

-- v1000 register-with-policy entrypoint. Calls into donto_register_document
-- and additionally enforces policy presence. Existing donto_register_document
-- continues to work for legacy callers.
create or replace function donto_register_source_v1000(
    p_iri              text,
    p_source_kind      text,
    p_policy_id        text,
    p_media_type       text default 'text/plain',
    p_label            text default null,
    p_source_url       text default null,
    p_language         text default null,
    p_creators         jsonb default '[]'::jsonb,
    p_source_date      jsonb default null,
    p_content_address  text default null,
    p_native_format    text default null,
    p_adapter_used     text default null,
    p_registered_by    text default null,
    p_metadata         jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    if p_policy_id is null or length(trim(p_policy_id)) = 0 then
        raise exception 'donto_register_source_v1000: policy_id is required (PRD I2)';
    end if;

    insert into donto_document
        (iri, media_type, label, source_url, language, metadata,
         source_kind, creators, source_date, registered_by,
         policy_id, content_address, native_format, adapter_used, status)
    values
        (p_iri, p_media_type, p_label, p_source_url, p_language, p_metadata,
         p_source_kind, p_creators, p_source_date, p_registered_by,
         p_policy_id, p_content_address, p_native_format, p_adapter_used,
         'registered')
    on conflict (iri) do update set
        media_type = excluded.media_type,
        label      = coalesce(excluded.label, donto_document.label),
        source_url = coalesce(excluded.source_url, donto_document.source_url),
        language   = coalesce(excluded.language, donto_document.language),
        metadata   = donto_document.metadata || excluded.metadata,
        source_kind = coalesce(excluded.source_kind, donto_document.source_kind),
        creators   = case
                        when excluded.creators = '[]'::jsonb then donto_document.creators
                        else excluded.creators
                     end,
        source_date = coalesce(excluded.source_date, donto_document.source_date),
        registered_by = coalesce(excluded.registered_by, donto_document.registered_by),
        policy_id  = coalesce(excluded.policy_id, donto_document.policy_id),
        content_address = coalesce(excluded.content_address, donto_document.content_address),
        native_format = coalesce(excluded.native_format, donto_document.native_format),
        adapter_used = coalesce(excluded.adapter_used, donto_document.adapter_used)
    returning document_id into v_id;

    return v_id;
end;
$$;
