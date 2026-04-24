-- Evidence substrate §11: vector/embedding layer.
--
-- Sibling retrieval system for semantic similarity over statements,
-- documents, spans, and annotations. Uses float4[] for portability;
-- pgvector can be swapped in when available for ANN indexing.
--
-- Vectors are not the semantic core — the statement ledger is. Vectors
-- are a retrieval acceleration layer that lives alongside structural
-- retrieval (pattern match) and text retrieval (FTS).

create table if not exists donto_vector (
    vector_id      uuid primary key default gen_random_uuid(),
    subject_type   text not null check (subject_type in (
        'statement', 'document', 'revision', 'span', 'annotation'
    )),
    subject_id     uuid not null,
    model_id       text not null,
    model_version  text,
    dimensions     int not null,
    embedding      float4[] not null,
    created_at     timestamptz not null default now(),
    unique (subject_type, subject_id, model_id)
);

create index if not exists donto_vector_subject_idx
    on donto_vector (subject_type, subject_id);
create index if not exists donto_vector_model_idx
    on donto_vector (model_id);

-- Store an embedding. Upserts: if (subject, model) already exists,
-- replace the embedding.
create or replace function donto_store_vector(
    p_subject_type  text,
    p_subject_id    uuid,
    p_model_id      text,
    p_model_version text,
    p_embedding     float4[]
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    insert into donto_vector
        (subject_type, subject_id, model_id, model_version,
         dimensions, embedding)
    values (p_subject_type, p_subject_id, p_model_id, p_model_version,
            array_length(p_embedding, 1), p_embedding)
    on conflict (subject_type, subject_id, model_id) do update set
        model_version = excluded.model_version,
        dimensions    = excluded.dimensions,
        embedding     = excluded.embedding,
        created_at    = now()
    returning vector_id into v_id;
    return v_id;
end;
$$;

-- Cosine similarity between two embeddings.
create or replace function donto_cosine_similarity(a float4[], b float4[])
returns double precision
language sql immutable as $$
    select case
        when array_length(a, 1) <> array_length(b, 1) then null
        when array_length(a, 1) is null then null
        else (
            select sum(a[i] * b[i])
                 / nullif(
                     sqrt(sum(a[i] * a[i])) * sqrt(sum(b[i] * b[i])),
                     0
                   )
            from generate_subscripts(a, 1) i
        )
    end
$$;

-- Find nearest neighbors by cosine similarity. Brute-force scan;
-- pgvector would replace this with ANN. Good enough for Phase 0
-- scale; perf is not a goal yet.
create or replace function donto_nearest_vectors(
    p_subject_type text,
    p_model_id     text,
    p_query        float4[],
    p_limit        int default 10
) returns table(
    vector_id uuid, subject_id uuid,
    similarity double precision
)
language sql stable as $$
    select vector_id, subject_id,
           donto_cosine_similarity(embedding, p_query) as similarity
    from donto_vector
    where subject_type = p_subject_type
      and model_id = p_model_id
      and array_length(embedding, 1) = array_length(p_query, 1)
    order by donto_cosine_similarity(embedding, p_query) desc nulls last
    limit p_limit
$$;
