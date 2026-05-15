//! Reviewer-acceptance analyzer (M5).
//!
//! Aggregates `donto_review_decision` rows over a window and emits
//! per-reviewer + per-extractor-model acceptance/rejection rates.
//! Used to calibrate extractor confidence against human review.
//!
//! Algorithm:
//!   1. Query `donto_review_decision` in window for review_context
//!      (which we treat as a stand-in for "extractor model" when the
//!      review was performed against an extraction run).
//!   2. For each (review_context, reviewer_id) bucket, compute
//!      counts of {accept, reject, qualify, request_evidence,
//!      merge, split, escalate, mark_sensitive, defer} decisions.
//!   3. Emit a `_self` info finding with the run summary so the
//!      detect-the-detector health loop covers this analyzer.
//!   4. Emit a `warning` finding per bucket whose reject rate
//!      exceeds `warn_reject_rate` (default 0.4). This is the
//!      "this extractor is producing too much that gets rejected"
//!      signal.
//!
//! The PRD M5 acceptance bullet: "Reviewer acceptance rates
//! calibrate extractor confidence". This analyzer is the producer;
//! a future scheduler step consumes the findings to lower the
//! extractor's default confidence threshold per model.

use crate::findings::{record_finding, record_generic_self_metric, Finding, Severity};
use chrono::{DateTime, Utc};
use donto_client::DontoClient;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ReviewerAcceptanceConfig {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub detector_iri: String,
    /// Buckets with reject_rate >= warn_reject_rate emit a warning
    /// finding. Defaults to 0.4 (i.e. 40% rejections is unhealthy).
    pub warn_reject_rate: f64,
}

impl Default for ReviewerAcceptanceConfig {
    fn default() -> Self {
        Self {
            window_start: Utc::now() - chrono::Duration::hours(24),
            window_end: Utc::now(),
            detector_iri: "donto:detector/reviewer-acceptance".into(),
            warn_reject_rate: 0.4,
        }
    }
}

/// One (review_context, reviewer_id) bucket with its decision counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bucket {
    pub review_context: String,
    pub reviewer_id: String,
    pub total: i64,
    pub accept: i64,
    pub reject: i64,
    pub qualify: i64,
    pub request_evidence: i64,
    pub merge: i64,
    pub split: i64,
    pub escalate: i64,
    pub mark_sensitive: i64,
    pub defer: i64,
}

impl Bucket {
    pub fn accept_rate(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.accept as f64 / self.total as f64
    }
    pub fn reject_rate(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.reject as f64 / self.total as f64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub buckets: Vec<Bucket>,
    pub buckets_emitted: u64,
    pub findings: Vec<Finding>,
}

pub async fn run_analyzer(
    client: &DontoClient,
    cfg: &ReviewerAcceptanceConfig,
) -> Result<Report, crate::AnalyticsError> {
    let conn = client.pool().get().await?;
    let rows = conn
        .query(
            "select \
                coalesce(review_context, '_no_context') as review_context, \
                reviewer_id, \
                count(*)::bigint as total, \
                count(*) filter (where decision = 'accept')::bigint as accept, \
                count(*) filter (where decision = 'reject')::bigint as reject, \
                count(*) filter (where decision = 'qualify')::bigint as qualify, \
                count(*) filter (where decision = 'request_evidence')::bigint as request_evidence, \
                count(*) filter (where decision = 'merge')::bigint as merge_, \
                count(*) filter (where decision = 'split')::bigint as split_, \
                count(*) filter (where decision = 'escalate')::bigint as escalate, \
                count(*) filter (where decision = 'mark_sensitive')::bigint as mark_sensitive, \
                count(*) filter (where decision = 'defer')::bigint as defer_ \
             from donto_review_decision \
             where created_at >= $1 and created_at < $2 \
             group by review_context, reviewer_id \
             order by total desc",
            &[&cfg.window_start, &cfg.window_end],
        )
        .await
        .map_err(donto_client::Error::Postgres)?;

    let mut buckets = Vec::with_capacity(rows.len());
    for r in rows {
        buckets.push(Bucket {
            review_context: r.get(0),
            reviewer_id: r.get(1),
            total: r.get(2),
            accept: r.get(3),
            reject: r.get(4),
            qualify: r.get(5),
            request_evidence: r.get(6),
            merge: r.get(7),
            split: r.get(8),
            escalate: r.get(9),
            mark_sensitive: r.get(10),
            defer: r.get(11),
        });
    }

    // Always emit a _self finding so `donto analyze health`
    // catches this detector. The payload is the bucket summary.
    let total_decisions: i64 = buckets.iter().map(|b| b.total).sum();
    let run_id = uuid::Uuid::new_v4();
    let _self_finding = record_generic_self_metric(
        client,
        &cfg.detector_iri,
        run_id,
        serde_json::json!({
            "kind": "reviewer_acceptance_run",
            "window_start": cfg.window_start.to_rfc3339(),
            "window_end": cfg.window_end.to_rfc3339(),
            "buckets": buckets.len(),
            "total_decisions": total_decisions,
        }),
    )
    .await?;

    let mut findings: Vec<Finding> = Vec::new();
    let mut emitted: u64 = 0;
    for b in &buckets {
        if b.total >= 5 && b.reject_rate() >= cfg.warn_reject_rate {
            let payload = serde_json::json!({
                "kind": "high_reject_rate",
                "reviewer_id": b.reviewer_id,
                "total": b.total,
                "accept": b.accept,
                "reject": b.reject,
                "reject_rate": b.reject_rate(),
                "warn_threshold": cfg.warn_reject_rate,
            });
            let finding = record_finding(
                client,
                &cfg.detector_iri,
                "review_context",
                &b.review_context,
                Severity::Warning,
                payload,
            )
            .await?;
            findings.push(finding);
            emitted += 1;
        }
    }

    // Include the _self finding in the returned vec for symmetry
    // with the other analyzers.
    findings.push(_self_finding);

    Ok(Report {
        buckets,
        buckets_emitted: emitted,
        findings,
    })
}
