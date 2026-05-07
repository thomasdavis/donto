-- Trust Kernel / §6.7 EntityRecord extension.
--
-- Migration 0057 introduced donto_entity_symbol with iri, hash, kind hint,
-- and lifecycle. PRD §6.7 specifies a richer entity record:
--   entity_kind, labels (multilingual), external_ids, identity_status, policy_id
--
-- We add nullable columns to donto_entity_symbol (treating it as the
-- entity table) plus a typed labels overlay for multilingual labels.

alter table donto_entity_symbol
    add column if not exists entity_kind text
        check (entity_kind is null or entity_kind in (
            'language_variety', 'person', 'lexeme', 'morpheme',
            'place', 'concept', 'artifact', 'case', 'condition',
            'gene', 'event', 'organization', 'phoneme', 'phone',
            'allophone', 'grapheme', 'paradigm_cell', 'construction',
            'discourse_unit', 'speaker', 'annotation_tier',
            'predicate', 'value_code', 'other'
        ));

alter table donto_entity_symbol
    add column if not exists external_ids jsonb not null default '[]'::jsonb;

alter table donto_entity_symbol
    add column if not exists identity_status text not null default 'provisional'
        check (identity_status in (
            'provisional', 'stable', 'deprecated',
            'split', 'merged', 'contested'
        ));

alter table donto_entity_symbol
    add column if not exists policy_id text;       -- FK in 0111

create index if not exists donto_entity_symbol_kind_idx
    on donto_entity_symbol (entity_kind) where entity_kind is not null;
create index if not exists donto_entity_symbol_identity_status_idx
    on donto_entity_symbol (identity_status) where identity_status <> 'stable';
create index if not exists donto_entity_symbol_external_ids_gin
    on donto_entity_symbol using gin (external_ids);
create index if not exists donto_entity_symbol_policy_idx
    on donto_entity_symbol (policy_id) where policy_id is not null;

-- Multilingual labels overlay.
-- Each row is one (entity, label, language, script) tuple. The existing
-- normalized_label column on donto_entity_symbol stays as the canonical
-- search label; this table holds all observed forms.
create table if not exists donto_entity_label (
    label_id        bigserial primary key,
    symbol_id       bigint not null references donto_entity_symbol(symbol_id) on delete cascade,
    label           text not null,
    language        text,                              -- BCP 47
    script          text,                              -- ISO 15924
    label_status    text not null default 'observed'
                    check (label_status in (
                        'observed', 'preferred', 'alternate',
                        'historical', 'exonym', 'endonym', 'deprecated'
                    )),
    source_id       uuid references donto_document(document_id),
    confidence      double precision,
    created_at      timestamptz not null default now(),
    constraint donto_entity_label_uniq unique (symbol_id, label, language, script)
);

create index if not exists donto_entity_label_symbol_idx
    on donto_entity_label (symbol_id);
create index if not exists donto_entity_label_language_idx
    on donto_entity_label (language) where language is not null;
create index if not exists donto_entity_label_status_idx
    on donto_entity_label (label_status) where label_status <> 'observed';
create index if not exists donto_entity_label_trgm
    on donto_entity_label using gin (label gin_trgm_ops);

-- Add a label.
create or replace function donto_add_entity_label(
    p_symbol_id    bigint,
    p_label        text,
    p_language     text default null,
    p_script       text default null,
    p_label_status text default 'observed',
    p_source_id    uuid default null,
    p_confidence   double precision default null
) returns bigint
language plpgsql as $$
declare
    v_id bigint;
begin
    insert into donto_entity_label
        (symbol_id, label, language, script, label_status, source_id, confidence)
    values
        (p_symbol_id, p_label, p_language, p_script, p_label_status, p_source_id, p_confidence)
    on conflict (symbol_id, label, language, script) do update set
        label_status = excluded.label_status,
        confidence   = coalesce(excluded.confidence, donto_entity_label.confidence)
    returning label_id into v_id;
    return v_id;
end;
$$;

-- Add a typed external identifier.
create or replace function donto_add_external_id(
    p_symbol_id   bigint,
    p_registry    text,                                -- 'glottolog', 'iso639-3', 'wals', ...
    p_external_id text,
    p_confidence  double precision default 1.0
) returns void
language plpgsql as $$
begin
    update donto_entity_symbol
    set external_ids = external_ids || jsonb_build_array(
        jsonb_build_object(
            'registry', p_registry,
            'id', p_external_id,
            'confidence', p_confidence,
            'added_at', to_char(now(), 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
        )
    )
    where symbol_id = p_symbol_id;
end;
$$;

-- Lookup helper: find symbol by external id.
create or replace function donto_symbol_by_external_id(
    p_registry    text,
    p_external_id text
) returns bigint
language sql stable as $$
    select symbol_id from donto_entity_symbol
    where external_ids @> jsonb_build_array(
        jsonb_build_object('registry', p_registry, 'id', p_external_id)
    )
    limit 1
$$;
