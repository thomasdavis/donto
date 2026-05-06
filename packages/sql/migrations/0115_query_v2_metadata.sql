-- v1000 / FR-015 native query metadata.
--
-- Records the v1000 query-language clause vocabulary so that clients
-- (sidecar / parser / TUI) can introspect what's supported and so
-- adapter code can validate query specs against this registry.
--
-- This migration adds storage only; the parser/evaluator extensions
-- are application-layer (donto-query crate). The registry is the
-- source of truth for which clauses exist.

create table if not exists donto_query_clause_v1000 (
    clause_name     text primary key,
    clause_kind     text not null check (clause_kind in (
        'scope', 'lens', 'filter', 'aggregate',
        'projection', 'temporal', 'policy', 'order', 'meta'
    )),
    description     text not null,
    accepts_args    boolean not null default true,
    introduced_in   text not null default 'v1000',
    deprecated_in   text,
    metadata        jsonb not null default '{}'::jsonb
);

insert into donto_query_clause_v1000
    (clause_name, clause_kind, description) values
    -- v0 (preserved)
    ('SCOPE',                'scope',
     'Constrain by context include/exclude with descendants/ancestors flags.'),
    ('PRESET',               'scope',
     'Apply a named scope preset (latest|raw|curated|under_hypothesis|as_of|anywhere).'),
    ('MATCH',                'projection',
     'Basic graph pattern with optional graph binding.'),
    ('FILTER',               'filter',
     'Boolean expressions over bound variables.'),
    ('POLARITY',             'filter',
     'Filter by stored polarity (asserted|negated|absent|unknown).'),
    ('MATURITY',             'filter',
     'Filter by stored maturity (E0..E5).'),
    ('IDENTITY',              'lens',
     'Identity expansion mode (default|expand_clusters|expand_sameas_transitive|strict).'),
    ('PREDICATES',            'lens',
     'Predicate-closure expansion mode (EXPAND|STRICT|EXPAND_ABOVE n).'),
    ('PROJECT',               'projection',
     'Project a subset of bound variables.'),
    ('LIMIT',                 'projection',
     'Limit result count.'),
    ('OFFSET',                'projection',
     'Offset result count for paging.'),
    -- v1000 additions
    ('MODALITY',              'filter',
     'Filter by modality (descriptive|prescriptive|reconstructed|...).'),
    ('EXTRACTION_LEVEL',      'filter',
     'Filter by extraction level (quoted|table_read|...).'),
    ('IDENTITY_LENS',         'lens',
     'Apply a named identity lens at query time.'),
    ('SCHEMA_LENS',           'lens',
     'Apply a named schema lens at query time.'),
    ('REVIEW_STATE',          'filter',
     'Filter by review state (unreviewed|triaged|approved_*|rejected|superseded).'),
    ('VALIDATION_STATE',      'filter',
     'Filter by validation state.'),
    ('ARGUMENT_STATE',        'filter',
     'Filter by argument state (under_pressure, supported, isolated).'),
    ('RELEASE_ELIGIBLE',      'filter',
     'Filter to claims eligible for the given release scope.'),
    ('POLICY_ALLOWS',         'policy',
     'Filter by caller-permitted action under effective policy.'),
    ('POLICY_REQUIRE',        'policy',
     'Hard-require an action; return 403 rather than empty if denied.'),
    ('VALID_TIME',            'temporal',
     'Filter by valid_time interval.'),
    ('TRANSACTION_TIME',      'temporal',
     'Filter by transaction_time interval (AS_OF semantics).'),
    ('AS_OF',                 'temporal',
     'Bitemporal time-travel: report state as of given timestamp.'),
    ('ORDER_BY_CONTRADICTION_PRESSURE', 'order',
     'Ordering helper for contradiction-frontier queries.'),
    ('WITH_EVIDENCE',         'meta',
     'Post-clause for evidence redaction modes.')
on conflict (clause_name) do update set
    clause_kind = excluded.clause_kind,
    description = excluded.description;

-- Helper: list active clauses by kind.
create or replace function donto_query_clauses(p_kind text default null)
returns table(clause_name text, clause_kind text, description text)
language sql stable as $$
    select clause_name, clause_kind, description
    from donto_query_clause_v1000
    where deprecated_in is null
      and (p_kind is null or clause_kind = p_kind)
    order by clause_kind, clause_name
$$;
