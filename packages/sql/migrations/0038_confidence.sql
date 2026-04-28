-- Evidence substrate: statement-level confidence overlay.
--
-- Sparse overlay on donto_statement. Not a column add (which would
-- rewrite 35M+ rows). Follows the pattern of donto_retrofit,
-- donto_stmt_certificate, and donto_stmt_shape_annotation.

create table if not exists donto_stmt_confidence (
    statement_id      uuid primary key
                      references donto_statement(statement_id) on delete cascade,
    confidence        double precision not null
                      check (confidence >= 0 and confidence <= 1),
    confidence_source text not null default 'extraction'
                      check (confidence_source in (
                          'extraction', 'human', 'model', 'aggregated',
                          'rule', 'calibrated', 'custom'
                      )),
    run_id            uuid references donto_extraction_run(run_id),
    set_at            timestamptz not null default now(),
    metadata          jsonb not null default '{}'::jsonb
);

create index if not exists donto_stmt_confidence_low_idx
    on donto_stmt_confidence (confidence)
    where confidence < 0.5;
create index if not exists donto_stmt_confidence_source_idx
    on donto_stmt_confidence (confidence_source);
create index if not exists donto_stmt_confidence_run_idx
    on donto_stmt_confidence (run_id)
    where run_id is not null;

-- Set or update confidence for a statement
create or replace function donto_set_confidence(
    p_statement_id      uuid,
    p_confidence        double precision,
    p_confidence_source text default 'extraction',
    p_run_id            uuid default null
) returns void
language plpgsql as $$
begin
    insert into donto_stmt_confidence
        (statement_id, confidence, confidence_source, run_id)
    values (p_statement_id, p_confidence, p_confidence_source, p_run_id)
    on conflict (statement_id) do update set
        confidence        = excluded.confidence,
        confidence_source = excluded.confidence_source,
        run_id            = excluded.run_id,
        set_at            = now();
end;
$$;

-- Get confidence for a statement (null if not set)
create or replace function donto_get_confidence(p_statement_id uuid)
returns double precision
language sql stable as $$
    select confidence from donto_stmt_confidence
    where statement_id = p_statement_id
$$;

-- Low-confidence statements in a context
create or replace function donto_low_confidence_statements(
    p_context   text default null,
    p_threshold double precision default 0.5,
    p_limit     int default 100
) returns table(
    statement_id uuid, subject text, predicate text,
    confidence double precision, confidence_source text
)
language sql stable as $$
    select s.statement_id, s.subject, s.predicate,
           c.confidence, c.confidence_source
    from donto_stmt_confidence c
    join donto_statement s using (statement_id)
    where c.confidence < p_threshold
      and upper(s.tx_time) is null
      and (p_context is null or s.context = p_context)
    order by c.confidence
    limit p_limit
$$;
