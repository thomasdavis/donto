-- Phase 5: Shapes catalog and reports (PRD §16).
--
-- Shapes themselves are Lean-authored; this schema records:
--   * registered shape IRIs (so SQL can list and reference them),
--   * cached validation reports keyed by (shape, scope_fingerprint).
-- The Lean engine produces the reports via dontosrv; the cache is
-- consulted before re-execution.

create table if not exists donto_shape (
    iri          text primary key,
    label        text,
    description  text,
    severity     text not null default 'violation'
                 check (severity in ('info','warning','violation')),
    body_kind    text not null check (body_kind in ('builtin','lean','dir')),
    body         jsonb,           -- builtin params; dir blob; or lean-source ref
    registered_at timestamptz not null default now()
);

create table if not exists donto_shape_report (
    report_id          bigserial primary key,
    shape_iri          text not null,
    scope_fingerprint  bytea not null,
    scope              jsonb not null,
    report             jsonb not null,
    focus_count        bigint not null,
    violation_count    bigint not null,
    evaluated_at       timestamptz not null default now()
);

create index if not exists donto_shape_report_lookup
    on donto_shape_report (shape_iri, scope_fingerprint, evaluated_at desc);

-- Per-statement report attachment (sparse overlay; PRD §5).
create table if not exists donto_stmt_shape_reports (
    statement_id  uuid not null references donto_statement(statement_id) on delete cascade,
    report_id     bigint not null references donto_shape_report(report_id) on delete cascade,
    severity      text not null,
    primary key (statement_id, report_id)
);

create or replace function donto_register_shape(
    p_iri text, p_kind text, p_body jsonb,
    p_label text default null, p_description text default null, p_severity text default 'violation'
) returns text language sql as $$
    insert into donto_shape (iri, body_kind, body, label, description, severity)
    values (p_iri, p_kind, p_body, p_label, p_description, p_severity)
    on conflict (iri) do update set
        body_kind   = excluded.body_kind,
        body        = excluded.body,
        label       = coalesce(excluded.label, donto_shape.label),
        description = coalesce(excluded.description, donto_shape.description),
        severity    = excluded.severity
    returning iri;
$$;

-- Helpful built-in shape registrations (just metadata; the dontosrv handler
-- knows how to evaluate them).
select donto_register_shape('builtin:functional/<predicate>', 'builtin',
    '{"kind":"functional"}'::jsonb,
    'FunctionalPredicate',
    'At most one object per subject in scope.');
select donto_register_shape('builtin:datatype/<predicate>/<datatype>', 'builtin',
    '{"kind":"datatype"}'::jsonb,
    'DatatypeShape',
    'Object literals must have the named datatype.');
