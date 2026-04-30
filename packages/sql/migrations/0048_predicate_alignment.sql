-- Predicate alignment layer: extended alignment relations.
--
-- Replaces the simple donto_predicate.canonical_of / inverse_of columns with
-- a richer first-class alignment relation. Bitemporal (valid_time daterange +
-- tx_time tstzrange) and append-only: retraction closes tx_time rather than
-- deleting. Six relation types cover exact equivalence, inverse equivalence,
-- subsumption, fuzzy match, decomposition into event frames, and explicit
-- negative ("looks alike but differs in meaning").
--
-- Open-world: source_iri / target_iri are not FKs to donto_predicate. A
-- BEFORE INSERT trigger calls donto_implicit_register so the predicates
-- become known to the registry the first time they are aligned.
--
-- The run_id FK back to donto_alignment_run is wired in migration 0050
-- (the run table doesn't exist yet at this point).

create table if not exists donto_predicate_alignment (
    alignment_id     uuid primary key default gen_random_uuid(),
    source_iri       text not null,
    target_iri       text not null,
    relation         text not null check (relation in (
        'exact_equivalent',    -- owl:equivalentProperty
        'inverse_equivalent',  -- owl:inverseOf (directional, swap s/o)
        'sub_property_of',     -- rdfs:subPropertyOf (source is narrower/child of target)
        'close_match',         -- skos:closeMatch (fuzzy, human-review tier)
        'decomposition',       -- source decomposes into event frame using target roles
        'not_equivalent'       -- explicit negative: do not align
    )),
    confidence       double precision not null default 1.0
                     check (confidence >= 0 and confidence <= 1),
    valid_time       daterange not null default daterange(null, null, '[)'),
    tx_time          tstzrange not null default tstzrange(now(), null, '[)'),
    run_id           uuid,  -- FK added in migration 0050
    provenance       jsonb not null default '{}'::jsonb,
    registered_by    text,
    registered_at    timestamptz not null default now(),
    constraint donto_pa_distinct check (source_iri <> target_iri),
    constraint donto_pa_tx_lower_inc check (lower_inc(tx_time))
);

create index if not exists donto_pa_source_idx
    on donto_predicate_alignment (source_iri) where upper(tx_time) is null;
create index if not exists donto_pa_target_idx
    on donto_predicate_alignment (target_iri) where upper(tx_time) is null;
create index if not exists donto_pa_relation_idx
    on donto_predicate_alignment (relation) where upper(tx_time) is null;
create index if not exists donto_pa_valid_gist
    on donto_predicate_alignment using gist (valid_time);
create index if not exists donto_pa_tx_gist
    on donto_predicate_alignment using gist (tx_time);
create index if not exists donto_pa_run_idx
    on donto_predicate_alignment (run_id) where run_id is not null;

-- ---------------------------------------------------------------------------
-- Trigger: implicitly register source/target predicates on insert.
-- ---------------------------------------------------------------------------

create or replace function donto_pa_implicit_register()
returns trigger language plpgsql as $$
begin
    perform donto_implicit_register(new.source_iri);
    perform donto_implicit_register(new.target_iri);
    return new;
end;
$$;

drop trigger if exists donto_pa_implicit_register_trg on donto_predicate_alignment;
create trigger donto_pa_implicit_register_trg
    before insert on donto_predicate_alignment
    for each row execute function donto_pa_implicit_register();

-- ---------------------------------------------------------------------------
-- Functions.
-- ---------------------------------------------------------------------------

create or replace function donto_register_alignment(
    p_source      text,
    p_target      text,
    p_relation    text,
    p_confidence  double precision default 1.0,
    p_valid_lo    date default null,
    p_valid_hi    date default null,
    p_run_id      uuid default null,
    p_provenance  jsonb default '{}'::jsonb,
    p_actor       text default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    if p_source = p_target then
        raise exception 'donto_register_alignment: source cannot equal target';
    end if;
    insert into donto_predicate_alignment
        (source_iri, target_iri, relation, confidence,
         valid_time, run_id, provenance, registered_by)
    values (p_source, p_target, p_relation, p_confidence,
            daterange(p_valid_lo, p_valid_hi, '[)'),
            p_run_id, p_provenance, p_actor)
    returning alignment_id into v_id;
    return v_id;
end;
$$;

create or replace function donto_retract_alignment(p_alignment_id uuid)
returns boolean
language plpgsql as $$
declare
    v_n int;
begin
    update donto_predicate_alignment
    set tx_time = tstzrange(lower(tx_time), now(), '[)')
    where alignment_id = p_alignment_id and upper(tx_time) is null;
    get diagnostics v_n = row_count;
    return v_n > 0;
end;
$$;

-- ---------------------------------------------------------------------------
-- Backfill from legacy alignment storage.
-- The old columns/table remain as a compatibility layer; the new alignment
-- table is the source of truth going forward.
-- ---------------------------------------------------------------------------

do $$
begin
    insert into donto_predicate_alignment
        (source_iri, target_iri, relation, confidence, provenance)
    select iri, canonical_of, 'exact_equivalent', 1.0,
           '{"migrated_from":"canonical_of"}'::jsonb
    from donto_predicate
    where canonical_of is not null
      and canonical_of <> iri
      and not exists (
          select 1 from donto_predicate_alignment a
          where a.source_iri = donto_predicate.iri
            and a.target_iri = donto_predicate.canonical_of
            and a.relation = 'exact_equivalent'
      );

    insert into donto_predicate_alignment
        (source_iri, target_iri, relation, confidence, provenance)
    select iri, inverse_of, 'inverse_equivalent', 1.0,
           '{"migrated_from":"inverse_of"}'::jsonb
    from donto_predicate
    where inverse_of is not null
      and inverse_of <> iri
      and not exists (
          select 1 from donto_predicate_alignment a
          where a.source_iri = donto_predicate.iri
            and a.target_iri = donto_predicate.inverse_of
            and a.relation = 'inverse_equivalent'
      );
end $$;

do $$
begin
    if exists (select 1 from information_schema.tables where table_name = 'donto_predicate_alias') then
        insert into donto_predicate_alignment
            (source_iri, target_iri, relation, confidence, valid_time, provenance)
        select alias_iri, canonical_iri, 'exact_equivalent', 1.0, valid_time,
               jsonb_build_object('migrated_from', 'donto_predicate_alias')
        from donto_predicate_alias pa
        where alias_iri <> canonical_iri
          and not exists (
              select 1 from donto_predicate_alignment a
              where a.source_iri = pa.alias_iri
                and a.target_iri = pa.canonical_iri
                and a.relation = 'exact_equivalent'
                and a.valid_time = pa.valid_time
          );
    end if;
end $$;
