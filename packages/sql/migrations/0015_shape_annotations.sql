-- Alexandria §3.5: shape reports as first-class attached annotations.
--
-- 0008_shape.sql has two concepts:
--   * donto_shape_report       — batch-level run log (one per evaluation run)
--   * donto_stmt_shape_reports — per-statement attachment to a run
--
-- The alexandria PRD calls for a direct, self-contained per-statement
-- annotation that doesn't require joining through a batch run: every shape
-- annotation carries (stmt_id, shape_iri, verdict, context_id, tx_time,
-- detail_literal). That's the table below. It does not replace the batch
-- log; it complements it. User-submitted "flag this" rows use the same
-- table with the flag-author's context.
--
-- Invariant: a shape annotation never modifies the underlying statement.
-- It is purely additive.

create table if not exists donto_stmt_shape_annotation (
    annotation_id   bigserial primary key,
    statement_id    uuid not null
                    references donto_statement(statement_id) on delete cascade,
    shape_iri       text not null,
    verdict         text not null
                    check (verdict in ('pass','warn','violate')),
    context         text not null references donto_context(iri),
    detail          jsonb,
    -- Bitemporal: annotations inherit transaction-time. Retracting an
    -- annotation closes tx_time; a new annotation opens a fresh row. Valid
    -- time isn't interesting for annotations (they apply to the statement
    -- as it exists), so we omit it.
    tx_time         tstzrange not null default tstzrange(now(), null, '[)'),
    constraint donto_stmt_shape_annotation_tx_lower_inc
        check (lower_inc(tx_time))
);

-- A statement/shape pair has at most one open annotation at a time. New
-- verdicts close the prior row.
create unique index if not exists donto_stmt_shape_annotation_open_uniq
    on donto_stmt_shape_annotation (statement_id, shape_iri)
    where upper(tx_time) is null;

create index if not exists donto_stmt_shape_annotation_shape_idx
    on donto_stmt_shape_annotation (shape_iri);
create index if not exists donto_stmt_shape_annotation_verdict_idx
    on donto_stmt_shape_annotation (verdict)
    where upper(tx_time) is null;
create index if not exists donto_stmt_shape_annotation_context_idx
    on donto_stmt_shape_annotation (context);

-- Attach (or replace) an annotation. The caller passes the context that
-- authored the annotation — usually a Lean sidecar context, or a user
-- context for user-submitted flags. `donto_ensure_context` is called for
-- the caller's convenience.
create or replace function donto_attach_shape_report(
    p_statement_id uuid,
    p_shape_iri    text,
    p_verdict      text,
    p_context      text default 'donto:anonymous',
    p_detail       jsonb default null
) returns bigint
language plpgsql as $$
declare
    v_id bigint;
begin
    if p_verdict not in ('pass','warn','violate') then
        raise exception 'donto_attach_shape_report: verdict must be pass|warn|violate, got %', p_verdict;
    end if;
    perform donto_ensure_context(p_context);

    -- Close any prior open annotation for (stmt, shape). Idempotent — the
    -- exact same verdict+detail skips the close-and-reopen cycle so the
    -- same sidecar run re-applied doesn't churn tx_time.
    if not exists (
        select 1 from donto_stmt_shape_annotation
        where statement_id = p_statement_id
          and shape_iri    = p_shape_iri
          and verdict      = p_verdict
          and context      = p_context
          and coalesce(detail, 'null'::jsonb) is not distinct from coalesce(p_detail, 'null'::jsonb)
          and upper(tx_time) is null
    ) then
        update donto_stmt_shape_annotation
           set tx_time = tstzrange(lower(tx_time), now(), '[)')
         where statement_id = p_statement_id
           and shape_iri    = p_shape_iri
           and upper(tx_time) is null;

        insert into donto_stmt_shape_annotation
            (statement_id, shape_iri, verdict, context, detail)
        values (p_statement_id, p_shape_iri, p_verdict, p_context, p_detail)
        returning annotation_id into v_id;
    else
        select annotation_id into v_id from donto_stmt_shape_annotation
        where statement_id = p_statement_id and shape_iri = p_shape_iri
          and upper(tx_time) is null;
    end if;
    return v_id;
end;
$$;

-- Filter predicate: does this statement currently carry an annotation with
-- the given verdict (and optionally the given shape)? Useful in DontoQL
-- `where exists shape_report(shape=X, verdict=violate)` patterns.
create or replace function donto_has_shape_verdict(
    p_statement_id uuid,
    p_verdict      text,
    p_shape_iri    text default null
) returns boolean
language sql stable as $$
    select exists (
        select 1 from donto_stmt_shape_annotation
        where statement_id = p_statement_id
          and verdict      = p_verdict
          and (p_shape_iri is null or shape_iri = p_shape_iri)
          and upper(tx_time) is null
    );
$$;

-- Open annotations joined back to the atom — convenient view for anyone
-- building "show me all current violations in scope X".
create or replace view donto_stmt_shape_annotation_open as
select
    a.annotation_id,
    a.statement_id,
    a.shape_iri,
    a.verdict,
    a.context as annotation_context,
    a.detail,
    lower(a.tx_time) as annotated_at,
    s.subject,
    s.predicate,
    s.object_iri,
    s.object_lit,
    s.context as statement_context
from donto_stmt_shape_annotation a
join donto_statement s using (statement_id)
where upper(a.tx_time) is null;
