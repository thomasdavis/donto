-- Lexical predicate normalizer + extraction-time guidance.
--
-- This migration provides the "obvious" alignment scaffolding: a string
-- normalizer for predicate IRIs (so "ex:birthYear", "ex:BirthYear", and
-- "ex:birth-year" collapse to the same token), trigram-based lexical
-- similarity over normalized labels, a suggest-alignments helper, a batch
-- auto-aligner that records its work in donto_alignment_run, and the two
-- extraction-time candidate lookups (embedding-based and lexical).
--
-- pg_trgm is already enabled by migration 0013.

-- ---------------------------------------------------------------------------
-- Predicate IRI normalization.
-- ---------------------------------------------------------------------------

-- Strip any namespace prefix (everything up to and including the last ':' or
-- '#' or '/'), lowercase the local name, and replace non-alphanumerics with
-- spaces so trigram similarity has word boundaries to chew on. CamelCase
-- and snake_case both flatten to space-separated lowercase words.
create or replace function donto_normalize_predicate(p_iri text)
returns text
language sql immutable as $$
    select case
        when p_iri is null then null
        else trim(both ' ' from
                  regexp_replace(
                      lower(
                          regexp_replace(
                              regexp_replace(p_iri, '^.*[:#/]', ''),
                              '([a-z0-9])([A-Z])', '\1 \2', 'g'
                          )
                      ),
                      '[^a-z0-9]+', ' ', 'g'
                  ))
    end
$$;

-- Trigram similarity between two normalized predicate IRIs. Returns a
-- value in [0, 1].
create or replace function donto_predicate_lexical_similarity(
    p_iri_a text,
    p_iri_b text
) returns double precision
language sql immutable as $$
    select similarity(
        donto_normalize_predicate(p_iri_a),
        donto_normalize_predicate(p_iri_b)
    )::double precision
$$;

-- ---------------------------------------------------------------------------
-- Suggesting and applying alignments.
-- ---------------------------------------------------------------------------

-- For a given source predicate, return registered predicates whose normalized
-- label is similar above a threshold. Skips self, predicates already aligned
-- (in either direction) to the source, and inactive predicates. Useful as a
-- candidate generator for human review or automated pipelines.
create or replace function donto_suggest_alignments(
    p_source        text,
    p_min_similarity double precision default 0.5,
    p_limit         int default 20
) returns table(
    target_iri  text,
    similarity  double precision,
    target_label text
)
language sql stable as $$
    select p.iri as target_iri,
           donto_predicate_lexical_similarity(p_source, p.iri) as similarity,
           coalesce(d.label, p.label) as target_label
    from donto_predicate p
    left join donto_predicate_descriptor d on d.iri = p.iri
    where p.iri <> p_source
      and p.status in ('active', 'implicit')
      and not exists (
          select 1 from donto_predicate_alignment a
          where upper(a.tx_time) is null
            and ((a.source_iri = p_source and a.target_iri = p.iri)
              or (a.source_iri = p.iri and a.target_iri = p_source))
      )
      and donto_predicate_lexical_similarity(p_source, p.iri) >= p_min_similarity
    order by similarity desc, p.iri
    limit p_limit
$$;

-- Run a lexical auto-align pass over the given source predicates (or, if
-- p_sources is null, all active/implicit predicates). For each source, take
-- the top suggestion above p_min_similarity and register it as a close_match
-- (humans can promote it to exact_equivalent later) attributed to a fresh
-- alignment run. Returns the run_id.
create or replace function donto_auto_align_batch(
    p_sources        text[] default null,
    p_min_similarity double precision default 0.7,
    p_actor          text default null
) returns uuid
language plpgsql as $$
declare
    v_run_id   uuid;
    v_proposed int := 0;
    v_accepted int := 0;
    v_src      text;
    v_match    record;
