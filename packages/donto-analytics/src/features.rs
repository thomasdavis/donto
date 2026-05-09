//! Feature extractors that pull rows from the donto DB into owned structs.
//!
//! Both extractors are pure data fetchers — no anomaly logic here.
//! The detector modules consume these structs.

use chrono::{DateTime, Utc};
use donto_client::DontoClient;
use uuid::Uuid;

/// One row from `donto_derivation_report` for a single rule, ordered by
/// `evaluated_at` ascending so rolling-window code can iterate left-to-right.
#[derive(Debug, Clone)]
pub struct RuleDurationFeature {
    pub rule_iri: String,
    pub evaluated_at: DateTime<Utc>,
    /// NULL when the Lean sidecar did not respond in time.
    pub duration_ms: Option<i32>,
    pub emitted_count: i64,
    /// Proxy for scope size: number of JSON keys in the `scope` column.
    pub scope_size_proxy: i32,
}

/// Aggregated paraconsistency signal for a single (subject, predicate) pair
/// within a time window.
#[derive(Debug, Clone)]
pub struct ParaconsistencyFeature {
    pub subject: String,
    pub predicate: String,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    /// Number of distinct polarity values observed.
    pub distinct_polarities: u32,
    /// Number of distinct contexts (primary + secondary via junction table).
    pub distinct_contexts: u32,
    /// Shannon entropy of polarity distribution, normalised to [0, 1].
    pub conflict_score: f64,
    /// Up to 5 statement_ids as evidence for the conflict.
    pub sample_statements: Vec<Uuid>,
    /// Polarity label strings parallel to `polarity_cnts`, ordered
    /// alphabetically. Available for future "dominant polarity" surfacing (I7).
    pub polarity_labels: Vec<String>,
    /// Raw polarity counts, parallel to `polarity_labels`.
    pub polarity_cnts: Vec<i64>,
}

/// Fetch rule-duration features for all rules with evaluations since `since`,
/// ordered by `(rule_iri, evaluated_at)` ascending.
pub async fn fetch_rule_duration_features(
    client: &DontoClient,
    since: &DateTime<Utc>,
) -> Result<Vec<RuleDurationFeature>, donto_client::Error> {
    let c = client.pool().get().await?;
    let rows = c
        .query(
            "select rule_iri,
                    evaluated_at,
                    duration_ms,
                    emitted_count,
                    coalesce(
                        (select count(*)::int from jsonb_object_keys(dr.scope) k),
                        0
                    ) as scope_size_proxy
             from donto_derivation_report dr
             where evaluated_at >= $1
             order by rule_iri, evaluated_at",
            &[since],
        )
        .await?;

    let features = rows
        .into_iter()
        .map(|r| RuleDurationFeature {
            rule_iri: r.get("rule_iri"),
            evaluated_at: r.get("evaluated_at"),
            duration_ms: r.get("duration_ms"),
            emitted_count: r.get("emitted_count"),
            scope_size_proxy: r.get("scope_size_proxy"),
        })
        .collect();

    Ok(features)
}

/// Fetch paraconsistency features for all (subject, predicate) pairs whose
/// open statements fall within [window_start, window_end].
///
/// Does NOT use `donto_v_statement_polarity_v1000` (O(N²)).
/// Aggregates directly from `donto_statement` joined to
/// `donto_statement_context` for secondary contexts.
///
/// A single query pulls polarity counts and context counts together so
/// entropy can be computed inline — no N+1 secondary queries.
pub async fn fetch_paraconsistency_features(
    client: &DontoClient,
    window_start: &DateTime<Utc>,
    window_end: &DateTime<Utc>,
) -> Result<Vec<ParaconsistencyFeature>, donto_client::Error> {
    let c = client.pool().get().await?;

    // Aggregate polarity counts + context counts per (subject, predicate).
    // All arithmetic (entropy) is done in Rust; SQL only aggregates.
    //
    // polarity_counts: array of (polarity_text, count) pairs for entropy.
    // We represent them as two parallel arrays for easy extraction.
    //
    // Context count merges primary context column with any rows in
    // donto_statement_context (secondary contexts).
    //
    // sample_statements: up to 5 statement_ids, deterministic via ordering.
    let rows = c
        .query(
            "
            with open_stmts as (
                select statement_id,
                       subject,
                       predicate,
                       donto_polarity(flags) as polarity,
                       context
                from donto_statement
                where upper(tx_time) is null
                  and lower(tx_time) <= $1
            ),
            secondary_ctxs as (
                select os.statement_id,
                       os.subject,
                       os.predicate,
                       sc.context
                from open_stmts os
                join donto_statement_context sc on sc.statement_id = os.statement_id
            ),
            all_ctxs as (
                select subject, predicate, statement_id, context
                from open_stmts
                union
                select subject, predicate, statement_id, context
                from secondary_ctxs
            ),
            pol_counts as (
                select subject,
                       predicate,
                       polarity,
                       count(*)::bigint as cnt
                from open_stmts
                group by subject, predicate, polarity
            ),
            pol_agg as (
                select subject,
                       predicate,
                       count(distinct polarity)::int as distinct_polarities,
                       array_agg(cnt order by polarity) as polarity_cnts,
                       array_agg(polarity::text order by polarity) as polarity_labels
                from pol_counts
                group by subject, predicate
                having count(distinct polarity) >= 2
            ),
            ctx_agg as (
                select subject,
                       predicate,
                       count(distinct context)::int as distinct_contexts
                from all_ctxs
                group by subject, predicate
            ),
            sample_agg as (
                select subject,
                       predicate,
                       (array_agg(statement_id order by statement_id))[1:5]
                           as sample_statements
                from open_stmts
                group by subject, predicate
            )
            select pa.subject,
                   pa.predicate,
                   pa.distinct_polarities,
                   ca.distinct_contexts,
                   pa.polarity_cnts,
                   pa.polarity_labels,
                   sa.sample_statements
            from pol_agg pa
            join ctx_agg ca  on ca.subject = pa.subject and ca.predicate = pa.predicate
            join sample_agg sa on sa.subject = pa.subject and sa.predicate = pa.predicate
            order by pa.distinct_polarities desc, pa.subject, pa.predicate
            ",
            // window_start is no longer used in the SQL after the I1 fix
            // (we now match temporal-overlap semantics: any statement still
            // open at-or-before window_end). The Rust struct still records it.
            &[window_end],
        )
        .await?;

    let features = rows
        .into_iter()
        .map(|r| {
            let subject: String = r.get("subject");
            let predicate: String = r.get("predicate");
            let distinct_polarities: i32 = r.get("distinct_polarities");
            let distinct_contexts: i32 = r.get("distinct_contexts");
            let polarity_cnts: Vec<i64> = r.get("polarity_cnts");
            let polarity_labels: Vec<String> = r.get("polarity_labels");
            let sample_ids: Vec<Uuid> = r.get("sample_statements");

            let counts: Vec<u64> = polarity_cnts.iter().map(|&c| c as u64).collect();
            let conflict_score = crate::time_series::normalized_entropy(&counts);

            ParaconsistencyFeature {
                subject,
                predicate,
                window_start: *window_start,
                window_end: *window_end,
                distinct_polarities: distinct_polarities as u32,
                distinct_contexts: distinct_contexts as u32,
                conflict_score,
                sample_statements: sample_ids,
                polarity_labels,
                polarity_cnts,
            }
        })
        .collect();

    Ok(features)
}
