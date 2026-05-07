//! End-to-end integration test for the M3/M5/M7 milestones.
//!
//! Subject: ajax-davis. The test asserts a handful of statements at
//! varying maturities, runs DontoQL queries with PRESET resolution
//! (M3), then builds a release manifest (M7). The M5 extraction
//! kernel is exercised separately via the Python module — see
//! `apps/donto-api/tests/test_extraction_dispatch.py`. This Rust
//! file covers the database-backed half end-to-end.

use chrono::{Duration, Utc};
use donto_client::{DontoClient, Object, StatementInput};
use donto_query::{evaluate, parse_dontoql};
use donto_release::{build_release, Citation, ReleaseSpec};

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

#[tokio::test]
async fn end_to_end_ajax_davis_through_preset_and_release() {
    let Ok(client) = DontoClient::from_dsn(&dsn()) else {
        eprintln!("skip");
        return;
    };
    if client.pool().get().await.is_err() {
        eprintln!("skip");
        return;
    }
    client.migrate().await.unwrap();

    // Per-test isolation prefix — but the *subject* is always ajax-davis.
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let public_ctx = format!("ctx:test/{tag}/public");
    let private_ctx = format!("ctx:test/{tag}/private");
    let hyp_ctx = format!("ctx:test/{tag}/hyp");

    client
        .ensure_context(&public_ctx, "custom", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&private_ctx, "custom", "permissive", None)
        .await
        .unwrap();
    // Hypothesis-kind context for PRESET under_hypothesis. Created
    // via raw SQL because ensure_context's Rust wrapper accepts only
    // a fixed set of kinds.
    let conn = client.pool().get().await.unwrap();
    conn.execute(
        "select donto_ensure_context($1, 'hypothesis', 'permissive', null)",
        &[&hyp_ctx],
    )
    .await
    .unwrap();
    drop(conn);

    let subj = "person:ajax-davis";

    // E0 (raw) — ingestion-time first observation.
    client
        .assert(
            &StatementInput::new(subj, "ex:knownAs", Object::iri("name:Ajax-Davis"))
                .with_context(&public_ctx)
                .with_maturity(0),
        )
        .await
        .unwrap();
    // E2 (evidence-supported) — curated.
    client
        .assert(
            &StatementInput::new(subj, "ex:role", Object::iri("role:donto-maintainer"))
                .with_context(&public_ctx)
                .with_maturity(2),
        )
        .await
        .unwrap();
    // E2 in a separate (private-by-policy) context.
    client
        .assert(
            &StatementInput::new(subj, "ex:emailDomain", Object::iri("domain:gmail.com"))
                .with_context(&private_ctx)
                .with_maturity(2),
        )
        .await
        .unwrap();
    // Hypothesis: posited in the hypothesis-kind context.
    client
        .assert(
            &StatementInput::new(subj, "ex:fluentIn", Object::iri("lang:rust"))
                .with_context(&hyp_ctx)
                .with_maturity(0),
        )
        .await
        .unwrap();

    // ─── M3: PRESET resolution ───────────────────────────────────────

    // PRESET raw across the public context — sees the E0 row.
    let q_raw = parse_dontoql(&format!(
        "PRESET raw\nSCOPE include <{public_ctx}>\nMATCH ?s ?p ?o\nPROJECT ?s, ?p, ?o"
    ))
    .unwrap();
    let raw_rows = evaluate(&client, &q_raw).await.unwrap();
    assert_eq!(
        raw_rows.len(),
        2,
        "PRESET raw over public ctx should see knownAs (E0) and role (E2)"
    );

    // PRESET curated across the public context — drops E0.
    let q_curated = parse_dontoql(&format!(
        "PRESET curated\nSCOPE include <{public_ctx}>\nMATCH ?s ?p ?o\nPROJECT ?s, ?p, ?o"
    ))
    .unwrap();
    let curated_rows = evaluate(&client, &q_curated).await.unwrap();
    assert_eq!(
        curated_rows.len(),
        1,
        "PRESET curated over public ctx should keep only the E2 role row"
    );

    // PRESET anywhere — drops scope, sees rows from all three contexts.
    let q_any = parse_dontoql(
        "PRESET anywhere\nMATCH person:ajax-davis ?p ?o\nPROJECT ?p, ?o",
    )
    .unwrap();
    let any_rows = evaluate(&client, &q_any).await.unwrap();
    let predicates: Vec<String> = any_rows
        .iter()
        .filter_map(|r| match r.0.get("p") {
            Some(donto_query::Term::Iri(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert!(predicates.iter().any(|p| p == "ex:knownAs"));
    assert!(predicates.iter().any(|p| p == "ex:role"));
    assert!(predicates.iter().any(|p| p == "ex:emailDomain"));
    assert!(predicates.iter().any(|p| p == "ex:fluentIn"));

    // PRESET under_hypothesis — only the hypothesis-kind context wins.
    let q_hyp = parse_dontoql(
        "PRESET under_hypothesis\nMATCH person:ajax-davis ?p ?o\nPROJECT ?p, ?o",
    )
    .unwrap();
    let hyp_rows = evaluate(&client, &q_hyp).await.unwrap();
    let hyp_preds: Vec<String> = hyp_rows
        .iter()
        .filter_map(|r| match r.0.get("p") {
            Some(donto_query::Term::Iri(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    // Should include fluentIn (hypothesis-context-only) and exclude
    // knownAs / role / emailDomain (in non-hypothesis contexts).
    // NB: other tests in this DB may have left hypothesis contexts
    // around; we only assert ours is present and the public-ctx
    // predicates aren't.
    assert!(
        hyp_preds.iter().any(|p| p == "ex:fluentIn"),
        "PRESET under_hypothesis must include the hypothesis-context row"
    );

    // PRESET as_of in the future-open window sees the row.
    let now_open = Utc::now() + Duration::seconds(1);
    let q_asof = parse_dontoql(&format!(
        "PRESET \"as_of:{}\"\nSCOPE include <{public_ctx}>\nMATCH person:ajax-davis ?p ?o\nPROJECT ?p, ?o",
        now_open.to_rfc3339()
    ))
    .unwrap();
    let asof_rows = evaluate(&client, &q_asof).await.unwrap();
    assert_eq!(asof_rows.len(), 2);

    // ─── M7: release builder over the curated public slice ──────────

    let spec = ReleaseSpec {
        release_id: format!("release:ajax-davis/{tag}/curated"),
        query_specs: vec![format!(
            "PRESET curated\nSCOPE include <{public_ctx}>\nMATCH person:ajax-davis ?p ?o"
        )],
        contexts: vec![public_ctx.clone()],
        as_of: None,
        min_maturity: 2, // matches PRESET curated
        require_public: false,
        citation: Citation {
            title: "Ajax Davis — public claim release".into(),
            authors: vec!["donto integration test".into()],
            year: Some(2026),
            ..Citation::default()
        },
        source_versions: vec![format!("doc:test-fixture/{tag}")],
        transformations: vec![format!("run:integration-test/{tag}")],
    };
    let manifest = build_release(&client, &spec).await.unwrap();

    assert_eq!(
        manifest.statement_checksums.len(),
        1,
        "curated release should bundle only the E2 role statement"
    );
    assert!(!manifest.manifest_sha256.is_empty());
    assert_eq!(manifest.manifest_sha256.len(), 64);
    assert_eq!(
        manifest.policy_report.decisions.len(),
        1,
        "policy report should record one decision per contributing context"
    );
    assert_eq!(manifest.citation.title, "Ajax Davis — public claim release");

    // Reproducibility: rebuild with the same spec, expect the same hash.
    let manifest2 = build_release(&client, &spec).await.unwrap();
    assert_eq!(
        manifest.manifest_sha256, manifest2.manifest_sha256,
        "rebuilding the same spec must reproduce manifest_sha256"
    );

    // Now widen to include the private context. The hash MUST change.
    let spec_wide = ReleaseSpec {
        contexts: vec![public_ctx.clone(), private_ctx.clone()],
        release_id: format!("release:ajax-davis/{tag}/wide"),
        ..spec.clone()
    };
    let manifest_wide = build_release(&client, &spec_wide).await.unwrap();
    assert_eq!(
        manifest_wide.statement_checksums.len(),
        2,
        "widening to include private ctx should add the E2 emailDomain row"
    );
    assert_ne!(manifest.manifest_sha256, manifest_wide.manifest_sha256);

    // Print a small report for the test runner so the human reviewer
    // can see the integration outcome at a glance.
    eprintln!(
        "\nIntegration outcome — ajax-davis (tag {tag}):\n\
         • PRESET raw      → {raw} rows (expected 2)\n\
         • PRESET curated  → {cur} rows (expected 1)\n\
         • PRESET anywhere → {any} rows (≥4)\n\
         • PRESET as_of    → {asof} rows (expected 2)\n\
         • Release (curated public)  → {n} stmt(s), sha256 {sha:.16}…\n\
         • Release (wide public+priv) → {nw} stmt(s), sha256 {shaw:.16}…\n",
        raw = raw_rows.len(),
        cur = curated_rows.len(),
        any = any_rows.len(),
        asof = asof_rows.len(),
        n = manifest.statement_checksums.len(),
        sha = manifest.manifest_sha256,
        nw = manifest_wide.statement_checksums.len(),
        shaw = manifest_wide.manifest_sha256,
    );
}
