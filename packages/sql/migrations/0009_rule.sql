-- Phase 6: Derivation rules and reports (PRD §17).

create table if not exists donto_rule (
    iri           text primary key,
    label         text,
    description   text,
    body_kind     text not null check (body_kind in ('builtin','lean','dir')),
    body          jsonb,
    output_ctx    text,           -- preferred output context iri (advisory)
    mode          text not null default 'on_demand'
                  check (mode in ('eager','batch','on_demand')),
    registered_at timestamptz not null default now()
);

create table if not exists donto_derivation_report (
    report_id           bigserial primary key,
    rule_iri            text not null,
    inputs_fingerprint  bytea not null,
    scope               jsonb not null,
    into_ctx            text not null,
    emitted_count       bigint not null,
    duration_ms         integer,
    certificate         jsonb,
    evaluated_at        timestamptz not null default now()
);

create index if not exists donto_derivation_report_lookup
    on donto_derivation_report (rule_iri, inputs_fingerprint, evaluated_at desc);

create or replace function donto_register_rule(
    p_iri text, p_kind text, p_body jsonb,
    p_label text default null, p_description text default null,
    p_output_ctx text default null, p_mode text default 'on_demand'
) returns text language sql as $$
    insert into donto_rule (iri, body_kind, body, label, description, output_ctx, mode)
    values (p_iri, p_kind, p_body, p_label, p_description, p_output_ctx, p_mode)
    on conflict (iri) do update set
        body_kind   = excluded.body_kind,
        body        = excluded.body,
        label       = coalesce(excluded.label, donto_rule.label),
        description = coalesce(excluded.description, donto_rule.description),
        output_ctx  = coalesce(excluded.output_ctx, donto_rule.output_ctx),
        mode        = excluded.mode
    returning iri;
$$;

select donto_register_rule('builtin:transitive/<predicate>', 'builtin',
    '{"kind":"transitive_closure"}'::jsonb,
    'TransitiveClosure',
    'Emit p+ for the transitive closure of p over a scope.');
select donto_register_rule('builtin:inverse/<predicate>/<inverse>', 'builtin',
    '{"kind":"inverse_emission"}'::jsonb,
    'InverseEmission',
    'For each (s p o), emit (o inverse s).');
select donto_register_rule('builtin:symmetric/<predicate>', 'builtin',
    '{"kind":"symmetric"}'::jsonb,
    'SymmetricClosure',
    'Emit (o p s) for each (s p o).');
