//! Rule-duration regression detector (C1).
//!
//! Algorithm:
//! 1. Load `RuleDurationFeature` rows for all rules since `since`.
//! 2. For each rule, iterate the sorted time-ordered rows with a rolling
//!    30-day window of non-NULL durations, computing causal median+MAD.
//! 3. Flag the most-recent `n_runs` rows where MAD-z > k.
//! 4. Separately compute null_rate over the trailing 24h for each rule.
//!    If null_rate > null_rate_threshold, emit a sidecar-health warning.
//! 5. Emit one `_self` finding per run.

use chrono::{DateTime, Duration, Utc};
use donto_client::DontoClient;
use uuid::Uuid;

use crate::features::{fetch_rule_duration_features, RuleDurationFeature};
use crate::findings::{record_finding, record_self_metric, Severity};
use crate::time_series::{mad_zscore, null_rate};

/// Configuration for the rule-duration detector.
#[derive(Debug, Clone)]
pub struct RuleDurationConfig {
    /// IRI that identifies this detector run.
    pub detector_iri: String,
    /// Fetch evaluations this far back.
    pub since: DateTime<Utc>,
    /// MAD-z threshold above which a run is flagged.
    pub k: f64,
    /// How many recent runs to evaluate per rule (the "most recent N").
    pub n_runs: usize,
    /// Rolling window for baseline (days).
    pub rolling_window_days: i64,
    /// Trailing window for null-rate computation (hours).
    pub null_rate_window_hours: i64,
    /// Null rate fraction above which a sidecar-health warning is emitted.
    pub null_rate_threshold: f64,
}

impl Default for RuleDurationConfig {
    fn default() -> Self {
        Self {
            detector_iri: "donto:detector/rule-duration/v1".into(),
            since: Utc::now() - Duration::days(7),
            k: 5.0,
            n_runs: 100,
            rolling_window_days: 30,
            null_rate_window_hours: 24,
            null_rate_threshold: 0.30,
        }
    }
}

/// Summary returned from a detector run (for the CLI to print).
#[derive(Debug)]
pub struct RuleDurationRunReport {
    pub run_id: Uuid,
    pub rules_examined: u64,
    pub anomaly_findings: u64,
    pub null_rate_findings: u64,
    pub self_finding_id: i64,
    pub overall_null_rate: f64,
}

/// Run the rule-duration detector and persist findings. Returns a summary.
pub async fn run(
    client: &DontoClient,
    cfg: &RuleDurationConfig,
) -> Result<RuleDurationRunReport, donto_client::Error> {
    let run_id = Uuid::new_v4();
    let all_rows = fetch_rule_duration_features(client, &cfg.since).await?;

    // Group rows by rule_iri, preserving evaluation order (already sorted).
    // BTreeMap ensures deterministic iteration order over rule IRIs (C1).
    let mut grouped: std::collections::BTreeMap<String, Vec<RuleDurationFeature>> =
        std::collections::BTreeMap::new();
    for row in &all_rows {
        grouped
            .entry(row.rule_iri.clone())
            .or_default()
            .push(row.clone());
    }

    let now = Utc::now();
    let null_window_start = now - Duration::hours(cfg.null_rate_window_hours);

    let mut anomaly_findings: u64 = 0;
    let mut null_rate_findings: u64 = 0;
    let mut total_null = 0u64;
    let mut total_count = 0u64;
    let mut rules_skipped_insufficient_window: u64 = 0;
    let mut rules_evaluated: u64 = 0;

    for (rule_iri, rows) in &grouped {
        // --- Null-rate check for trailing window ---
        let trailing: Vec<&RuleDurationFeature> = rows
            .iter()
            .filter(|r| r.evaluated_at >= null_window_start)
            .collect();

        let n_trailing = trailing.len() as u64;
        let n_null_trailing = trailing.iter().filter(|r| r.duration_ms.is_none()).count() as u64;

        total_count += n_trailing;
        total_null += n_null_trailing;

        let rule_null_rate = null_rate(n_null_trailing, n_trailing);
        if n_trailing > 0 && rule_null_rate > cfg.null_rate_threshold {
            let payload = serde_json::json!({
                "kind": "sidecar_health",
                "rule_iri": rule_iri,
                "null_rate": rule_null_rate,
                "trailing_window_hours": cfg.null_rate_window_hours,
                "n_trailing": n_trailing,
                "run_id": run_id,
            });
            record_finding(
                client,
                &cfg.detector_iri,
                "rule",
                rule_iri,
                Severity::Warning,
                payload,
            )
            .await?;
            null_rate_findings += 1;
        }

        // --- Regression check: rolling 30-day median/MAD, causal ---
        // Only non-NULL durations enter the baseline.
        let non_null_rows: Vec<&RuleDurationFeature> =
            rows.iter().filter(|r| r.duration_ms.is_some()).collect();

        // Take the most-recent n_runs rows for evaluation.
        let eval_start = if non_null_rows.len() > cfg.n_runs {
            non_null_rows.len() - cfg.n_runs
        } else {
            0
        };

        let mut rule_had_sufficient_window = false;
        let mut rule_had_skipped = false;

        for eval_idx in eval_start..non_null_rows.len() {
            let current = non_null_rows[eval_idx];
            let current_ms = current.duration_ms.unwrap() as f64;
            let window_cutoff = current.evaluated_at - Duration::days(cfg.rolling_window_days);

            // Build causal window: all non-NULL rows strictly BEFORE current,
            // within the rolling window.
            let window: Vec<f64> = non_null_rows[..eval_idx]
                .iter()
                .filter(|r| r.evaluated_at >= window_cutoff)
                .map(|r| r.duration_ms.unwrap() as f64)
                .collect();

            // Need at least 3 points to compute a meaningful MAD.
            if window.len() < 3 {
                rule_had_skipped = true;
                continue;
            }
            rule_had_sufficient_window = true;

            if let Some(z) = mad_zscore(current_ms, &window) {
                if z > cfg.k {
                    let payload = serde_json::json!({
                        "kind": "duration_regression",
                        "rule_iri": rule_iri,
                        "evaluated_at": current.evaluated_at,
                        "duration_ms": current.duration_ms,
                        "mad_zscore": z,
                        "k_threshold": cfg.k,
                        "window_size": window.len(),
                        "run_id": run_id,
                    });
                    let severity = if z > cfg.k * 2.0 {
                        Severity::Critical
                    } else {
                        Severity::Warning
                    };
                    record_finding(
                        client,
                        &cfg.detector_iri,
                        "rule",
                        rule_iri,
                        severity,
                        payload,
                    )
                    .await?;
                    anomaly_findings += 1;
                }
            }
        }

        // Tally per-rule window sufficiency (I3).
        if rule_had_sufficient_window {
            rules_evaluated += 1;
        } else if rule_had_skipped {
            rules_skipped_insufficient_window += 1;
        }
    }

    let rules_examined = grouped.len() as u64;
    let overall_null_rate = null_rate(total_null, total_count);
    let findings_count = anomaly_findings + null_rate_findings;

    let self_finding_id = record_self_metric(
        client,
        &cfg.detector_iri,
        run_id,
        &format!("{}d", (now - cfg.since).num_days()),
        rules_examined,
        findings_count,
        overall_null_rate,
        rules_skipped_insufficient_window,
        rules_evaluated,
    )
    .await?;

    Ok(RuleDurationRunReport {
        run_id,
        rules_examined,
        anomaly_findings,
        null_rate_findings,
        self_finding_id,
        overall_null_rate,
    })
}
