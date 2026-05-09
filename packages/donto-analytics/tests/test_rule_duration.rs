//! Integration tests for the rule-duration regression detector (C1).
//!
//! Tests that require a DB use `pg_or_skip!`. The recall/precision assertions
//! against synthetic anomalies are skipped if `packages/donto-synthetic/anomalies.json`
//! is absent (data-engineer hasn't generated data yet).

mod common;

use chrono::{Duration, Utc};
use donto_analytics::{
    detector_rule_duration::{run as run_detector, RuleDurationConfig},
    findings::recent_findings,
};
use serde_json::Value as Json;

const DETECTOR_IRI: &str = "donto:detector/rule-duration/v1";

/// Insert synthetic derivation report rows for a rule.
async fn insert_reports(
    client: &donto_client::DontoClient,
    rule_iri: &str,
    durations_ms: &[Option<i32>],
    base_time: chrono::DateTime<chrono::Utc>,
) {
    let c = client.pool().get().await.expect("pool");
    for (i, dur) in durations_ms.iter().enumerate() {
        let evaluated_at = base_time + Duration::hours(i as i64);
        c.execute(
            "insert into donto_derivation_report
                 (rule_iri, inputs_fingerprint, scope, into_ctx,
                  emitted_count, duration_ms, evaluated_at)
             values ($1, decode('00','hex'), '{}'::jsonb, 'donto:anonymous', 1, $2, $3)",
            &[&rule_iri, dur, &evaluated_at],
        )
        .await
        .expect("insert report");
    }
}

/// Cleanup derivation reports for a rule prefix.
async fn cleanup_rule(client: &donto_client::DontoClient, prefix: &str) {
    let c = client.pool().get().await.expect("pool");
    c.execute(
        "delete from donto_derivation_report where rule_iri like $1",
        &[&format!("{prefix}%")],
    )
    .await
    .ok();
}

#[tokio::test]
async fn detector_emits_self_metric_finding() {
    let client = pg_or_skip!(common::connect().await);
    let tag = common::tag("rdr_self");
    let rule_iri = format!("{tag}/rule:fast");

    cleanup_rule(&client, &tag).await;

    // Insert a handful of normal-looking reports.
    let base = Utc::now() - Duration::days(3);
    let durs: Vec<Option<i32>> = (0..20).map(|_| Some(100)).collect();
    insert_reports(&client, &rule_iri, &durs, base).await;

    let cfg = RuleDurationConfig {
        detector_iri: format!("{DETECTOR_IRI}:{tag}"),
        since: Utc::now() - Duration::days(7),
        k: 5.0,
        ..Default::default()
    };
    let report = run_detector(&client, &cfg).await.expect("detector run");

    assert!(
        report.self_finding_id > 0,
        "_self finding must have a positive id"
    );

    // Verify it's in the DB.
    let findings = recent_findings(&client, &cfg.detector_iri, 10)
        .await
        .expect("recent_findings");

    let self_finding = findings
        .iter()
        .find(|f| f.target_kind == "_self")
        .expect("_self finding must exist");

    assert_eq!(
        self_finding.severity,
        donto_analytics::findings::Severity::Info
    );

    // Payload should carry run_id and rules_examined.
    let payload = &self_finding.payload;
    assert!(payload.get("run_id").is_some(), "payload must have run_id");
    assert!(
        payload.get("rules_examined").is_some(),
        "payload must have rules_examined"
    );

    cleanup_rule(&client, &tag).await;
}

#[tokio::test]
async fn detector_flags_spike_as_anomaly() {
    let client = pg_or_skip!(common::connect().await);
    let tag = common::tag("rdr_spike");
    let rule_iri = format!("{tag}/rule:slow");

    cleanup_rule(&client, &tag).await;

    // 30 normal runs at 100ms, then one spike at 10_000ms.
    let base = Utc::now() - Duration::days(10);
    let mut durs: Vec<Option<i32>> = (0..30).map(|_| Some(100)).collect();
    durs.push(Some(10_000)); // spike
    insert_reports(&client, &rule_iri, &durs, base).await;

    let cfg = RuleDurationConfig {
        detector_iri: format!("{DETECTOR_IRI}:{tag}"),
        since: Utc::now() - Duration::days(15),
        k: 5.0,
        n_runs: 100,
        ..Default::default()
    };
    let report = run_detector(&client, &cfg).await.expect("detector run");

    assert!(
        report.anomaly_findings >= 1,
        "spike should produce at least 1 anomaly finding, got {}",
        report.anomaly_findings
    );

    let findings = recent_findings(&client, &cfg.detector_iri, 50)
        .await
        .expect("recent_findings");

    let anomaly = findings
        .iter()
        .find(|f| f.target_kind == "rule" && f.target_id == rule_iri);
    assert!(anomaly.is_some(), "expected a rule-level anomaly finding");

    let payload = &anomaly.unwrap().payload;
    let kind = payload.get("kind").and_then(Json::as_str).unwrap_or("");
    assert_eq!(kind, "duration_regression");

    cleanup_rule(&client, &tag).await;
}

