-- Evidence substrate: extraction chunks.
--
-- When an LLM processes a long document, it chunks it. Each chunk
-- produces some claims. Tracking chunks lets you debug which part
-- of the document and which prompt produced a bad extraction.

create table if not exists donto_extraction_chunk (
    chunk_id      uuid primary key default gen_random_uuid(),
    run_id        uuid not null references donto_extraction_run(run_id),
    revision_id   uuid not null references donto_document_revision(revision_id),
    chunk_index   int not null,
    start_offset  int,
    end_offset    int,
    token_count   int,
    prompt_hash   bytea,
    response_hash bytea,
    latency_ms    int,
    status        text not null default 'completed'
                  check (status in ('pending','running','completed','failed')),
    metadata      jsonb not null default '{}'::jsonb,
    created_at    timestamptz not null default now(),
    unique (run_id, chunk_index)
);

create index if not exists donto_extraction_chunk_run_idx
    on donto_extraction_chunk (run_id);
create index if not exists donto_extraction_chunk_rev_idx
    on donto_extraction_chunk (revision_id);

create or replace function donto_add_extraction_chunk(
    p_run_id       uuid,
    p_revision_id  uuid,
    p_chunk_index  int,
    p_start_offset int default null,
    p_end_offset   int default null,
    p_token_count  int default null,
    p_prompt_hash  bytea default null,
    p_latency_ms   int default null
) returns uuid
language plpgsql as $$
declare v_id uuid;
begin
    insert into donto_extraction_chunk
        (run_id, revision_id, chunk_index, start_offset, end_offset,
         token_count, prompt_hash, latency_ms)
    values (p_run_id, p_revision_id, p_chunk_index, p_start_offset,
            p_end_offset, p_token_count, p_prompt_hash, p_latency_ms)
    on conflict (run_id, chunk_index) do update set
        status = 'completed',
        latency_ms = coalesce(excluded.latency_ms, donto_extraction_chunk.latency_ms)
    returning chunk_id into v_id;
    return v_id;
end;
$$;

-- Chunks for a run, ordered
create or replace function donto_extraction_chunks(p_run_id uuid)
returns table(
    chunk_id uuid, chunk_index int, start_offset int, end_offset int,
    token_count int, latency_ms int, status text
)
language sql stable as $$
    select chunk_id, chunk_index, start_offset, end_offset,
           token_count, latency_ms, status
    from donto_extraction_chunk
    where run_id = p_run_id
    order by chunk_index
$$;
