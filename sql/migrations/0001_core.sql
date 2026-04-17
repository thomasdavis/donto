-- donto Phase 0 core schema.
-- See PRD §5 (atom), §7 (contexts), §8 (bitemporality).
-- Phase 0 simplifications:
--   * IRIs are stored as text directly. The 128-bit hashed `iri` type is Phase 1.
--   * No custom types. Plain timestamptz/daterange/text.
--   * Annotation overlays are sparse but present for lineage only.

create extension if not exists btree_gist;
create extension if not exists pgcrypto; -- digest(), gen_random_uuid()

-- Phase 0 lives in `public`. The `donto_` prefix on every identifier provides
-- the namespace. Schema isolation returns in Phase 1 with extension packaging.

-- ---------------------------------------------------------------------------
-- Contexts (PRD §7).
-- ---------------------------------------------------------------------------
create table if not exists donto_context (
    iri          text primary key,
    kind         text not null check (kind in (
        'source','snapshot','hypothesis','user','pipeline',
        'trust','derivation','quarantine','custom','system')),
    parent       text references donto_context(iri),
    label        text,
    metadata     jsonb not null default '{}'::jsonb,
    mode         text not null default 'permissive'
                 check (mode in ('permissive','curated')),
    created_at   timestamptz not null default now(),
    closed_at    timestamptz,
    constraint donto_context_no_self_parent check (parent is distinct from iri)
);

create index if not exists donto_context_parent_idx on donto_context(parent);
create index if not exists donto_context_kind_idx   on donto_context(kind);

-- The default context per PRD §3 principle 2 and §30.
insert into donto_context (iri, kind, mode, label)
values ('donto:anonymous', 'system', 'permissive', 'Default anonymous context')
on conflict (iri) do nothing;

-- ---------------------------------------------------------------------------
-- Statements (PRD §5: physical row).
-- ---------------------------------------------------------------------------
create table if not exists donto_statement (
    statement_id  uuid primary key default gen_random_uuid(),
    subject       text not null,
    predicate     text not null,
    object_iri    text,
    object_lit    jsonb,    -- {"v": <value>, "dt": <datatype-iri>, "lang": <tag-or-null>}
    context       text not null references donto_context(iri),
    tx_time       tstzrange not null default tstzrange(now(), null, '[)'),
    valid_time    daterange not null default daterange(null, null, '[)'),
    flags         smallint not null default 0,
    -- Content fingerprint for idempotent re-ingestion (PRD §19).
    -- Stored as days-since-epoch to keep the expression IMMUTABLE
    -- (date_out is STABLE because it depends on session DateStyle).
    content_hash  bytea generated always as (
        digest(
            coalesce(subject,'')   || chr(31) ||
            coalesce(predicate,'') || chr(31) ||
            coalesce(object_iri,'') || chr(31) ||
            coalesce(object_lit::text,'') || chr(31) ||
            coalesce(context,'') || chr(31) ||
            (flags & 3)::text /* polarity bits only */ || chr(31) ||
            coalesce((lower(valid_time) - '2000-01-01'::date)::text, '-inf') || chr(31) ||
            coalesce((upper(valid_time) - '2000-01-01'::date)::text, '+inf'),
            'sha256')
    ) stored,
    constraint donto_statement_object_one_of
        check ((object_iri is not null) <> (object_lit is not null)),
    constraint donto_statement_tx_lower_inc
        check (lower_inc(tx_time))
);

-- Idempotency: the same content in the same valid-time slot, in an OPEN tx_time,
-- collapses to one row. Closed (retracted) rows are kept; new assertions of the
-- same content open a fresh row.
create unique index if not exists donto_statement_open_content_uniq
    on donto_statement (content_hash)
    where upper(tx_time) is null;

-- Required access patterns (PRD §14).
create index if not exists donto_statement_spo_idx
    on donto_statement (subject, predicate, object_iri);
create index if not exists donto_statement_pos_idx
    on donto_statement (predicate, object_iri, subject);
create index if not exists donto_statement_osp_idx
    on donto_statement (object_iri, subject, predicate)
    where object_iri is not null;
create index if not exists donto_statement_context_idx
    on donto_statement (context);
create index if not exists donto_statement_valid_time_idx
    on donto_statement using gist (valid_time);
create index if not exists donto_statement_tx_time_idx
    on donto_statement using gist (tx_time);
create index if not exists donto_statement_object_lit_gin
    on donto_statement using gin (object_lit jsonb_path_ops)
    where object_lit is not null;

-- ---------------------------------------------------------------------------
-- Lineage overlay (PRD §5). The only annotation overlay materialized in Phase 0.
-- ---------------------------------------------------------------------------
create table if not exists donto_stmt_lineage (
    statement_id  uuid not null references donto_statement(statement_id) on delete cascade,
    source_stmt   uuid not null references donto_statement(statement_id),
    primary key (statement_id, source_stmt)
);
create index if not exists donto_stmt_lineage_source_idx
    on donto_stmt_lineage (source_stmt);

-- ---------------------------------------------------------------------------
-- Audit log (PRD §22).
-- ---------------------------------------------------------------------------
create table if not exists donto_audit (
    audit_id     bigserial primary key,
    at           timestamptz not null default now(),
    actor        text,
    action       text not null,
    statement_id uuid,
    detail       jsonb not null default '{}'::jsonb
);
create index if not exists donto_audit_at_idx on donto_audit (at);
create index if not exists donto_audit_stmt_idx on donto_audit (statement_id);