begin
    v_run_id := donto_start_alignment_run(
        'lexical',
        'donto_auto_align_batch',
        '1',
        jsonb_build_object('min_similarity', p_min_similarity),
        p_sources,
        '{}'::jsonb
    );

    for v_src in
        select iri from donto_predicate
        where status in ('active', 'implicit')
          and (p_sources is null or iri = any(p_sources))
        order by iri
    loop
        select target_iri, similarity into v_match
        from donto_suggest_alignments(v_src, p_min_similarity, 1);
        if v_match.target_iri is null then
            continue;
        end if;
        v_proposed := v_proposed + 1;
        begin
            perform donto_register_alignment(
                v_src,
                v_match.target_iri,
                'close_match',
                v_match.similarity,
                null, null,
                v_run_id,
                jsonb_build_object('method', 'lexical_trigram'),
                p_actor
            );
            v_accepted := v_accepted + 1;
        exception when others then
            -- swallow constraint violations (e.g., source = target) so a
            -- single bad pair doesn't fail the whole batch
            null;
        end;
    end loop;

    perform donto_complete_alignment_run(v_run_id, 'completed', v_proposed, v_accepted, 0);
    return v_run_id;
end;
$$;

-- ---------------------------------------------------------------------------
-- Extraction-time predicate candidates.
-- ---------------------------------------------------------------------------

-- Embedding-based candidate generator: given a query embedding (computed by
-- the extraction pipeline from a candidate relation phrase) and optional
-- type/domain filters, return the top-k registered predicates by cosine
-- similarity over their descriptor embeddings.
create or replace function donto_extraction_predicate_candidates(
    p_embedding    float4[],
    p_model_id     text,
    p_domain       text default null,
    p_subject_type text default null,
    p_object_type  text default null,
    p_limit        int default 30
) returns table(
    iri             text,
    label           text,
    gloss           text,
    subject_type    text,
    object_type     text,
    example_subject text,
    example_object  text,
    source_sentence text,
    similarity      double precision
)
language sql stable as $$
    select d.iri, d.label, d.gloss,
           d.subject_type, d.object_type,
           d.example_subject, d.example_object,
           d.source_sentence,
           donto_cosine_similarity(d.embedding, p_embedding) as similarity
    from donto_predicate_descriptor d
    join donto_predicate p on p.iri = d.iri
    where d.embedding is not null
      and d.embedding_model = p_model_id
      and array_length(d.embedding, 1) = array_length(p_embedding, 1)
      and p.status in ('active', 'implicit')
      and (p_domain is null or d.domain = p_domain)
      and (p_subject_type is null or d.subject_type = p_subject_type)
      and (p_object_type is null or d.object_type = p_object_type)
    order by donto_cosine_similarity(d.embedding, p_embedding) desc nulls last
    limit p_limit
$$;

-- Lexical (full-text) candidate generator. Useful when no embedding is
-- available (cold start, debugging) or as a re-ranking signal alongside
-- the embedding-based one. Searches over label || gloss || source_sentence.
create or replace function donto_extraction_predicate_candidates_lexical(
    p_query  text,
    p_domain text default null,
    p_limit  int default 20
) returns table(
    iri   text,
    label text,
    gloss text,
    rank  real
)
language sql stable as $$
    select d.iri, d.label, d.gloss,
           ts_rank_cd(
               to_tsvector('english',
                   coalesce(d.label, '') || ' ' ||
                   coalesce(d.gloss, '') || ' ' ||
                   coalesce(d.source_sentence, '')),
               plainto_tsquery('english', p_query)
           ) as rank
    from donto_predicate_descriptor d
    join donto_predicate p on p.iri = d.iri
    where p.status in ('active', 'implicit')
      and (p_domain is null or d.domain = p_domain)
      and to_tsvector('english',
              coalesce(d.label, '') || ' ' ||
              coalesce(d.gloss, '') || ' ' ||
              coalesce(d.source_sentence, ''))
          @@ plainto_tsquery('english', p_query)
    order by rank desc
    limit p_limit
$$;
