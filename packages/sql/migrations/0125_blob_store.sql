-- Blob store substrate (M1+ evidence chain hardening).
--
-- Problem: donto stores raw text in donto_document_revision.body for
-- every revision. That's correct as a substrate, but it's the wrong
-- place to keep the *canonical* bytes for arbitrary source material
-- (PDFs, images, audio, multi-megabyte transcripts). It also has no
-- de-duplication story: two revisions of the same OCR run hold two
-- copies of identical text.
--
-- Resolution:
--
--   donto_blob               — content-addressed registry (sha256 PK).
--                              One row per unique byte sequence.
--                              `bucket_uri` may be NULL when the blob
--                              lives only in donto_document_revision
--                              (for backwards compatibility).
--
--   donto_document_revision  — gains:
--       blob_hash    bytea references donto_blob(sha256)
--       body_uri     text   (e.g. gs://bucket/sha256/<hex>,
--                            file:///mnt/donto-data/blobs/sha256/<hex>)
--       body_inline  text   (denormalised from blob for FTS-eligible
--                            formats; same bytes, optional cache)
--       byte_size    bigint (canonical size in bytes)
--       body_storage text   ('inline' | 'bucket' | 'both')
--
--   Existing `body` column is preserved as a synonym/cache; nothing
--   that reads it breaks. A later migration can drop it once all
--   readers are on body_inline + body_uri.
--
-- Backfill:
--   For every existing donto_document_revision with content_hash set,
--   insert a donto_blob row (sha256=content_hash, byte_size=length of
--   body), copy body to body_inline, set blob_hash + body_storage=inline.
--
-- Idempotent: on conflict do nothing throughout. Re-runnable.

create table if not exists donto_blob (
    sha256        bytea primary key,
    byte_size     bigint not null check (byte_size >= 0),
    mime_type     text,
    bucket_uri    text,
    first_seen_at timestamptz not null default now(),
    metadata      jsonb not null default '{}'::jsonb
);

create index if not exists donto_blob_bucket_uri_idx
    on donto_blob (bucket_uri)
    where bucket_uri is not null;

create index if not exists donto_blob_mime_idx
    on donto_blob (mime_type)
    where mime_type is not null;

-- Add the columns to the revision table. Idempotent: postgres
-- ignores `add column if not exists`.
alter table donto_document_revision
    add column if not exists blob_hash    bytea references donto_blob(sha256) deferrable initially deferred,
    add column if not exists body_uri     text,
    add column if not exists body_inline  text,
    add column if not exists byte_size    bigint,
    add column if not exists body_storage text;

-- A check that body_storage is one of the documented values.
do $$
begin
    if not exists (
        select 1 from pg_constraint
        where conname = 'donto_document_revision_body_storage_check'
    ) then
        alter table donto_document_revision
            add constraint donto_document_revision_body_storage_check
            check (body_storage is null
                   or body_storage in ('inline', 'bucket', 'both'));
    end if;
end$$;

create index if not exists donto_revision_blob_idx
    on donto_document_revision (blob_hash)
    where blob_hash is not null;

-- Backfill: any revision with body text gets a corresponding blob
-- row and its own blob_hash. content_hash is already SHA-256 of the
-- body per migration 0023, so we can re-use it directly.
insert into donto_blob (sha256, byte_size)
select content_hash, octet_length(coalesce(body, ''))
from donto_document_revision
where content_hash is not null
on conflict (sha256) do nothing;

update donto_document_revision
   set blob_hash    = coalesce(blob_hash, content_hash),
       body_inline  = coalesce(body_inline, body),
       byte_size    = coalesce(byte_size, octet_length(coalesce(body, ''))),
       body_storage = coalesce(body_storage, 'inline')
 where content_hash is not null
   and (blob_hash is null or body_inline is null or byte_size is null or body_storage is null);

-- Helper: idempotent blob registration. Returns the sha256.
create or replace function donto_register_blob(
    p_sha256     bytea,
    p_byte_size  bigint,
    p_mime_type  text default null,
    p_bucket_uri text default null,
    p_metadata   jsonb default '{}'::jsonb
) returns bytea
language plpgsql as $$
declare
    v_sha bytea;
begin
    insert into donto_blob (sha256, byte_size, mime_type, bucket_uri, metadata)
    values (p_sha256, p_byte_size, p_mime_type, p_bucket_uri, p_metadata)
    on conflict (sha256) do update
       set mime_type   = coalesce(donto_blob.mime_type, excluded.mime_type),
           bucket_uri  = coalesce(donto_blob.bucket_uri, excluded.bucket_uri),
           metadata    = donto_blob.metadata || excluded.metadata
    returning sha256 into v_sha;
    return v_sha;
end$$;

-- Helper: bind a blob URI to an existing blob row (called after a
-- successful bucket upload).
create or replace function donto_blob_set_bucket_uri(
    p_sha256     bytea,
    p_bucket_uri text
) returns void
language sql as $$
    update donto_blob set bucket_uri = p_bucket_uri
     where sha256 = p_sha256
       and (bucket_uri is null or bucket_uri = p_bucket_uri);
$$;

-- Helper: ref-count blobs through their revisions. Useful for GC.
create or replace function donto_blob_ref_count(p_sha256 bytea)
returns bigint
language sql stable as $$
    select count(*)::bigint
      from donto_document_revision
     where blob_hash = p_sha256;
$$;

-- Per-blob mime sniff registry (caller-side; we don't ship a Rust
-- mime crate dep in the substrate). Trivial registry.
comment on table donto_blob is
    'Content-addressed registry of every unique byte sequence donto stores. '
    'sha256 is the primary key. bucket_uri is NULL when the blob lives only '
    'in donto_document_revision (legacy / dev) — that''s the cache of last '
    'resort. Once a blob has a bucket_uri set, that URI is the canonical '
    'home; body_inline on the revision becomes a denormalised cache for '
    'FTS-eligible formats.';

comment on column donto_document_revision.blob_hash is
    'FK to donto_blob(sha256). Every revision should set this. Legacy rows '
    'have it backfilled from the column''s existing content_hash by 0125.';

comment on column donto_document_revision.body_inline is
    'The bytes, inline. Set for FTS-eligible formats (markdown / plain '
    'text). NULL for binary blobs (pdf / image / audio) — those are bucket-only.';

comment on column donto_document_revision.body_storage is
    'Where the canonical bytes live: ''inline'' = donto only, ''bucket'' = '
    'GCS/S3 only, ''both'' = inline cache + bucket. Migration target is '
    '''bucket'' for everything; ''inline'' / ''both'' covers legacy.';
