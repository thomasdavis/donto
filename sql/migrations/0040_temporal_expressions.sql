-- Evidence substrate: temporal expressions.
--
-- Parsed temporal expressions linked to spans. Bridges raw text
-- ("last quarter", "circa 1850", "2023-10-10") to normalized
-- date ranges usable in valid_time queries.

create table if not exists donto_temporal_expression (
    expression_id  uuid primary key default gen_random_uuid(),
    span_id        uuid not null references donto_span(span_id),
    raw_text       text not null,
    resolved_from  date,
    resolved_to    date,
    resolution     text not null default 'exact'
                   check (resolution in (
                       'exact', 'day', 'month', 'year', 'decade',
                       'century', 'relative', 'vague', 'approximate'
                   )),
    reference_date date,
    confidence     double precision,
    run_id         uuid references donto_extraction_run(run_id),
    metadata       jsonb not null default '{}'::jsonb,
    created_at     timestamptz not null default now()
);

create index if not exists donto_temporal_expression_span_idx
    on donto_temporal_expression (span_id);
create index if not exists donto_temporal_expression_from_idx
    on donto_temporal_expression (resolved_from)
    where resolved_from is not null;
create index if not exists donto_temporal_expression_to_idx
    on donto_temporal_expression (resolved_to)
    where resolved_to is not null;

create or replace function donto_add_temporal_expression(
    p_span_id        uuid,
    p_raw_text       text,
    p_resolved_from  date default null,
    p_resolved_to    date default null,
    p_resolution     text default 'exact',
    p_reference_date date default null,
    p_confidence     double precision default null,
    p_run_id         uuid default null
) returns uuid
language plpgsql as $$
declare v_id uuid;
begin
    insert into donto_temporal_expression
        (span_id, raw_text, resolved_from, resolved_to, resolution,
         reference_date, confidence, run_id)
    values (p_span_id, p_raw_text, p_resolved_from, p_resolved_to,
            p_resolution, p_reference_date, p_confidence, p_run_id)
    returning expression_id into v_id;
    return v_id;
end;
$$;

-- Temporal expressions overlapping a date range
create or replace function donto_temporal_expressions_in_range(
    p_from date,
    p_to   date
) returns table(
    expression_id uuid, span_id uuid, raw_text text,
    resolved_from date, resolved_to date, resolution text
)
language sql stable as $$
    select expression_id, span_id, raw_text,
           resolved_from, resolved_to, resolution
    from donto_temporal_expression
    where (resolved_from is null or resolved_from < p_to)
      and (resolved_to is null or resolved_to > p_from)
$$;
