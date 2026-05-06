-- v1000 / §7.3 extraction level overlay.
--
-- Extraction level is the epistemic act behind a claim:
--   quoted | table_read | example_observed | source_generalization
--   | cross_source_inference | model_hypothesis | human_hypothesis
--   | manual_entry | registry_import | adapter_import
--
-- Orthogonal to confidence and maturity. Auto-promotion gating uses
-- this column: model_hypothesis caps at E1, manual_entry caps at E2,
-- quoted/table_read may auto-reach E2.

create table if not exists donto_stmt_extraction_level (
    statement_id uuid primary key
                 references donto_statement(statement_id) on delete cascade,
    level        text not null check (level in (
        'quoted', 'table_read', 'example_observed',
        'source_generalization', 'cross_source_inference',
        'model_hypothesis', 'human_hypothesis',
        'manual_entry', 'registry_import', 'adapter_import'
    )),
    set_at       timestamptz not null default now(),
    set_by       text,
    metadata     jsonb not null default '{}'::jsonb
);

create index if not exists donto_stmt_extraction_level_idx
    on donto_stmt_extraction_level (level);

create or replace function donto_set_extraction_level(
    p_statement_id uuid,
    p_level        text,
    p_set_by       text default null
) returns void
language plpgsql as $$
begin
    insert into donto_stmt_extraction_level (statement_id, level, set_by)
    values (p_statement_id, p_level, p_set_by)
    on conflict (statement_id) do update set
        level  = excluded.level,
        set_by = excluded.set_by,
        set_at = now();
end;
$$;

-- Maturity ceiling per extraction level. Returns the highest E-level
-- a claim with this extraction level may auto-reach without explicit
-- review action.
create or replace function donto_max_auto_maturity(p_level text)
returns int
language sql immutable as $$
    select case p_level
        when 'quoted'                 then 2  -- E2 evidence-supported
        when 'table_read'             then 2
        when 'example_observed'       then 2
        when 'registry_import'        then 2
        when 'adapter_import'         then 2
        when 'source_generalization'  then 2
        when 'manual_entry'           then 2
        when 'cross_source_inference' then 1  -- E1 candidate (needs review)
        when 'human_hypothesis'       then 1
        when 'model_hypothesis'       then 1  -- never auto past E1
        else 1
    end
$$;

create or replace function donto_get_extraction_level(p_statement_id uuid)
returns text
language sql stable as $$
    select level from donto_stmt_extraction_level where statement_id = p_statement_id
$$;
