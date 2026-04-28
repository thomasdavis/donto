-- Evidence substrate §6: extraction run provenance.
--
-- Every machine-generated observation should be traceable to an
-- extraction run. Runs record model identity, parameters, source
-- material, and status. PROV-O compatible: the run is an Activity,
-- the source revision is an Entity used, and the outputs (annotations
-- and statements) are Entities generated.

create table if not exists donto_extraction_run (
    run_id              uuid primary key default gen_random_uuid(),
    model_id            text,
    model_version       text,
    prompt_hash         bytea,
    prompt_template     text,
    chunking_strategy   text,
    temperature         double precision,
    seed                bigint,
    toolchain           jsonb not null default '{}'::jsonb,
    source_revision_id  uuid references donto_document_revision(revision_id),
    context             text references donto_context(iri),
    status              text not null default 'running'
                        check (status in ('running','completed','failed','partial')),
    started_at          timestamptz not null default now(),
    completed_at        timestamptz,
    statements_emitted  bigint not null default 0,
    annotations_emitted bigint not null default 0,
    metadata            jsonb not null default '{}'::jsonb
);

create index if not exists donto_extraction_run_source_idx
    on donto_extraction_run (source_revision_id)
    where source_revision_id is not null;
create index if not exists donto_extraction_run_model_idx
    on donto_extraction_run (model_id) where model_id is not null;
create index if not exists donto_extraction_run_status_idx
    on donto_extraction_run (status);
create index if not exists donto_extraction_run_context_idx
    on donto_extraction_run (context) where context is not null;
create index if not exists donto_extraction_run_started_idx
    on donto_extraction_run (started_at);

-- Now wire the FK from donto_annotation.run_id.
do $$ begin
    if not exists (
        select 1 from information_schema.table_constraints
        where constraint_name = 'donto_annotation_run_fk'
          and table_name = 'donto_annotation'
    ) then
        alter table donto_annotation
            add constraint donto_annotation_run_fk
            foreign key (run_id) references donto_extraction_run(run_id);
    end if;
end $$;

-- Start a new extraction run. Returns the run_id.
create or replace function donto_start_extraction(
    p_model_id           text default null,
    p_model_version      text default null,
    p_source_revision_id uuid default null,
    p_context            text default null,
    p_prompt_template    text default null,
    p_temperature        double precision default null,
    p_seed               bigint default null,
    p_chunking           text default null,
    p_toolchain          jsonb default '{}'::jsonb,
    p_metadata           jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    if p_context is not null then
        perform donto_ensure_context(p_context);
    end if;
    insert into donto_extraction_run
        (model_id, model_version, source_revision_id, context,
         prompt_template, temperature, seed, chunking_strategy,
         toolchain, metadata)
    values (p_model_id, p_model_version, p_source_revision_id, p_context,
            p_prompt_template, p_temperature, p_seed, p_chunking,
            p_toolchain, p_metadata)
    returning run_id into v_id;
    return v_id;
end;
$$;

-- Complete an extraction run with final counts.
create or replace function donto_complete_extraction(
    p_run_id              uuid,
    p_status              text default 'completed',
    p_statements_emitted  bigint default null,
    p_annotations_emitted bigint default null
) returns void
language plpgsql as $$
begin
    update donto_extraction_run
    set status              = p_status,
        completed_at        = now(),
        statements_emitted  = coalesce(p_statements_emitted, statements_emitted),
        annotations_emitted = coalesce(p_annotations_emitted, annotations_emitted)
    where run_id = p_run_id;
end;
$$;
