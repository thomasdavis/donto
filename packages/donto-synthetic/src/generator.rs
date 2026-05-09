//! Core synthetic data generation logic.
//!
//! Produces deterministic, seed-reproducible rows across:
//!   - donto_statement (genealogy-flavored, cross-context contradictions)
//!   - donto_audit     (assert/retract/correct/mature events)
//!   - donto_event_log (review, frame, alignment events)
//!   - donto_derivation_report (lognormal duration with anomaly windows)
//!   - donto_shape_report (pre/post retraction storm)
//!   - donto_statement_context (multi-context membership)
//!
//! All IRIs are prefixed `synth:run-{seed}/...` so multiple seeds coexist
//! and `--reset` can truncate cleanly by prefix.

use crate::rng::Rng;
use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use deadpool_postgres::Object as PgConn;
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Actors (PRD §B item 5)
// ---------------------------------------------------------------------------
const ACTORS: &[(&str, f64)] = &[
    ("agent:system", 65.0),
    ("agent:llm-extractor-v1", 12.0),
    ("agent:llm-extractor-v2", 13.0),
    ("agent:human-curator-1", 7.0),
    ("agent:rule-engine", 3.0),
];

// ---------------------------------------------------------------------------
// Rule definitions for derivation reports (§B derivation reports)
// ---------------------------------------------------------------------------
struct RuleSpec {
    iri: &'static str,
    mean_ms: f64,
    sigma: f64,
}

const RULES: &[RuleSpec] = &[
    RuleSpec {
        iri: "rule:transitive-birth",
        mean_ms: 50.0,
        sigma: 0.6,
    },
    RuleSpec {
        iri: "rule:cross-source-verify",
        mean_ms: 200.0,
        sigma: 0.5,
    },
    RuleSpec {
        iri: "rule:date-normalize",
        mean_ms: 20.0,
        sigma: 0.4,
    },
    RuleSpec {
        iri: "rule:identity-merge",
        mean_ms: 500.0,
        sigma: 0.7,
    },
    RuleSpec {
        iri: "rule:contradiction-flag",
        mean_ms: 100.0,
        sigma: 0.55,
    },
];

// ---------------------------------------------------------------------------
// Shape IRIs for shape reports
// ---------------------------------------------------------------------------
const SHAPES: &[&str] = &[
    "shape:person-birth-date",
    "shape:person-death-date",
    "shape:marriage-valid-time",
    "shape:birth-before-death",
    "shape:unique-birth-place",
    "shape:source-attribution",
    "shape:date-precision",
    "shape:cross-context-conflict",
    "shape:maturity-floor",
    "shape:actor-non-null",
    "shape:context-kind-check",
];

// ---------------------------------------------------------------------------
// Genealogy predicates
// ---------------------------------------------------------------------------
const GEN_PREDICATES: &[&str] = &[
    "gen:birthDate",
    "gen:deathDate",
    "gen:marriedTo",
    "gen:birthPlace",
    "gen:deathPlace",
    "gen:occupation",
    "gen:parentOf",
    "gen:siblingOf",
];

// ---------------------------------------------------------------------------
// Source contexts for cross-context contradictions
// ---------------------------------------------------------------------------
const SOURCE_CONTEXTS: &[&str] = &[
    "ctx:source-a",
    "ctx:source-b",
    "ctx:source-c",
    "ctx:source-d",
];

// ---------------------------------------------------------------------------
// Public report
// ---------------------------------------------------------------------------
#[derive(Debug, Serialize)]
pub struct GeneratorReport {
    pub seed: u64,
    pub scale: f64,
    pub prefix: String,
    pub statements_inserted: usize,
    pub audit_rows: usize,
    pub event_log_rows: usize,
    pub derivation_reports: usize,
    pub shape_reports: usize,
    pub multi_context_rows: usize,
    pub anomalies_json_path: String,
}

// ---------------------------------------------------------------------------
// Anomaly record (written to anomalies.json)
// ---------------------------------------------------------------------------
#[derive(Debug, Serialize)]
struct AnomalyRecord {
    rule_iri: String,
    windows: Vec<AnomalyWindow>,
}

