-- Phase 2: Named scope presets (PRD §7).
--
-- Presets are first-class objects: a name → JSON scope descriptor that
-- donto_resolve_scope can interpret. The five canonical presets are seeded
-- here. Applications may register additional ones with donto_define_preset.

create table if not exists donto_scope_preset (
    name        text primary key,
    scope       jsonb not null,
    description text,
    created_at  timestamptz not null default now()
);

create or replace function donto_define_preset(
    p_name text, p_scope jsonb, p_description text default null
) returns text language sql as $$
    insert into donto_scope_preset (name, scope, description)
    values (p_name, p_scope, p_description)
    on conflict (name) do update set scope = excluded.scope, description = excluded.description
    returning name;
$$;

create or replace function donto_preset_scope(p_name text)
returns jsonb language sql stable as $$
    select scope from donto_scope_preset where name = p_name
$$;

-- Resolve a preset name OR an inline scope to context IRIs.
create or replace function donto_resolve_scope_named(
    p_preset text default null,
    p_inline jsonb default null
) returns table(context_iri text)
language plpgsql stable as $$
declare
    v_scope jsonb;
begin
    if p_preset is not null and p_inline is not null then
        raise exception 'donto_resolve_scope_named: pass preset OR inline, not both';
    end if;
    if p_preset is not null then
        select scope into v_scope from donto_scope_preset where name = p_preset;
        if v_scope is null then
            raise exception 'unknown preset %', p_preset;
        end if;
    else
        v_scope := p_inline;
    end if;
    return query select * from donto_resolve_scope(v_scope);
end;
$$;

-- Seed the canonical presets (PRD §7).
-- "anywhere" is donto's escape hatch for forensic queries: empty include.
select donto_define_preset(
    'anywhere',
    '{"include":[], "exclude":[], "include_descendants":true, "include_ancestors":false}'::jsonb,
    'All contexts. Forensic / debugging default.');

-- "raw" excludes curated/snapshot/derivation contexts; includes permissive only.
-- Approximated: include everything, exclude well-known curated kinds. We
-- compute the actual exclusion list at scope time via a helper (Phase 3+).
select donto_define_preset(
    'raw',
    '{"include":[], "exclude":[], "include_descendants":true, "include_ancestors":false, "kind_filter":["source","pipeline"]}'::jsonb,
    'Raw ingestion contexts (source + pipeline). Permissive.');

select donto_define_preset(
    'curated',
    '{"include":[], "exclude":[], "include_descendants":true, "include_ancestors":false, "kind_filter":["snapshot","derivation","trust","custom","system"], "min_maturity":1}'::jsonb,
    'Registry-curated / shape-checked / certified data only.');

select donto_define_preset(
    'latest',
    '{"include":[], "exclude":[], "include_descendants":true, "include_ancestors":false, "exclude_kind":["hypothesis","quarantine"]}'::jsonb,
    'Default read scope. Excludes hypothesis and quarantine.');

-- under_hypothesis(h) and as_of(snapshot) are macros; they require a parameter.
-- Phase 2 ships them as helper functions rather than parameter-less presets.
create or replace function donto_scope_under_hypothesis(p_hypo_iri text)
returns jsonb language sql stable as $$
    select jsonb_build_object(
        'include',             jsonb_build_array(p_hypo_iri),
        'exclude',             '[]'::jsonb,
        'include_descendants', true,
        'include_ancestors',   true
    )
$$;

create or replace function donto_scope_as_of(p_snapshot_iri text)
returns jsonb language sql stable as $$
    select jsonb_build_object(
        'include',             jsonb_build_array(p_snapshot_iri),
        'exclude',             '[]'::jsonb,
        'include_descendants', false,
        'include_ancestors',   true
    )
$$;

-- Extend resolve_scope to honor kind_filter / exclude_kind hints used by
-- the seeded presets. Older callers passing plain {include,exclude,...}
-- continue to work because the new keys default to null.
create or replace function donto_resolve_scope(p_scope jsonb)
returns table(context_iri text)
language plpgsql stable as $$
declare
    v_include              text[]   := coalesce(array(select jsonb_array_elements_text(p_scope -> 'include')), '{}');
    v_exclude              text[]   := coalesce(array(select jsonb_array_elements_text(p_scope -> 'exclude')), '{}');
    v_include_descendants  boolean  := coalesce((p_scope ->> 'include_descendants')::boolean, true);
    v_include_ancestors    boolean  := coalesce((p_scope ->> 'include_ancestors')::boolean,   false);
    v_kind_filter          text[]   := coalesce(array(select jsonb_array_elements_text(p_scope -> 'kind_filter')), '{}');
    v_exclude_kind         text[]   := coalesce(array(select jsonb_array_elements_text(p_scope -> 'exclude_kind')), '{}');
begin
    if array_length(v_include, 1) is null then
        return query
        select c.iri from donto_context c
        where c.iri <> all(v_exclude)
          and (array_length(v_kind_filter, 1) is null  or c.kind = any(v_kind_filter))
          and (array_length(v_exclude_kind, 1) is null or c.kind <> all(v_exclude_kind));
        return;
    end if;

    return query
    with recursive
    seed as (select unnest(v_include) as iri),
    descend as (
        select c.iri, c.parent, c.kind from donto_context c join seed s on c.iri = s.iri
        union all
        select c.iri, c.parent, c.kind from donto_context c join descend d on c.parent = d.iri
            where v_include_descendants
    ),
    ascend as (
        select c.iri, c.parent, c.kind from donto_context c join seed s on c.iri = s.iri
        union all
        select c.iri, c.parent, c.kind from donto_context c join ascend a on c.iri = a.parent
            where v_include_ancestors
    ),
    combined as (
        select iri, kind from descend
        union
        select iri, kind from ascend
    )
    select iri from combined
    where iri <> all(v_exclude)
      and (array_length(v_kind_filter, 1) is null  or kind = any(v_kind_filter))
      and (array_length(v_exclude_kind, 1) is null or kind <> all(v_exclude_kind));
end;
$$;