#[tokio::test]
async fn detector_emits_sidecar_health_warning_on_high_null_rate() {
    let client = pg_or_skip!(common::connect().await);
    let tag = common::tag("rdr_null");
    let rule_iri = format!("{tag}/rule:null");

    cleanup_rule(&client, &tag).await;

    // Insert 10 NULLs in the trailing 24h.
    let base = Utc::now() - Duration::hours(10);
    let durs: Vec<Option<i32>> = (0..10).map(|_| None).collect();
    insert_reports(&client, &rule_iri, &durs, base).await;

    let cfg = RuleDurationConfig {
        detector_iri: format!("{DETECTOR_IRI}:{tag}"),
        since: Utc::now() - Duration::days(1),
        null_rate_threshold: 0.30,
        ..Default::default()
    };
    let report = run_detector(&client, &cfg).await.expect("detector run");

    assert!(
        report.null_rate_findings >= 1,
        "10 NULLs out of 10 should trigger null_rate warning"
    );

    cleanup_rule(&client, &tag).await;
}

/// Evaluation test: if anomalies.json exists, assert recall and precision
/// against the synthetic ground truth.
#[tokio::test]
async fn detector_recall_precision_against_synthetic() {
    let client = pg_or_skip!(common::connect().await);

    // Load anomalies.json via the canonical path helper (I6).
    // Using a compile-time-stable absolute path instead of a relative path
    // that depends on the test binary's working directory.
    let anomalies_path = donto_synthetic::anomalies_json_path();
    if !anomalies_path.exists() {
        eprintln!("skipping recall/precision: anomalies.json not yet generated by data-engineer");
        return;
    }

    let raw = std::fs::read_to_string(anomalies_path).expect("read anomalies.json");
    let anomalies: serde_json::Value = serde_json::from_str(&raw).expect("parse anomalies.json");

    // anomalies.json schema: { "rule_iri": [[window_start, window_end], ...], ... }
    let mut ground_truth: Vec<(String, chrono::DateTime<Utc>, chrono::DateTime<Utc>)> = Vec::new();
    if let Some(map) = anomalies.as_object() {
        for (rule_iri, windows) in map {
            if let Some(arr) = windows.as_array() {
                for w in arr {
                    if let [start, end] = w.as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
                        let ws: chrono::DateTime<Utc> =
                            start.as_str().unwrap_or("").parse().unwrap_or(Utc::now());
                        let we: chrono::DateTime<Utc> =
                            end.as_str().unwrap_or("").parse().unwrap_or(Utc::now());
                        ground_truth.push((rule_iri.clone(), ws, we));
                    }
                }
            }
        }
    }

    if ground_truth.is_empty() {
        eprintln!("skipping: anomalies.json has no entries");
        return;
    }

    let cfg = RuleDurationConfig {
        detector_iri: "donto:detector/rule-duration/v1:eval".into(),
        since: Utc::now() - Duration::days(90),
        k: 5.0,
        n_runs: 100,
        ..Default::default()
    };
    let _report = run_detector(&client, &cfg)
        .await
        .expect("detector run for eval");

    let findings = recent_findings(&client, &cfg.detector_iri, 10_000)
        .await
        .expect("recent_findings");

    // Count true positives: anomaly windows hit by at least one finding.
    // We compare `evaluated_at` from the finding payload (the timestamp of the
    // rule execution that was flagged) against the ground-truth windows, NOT
    // `observed_at` (which is the wall-clock time the detector ran — always
    // near `now`, never inside a 90-day-old anomaly window). C2 fix.
    let mut tp = 0usize;
    for (rule_iri, ws, we) in &ground_truth {
        let hit = findings.iter().any(|f| {
            if f.target_kind != "rule" || &f.target_id != rule_iri {
                return false;
            }
            // Parse evaluated_at from the payload; skip this finding if absent.
            let evaluated_at: Option<chrono::DateTime<Utc>> = f
                .payload
                .get("evaluated_at")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok());
            match evaluated_at {
                Some(t) => t >= *ws && t <= *we,
                None => false,
            }
        });
        if hit {
            tp += 1;
        }
    }

    let rule_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.target_kind == "rule")
        .collect();
    let total_positives = rule_findings.len();
    let total_ground_truth = ground_truth.len();

    let recall = if total_ground_truth > 0 {
        tp as f64 / total_ground_truth as f64
    } else {
        1.0
    };
    let precision = if total_positives > 0 {
        tp as f64 / total_positives as f64
    } else {
        0.0
    };

    assert!(
        recall >= 0.7,
        "recall {recall:.2} below 0.70 threshold (tp={tp}, gt={total_ground_truth})"
    );
    assert!(
        precision >= 0.5,
        "precision {precision:.2} below 0.50 threshold (tp={tp}, positives={total_positives})"
    );
}
