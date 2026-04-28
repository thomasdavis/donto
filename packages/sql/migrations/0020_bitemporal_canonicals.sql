-- Alexandria §3.1: bitemporal canonicals.
--
-- The existing donto_predicate.canonical_of is a timeless alias edge. That
-- models stable mappings ("hasName" → "schema:name") but can't express the
-- drift the PRD is about: "lit" meant *bright* in 1950 and *excellent* in
-- 2020. Different canonical for the same alias at different times.
--
-- This migration adds a side-table donto_predicate_alias that holds
-- alias → canonical edges with a valid_time interval. Resolution at query
-- time takes an "as-of" date and picks the canonical whose interval
-- contains it. When no interval matches we fall back to the original
-- donto_predicate.canonical_of (timeless), and then to the alias itself
-- (open-world).

create table if not exists donto_predicate_alias (
    alias_iri     text not null references donto_predicate(iri),
    canonical_iri text not null references donto_predicate(iri),
    valid_time    daterange not null default daterange(null, null, '[)'),
    registered_by text,
    registered_at timestamptz not null default now(),
    -- Same (alias, canonical, interval) can only be registered once.
    primary key (alias_iri, canonical_iri, valid_time),
    -- Alias != canonical.
    constraint donto_predicate_alias_distinct
        check (alias_iri <> canonical_iri),
    -- Canonical must itself be canonical-terminal (no alias chains).
    -- Enforced by trigger below rather than CHECK (can't subquery in CHECK).
    constraint donto_predicate_alias_bound_ok
        check (not isempty(valid_time))
);

create index if not exists donto_predicate_alias_alias_idx
    on donto_predicate_alias (alias_iri);
create index if not exists donto_predicate_alias_canonical_idx
    on donto_predicate_alias (canonical_iri);
create index if not exists donto_predicate_alias_validtime_gist
    on donto_predicate_alias using gist (valid_time);

-- Single-hop: the canonical_iri cannot itself be a (timeless) alias.
create or replace function donto_predicate_alias_no_chain()
returns trigger language plpgsql as $$
begin
    if exists (
        select 1 from donto_predicate
        where iri = new.canonical_iri and canonical_of is not null
    ) then
        raise exception 'donto_predicate_alias: canonical % is itself an alias', new.canonical_iri;
    end if;
    return new;
end;
$$;

drop trigger if exists donto_predicate_alias_no_chain_trg on donto_predicate_alias;
create trigger donto_predicate_alias_no_chain_trg
    before insert or update on donto_predicate_alias
    for each row execute function donto_predicate_alias_no_chain();

-- Register a bitemporal alias. If the predicate rows don't exist yet,
-- create them as 'implicit' (open-world).
create or replace function donto_register_alias_at(
    p_alias     text,
    p_canonical text,
    p_valid_lo  date default null,
    p_valid_hi  date default null,
    p_actor     text default null
) returns void
language plpgsql as $$
begin
    perform donto_implicit_register(p_alias);
    perform donto_implicit_register(p_canonical);
    insert into donto_predicate_alias (alias_iri, canonical_iri, valid_time, registered_by)
    values (p_alias, p_canonical, daterange(p_valid_lo, p_valid_hi, '[)'), p_actor)
    on conflict do nothing;
end;
$$;

-- Resolve an alias at a given valid_time. Order of preference:
--   1. Bitemporal alias whose interval contains the as-of date.
--   2. The timeless canonical_of (if any).
--   3. The alias itself (pass-through, open-world).
create or replace function donto_canonical_predicate_at(
    p_iri     text,
    p_as_of   date default null
) returns text
language plpgsql stable as $$
declare
    v_canonical text;
begin
    -- 1. Bitemporal lookup.
    if p_as_of is not null then
        select canonical_iri into v_canonical
        from donto_predicate_alias
        where alias_iri = p_iri
          and valid_time @> p_as_of
        -- Prefer the narrowest containing interval (most specific);
        -- ties broken deterministically by canonical_iri.
        order by
            (upper(valid_time) is not null) desc,
            (lower(valid_time) is not null) desc,
            canonical_iri
        limit 1;
        if v_canonical is not null then
            return v_canonical;
        end if;
    end if;

    -- 2. Timeless canonical_of.
    select canonical_of into v_canonical
    from donto_predicate where iri = p_iri and canonical_of is not null;
    if v_canonical is not null then
        return v_canonical;
    end if;

    -- 3. Pass-through.
    return p_iri;
end;
$$;
