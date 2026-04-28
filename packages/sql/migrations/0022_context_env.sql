-- Alexandria §3.7: environment / bias overlay on contexts.
--
-- A bag of (key, literal) pairs attached to a context. Keys are open —
-- we don't enforce a fixed schema; `location`, `climate_band`,
-- `speaker_demographic`, `dialect`, `observation_device` are all legal.
-- Overlay values are literals, encoded the same way donto_statement stores
-- object_lit ({v, dt, lang}).
--
-- Invariant (per PRD §4 non-goals and §3.7): advisory only. A query that
-- ignores the overlay gets every statement in scope; a query that filters
-- on overlay keys narrows the result. The overlay is NOT a pre-filter
-- applied at ingest. Anyone who wants bias-aware retrieval opts in.

create table if not exists donto_context_env (
    context     text not null references donto_context(iri) on delete cascade,
    key         text not null,
    value       jsonb not null,
    set_by      text,
    set_at      timestamptz not null default now(),
    primary key (context, key)
);

create index if not exists donto_context_env_key_idx on donto_context_env (key);

-- Set (or overwrite) an env key on a context. Returns the value stored.
create or replace function donto_context_env_set(
    p_context text,
    p_key     text,
    p_value   jsonb,
    p_actor   text default null
) returns jsonb
language plpgsql as $$
begin
    perform donto_ensure_context(p_context);
    insert into donto_context_env (context, key, value, set_by)
    values (p_context, p_key, p_value, p_actor)
    on conflict (context, key) do update
        set value  = excluded.value,
            set_by = excluded.set_by,
            set_at = now();
    return p_value;
end;
$$;

create or replace function donto_context_env_get(
    p_context text,
    p_key     text
) returns jsonb
language sql stable as $$
    select value from donto_context_env where context = p_context and key = p_key
$$;

create or replace function donto_context_env_delete(
    p_context text,
    p_key     text
) returns boolean
language plpgsql as $$
declare v_n int;
begin
    delete from donto_context_env where context = p_context and key = p_key;
    get diagnostics v_n = row_count;
    return v_n > 0;
end;
$$;

-- Given a jsonb object of required key→value pairs, return the set of
-- contexts that match every pair. Values are compared as jsonb (exact).
-- Passing an empty object returns every context (advisory: match-all).
create or replace function donto_contexts_with_env(
    p_required jsonb
) returns table(context_iri text)
language plpgsql stable as $$
declare
    v_count int := 0;
    v_key   text;
    v_val   jsonb;
begin
    if p_required is null or p_required = '{}'::jsonb then
        return query select iri from donto_context;
        return;
    end if;

    -- Contexts that match ALL required pairs.
    return query
    select c.iri
    from donto_context c
    where not exists (
        select 1 from jsonb_each(p_required) r
        where not exists (
            select 1 from donto_context_env e
            where e.context = c.iri
              and e.key = r.key
              and e.value = r.value
        )
    );
    -- Silence "unused variable" warnings.
    v_count := v_count;
    v_key := v_key;
    v_val := v_val;
end;
$$;
