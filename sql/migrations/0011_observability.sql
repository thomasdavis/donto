-- Phase 10: Observability views (PRD §23).

create or replace view donto_stats_context as
select
    c.iri,
    c.kind,
    c.mode,
    c.parent,
    coalesce((select count(*) from donto_statement s where s.context = c.iri), 0) as statement_count,
    coalesce((select count(*) from donto_statement s
              where s.context = c.iri and upper(s.tx_time) is null), 0) as open_count,
    (select max(lower(s.tx_time)) from donto_statement s where s.context = c.iri) as last_assert_at,
    c.created_at,
    c.closed_at
from donto_context c;

create or replace view donto_stats_maturity as
select
    s.context,
    donto_maturity(s.flags) as maturity,
    donto_polarity(s.flags) as polarity,
    count(*) as cnt
from donto_statement s
where upper(s.tx_time) is null
group by s.context, donto_maturity(s.flags), donto_polarity(s.flags);

create or replace view donto_stats_predicate as
select
    p.iri,
    p.canonical_of,
    p.status,
    coalesce((select count(*) from donto_statement s where s.predicate = p.iri), 0) as use_count,
    coalesce((select count(distinct s.context) from donto_statement s where s.predicate = p.iri), 0) as context_count
from donto_predicate p;

create or replace view donto_stats_shape as
select
    shape_iri,
    count(*) as report_count,
    sum(focus_count) as total_focus,
    sum(violation_count) as total_violations,
    max(evaluated_at) as last_run
from donto_shape_report
group by shape_iri;

create or replace view donto_stats_rule as
select
    rule_iri,
    count(*) as run_count,
    sum(emitted_count) as total_emitted,
    avg(duration_ms) as avg_duration_ms,
    max(evaluated_at) as last_run
from donto_derivation_report
group by rule_iri;

create or replace view donto_stats_audit as
select
    action,
    count(*) as cnt,
    min(at) as first_at,
    max(at) as last_at
from donto_audit
group by action;
