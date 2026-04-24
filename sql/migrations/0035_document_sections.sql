-- Evidence substrate: structured document regions.
--
-- Hierarchical sections (h1/h2/h3), tables with row/column structure,
-- and individual table cells. These let extractors anchor claims to
-- specific structural elements, not just character offsets.

-- Sections: hierarchical document structure
create table if not exists donto_document_section (
    section_id        uuid primary key default gen_random_uuid(),
    revision_id       uuid not null references donto_document_revision(revision_id),
    parent_section_id uuid references donto_document_section(section_id),
    level             smallint not null default 1,
    title             text,
    ordinal           int not null default 0,
    span_id           uuid references donto_span(span_id),
    metadata          jsonb not null default '{}'::jsonb,
    constraint donto_section_no_self_parent
        check (parent_section_id is distinct from section_id)
);

create index if not exists donto_document_section_rev_idx
    on donto_document_section (revision_id);
create index if not exists donto_document_section_parent_idx
    on donto_document_section (parent_section_id)
    where parent_section_id is not null;

-- Tables within documents
create table if not exists donto_table (
    table_id      uuid primary key default gen_random_uuid(),
    revision_id   uuid not null references donto_document_revision(revision_id),
    section_id    uuid references donto_document_section(section_id),
    label         text,
    caption       text,
    row_count     int,
    col_count     int,
    span_id       uuid references donto_span(span_id),
    metadata      jsonb not null default '{}'::jsonb
);

create index if not exists donto_table_rev_idx
    on donto_table (revision_id);
create index if not exists donto_table_section_idx
    on donto_table (section_id) where section_id is not null;

-- Table cells
create table if not exists donto_table_cell (
    cell_id       uuid primary key default gen_random_uuid(),
    table_id      uuid not null references donto_table(table_id),
    row_idx       int not null,
    col_idx       int not null,
    is_header     boolean not null default false,
    row_header    text,
    col_header    text,
    value         text,
    value_numeric double precision,
    span_id       uuid references donto_span(span_id),
    metadata      jsonb not null default '{}'::jsonb,
    unique (table_id, row_idx, col_idx)
);

create index if not exists donto_table_cell_table_idx
    on donto_table_cell (table_id);
create index if not exists donto_table_cell_header_idx
    on donto_table_cell (table_id, col_header)
    where col_header is not null;

-- Helper: register a section
create or replace function donto_add_section(
    p_revision_id uuid,
    p_title       text,
    p_level       smallint default 1,
    p_parent      uuid default null,
    p_ordinal     int default 0,
    p_span_id     uuid default null
) returns uuid
language plpgsql as $$
declare v_id uuid;
begin
    insert into donto_document_section
        (revision_id, title, level, parent_section_id, ordinal, span_id)
    values (p_revision_id, p_title, p_level, p_parent, p_ordinal, p_span_id)
    returning section_id into v_id;
    return v_id;
end;
$$;

-- Helper: register a table
create or replace function donto_add_table(
    p_revision_id uuid,
    p_label       text,
    p_caption     text default null,
    p_row_count   int default null,
    p_col_count   int default null,
    p_section_id  uuid default null,
    p_span_id     uuid default null
) returns uuid
language plpgsql as $$
declare v_id uuid;
begin
    insert into donto_table
        (revision_id, label, caption, row_count, col_count, section_id, span_id)
    values (p_revision_id, p_label, p_caption, p_row_count, p_col_count,
            p_section_id, p_span_id)
    returning table_id into v_id;
    return v_id;
end;
$$;

-- Helper: add a cell
create or replace function donto_add_table_cell(
    p_table_id    uuid,
    p_row_idx     int,
    p_col_idx     int,
    p_value       text,
    p_row_header  text default null,
    p_col_header  text default null,
    p_is_header   boolean default false,
    p_value_numeric double precision default null,
    p_span_id     uuid default null
) returns uuid
language plpgsql as $$
declare v_id uuid;
begin
    insert into donto_table_cell
        (table_id, row_idx, col_idx, value, row_header, col_header,
         is_header, value_numeric, span_id)
    values (p_table_id, p_row_idx, p_col_idx, p_value, p_row_header,
            p_col_header, p_is_header, p_value_numeric, p_span_id)
    on conflict (table_id, row_idx, col_idx) do update set
        value = excluded.value,
        value_numeric = excluded.value_numeric,
        row_header = coalesce(excluded.row_header, donto_table_cell.row_header),
        col_header = coalesce(excluded.col_header, donto_table_cell.col_header)
    returning cell_id into v_id;
    return v_id;
end;
$$;

-- Query: get all cells for a table, ordered
create or replace function donto_table_cells(p_table_id uuid)
returns table(
    cell_id uuid, row_idx int, col_idx int,
    is_header boolean, row_header text, col_header text,
    value text, value_numeric double precision
)
language sql stable as $$
    select cell_id, row_idx, col_idx, is_header, row_header, col_header,
           value, value_numeric
    from donto_table_cell where table_id = p_table_id
    order by row_idx, col_idx
$$;
