-- Evidence substrate: non-textual content regions.
--
-- Images, charts, diagrams, code blocks, formulas, and other
-- non-textual regions within a document revision. Claims extracted
-- from visual or structured content anchor here.

create table if not exists donto_content_region (
    region_id     uuid primary key default gen_random_uuid(),
    revision_id   uuid not null references donto_document_revision(revision_id),
    region_type   text not null check (region_type in (
        'image', 'chart', 'diagram', 'code_block', 'formula',
        'video', 'audio', 'map', 'screenshot', 'custom'
    )),
    label         text,
    caption       text,
    content_hash  bytea,
    content_bytes bytea,
    alt_text      text,
    span_id       uuid references donto_span(span_id),
    section_id    uuid references donto_document_section(section_id),
    metadata      jsonb not null default '{}'::jsonb,
    created_at    timestamptz not null default now()
);

create index if not exists donto_content_region_rev_idx
    on donto_content_region (revision_id);
create index if not exists donto_content_region_type_idx
    on donto_content_region (region_type);
create index if not exists donto_content_region_section_idx
    on donto_content_region (section_id)
    where section_id is not null;

create or replace function donto_add_content_region(
    p_revision_id uuid,
    p_region_type text,
    p_label       text default null,
    p_caption     text default null,
    p_alt_text    text default null,
    p_span_id     uuid default null,
    p_section_id  uuid default null
) returns uuid
language plpgsql as $$
declare v_id uuid;
begin
    insert into donto_content_region
        (revision_id, region_type, label, caption, alt_text,
         span_id, section_id)
    values (p_revision_id, p_region_type, p_label, p_caption,
            p_alt_text, p_span_id, p_section_id)
    returning region_id into v_id;
    return v_id;
end;
$$;
