-- Alignment run tracking.
--
-- Mirrors donto_extraction_run (migration 0028) but for predicate alignment
-- operations: lexical / embedding / graph / composite / manual / rule. Every
-- alignment edge in donto_predicate_alignment can point back at the run that
-- produced it via run_id (the FK is wired here, since donto_predicate_alignment
-- was created in 0048 before this table existed).

create table if not exists donto_alignment_run (
    run_id              uuid primary key default gen_random_uuid(),
    run_type            text not null check (run_type in (
        'lexical', 'embedding', 'graph', 'composite', 'manual', 'rule'
    )),
    model_id            text,
    model_version       text,
    config              jsonb not null default '{}'::jsonb,
    status              text not null default 'running'
                        check (status in ('running','completed','failed','partial')),
    source_predicates   text[],
    started_at          timestamptz not null default now(),
    completed_at        timestamptz,
    alignments_proposed int not null default 0,
    alignments_accepted int not null default 0,
    alignments_rejected int not null default 0,
    metadata            jsonb not null default '{}'::jsonb
);

create index if not exists donto_alignment_run_status_idx
    on donto_alignment_run (status);
create index if not exists donto_alignment_run_type_idx
    on donto_alignment_run (run_type);
create index if not exists donto_alignment_run_model_idx
    on donto_alignment_run (model_id) where model_id is not null;
create index if not exists donto_alignment_run_started_idx
    on donto_alignment_run (started_at);

-- Now wire the FK from donto_predicate_alignment.run_id.
do $$ begin
    if not exists (
        select 1 from information_schema.table_constraints
        where constraint_name = 'donto_pa_run_fk'
          and table_name = 'donto_predicate_alignment'
    ) then
        alter table donto_predicate_alignment
            add constraint donto_pa_run_fk
            foreign key (run_id) references donto_alignment_run(run_id);
    end if;
end $$;

-- ---------------------------------------------------------------------------
-- Functions.
-- ---------------------------------------------------------------------------

create or replace function donto_start_alignment_run(
    p_run_type      text,
    p_model_id      text default null,
    p_model_version text default null,
    p_config        jsonb default '{}'::jsonb,
    p_source_preds  text[] default null,
    p_metadata      jsonb default '{}'::jsonb
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    insert into donto_alignment_run
        (run_type, model_id, model_version, config, source_predicates, metadata)
    values (p_run_type, p_model_id, p_model_version, p_config, p_source_preds, p_metadata)
    returning run_id into v_id;
    return v_id;
end;
$$;

create or replace function donto_complete_alignment_run(
    p_run_id   uuid,
    p_status   text default 'completed',
    p_proposed int default null,
    p_accepted int default null,
    p_rejected int default null
) returns void
language plpgsql as $$
begin
    update donto_alignment_run
    set status              = p_status,
        completed_at        = now(),
        alignments_proposed = coalesce(p_proposed, alignments_proposed),
        alignments_accepted = coalesce(p_accepted, alignments_accepted),
        alignments_rejected = coalesce(p_rejected, alignments_rejected)
    where run_id = p_run_id;
end;
$$;
