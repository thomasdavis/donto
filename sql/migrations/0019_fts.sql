-- Alexandria §3.9: full-text search over literal values.
--
-- Design note: we deliberately do NOT add a generated-stored tsvector
-- column to donto_statement. ALTER TABLE with a stored generated column
-- triggers a full table rewrite, which is hostile on a populated store
-- (and locks out concurrent writers). Instead we use a functional GIN
-- index on the tsvector expression — the index is built from the existing
-- table without rewriting it, and donto_match_text uses the same
-- expression so the planner picks the index.
--
-- Scope for Phase B: English default, per-row language tag drives the
-- stemming config. Synonym expansion, ranking beyond ts_rank_cd, and
-- language detection are out of scope — those are caller concerns
-- (PRD §4 rabbit-hole list).

-- Map a BCP-47 language tag (primary subtag only) to a text-search
-- configuration. Returns 'simple' when unknown.
-- Missing/empty tags default to 'english' — most xsd:string literals in
-- practice lack an explicit lang tag but are English text; defaulting to
-- 'simple' would skip stemming and make searches less useful. Callers who
-- want the naive tokenizer should tag the literal with lang='*' or any
-- tag we don't recognise.
create or replace function donto_lang_to_regconfig(p_lang text)
returns regconfig language sql immutable as $$
    select case lower(coalesce(split_part(p_lang, '-', 1), ''))
        when ''   then 'english'::regconfig
        when 'en' then 'english'::regconfig
        when 'fr' then 'french'::regconfig
        when 'de' then 'german'::regconfig
        when 'es' then 'spanish'::regconfig
        when 'it' then 'italian'::regconfig
        when 'pt' then 'portuguese'::regconfig
        when 'nl' then 'dutch'::regconfig
        when 'ru' then 'russian'::regconfig
        when 'sv' then 'swedish'::regconfig
        when 'da' then 'danish'::regconfig
        when 'no' then 'norwegian'::regconfig
        when 'fi' then 'finnish'::regconfig
        else 'simple'::regconfig
    end
$$;

-- Composite tsvector expression reused by the index and the search fn.
-- Wrapped in a SQL function so the two sites stay in lock-step.
create or replace function donto_stmt_lit_tsv(p_lit jsonb)
returns tsvector
language sql immutable as $$
    select case when p_lit is not null then
        to_tsvector(
            donto_lang_to_regconfig(p_lit ->> 'lang'),
            coalesce(p_lit ->> 'v', '')
        )
    end
$$;

-- Functional GIN index. Deliberately NOT created in this migration because
-- CREATE INDEX takes an exclusive lock on donto_statement, and on a
-- populated store this blocks all writers for the duration of the build.
-- The project convention (CLAUDE.md §performance) also steers away from
-- index-shaped optimisation in Phase 0/1.
--
-- Operators who want it can run, outside any transaction:
--
--   create index concurrently if not exists donto_statement_lit_tsv_gin
--       on donto_statement using gin (donto_stmt_lit_tsv(object_lit))
--       where object_lit is not null and upper(tx_time) is null;
--
-- donto_match_text uses the same expression so the planner will pick it up
-- once it exists; with the index absent the planner falls back to a
-- partial-index-less GIN-style seq scan which is correct but O(n).

-- Search helper. websearch_to_tsquery accepts natural-language queries
-- ("foo OR bar", quoted phrases, `-exclusions`). p_query_lang defaults
-- to 'en'; callers who know the query language can override.
--
-- Returns matches joined back to the atom shape, with a ts_rank_cd score.
-- No ORDER BY: callers who want ranking say `order by score desc`.
create or replace function donto_match_text(
    p_query        text,
    p_query_lang   text default 'en',
    p_scope        jsonb default null,
    p_predicate    text default null,
    p_polarity     text default 'asserted',
    p_min_maturity int default 0
) returns table(
    statement_id uuid,
    subject      text,
    predicate    text,
    object_lit   jsonb,
    context      text,
    polarity     text,
    maturity     int,
    score        real
)
language plpgsql stable as $$
declare
    v_resolved text[];
    v_cfg      regconfig := donto_lang_to_regconfig(p_query_lang);
    v_query    tsquery  := websearch_to_tsquery(v_cfg, p_query);
begin
    if p_scope is null then
        v_resolved := null;
    else
        select array_agg(context_iri) into v_resolved from donto_resolve_scope(p_scope);
    end if;

    return query
    select
        s.statement_id,
        s.subject,
        s.predicate,
        s.object_lit,
        s.context,
        donto_polarity(s.flags),
        donto_maturity(s.flags),
        ts_rank_cd(donto_stmt_lit_tsv(s.object_lit), v_query) as score
    from donto_statement s
    where s.object_lit is not null
      and upper(s.tx_time) is null
      and donto_stmt_lit_tsv(s.object_lit) @@ v_query
      and (p_predicate is null or s.predicate = p_predicate)
      and (v_resolved  is null or s.context = any(v_resolved))
      and (p_polarity  is null or donto_polarity(s.flags) = p_polarity)
      and donto_maturity(s.flags) >= p_min_maturity;
end;
$$;
