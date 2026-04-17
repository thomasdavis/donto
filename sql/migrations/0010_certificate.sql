-- Phase 7: Certificates (PRD §18). A certificate is a self-describing,
-- machine-checkable justification for a statement or derivation result.
-- Stored as an annotation overlay; verification is performed by dontosrv.

create table if not exists donto_stmt_certificate (
    statement_id  uuid primary key references donto_statement(statement_id) on delete cascade,
    kind          text not null check (kind in (
        'direct_assertion','substitution','transitive_closure',
        'confidence_justification','shape_entailment',
        'hypothesis_scoped','replay')),
    rule_iri      text,
    inputs        uuid[] not null default '{}'::uuid[],
    body          jsonb not null,
    signature     bytea,
    produced_at   timestamptz not null default now(),
    verified_at   timestamptz,
    verifier      text,
    verified_ok   boolean
);

create index if not exists donto_stmt_certificate_kind on donto_stmt_certificate(kind);
create index if not exists donto_stmt_certificate_verified on donto_stmt_certificate(verified_ok)
    where verified_ok is not null;

create or replace function donto_attach_certificate(
    p_stmt uuid, p_kind text, p_body jsonb,
    p_rule_iri text default null, p_inputs uuid[] default '{}'::uuid[],
    p_signature bytea default null
) returns void language sql as $$
    insert into donto_stmt_certificate (statement_id, kind, rule_iri, inputs, body, signature)
    values (p_stmt, p_kind, p_rule_iri, p_inputs, p_body, p_signature)
    on conflict (statement_id) do update set
        kind = excluded.kind, rule_iri = excluded.rule_iri,
        inputs = excluded.inputs, body = excluded.body,
        signature = excluded.signature, produced_at = now(),
        verified_at = null, verified_ok = null;
$$;

create or replace function donto_record_verification(
    p_stmt uuid, p_verifier text, p_ok boolean
) returns void language sql as $$
    update donto_stmt_certificate
       set verified_at = now(), verifier = p_verifier, verified_ok = p_ok
     where statement_id = p_stmt;
$$;
