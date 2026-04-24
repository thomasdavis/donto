-- Evidence substrate §7: evidence links.
--
-- Links between statements and their supporting evidence. A statement
-- may be linked to documents, spans, annotations, extraction runs, or
-- other statements. This generalizes the lineage overlay (0001) and
-- shape annotations (0015) into a universal evidence graph.
--
-- Invariant: evidence links are additive. They never modify the
-- underlying statement. Retracting a link closes its tx_time.

create table if not exists donto_evidence_link (
    link_id              uuid primary key default gen_random_uuid(),
    statement_id         uuid not null references donto_statement(statement_id),
    link_type            text not null check (link_type in (
        'extracted_from', 'supported_by', 'contradicted_by',
        'derived_from', 'cited_in', 'anchored_at', 'produced_by'
    )),
    target_document_id   uuid references donto_document(document_id),
    target_revision_id   uuid references donto_document_revision(revision_id),
    target_span_id       uuid references donto_span(span_id),
    target_annotation_id uuid references donto_annotation(annotation_id),
    target_run_id        uuid references donto_extraction_run(run_id),
    target_statement_id  uuid references donto_statement(statement_id),
    confidence           double precision,
    context              text references donto_context(iri),
    tx_time              tstzrange not null default tstzrange(now(), null, '[)'),
    metadata             jsonb not null default '{}'::jsonb,
    created_at           timestamptz not null default now(),
    constraint donto_evidence_link_has_target check (
        (target_document_id   is not null)::int +
        (target_revision_id   is not null)::int +
        (target_span_id       is not null)::int +
        (target_annotation_id is not null)::int +
        (target_run_id        is not null)::int +
        (target_statement_id  is not null)::int = 1
    ),
    constraint donto_evidence_link_tx_lower_inc
        check (lower_inc(tx_time))
);

create index if not exists donto_evidence_link_stmt_idx
    on donto_evidence_link (statement_id);
create index if not exists donto_evidence_link_type_idx
    on donto_evidence_link (link_type)
    where upper(tx_time) is null;
create index if not exists donto_evidence_link_doc_idx
    on donto_evidence_link (target_document_id)
    where target_document_id is not null;
create index if not exists donto_evidence_link_span_idx
    on donto_evidence_link (target_span_id)
    where target_span_id is not null;
create index if not exists donto_evidence_link_run_idx
    on donto_evidence_link (target_run_id)
    where target_run_id is not null;
create index if not exists donto_evidence_link_target_stmt_idx
    on donto_evidence_link (target_statement_id)
    where target_statement_id is not null;
create index if not exists donto_evidence_link_tx_time_idx
    on donto_evidence_link using gist (tx_time);

-- Link a statement to a span (most common: extraction anchor).
create or replace function donto_link_evidence_span(
    p_statement_id uuid,
    p_span_id      uuid,
    p_link_type    text default 'extracted_from',
    p_confidence   double precision default null,
    p_context      text default null,
    p_run_id       uuid default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    insert into donto_evidence_link
        (statement_id, link_type, target_span_id, confidence, context)
    values (p_statement_id, p_link_type, p_span_id, p_confidence, p_context)
    returning link_id into v_id;
    return v_id;
end;
$$;

-- Link a statement to an extraction run.
create or replace function donto_link_evidence_run(
    p_statement_id uuid,
    p_run_id       uuid,
    p_link_type    text default 'produced_by',
    p_context      text default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    insert into donto_evidence_link
        (statement_id, link_type, target_run_id, confidence, context)
    values (p_statement_id, p_link_type, p_run_id, null, p_context)
    returning link_id into v_id;
    return v_id;
end;
$$;

-- Link a statement to another statement (derived_from, supported_by, etc).
create or replace function donto_link_evidence_statement(
    p_statement_id        uuid,
    p_target_statement_id uuid,
    p_link_type           text default 'derived_from',
    p_confidence          double precision default null,
    p_context             text default null
) returns uuid
language plpgsql as $$
declare
    v_id uuid;
begin
    insert into donto_evidence_link
        (statement_id, link_type, target_statement_id, confidence, context)
    values (p_statement_id, p_link_type, p_target_statement_id, p_confidence, p_context)
    returning link_id into v_id;
    return v_id;
end;
$$;

-- Retract an evidence link (close tx_time).
create or replace function donto_retract_evidence_link(p_link_id uuid)
returns boolean
language plpgsql as $$
declare
    v_n int;
begin
    update donto_evidence_link
    set tx_time = tstzrange(lower(tx_time), now(), '[)')
    where link_id = p_link_id and upper(tx_time) is null;
    get diagnostics v_n = row_count;
    return v_n > 0;
end;
$$;

-- All current evidence for a statement.
create or replace function donto_evidence_for(p_statement_id uuid)
returns table(
    link_id uuid, link_type text,
    target_document_id uuid, target_revision_id uuid,
    target_span_id uuid, target_annotation_id uuid,
    target_run_id uuid, target_statement_id uuid,
    confidence double precision
)
language sql stable as $$
    select link_id, link_type,
           target_document_id, target_revision_id,
           target_span_id, target_annotation_id,
           target_run_id, target_statement_id,
           confidence
    from donto_evidence_link
    where statement_id = p_statement_id
      and upper(tx_time) is null
$$;
