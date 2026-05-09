-- Telemetry analysis: pre-aggregated paraconsistency density table (PRD analytics C2).
--
-- The O(N²) conflict view donto_v_statement_polarity_v1000 (from 0098) must NOT
-- be queried at runtime. Instead, analyzers pre-aggregate into this table and
-- the top-K views read only from here.

create table if not exists donto_paraconsistency_density (
    subject              text not null,
    predicate            text not null,
    window_start         timestamptz not null,
    window_end           timestamptz not null,
    distinct_polarities  int not null,
    distinct_contexts    int not null,
    conflict_score       double precision not null,  -- Shannon entropy normalised to [0, 1]
    sample_statements    uuid[] not null default '{}',
    computed_at          timestamptz not null default now(),
    primary key (subject, predicate, window_start)
);

-- Index for top-K contested queries; only rows with actual conflict are indexed.
create index if not exists donto_paraconsistency_density_score_idx
    on donto_paraconsistency_density (conflict_score desc, window_start desc)
    where distinct_polarities >= 2;

-- Read-only top-K views — safe because they query the pre-aggregated table only.

create or replace view donto_v_top_contested_predicates as
    select predicate,
           sum(conflict_score) as total_score,
           count(*) as windows
    from donto_paraconsistency_density
    where distinct_polarities >= 2
    group by predicate;

create or replace view donto_v_top_contested_subjects as
    select subject,
           max(conflict_score) as peak_score,
           count(*) as windows
    from donto_paraconsistency_density
    where distinct_polarities >= 2
    group by subject;
