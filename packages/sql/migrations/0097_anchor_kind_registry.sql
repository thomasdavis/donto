-- Trust Kernel / §6.3 EvidenceAnchor: typed anchor-kind registry with
-- per-kind locator schemas.
--
-- donto_span (migration 0025) and donto_content_regions (0041) already
-- store anchor data. They do not enforce a typed locator schema; the
-- region jsonb is free-form. This migration introduces a registry of
-- anchor kinds plus a JSON-Schema-like locator validator.
--
-- The validator is intentionally simple. SHACL-grade validation is
-- deferred. Today we check required keys per kind; adapters that need
-- richer validation can add their own pre-write checks.

create table if not exists donto_anchor_kind (
    kind                text primary key,
    description         text not null,
    required_keys       text[] not null,
    optional_keys       text[] not null default '{}',
    locator_schema_version text not null default 'anchor-schema-1',
    is_active           boolean not null default true
);

-- Seed the anchor kinds.
insert into donto_anchor_kind (kind, description, required_keys, optional_keys) values
    ('whole_source',
     'The whole source object; locator is empty.',
     '{}', '{}'),
    ('char_span',
     'Character span in extracted text. Locator: {start, end}.',
     '{start,end}', '{text}'),
    ('page_box',
     'Bounding box on a PDF page. Locator: {page, x, y, w, h} normalized 0..1.',
     '{page,x,y,w,h}', '{text}'),
    ('image_box',
     'Bounding box on an image. Locator: {x, y, w, h}.',
     '{x,y,w,h}', '{caption}'),
    ('media_time',
     'Time range in audio/video. Locator: {start_ms, end_ms}; optional track.',
     '{start_ms,end_ms}', '{track,channel}'),
    ('table_cell',
     'Specific cell in a parsed table. Locator: {row_id, column}; optional sheet.',
     '{row_id,column}', '{sheet,table_id}'),
    ('csv_row',
     'CSV row. Locator: {row_index, columns}.',
     '{row_index,columns}', '{file}'),
    ('json_pointer',
     'RFC 6901 JSON Pointer. Locator: {pointer}.',
     '{pointer}', '{value}'),
    ('xml_xpath',
     'XPath into an XML document. Locator: {xpath}.',
     '{xpath}', '{namespace}'),
    ('html_css',
     'CSS selector into HTML. Locator: {selector}.',
     '{selector}', '{nth}'),
    ('token_range',
     'Token range in a tokenized corpus. Locator: {sentence_id, start, end}.',
     '{sentence_id,start,end}', '{text_id}'),
    ('annotation_id',
     'External annotation system ID (e.g., ELAN annotation ID).',
     '{annotation_id}', '{tier_id}'),
    ('archive_field',
     'Field within an archival catalogue record.',
     '{record_id,field_name}', '{}')
on conflict (kind) do update set
    description = excluded.description,
    required_keys = excluded.required_keys,
    optional_keys = excluded.optional_keys;

-- Validate a locator against the registered kind.
create or replace function donto_validate_anchor_locator(
    p_kind     text,
    p_locator  jsonb
) returns boolean
language plpgsql stable as $$
declare
    v_required text[];
    v_key      text;
begin
    select required_keys into v_required
    from donto_anchor_kind
    where kind = p_kind and is_active = true;

    if v_required is null then
        raise exception 'donto_validate_anchor_locator: unknown anchor kind %', p_kind;
    end if;

    foreach v_key in array v_required loop
        if not (p_locator ? v_key) then
            return false;
        end if;
    end loop;
    return true;
end;
$$;

-- Validate-or-raise variant useful in triggers.
create or replace function donto_assert_anchor_locator(
    p_kind     text,
    p_locator  jsonb
) returns void
language plpgsql as $$
begin
    if not donto_validate_anchor_locator(p_kind, p_locator) then
        raise exception
            'invalid locator for anchor kind %: missing required key(s)', p_kind;
    end if;
end;
$$;
