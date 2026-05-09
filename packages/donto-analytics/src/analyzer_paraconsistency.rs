//! Paraconsistency density analyzer (C2).
//!
//! Algorithm:
//! 1. Call `fetch_paraconsistency_features` to aggregate (s,p) pairs with
//!    ≥2 distinct polarities over the requested window.
//! 2. Upsert each result into `donto_paraconsistency_density` using the
//!    correct partial-index `on conflict (subject, predicate, window_start)`
//!    form (CLAUDE.md SQL idiom — named constraint not available for partial
//!    unique index).
//! 3. Pairs whose `conflict_score >= min_emit_score` also produce a finding
//!    in `donto_detector_finding` (target_kind='predicate_pair') so that
//!    `--alert-sink` and `donto analyze health` work the same way as for the
//!    rule-duration detector.
//! 4. Always emit a `_self` info finding with the run summary, so that
//!    `donto analyze health` covers this detector too (otherwise it would
//!    be invisible to the detect-the-detector loop).

use chrono::{DateTime, Utc};
use donto_client::DontoClient;
use uuid::Uuid;

use crate::features::fetch_paraconsistency_features;
use crate::findings::{record_finding, record_generic_self_metric, Finding, Severity};

/// Configuration for the paraconsistency analyzer.
#[derive(Debug, Clone)]
pub struct ParaconsistencyConfig {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    /// IRI for the `_self` and per-pair findings written to
    /// `donto_detector_finding`.
    pub detector_iri: String,
    /// Pairs with `conflict_score >= min_emit_score` are also written to
    /// `donto_detector_finding` (target_kind='predicate_pair'). Set to a
    /// value > 1.0 to suppress per-pair findings entirely.
    pub min_emit_score: f64,
}

impl Default for ParaconsistencyConfig {
    fn default() -> Self {
        Self {
            window_start: Utc::now() - chrono::Duration::hours(24),
            window_end: Utc::now(),
            detector_iri: "donto:detector/paraconsistency/v1".into(),
            min_emit_score: 0.6,
        }
    }
}

/// Summary returned after a run.
///
/// `findings` includes all per-pair findings plus the trailing `_self` row,
/// in insertion order. Detectors return findings inline so the CLI can
/// forward them to an `AlertSink` without a follow-up query (see
/// `detector_rule_duration` for the same pattern).
#[derive(Debug)]
pub struct ParaconsistencyRunReport {
    pub run_id: Uuid,
    pub pairs_examined: u64,
    pub pairs_upserted: u64,
    pub pairs_emitted: u64,
    pub max_conflict_score: f64,
    pub self_finding_id: i64,
    pub findings: Vec<Finding>,
}

/// Run the paraconsistency analyzer: aggregate features, upsert into
/// `donto_paraconsistency_density`, and emit findings for high-conflict pairs.
pub async fn run(
    client: &DontoClient,
    cfg: &ParaconsistencyConfig,
) -> Result<ParaconsistencyRunReport, donto_client::Error> {
    let run_id = Uuid::new_v4();
    let features =
        fetch_paraconsistency_features(client, &cfg.window_start, &cfg.window_end).await?;

    let pairs_examined = features.len() as u64;
    let mut pairs_upserted = 0u64;
    let mut pairs_emitted = 0u64;
    let mut max_score = 0f64;
    let mut findings: Vec<Finding> = Vec::new();

    let c = client.pool().get().await?;

    for f in &features {
        // sample_statements is Vec<Uuid>; we need to pass it as a slice.
        let samples: Vec<uuid::Uuid> = f.sample_statements.clone();

        c.execute(
            "insert into donto_paraconsistency_density
                 (subject, predicate, window_start, window_end,
                  distinct_polarities, distinct_contexts, conflict_score,
                  sample_statements, computed_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, now())
             on conflict (subject, predicate, window_start)
             do update set
                 window_end          = excluded.window_end,
                 distinct_polarities = excluded.distinct_polarities,
                 distinct_contexts   = excluded.distinct_contexts,
                 conflict_score      = excluded.conflict_score,
                 sample_statements   = excluded.sample_statements,
                 computed_at         = now()",
            &[
                &f.subject,
                &f.predicate,
                &cfg.window_start,
                &cfg.window_end,
                &(f.distinct_polarities as i32),
                &(f.distinct_contexts as i32),
                &f.conflict_score,
                &samples,
            ],
        )
        .await?;

        pairs_upserted += 1;
        if f.conflict_score > max_score {
            max_score = f.conflict_score;
        }

        // Emit a finding for sufficiently-conflicted pairs. Using `>=` so a
        // threshold of 0.0 emits everything (useful for debugging).
        if f.conflict_score >= cfg.min_emit_score {
            // Severity rises with conflict intensity. The split keeps the
            // alert volume manageable for dashboards while still surfacing
            // catastrophic disagreements as `critical`.
            let severity = if f.conflict_score >= 0.9 {
                Severity::Critical
            } else {
                Severity::Warning
            };
            let payload = serde_json::json!({
                "kind": "polarity_disagreement",
                "subject": f.subject,
                "predicate": f.predicate,
                "window_start": cfg.window_start,
                "window_end": cfg.window_end,
                "conflict_score": f.conflict_score,
                "distinct_polarities": f.distinct_polarities,
                "distinct_contexts": f.distinct_contexts,
                "polarity_labels": f.polarity_labels,
                "polarity_cnts": f.polarity_cnts,
                "sample_statements": f.sample_statements,
                "run_id": run_id,
            });
            // target_id is "subject\u{1F}predicate" — chr(31) unit separator
            // is safe inside text concat (CLAUDE.md SQL idiom note).
            let target_id = format!("{}\u{1F}{}", f.subject, f.predicate);
            let finding = record_finding(
                client,
                &cfg.detector_iri,
                "predicate_pair",
                &target_id,
                severity,
                payload,
            )
            .await?;
            findings.push(finding);
            pairs_emitted += 1;
        }
    }

    // Emit the _self finding so `donto analyze health` covers this detector.
    // null_rate_observed is set to 0.0 because paraconsistency has no NULL
    // sidecar-health analogue; the field exists in the payload schema for
    // uniformity with the rule-duration detector.
    let self_payload = serde_json::json!({
        "run_id": run_id,
        "lookback_window": format!("{}h", (cfg.window_end - cfg.window_start).num_hours()),
        "pairs_examined": pairs_examined,
        "pairs_upserted": pairs_upserted,
        "pairs_emitted": pairs_emitted,
        "max_conflict_score": max_score,
        "min_emit_score": cfg.min_emit_score,
        "findings_count": pairs_emitted,
        "null_rate_observed": 0.0,
        "window_start": cfg.window_start,
        "window_end": cfg.window_end,
    });
    let self_finding =
        record_generic_self_metric(client, &cfg.detector_iri, run_id, self_payload).await?;
    let self_finding_id = self_finding.finding_id;
    findings.push(self_finding);

    Ok(ParaconsistencyRunReport {
        run_id,
        pairs_examined,
        pairs_upserted,
        pairs_emitted,
        max_conflict_score: max_score,
        self_finding_id,
        findings,
    })
}
