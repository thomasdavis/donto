-- Evidence substrate: cross-system entity identity.
--
-- The same entity has different identifiers in different systems.
-- This table maps between them. Distinct from donto_predicate's
-- canonical_of (which is for predicates) and SameMeaning (which is
-- for statements).

create table if not exists donto_entity_alias (
    alias_iri     text not null,
    canonical_iri text not null,
    system        text,
    confidence    double precision default 1.0,
    registered_by text,
    registered_at timestamptz not null default now(),
    metadata      jsonb not null default '{}'::jsonb,
    primary key (alias_iri, canonical_iri),
    constraint donto_entity_alias_distinct
        check (alias_iri <> canonical_iri)
);

create index if not exists donto_entity_alias_canonical_idx
    on donto_entity_alias (canonical_iri);
create index if not exists donto_entity_alias_system_idx
    on donto_entity_alias (system) where system is not null;

-- Register an entity alias. Idempotent.
create or replace function donto_register_entity_alias(
    p_alias     text,
    p_canonical text,
    p_system    text default null,
    p_confidence double precision default 1.0,
    p_actor     text default null
) returns void
language plpgsql as $$
begin
    insert into donto_entity_alias
        (alias_iri, canonical_iri, system, confidence, registered_by)
    values (p_alias, p_canonical, p_system, p_confidence, p_actor)
    on conflict (alias_iri, canonical_iri) do update set
        confidence = greatest(excluded.confidence, donto_entity_alias.confidence),
        system = coalesce(excluded.system, donto_entity_alias.system);
end;
$$;

-- Resolve an entity IRI to its canonical. One-hop only (no chains).
-- Returns self if not aliased (open-world).
create or replace function donto_resolve_entity(p_iri text)
returns text
language sql stable as $$
    select coalesce(
        (select canonical_iri from donto_entity_alias
         where alias_iri = p_iri
         order by confidence desc limit 1),
        p_iri
    )
$$;

-- All known aliases for an entity (both directions).
create or replace function donto_entity_aliases(p_iri text)
returns table(alias_iri text, canonical_iri text, system text, confidence double precision)
language sql stable as $$
    select alias_iri, canonical_iri, system, confidence
    from donto_entity_alias
    where canonical_iri = p_iri or alias_iri = p_iri
$$;
