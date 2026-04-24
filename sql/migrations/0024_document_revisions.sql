-- Evidence substrate §2: immutable document revisions.
--
-- A revision is a concrete content snapshot of a document. The same
-- document may have multiple revisions (OCR re-runs, re-parses, edits).
-- Body is text for textual content; body_bytes for binary. At least one
-- is required. The content_hash ensures deduplication.

create table if not exists donto_document_revision (
    revision_id     uuid primary key default gen_random_uuid(),
    document_id     uuid not null references donto_document(document_id),
    revision_number int not null default 1,
    body            text,
    body_bytes      bytea,
    content_hash    bytea not null,
    parser_version  text,
    metadata        jsonb not null default '{}'::jsonb,
    created_at      timestamptz not null default now(),
    unique (document_id, revision_number),
    constraint donto_revision_has_content
        check (body is not null or body_bytes is not null)
);

create index if not exists donto_document_revision_doc_idx
    on donto_document_revision (document_id);
create index if not exists donto_document_revision_hash_idx
    on donto_document_revision (content_hash);

-- Add a revision to a document. Auto-increments revision_number.
-- Returns the revision_id. Idempotent on content_hash per document.
create or replace function donto_add_revision(
    p_document_id    uuid,
    p_body           text default null,
    p_body_bytes     bytea default null,
    p_parser_version text default null,
    p_metadata       jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_hash bytea;
    v_next int;
    v_id   uuid;
begin
    if p_body is null and p_body_bytes is null then
        raise exception 'donto_add_revision: body or body_bytes required';
    end if;

    v_hash := digest(coalesce(p_body, '') || coalesce(encode(p_body_bytes, 'hex'), ''), 'sha256');

    select revision_id into v_id
    from donto_document_revision
    where document_id = p_document_id and content_hash = v_hash;
    if v_id is not null then
        return v_id;
    end if;

    select coalesce(max(revision_number), 0) + 1 into v_next
    from donto_document_revision where document_id = p_document_id;

    insert into donto_document_revision
        (document_id, revision_number, body, body_bytes, content_hash,
         parser_version, metadata)
    values (p_document_id, v_next, p_body, p_body_bytes, v_hash,
            p_parser_version, p_metadata)
    returning revision_id into v_id;
    return v_id;
end;
$$;

-- Convenience: get the latest revision of a document.
create or replace function donto_latest_revision(p_document_id uuid)
returns uuid
language sql stable as $$
    select revision_id from donto_document_revision
    where document_id = p_document_id
    order by revision_number desc limit 1
$$;
