-- Evidence substrate §3: standoff spans over document revisions.
--
-- A span identifies a contiguous region within a document revision.
-- Modeled after W3C Web Annotation selectors (TextPositionSelector,
-- XPathSelector, CssSelector, FragmentSelector). Spans are the
-- anchoring layer between documents and annotations/statements.
--
-- char_offset spans use start_offset/end_offset (0-based, half-open).
-- Other span types use the selector jsonb for type-specific addressing.

create table if not exists donto_span (
    span_id       uuid primary key default gen_random_uuid(),
    revision_id   uuid not null references donto_document_revision(revision_id),
    span_type     text not null check (span_type in (
        'char_offset', 'token', 'sentence', 'paragraph',
        'page', 'line', 'region', 'xpath', 'css', 'custom'
    )),
    start_offset  int,
    end_offset    int,
    selector      jsonb,
    surface_text  text,
    metadata      jsonb not null default '{}'::jsonb,
    created_at    timestamptz not null default now(),
    constraint donto_span_offset_order
        check (start_offset is null or end_offset is null
               or start_offset <= end_offset)
);

create index if not exists donto_span_revision_idx
    on donto_span (revision_id);
create index if not exists donto_span_type_idx
    on donto_span (span_type);
create index if not exists donto_span_offsets_idx
    on donto_span (revision_id, start_offset, end_offset)
    where start_offset is not null;
create index if not exists donto_span_surface_trgm_idx
    on donto_span using gin (surface_text gin_trgm_ops)
    where surface_text is not null;

-- Create a character-offset span. Convenience wrapper.
create or replace function donto_create_char_span(
    p_revision_id  uuid,
    p_start        int,
    p_end          int,
    p_surface_text text default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    if p_start > p_end then
        raise exception 'donto_create_char_span: start (%) > end (%)', p_start, p_end;
    end if;
    insert into donto_span (revision_id, span_type, start_offset, end_offset, surface_text)
    values (p_revision_id, 'char_offset', p_start, p_end, p_surface_text)
    returning span_id into v_id;
    return v_id;
end;
$$;

-- Find spans overlapping a given character range in a revision.
create or replace function donto_spans_overlapping(
    p_revision_id uuid,
    p_start       int,
    p_end         int
) returns table(span_id uuid, span_type text, start_offset int, end_offset int, surface_text text)
language sql stable as $$
    select span_id, span_type, start_offset, end_offset, surface_text
    from donto_span
    where revision_id = p_revision_id
      and start_offset is not null
      and end_offset is not null
      and start_offset < p_end
      and end_offset > p_start
$$;
