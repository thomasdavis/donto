//! Typed wrappers over `donto_detector_finding`.
//!
//! Detectors call `record_finding` to persist a finding.
//! Callers should not write to the table directly.

use chrono::{DateTime, Utc};
use donto_client::DontoClient;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Severity level for a detector finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Critical => "critical",
        }
    }
}

/// A finding read back from `donto_detector_finding`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub finding_id: i64,
    pub detector_iri: String,
    pub target_kind: String,
    pub target_id: String,
    pub severity: Severity,
    pub observed_at: DateTime<Utc>,
    pub payload: serde_json::Value,
}

/// Insert a finding into `donto_detector_finding`. Returns the assigned
/// `finding_id` (bigserial).
pub async fn record_finding(
    client: &DontoClient,
    detector_iri: &str,
    target_kind: &str,
    target_id: &str,
    severity: Severity,
    payload: serde_json::Value,
) -> Result<i64, donto_client::Error> {
    let c = client.pool().get().await?;
    let row = c
        .query_one(
            "insert into donto_detector_finding
                 (detector_iri, target_kind, target_id, severity, payload)
             values ($1, $2, $3, $4, $5)
             returning finding_id",
            &[
                &detector_iri,
                &target_kind,
                &target_id,
                &severity.as_str(),
                &payload,
            ],
        )
        .await?;
    Ok(row.get(0))
}

/// Helper to build and record the mandatory `_self` self-metric finding.
///
/// One of these is emitted per detector run with the run-level diagnostics.
/// `rules_skipped_insufficient_window` counts rules where every candidate eval
/// point had fewer than 3 causal window points (I3).
/// `rules_evaluated` counts rules where at least one point was successfully
/// evaluated against a sufficient causal window (I3).
pub async fn record_self_metric(
    client: &DontoClient,
    detector_iri: &str,
    run_id: Uuid,
    lookback_window: &str,
    rules_examined: u64,
    findings_count: u64,
    null_rate_observed: f64,
    rules_skipped_insufficient_window: u64,
    rules_evaluated: u64,
) -> Result<i64, donto_client::Error> {
    let payload = serde_json::json!({
        "run_id": run_id,
        "lookback_window": lookback_window,
        "rules_examined": rules_examined,
        "findings_count": findings_count,
        "null_rate_observed": null_rate_observed,
        "rules_skipped_insufficient_window": rules_skipped_insufficient_window,
        "rules_evaluated": rules_evaluated,
    });
    record_finding(
        client,
        detector_iri,
        "_self",
        &run_id.to_string(),
        Severity::Info,
        payload,
    )
    .await
}

/// One row of detector health data, used by `donto-cli analyze health`.
///
/// Populated from the most-recent `_self` finding per `detector_iri`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorHealthRow {
    pub finding_id: i64,
    pub detector_iri: String,
    pub last_run_at: DateTime<Utc>,
    pub last_findings_count: u64,
    pub null_rate_observed: f64,
}

/// Fetch the most-recent `_self` finding for each known detector.
///
/// Used by `donto-cli analyze health` to verify liveness.
pub async fn fetch_self_metrics(
    client: &DontoClient,
) -> Result<Vec<DetectorHealthRow>, donto_client::Error> {
    let c = client.pool().get().await?;
    let rows = c
        .query(
            "select distinct on (detector_iri)
                    finding_id,
                    detector_iri,
                    observed_at,
                    payload
             from donto_detector_finding
             where target_kind = '_self'
             order by detector_iri, observed_at desc",
            &[],
        )
        .await?;

    let results = rows
        .into_iter()
        .map(|r| {
            let finding_id: i64 = r.get("finding_id");
            let detector_iri: String = r.get("detector_iri");
            let observed_at: DateTime<Utc> = r.get("observed_at");
            let payload: serde_json::Value = r.get("payload");

            let last_findings_count = payload
                .get("findings_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let null_rate_observed = payload
                .get("null_rate_observed")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            DetectorHealthRow {
                finding_id,
                detector_iri,
                last_run_at: observed_at,
                last_findings_count,
                null_rate_observed,
            }
        })
        .collect();

    Ok(results)
}

/// Fetch recent findings for a detector, newest first.
pub async fn recent_findings(
    client: &DontoClient,
    detector_iri: &str,
    limit: i64,
) -> Result<Vec<Finding>, donto_client::Error> {
    let c = client.pool().get().await?;
    let rows = c
        .query(
            "select finding_id, detector_iri, target_kind, target_id,
                    severity, observed_at, payload
             from donto_detector_finding
             where detector_iri = $1
             order by observed_at desc
             limit $2",
            &[&detector_iri, &limit],
        )
        .await?;

    rows.into_iter()
        .map(|r| {
            let sev_str: String = r.get("severity");
            let severity = match sev_str.as_str() {
                "warning" => Severity::Warning,
                "critical" => Severity::Critical,
                _ => Severity::Info,
            };
            Ok(Finding {
                finding_id: r.get("finding_id"),
                detector_iri: r.get("detector_iri"),
                target_kind: r.get("target_kind"),
                target_id: r.get("target_id"),
                severity,
                observed_at: r.get("observed_at"),
                payload: r.get("payload"),
            })
        })
        .collect()
}