#[derive(Debug, Serialize)]
struct AnomalyWindow {
    window_start: String,
    window_end: String,
    multiplier: f64,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------
pub async fn run(
    client: &donto_client::DontoClient,
    seed: u64,
    scale: f64,
    reset: bool,
) -> Result<GeneratorReport> {
    let prefix = format!("synth:run-{seed}");
    let mut c = client.pool().get().await?;

    if reset {
        purge_prefix(&mut c, &prefix).await?;
        tracing::info!(%prefix, "synthetic data purged");
    }

    // Deterministic epoch: 2020-01-01 00:00:00 UTC as simulated tx_time start.
    let epoch: DateTime<Utc> = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    // Simulation window: 90 days.
    let sim_days = 90i64;

    // Scale targets.
    let target_stmts = (500_000f64 * scale).max(10.0) as usize;
    let target_deriv = (2_000f64 * scale).max(5.0) as usize;
    let target_shapes = (5_000f64 * scale).max(5.0) as usize;
    let target_events = (50_000f64 * scale).max(10.0) as usize;

    tracing::info!(
        target_stmts,
        target_deriv,
        target_shapes,
        target_events,
        "generation targets"
    );

    // Ensure contexts.
    ensure_contexts(&c, &prefix).await?;

    let mut rng = Rng::new(seed);

    // Build retraction storm windows (2 windows, deterministic offsets).
    // Window 1: day 20–21, Window 2: day 60–61.
    let storm1_lo = epoch + Duration::hours(20 * 24);
    let storm1_hi = epoch + Duration::hours(21 * 24);
    let storm2_lo = epoch + Duration::hours(60 * 24);
    let storm2_hi = epoch + Duration::hours(61 * 24);

    // Build burst windows (5 burst windows at known offsets).
    let burst_windows: Vec<(DateTime<Utc>, DateTime<Utc>)> = vec![
        (
            epoch + Duration::hours(5 * 24),
            epoch + Duration::hours(5 * 24) + Duration::hours(2),
        ),
        (
            epoch + Duration::hours(25 * 24),
            epoch + Duration::hours(25 * 24) + Duration::hours(3),
        ),
        (
            epoch + Duration::hours(45 * 24),
            epoch + Duration::hours(45 * 24) + Duration::hours(2),
        ),
        (
            epoch + Duration::hours(65 * 24),
            epoch + Duration::hours(65 * 24) + Duration::hours(4),
        ),
        (
            epoch + Duration::hours(80 * 24),
            epoch + Duration::hours(80 * 24) + Duration::hours(2),
        ),
    ];

    // --- Phase 1: Generate statements ---
    // (stmt_id, subject, predicate, context, sim_tx_lo) — reserved for future use
    let _stmt_ids: Vec<(Uuid, String, String, String, DateTime<Utc>)> = Vec::new();

    // Person pool: split into clusters for contradiction generation.
    let n_persons = (target_stmts / 6).max(20);
    let n_clusters = (n_persons / 50).max(3);

    // Pre-assign persons to clusters.
    let mut person_cluster: Vec<usize> = (0..n_persons).map(|i| i % n_clusters).collect();
    // Shuffle cluster assignments deterministically.
    for i in (1..n_persons).rev() {
        let j = rng.next_usize(i + 1);
        person_cluster.swap(i, j);
    }

    let mut statements_inserted = 0usize;
    let mut rng_stmt = rng.child(1);

    let batch_size = 200usize;
    let mut batch: Vec<serde_json::Value> = Vec::with_capacity(batch_size);

    // Track (subject, predicate) → set of contexts for contradiction detection.
    let mut sp_contexts: HashMap<(String, String), Vec<String>> = HashMap::new();

    for stmt_idx in 0..target_stmts {
        let person_idx = rng_stmt.next_usize(n_persons);
        let _cluster = person_cluster[person_idx];

        // Subject.
        let subject = format!("{prefix}/person/{person_idx}");

        // Predicate: genealogy-flavored.
        let pred_idx = rng_stmt.next_usize(GEN_PREDICATES.len());
        let predicate = GEN_PREDICATES[pred_idx].to_string();

        // Context: 5% cross-context contradictions — use 2-3 source contexts
        // for subjects in the same cluster.
        let context = if rng_stmt.bernoulli(0.05) {
            // Contradiction: pick a source context different from any prior for
            // this (subject, predicate), or same as one that already exists.
            let ctx_idx = rng_stmt.next_usize(SOURCE_CONTEXTS.len().min(3));
            format!("{prefix}/{}", SOURCE_CONTEXTS[ctx_idx])
        } else {
            // Normal: context tied to cluster so contradictions cluster.
            let src_idx = rng_stmt.next_usize(4).min(SOURCE_CONTEXTS.len() - 1);
            format!("{prefix}/{}", SOURCE_CONTEXTS[src_idx])
        };

        // Polarity: for cross-context, sometimes negate to create contradiction.
        let polarity = if rng_stmt.bernoulli(0.06) {
            "negated"
        } else {
            "asserted"
        };

        // Simulate tx_time: bursty distribution.
        let tx_sim = simulated_tx_time(&mut rng_stmt, epoch, sim_days, &burst_windows);

        // Valid_time: genealogy bimodal.
        // 30%+ of statements have valid_lo more than 50 years before tx_lo.
        let backdated = rng_stmt.bernoulli(0.35);
        let (valid_lo, valid_hi) = genealogy_valid_time(&mut rng_stmt, tx_sim, backdated);

        // Object: a date literal or IRI depending on predicate.
        let (object_iri, object_lit) =
            genealogy_object(&mut rng_stmt, &predicate, valid_lo, &prefix, person_idx);

        // Track for contradiction detection.
        sp_contexts
            .entry((subject.clone(), predicate.clone()))
            .or_default()
            .push(context.clone());

        // Build the row with `Map` so we can OMIT object_lit when it's None.
        // The `json!({"object_lit": object_lit})` shape would emit JSON null,
        // and donto_assert_batch extracts object_lit via `->` (jsonb), which
        // returns the JSONB scalar `null` (non-NULL) — that combined with a
        // non-null object_iri trips the donto_statement_object_one_of XOR
        // check. Omitting the key makes `->` return SQL NULL instead.
        let mut row = serde_json::Map::new();
        row.insert("subject".into(), serde_json::Value::String(subject.clone()));
        row.insert(
            "predicate".into(),
            serde_json::Value::String(predicate.clone()),
        );
        if let Some(iri) = &object_iri {
            row.insert("object_iri".into(), serde_json::Value::String(iri.clone()));
        }
        if let Some(lit) = &object_lit {
            row.insert("object_lit".into(), lit.clone());
        }
        row.insert("context".into(), serde_json::Value::String(context.clone()));
        row.insert(
            "polarity".into(),
            serde_json::Value::String(polarity.into()),
        );
        row.insert("maturity".into(), serde_json::json!(0));
        if let Some(d) = valid_lo {
            row.insert("valid_lo".into(), serde_json::Value::String(d.to_string()));
        }
        if let Some(d) = valid_hi {
            row.insert("valid_hi".into(), serde_json::Value::String(d.to_string()));
        }
        batch.push(serde_json::Value::Object(row));

        if batch.len() == batch_size || stmt_idx == target_stmts - 1 {
            // Insert batch via donto_assert_batch.
            let payload = serde_json::Value::Array(batch.clone());
            let inserted: i32 = c
                .query_one(
                    "select donto_assert_batch($1::jsonb, 'agent:system')",
                    &[&payload],
                )
                .await?
                .get(0);
            statements_inserted += inserted as usize;
            batch.clear();
        }
    }

    tracing::info!(statements_inserted, "statements done");

    // Collect open statement IDs for retraction storms and maturity progression.
    let open_stmts: Vec<(Uuid, String, String, String)> = c
        .query(
            "select statement_id, subject, predicate, context \
             from donto_statement \
             where context like $1 \
               and upper(tx_time) is null \
             limit 600000",
            &[&format!("{prefix}%")],
        )
        .await?
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get(2), r.get(3)))
        .collect();

    let total_open = open_stmts.len();

    // --- Phase 2: Retraction storms ---
    // Storm 1: retract 5-15% of context source-a statements.
    let ctx_a = format!("{prefix}/{}", SOURCE_CONTEXTS[0]);
    let storm_stmts_a: Vec<Uuid> = open_stmts
        .iter()
        .filter(|(_, _, _, ctx)| ctx == &ctx_a)
        .map(|(id, _, _, _)| *id)
        .collect();
    let storm1_count = (storm_stmts_a.len() as f64 * 0.10).ceil() as usize;
    let _rng_retract = rng.child(2);
    let mut retracted = 0usize;
    for id in storm_stmts_a.iter().take(storm1_count) {
        c.execute("select donto_retract($1, 'agent:system')", &[id])
            .await?;
        retracted += 1;
    }

    // Storm 2: retract 5-15% of context source-b statements.
    let ctx_b = format!("{prefix}/{}", SOURCE_CONTEXTS[1]);
    let storm_stmts_b: Vec<Uuid> = open_stmts
        .iter()
        .filter(|(_, _, _, ctx)| ctx == &ctx_b)
        .map(|(id, _, _, _)| *id)
        .collect();
    let storm2_count = (storm_stmts_b.len() as f64 * 0.08).ceil() as usize;
    for id in storm_stmts_b.iter().take(storm2_count) {
        c.execute("select donto_retract($1, 'agent:system')", &[id])
            .await?;
        retracted += 1;
    }
    let _ = retracted; // used for info
    tracing::info!(storm1_count, storm2_count, "retraction storms done");

    // --- Phase 3: Maturity progression (uses the A1 trigger) ---
    // Promote ≥10k statements through E0→E1→E2, a subset through E3→E5.
    let maturity_pool: Vec<Uuid> = open_stmts
        .iter()
        .filter(|(_, _, _, ctx)| !ctx.contains("source-a") || storm_stmts_a.len() < 2) // prefer non-retracted
        .map(|(id, _, _, _)| *id)
        .take((target_stmts as f64 * 0.025).ceil() as usize + 10_000)
        .collect();

    let promote_count = maturity_pool
        .len()
        .min((target_stmts as f64 * 0.02 + 10_000.0) as usize);
    let mut rng_mature = rng.child(3);

    // E0 → E1 batch (SET LOCAL so trigger fires with correct actor).
    let e1_flags: i16 = 1 << 2; // stored 1 = E1, polarity 0 = asserted
    let e2_flags: i16 = 2 << 2; // stored 2 = E2
    let e3_flags: i16 = 3 << 2; // stored 3 = E3
    let e5_flags: i16 = 4 << 2; // stored 4 = E5 (note: 4=E5 per maturity spec)

    // E0 → E1: each chunk runs in its own explicit transaction so that
    // SET LOCAL donto.actor covers all the UPDATE statements before commit.
    // Without an explicit transaction, tokio_postgres auto-commits each
    // execute() call individually, meaning SET LOCAL only scopes to its own
    // implicit mini-transaction and the GUC is gone before the first UPDATE
    // fires — causing the 0118 maturity-audit trigger to fall back to 'system'.
    for chunk in maturity_pool[..promote_count].chunks(500) {
        let actor = rng_mature.weighted_pick(ACTORS);
        let txn = c.build_transaction().start().await?;
        // set_config(name, value, is_local=true) is the function form of
        // SET LOCAL — and unlike `SET LOCAL ... = $1` (which is rejected by
        // Postgres because the parser needs a literal there), it accepts a
        // bound parameter cleanly.
        txn.execute("select set_config('donto.actor', $1, true)", &[actor])
            .await?;
        for id in chunk {
            txn.execute(
                "update donto_statement set flags = $1 \
                 where statement_id = $2 and upper(tx_time) is null",
                &[&e1_flags, id],
            )
            .await?;
        }
        txn.commit().await?;
    }

    // Subset E1 → E2.
    let e2_subset = &maturity_pool[..promote_count.min(promote_count * 2 / 3)];
    for chunk in e2_subset.chunks(500) {
        let actor = rng_mature.weighted_pick(ACTORS);
        let txn = c.build_transaction().start().await?;
        // set_config(name, value, is_local=true) is the function form of
        // SET LOCAL — and unlike `SET LOCAL ... = $1` (which is rejected by
        // Postgres because the parser needs a literal there), it accepts a
        // bound parameter cleanly.
        txn.execute("select set_config('donto.actor', $1, true)", &[actor])
            .await?;
        for id in chunk {
            txn.execute(
                "update donto_statement set flags = $1 \
                 where statement_id = $2 and upper(tx_time) is null and flags = $3",
                &[&e2_flags, id, &e1_flags],
            )
            .await?;
        }
        txn.commit().await?;
    }

    // Small subset E2 → E3 → E5 (small enough to not need actor sampling;
    // use a single transaction per pass).
    let e3_subset = &maturity_pool[..promote_count.min(2000)];
    {
        let actor = rng_mature.weighted_pick(ACTORS);
        let txn = c.build_transaction().start().await?;
        // set_config(name, value, is_local=true) is the function form of
        // SET LOCAL — and unlike `SET LOCAL ... = $1` (which is rejected by
        // Postgres because the parser needs a literal there), it accepts a
        // bound parameter cleanly.
        txn.execute("select set_config('donto.actor', $1, true)", &[actor])
            .await?;
        for id in e3_subset {
            txn.execute(
                "update donto_statement set flags = $1 \
                 where statement_id = $2 and upper(tx_time) is null and flags = $3",
                &[&e3_flags, id, &e2_flags],
            )
            .await?;
        }
        txn.commit().await?;
    }
    let e5_subset = &maturity_pool[..promote_count.min(500)];
    {
        let actor = rng_mature.weighted_pick(ACTORS);
        let txn = c.build_transaction().start().await?;
        // set_config(name, value, is_local=true) is the function form of
        // SET LOCAL — and unlike `SET LOCAL ... = $1` (which is rejected by
        // Postgres because the parser needs a literal there), it accepts a
        // bound parameter cleanly.
        txn.execute("select set_config('donto.actor', $1, true)", &[actor])
            .await?;
        for id in e5_subset {
            txn.execute(
                "update donto_statement set flags = $1 \
                 where statement_id = $2 and upper(tx_time) is null and flags = $3",
                &[&e5_flags, id, &e3_flags],
            )
            .await?;
        }
        txn.commit().await?;
    }
    tracing::info!(promote_count, "maturity progression done");

    // --- Phase 4: Multi-context membership (15% of statements) ---
    let mc_count = (total_open as f64 * 0.15).ceil() as usize;
    let mc_pool: Vec<Uuid> = open_stmts
        .iter()
        .map(|(id, _, _, _)| *id)
        .take(mc_count)
        .collect();
    let lens_roles = ["hypothesis_lens", "review_lens"];
    let mut rng_mc = rng.child(4);
    let mut multi_context_rows = 0usize;

    for id in &mc_pool {
        let role = lens_roles[rng_mc.next_usize(2)];
        let ctx_idx = rng_mc.next_usize(SOURCE_CONTEXTS.len());
        let ctx = format!("{prefix}/{}", SOURCE_CONTEXTS[ctx_idx]);
        c.execute(
            "select donto_add_statement_context($1, $2, $3, 'agent:system')",
            &[id, &ctx, &role],
        )
        .await
        .ok(); // ignore FK mismatches on retracted stmts
        multi_context_rows += 1;
    }
    tracing::info!(multi_context_rows, "multi-context rows done");

    // --- Phase 5: Derivation reports with anomaly windows ---
    let mut rng_deriv = rng.child(5);
    // .max(1) so a tiny --scale that targets fewer reports than there are
    // rules still emits at least one row per rule (otherwise integer division
    // floors to zero and the synthetic dataset has no derivation_reports at
    // all — silently breaking downstream detector tests).
    let reports_per_rule = (target_deriv / RULES.len()).max(1);
    let mut deriv_count = 0usize;
    let mut anomaly_records: Vec<AnomalyRecord> = Vec::new();

    // Anomaly windows: 3-5 per rule, spaced deterministically.
    for rule in RULES {
        let n_anomaly_windows = 3 + rng_deriv.next_usize(3); // 3-5
        let mut windows: Vec<AnomalyWindow> = Vec::new();

        for w in 0..n_anomaly_windows {
            let day_offset = 5 + w * (sim_days as usize / n_anomaly_windows);
            let ws = epoch + Duration::hours(day_offset as i64 * 24);
            let we = ws + Duration::hours(2);
            let mult = 5.0 + rng_deriv.next_f64() * 5.0; // 5x-10x
            windows.push(AnomalyWindow {
                window_start: ws.to_rfc3339(),
                window_end: we.to_rfc3339(),
                multiplier: mult,
            });
        }

        anomaly_records.push(AnomalyRecord {
            rule_iri: rule.iri.to_string(),
            windows,
        });
    }

    // Insert derivation reports.
    for (rule_idx, rule) in RULES.iter().enumerate() {
        let anomaly_windows = &anomaly_records[rule_idx].windows;

        for _ in 0..reports_per_rule {
            // ~10% have NULL duration (sidecar-absent).
            let is_null_duration = rng_deriv.bernoulli(0.10);

            let eval_offset_secs = (rng_deriv.next_f64() * sim_days as f64 * 86400.0) as i64;
            let evaluated_at = epoch + Duration::seconds(eval_offset_secs);

            // Check if this falls in an anomaly window.
            let mut duration_ms: Option<i32> = None;
            if !is_null_duration {
                let in_anomaly = anomaly_windows.iter().any(|w| {
                    let ws = chrono::DateTime::parse_from_rfc3339(&w.window_start)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or(epoch);
                    let we = chrono::DateTime::parse_from_rfc3339(&w.window_end)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or(epoch);
                    evaluated_at >= ws && evaluated_at < we
                });

                let base_ms = if in_anomaly {
                    // Find the matching window's multiplier.
                    let mult = anomaly_windows
                        .iter()
                        .find_map(|w| {
                            let ws = chrono::DateTime::parse_from_rfc3339(&w.window_start)
                                .map(|d| d.with_timezone(&Utc))
                                .unwrap_or(epoch);
                            let we = chrono::DateTime::parse_from_rfc3339(&w.window_end)
                                .map(|d| d.with_timezone(&Utc))
                                .unwrap_or(epoch);
                            if evaluated_at >= ws && evaluated_at < we {
                                Some(w.multiplier)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(5.0);
                    rng_deriv.lognormal(rule.mean_ms * mult, rule.sigma)
                } else {
                    rng_deriv.lognormal(rule.mean_ms, rule.sigma)
                };
                duration_ms = Some(base_ms.round() as i32);
            }

            // emitted_count correlated with duration (larger scope → longer eval).
            let emitted = duration_ms
                .map(|d| (d as f64 / rule.mean_ms * 10.0).round() as i64 + 1)
                .unwrap_or(5);

            let fp = uuid::Uuid::new_v4(); // synthetic fingerprint
            let scope = serde_json::json!({"include": [format!("{prefix}/ctx")]});
            let into_ctx = format!("{prefix}/derivation-output");

            c.execute(
                "insert into donto_derivation_report \
                 (rule_iri, inputs_fingerprint, scope, into_ctx, emitted_count, duration_ms, evaluated_at) \
                 values ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &rule.iri,
                    &fp.as_bytes().as_slice(),
                    &scope,
                    &into_ctx,
                    &emitted,
                    &duration_ms,
                    &evaluated_at,
                ],
            ).await?;
            deriv_count += 1;
        }
    }
    tracing::info!(deriv_count, "derivation reports done");

    // --- Phase 6: Shape reports ---
    let mut rng_shape = rng.child(6);
    // .max(1): see the rationale on reports_per_rule above.
    let reports_per_shape = (target_shapes / SHAPES.len()).max(1);
    let mut shape_count = 0usize;

    for shape_iri in SHAPES {
        for _ in 0..reports_per_shape {
            let eval_offset_secs = (rng_shape.next_f64() * sim_days as f64 * 86400.0) as i64;
            let evaluated_at = epoch + Duration::seconds(eval_offset_secs);

            // Pre-storm: violations higher; post-storm: violations lower.
            let in_storm = (evaluated_at >= storm1_lo && evaluated_at < storm1_hi)
                || (evaluated_at >= storm2_lo && evaluated_at < storm2_hi);
            let post_storm = evaluated_at > storm1_hi || evaluated_at > storm2_hi;

            let focus: i64 = (100 + rng_shape.next_usize(500)) as i64;
            let violations: i64 = if in_storm {
                (focus as f64 * 0.30) as i64 // higher during storm
            } else if post_storm {
                (focus as f64 * 0.05) as i64 // lower after
            } else {
                (focus as f64 * 0.12) as i64
            };

            let fp = uuid::Uuid::new_v4();
            let scope = serde_json::json!({"include": [format!("{prefix}/ctx")]});
            let report = serde_json::json!({"evaluated": true, "shape": shape_iri});

            c.execute(
                "insert into donto_shape_report \
                 (shape_iri, scope_fingerprint, scope, report, focus_count, violation_count, evaluated_at) \
                 values ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    shape_iri,
                    &fp.as_bytes().as_slice(),
                    &scope,
                    &report,
                    &focus,
                    &violations,
                    &evaluated_at,
                ],
            ).await?;
            shape_count += 1;
        }
    }
    tracing::info!(shape_count, "shape reports done");

    // --- Phase 7: Event log rows ---
    let target_kinds = [
        "alignment",
        "review_decision",
        "attestation",
        "identity_hypothesis",
        "frame",
    ];
    let event_types_by_kind: &[(&str, &[&str])] = &[
        ("alignment", &["created", "updated", "approved"]),
        ("review_decision", &["created", "approved", "rejected"]),
        ("attestation", &["created", "revoked"]),
        ("identity_hypothesis", &["created", "approved", "rejected"]),
        ("frame", &["created", "updated", "retracted"]),
    ];
    // .max(1): see the rationale on reports_per_rule above.
    let events_per_kind = (target_events / target_kinds.len()).max(1);
    let mut event_count = 0usize;
    let mut rng_ev = rng.child(7);

    for (kind, types) in event_types_by_kind {
        for i in 0..events_per_kind {
            let target_id = format!("{prefix}/evt/{kind}/{i}");
            let ev_type = types[rng_ev.next_usize(types.len())];
            let actor = rng_ev.weighted_pick(ACTORS);
            let offset_secs = (rng_ev.next_f64() * sim_days as f64 * 86400.0) as i64;
            let occurred_at = epoch + Duration::seconds(offset_secs);

            c.execute(
                "insert into donto_event_log \
                 (target_kind, target_id, event_type, actor, occurred_at) \
                 values ($1, $2, $3, $4, $5)",
                &[kind, &target_id, &ev_type, actor, &occurred_at],
            )
            .await?;
            event_count += 1;
        }
    }
    tracing::info!(event_count, "event log rows done");

    // Count audit rows (approximate: total in table after generation).
    let audit_rows: i64 = c
        .query_one("select count(*) from donto_audit", &[])
        .await?
        .get(0);

    // Write anomalies.json alongside the crate.
    let anomalies_path = anomalies_json_path();
    let anomalies_content = serde_json::to_string_pretty(&anomaly_records)?;
    std::fs::write(&anomalies_path, &anomalies_content)?;
    let anomalies_path_str = anomalies_path.display().to_string();
    tracing::info!(path = %anomalies_path_str, "anomalies.json written");

    Ok(GeneratorReport {
        seed,
        scale,
        prefix,
        statements_inserted,
        audit_rows: audit_rows as usize,
        event_log_rows: event_count,
        derivation_reports: deriv_count,
        shape_reports: shape_count,
        multi_context_rows,
        anomalies_json_path: anomalies_path_str,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Path to the sidecar anomalies.json file.
///
/// Delegates to `crate::anomalies_json_path()` in lib.rs, which uses the
/// compile-time `env!("CARGO_MANIFEST_DIR")` macro. This is stable even when
/// the function is called from another crate's test binary because the macro
/// expands at compile time relative to *this* crate's Cargo.toml.
pub fn anomalies_json_path() -> std::path::PathBuf {
    crate::anomalies_json_path()
}

/// Purge all synthetic data for a given prefix.
///
/// PRD contract: `donto_statement` rows are never deleted. Open statements
/// (those with `upper(tx_time) IS NULL`) are closed via `donto_retract` in
/// batches of 500 so the bitemporal tx_time is properly terminated and an
/// audit row is written. Already-retracted rows are left untouched (they
/// are historical and should not be disturbed).
///
/// Aux tables that are NOT governed by the no-delete rule are cleaned with
/// plain DELETE: `donto_statement_context`, `donto_derivation_report`,
/// `donto_shape_report`, `donto_detector_finding`, `donto_event_log`, and
/// `donto_context`.
async fn purge_prefix(c: &mut PgConn, prefix: &str) -> Result<()> {
    let like = format!("{prefix}%");
    const ACTOR: &str = "agent:synthetic-reset";
    const BATCH: i64 = 500;

    // --- Retract open donto_statement rows in batches of BATCH ---
    // Collect open IDs first, then retract in chunks. This avoids issuing
    // 500k single-row RPCs while still keeping each retraction within the
    // function that writes the audit row.
    loop {
        // Fetch the next batch of open statement IDs for this prefix.
        let rows = c
            .query(
                "select statement_id from donto_statement \
                 where context like $1 and upper(tx_time) is null \
                 limit $2",
                &[&like, &BATCH],
            )
            .await?;

        if rows.is_empty() {
            break;
        }

        let ids: Vec<uuid::Uuid> = rows.iter().map(|r| r.get(0)).collect();

        // Retract the whole batch in one server-round-trip using an
        // unnest-driven UPDATE + audit INSERT inside the SQL function.
        // We call donto_retract per-id but do it in a single transaction
        // so all audit rows land atomically.
        let txn = c.build_transaction().start().await?;
        for id in &ids {
            txn.execute("select donto_retract($1, $2)", &[id, &ACTOR])
                .await?;
        }
        txn.commit().await?;
    }

    // --- Delete aux tables (not governed by the no-delete rule) ---

    // donto_statement_context: membership overlay, safe to delete.
    c.execute(
        "delete from donto_statement_context sc \
         using donto_statement s \
         where sc.statement_id = s.statement_id and s.context like $1",
        &[&like],
    )
    .await
    .ok();

    // donto_derivation_report: keyed by into_ctx.
    c.execute(
        "delete from donto_derivation_report where into_ctx like $1",
        &[&like],
    )
    .await
    .ok();

    // donto_shape_report: keyed by scope (prefix match on JSONB include array).
    // The scope jsonb stores context IRIs; the simplest safe match is on the
    // report's evaluated scope containing the prefix in its serialised form.
    c.execute(
        "delete from donto_shape_report \
         where scope::text like $1",
        &[&like],
    )
    .await
    .ok();

    // donto_detector_finding: keyed by target_id (synthetic IRIs carry the prefix).
    c.execute(
        "delete from donto_detector_finding where target_id like $1",
        &[&like],
    )
    .await
    .ok();

    // donto_event_log: keyed by target_id.
    c.execute(
        "delete from donto_event_log where target_id like $1",
        &[&like],
    )
    .await
    .ok();

    // donto_context: synthetic contexts whose IRI matches the prefix.
    c.execute("delete from donto_context where iri like $1", &[&like])
        .await
        .ok();

    Ok(())
}

/// Ensure all contexts exist.
async fn ensure_contexts(c: &PgConn, prefix: &str) -> Result<()> {
    let contexts = [
        (format!("{prefix}/ctx"), "source"),
        (format!("{prefix}/derivation-output"), "derivation"),
        (format!("{prefix}/{}", SOURCE_CONTEXTS[0]), "source"),
        (format!("{prefix}/{}", SOURCE_CONTEXTS[1]), "source"),
        (format!("{prefix}/{}", SOURCE_CONTEXTS[2]), "source"),
        (format!("{prefix}/{}", SOURCE_CONTEXTS[3]), "source"),
    ];
    for (iri, kind) in &contexts {
        c.execute(
            "select donto_ensure_context($1, $2, 'permissive', null)",
            &[iri, kind],
        )
        .await?;
    }
    Ok(())
}

/// Compute a simulated tx_time within the 90-day window, with burst spikes.
fn simulated_tx_time(
    rng: &mut Rng,
    epoch: DateTime<Utc>,
    sim_days: i64,
    bursts: &[(DateTime<Utc>, DateTime<Utc>)],
) -> DateTime<Utc> {
    // With 20% probability, land in a burst window.
    if rng.bernoulli(0.20) {
        let burst_idx = rng.next_usize(bursts.len());
        let (bs, be) = bursts[burst_idx];
        let span = (be - bs).num_seconds().max(1);
        let offset = (rng.next_f64() * span as f64) as i64;
        return bs + Duration::seconds(offset);
    }
    // Uniform baseline.
    let offset_secs = (rng.next_f64() * sim_days as f64 * 86400.0) as i64;
    epoch + Duration::seconds(offset_secs)
}

/// Return genealogy-flavored (valid_lo, valid_hi).
/// 35% of calls produce a backdated valid_lo more than 50 years before tx.
fn genealogy_valid_time(
    rng: &mut Rng,
    tx_sim: DateTime<Utc>,
    backdated: bool,
) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let tx_year = tx_sim.date_naive().year();

    let lo_year = if backdated {
        // 50-200 years before tx_time.
        tx_year - 50 - rng.next_usize(150) as i32
    } else {
        // Within recent 30 years.
        tx_year - rng.next_usize(30) as i32
    };
    let lo_year = lo_year.max(1600);

    let lo = NaiveDate::from_ymd_opt(
        lo_year,
        1 + rng.next_usize(12) as u32,
        1 + rng.next_usize(28) as u32,
    )
    .unwrap_or(NaiveDate::from_ymd_opt(lo_year, 1, 1).unwrap());

    // 1-year interval for events like births/marriages.
    let hi = NaiveDate::from_ymd_opt(lo_year + 1, lo.month(), lo.day()).unwrap_or(lo);

    (Some(lo), Some(hi))
}

/// Build object for a genealogy predicate.
fn genealogy_object(
    rng: &mut Rng,
    predicate: &str,
    valid_lo: Option<NaiveDate>,
    prefix: &str,
    person_idx: usize,
) -> (Option<String>, Option<serde_json::Value>) {
    match predicate {
        "gen:birthDate" | "gen:deathDate" => {
            let date_str = valid_lo
                .map(|d| d.to_string())
                .unwrap_or_else(|| "1900-01-01".to_string());
            (
                None,
                Some(serde_json::json!({
                    "v": date_str,
                    "dt": "xsd:date",
                    "lang": null
                })),
            )
        }
        "gen:marriedTo" | "gen:parentOf" | "gen:siblingOf" => {
            let other = (person_idx + 1 + rng.next_usize(10)) % (person_idx + 11);
            (Some(format!("{prefix}/person/{other}")), None)
        }
        "gen:birthPlace" | "gen:deathPlace" => {
            let place = rng.next_usize(50);
            (Some(format!("{prefix}/place/{place}")), None)
        }
        _ => {
            // occupation etc.
            let val = rng.next_usize(20);
            (Some(format!("{prefix}/concept/{val}")), None)
        }
    }
}
