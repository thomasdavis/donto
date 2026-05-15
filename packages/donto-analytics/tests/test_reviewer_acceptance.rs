//! Tests for the reviewer-acceptance analyzer (M5).

use chrono::{Duration, Utc};
use donto_analytics::analyzer_reviewer_acceptance::{
    run_analyzer, ReviewerAcceptanceConfig,
};
use donto_client::DontoClient;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn connect() -> Option<DontoClient> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    Some(c)
}

fn tag(prefix: &str) -> String {
    format!("test:{prefix}:{}", uuid::Uuid::new_v4().simple())
}

#[tokio::test]
async fn analyzer_aggregates_by_reviewer_and_context() {
    let Some(client) = connect().await else {
        eprintln!("skip: no DB");
        return;
    };
    let conn = client.pool().get().await.unwrap();
    let prefix = tag("rev-aggregate");
    let ctx_a = format!("ctx:extract/{prefix}/model-a");
    let ctx_b = format!("ctx:extract/{prefix}/model-b");
    client.ensure_context(&ctx_a, "custom", "permissive", None).await.unwrap();
    client.ensure_context(&ctx_b, "custom", "permissive", None).await.unwrap();
    let now = Utc::now();

    // Seed: model-a has 8 accept + 2 reject from reviewer alice;
    //       model-b has 3 accept + 7 reject from reviewer bob.
    for _ in 0..8 {
        conn.execute(
            "insert into donto_review_decision \
                (target_type, target_id, decision, reviewer_id, review_context, rationale) \
             values ('claim', $1, 'accept', 'alice', $2, 'looks good')",
            &[&format!("{prefix}/s-{}", uuid::Uuid::new_v4().simple()), &ctx_a],
        )
        .await
        .unwrap();
    }
    for _ in 0..2 {
        conn.execute(
            "insert into donto_review_decision \
                (target_type, target_id, decision, reviewer_id, review_context, rationale) \
             values ('claim', $1, 'reject', 'alice', $2, 'bad')",
            &[&format!("{prefix}/s-{}", uuid::Uuid::new_v4().simple()), &ctx_a],
        )
        .await
        .unwrap();
    }
    for _ in 0..3 {
        conn.execute(
            "insert into donto_review_decision \
                (target_type, target_id, decision, reviewer_id, review_context, rationale) \
             values ('claim', $1, 'accept', 'bob', $2, 'ok')",
            &[&format!("{prefix}/s-{}", uuid::Uuid::new_v4().simple()), &ctx_b],
        )
        .await
        .unwrap();
    }
    for _ in 0..7 {
        conn.execute(
            "insert into donto_review_decision \
                (target_type, target_id, decision, reviewer_id, review_context, rationale) \
             values ('claim', $1, 'reject', 'bob', $2, 'no')",
            &[&format!("{prefix}/s-{}", uuid::Uuid::new_v4().simple()), &ctx_b],
        )
        .await
        .unwrap();
    }

    let detector_iri = format!("donto:detector/reviewer-acceptance/test:{prefix}");
    let cfg = ReviewerAcceptanceConfig {
        window_start: now - Duration::hours(1),
        window_end: now + Duration::seconds(5),
        detector_iri: detector_iri.clone(),
        warn_reject_rate: 0.4,
    };
    let report = run_analyzer(&client, &cfg).await.unwrap();

    // Find the two buckets we just inserted.
    let alice = report
        .buckets
        .iter()
        .find(|b| b.review_context == ctx_a && b.reviewer_id == "alice")
        .expect("alice bucket");
    assert_eq!(alice.total, 10);
    assert_eq!(alice.accept, 8);
    assert_eq!(alice.reject, 2);
    assert!((alice.accept_rate() - 0.8).abs() < 1e-9);
    assert!((alice.reject_rate() - 0.2).abs() < 1e-9);

    let bob = report
        .buckets
        .iter()
        .find(|b| b.review_context == ctx_b && b.reviewer_id == "bob")
        .expect("bob bucket");
    assert_eq!(bob.total, 10);
    assert_eq!(bob.reject, 7);
    assert!((bob.reject_rate() - 0.7).abs() < 1e-9);

    // Bob's bucket exceeds warn threshold (0.7 >= 0.4) and total >= 5;
    // a warning finding should be emitted.
    let warned: Vec<_> = report
        .findings
        .iter()
        .filter(|f| {
            f.target_kind == "review_context"
                && f.target_id == ctx_b
                && f.detector_iri == detector_iri
        })
        .collect();
    assert!(
        !warned.is_empty(),
        "expected a warning finding for bob's high reject rate"
    );

    // Alice's reject rate is 0.2 (< 0.4) — no warning.
    let alice_warned: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.target_id == ctx_a && f.target_kind == "review_context")
        .collect();
    assert!(
        alice_warned.is_empty(),
        "alice's bucket should not be flagged"
    );

    // _self finding always present.
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.target_kind == "_self" && f.detector_iri == detector_iri),
        "_self finding must be emitted",
    );

    // Cleanup.
    let _ = conn
        .execute(
            "delete from donto_review_decision where review_context in ($1, $2)",
            &[&ctx_a, &ctx_b],
        )
        .await;
    let _ = conn
        .execute(
            "delete from donto_detector_finding where detector_iri = $1",
            &[&detector_iri],
        )
        .await;
}

#[tokio::test]
async fn analyzer_low_volume_buckets_do_not_warn() {
    // A bucket with reject_rate >= 0.4 BUT total < 5 should not
    // produce a warning — the analyzer requires enough volume to
    // be confident.
    let Some(client) = connect().await else {
        eprintln!("skip: no DB");
        return;
    };
    let conn = client.pool().get().await.unwrap();
    let prefix = tag("rev-lowvol");
    let ctx = format!("ctx:extract/{prefix}/sparse-model");
    client.ensure_context(&ctx, "custom", "permissive", None).await.unwrap();
    for _ in 0..2 {
        conn.execute(
            "insert into donto_review_decision \
                (target_type, target_id, decision, reviewer_id, review_context, rationale) \
             values ('claim', $1, 'reject', 'carol', $2, 'thin sample')",
            &[&format!("{prefix}/s-{}", uuid::Uuid::new_v4().simple()), &ctx],
        )
        .await
        .unwrap();
    }
    let detector_iri = format!("donto:detector/reviewer-acceptance/test:{prefix}");
    let cfg = ReviewerAcceptanceConfig {
        window_start: Utc::now() - Duration::hours(1),
        window_end: Utc::now() + Duration::seconds(5),
        detector_iri: detector_iri.clone(),
        warn_reject_rate: 0.4,
    };
    let report = run_analyzer(&client, &cfg).await.unwrap();
    let warned: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.target_id == ctx && f.target_kind == "review_context")
        .collect();
    assert!(warned.is_empty(), "sparse buckets should not warn");
    let _ = conn
        .execute(
            "delete from donto_review_decision where review_context = $1",
            &[&ctx],
        )
        .await;
    let _ = conn
        .execute(
            "delete from donto_detector_finding where detector_iri = $1",
            &[&detector_iri],
        )
        .await;
}

#[tokio::test]
async fn empty_window_still_emits_self_finding() {
    let Some(client) = connect().await else {
        eprintln!("skip: no DB");
        return;
    };
    let detector_iri = format!("donto:detector/reviewer-acceptance/test:empty:{}", uuid::Uuid::new_v4().simple());
    let cfg = ReviewerAcceptanceConfig {
        window_start: Utc::now() - Duration::minutes(1),
        window_end: Utc::now(),
        detector_iri: detector_iri.clone(),
        warn_reject_rate: 0.4,
    };
    let report = run_analyzer(&client, &cfg).await.unwrap();
    assert!(report
        .findings
        .iter()
        .any(|f| f.target_kind == "_self" && f.detector_iri == detector_iri));
    let conn = client.pool().get().await.unwrap();
    let _ = conn
        .execute(
            "delete from donto_detector_finding where detector_iri = $1",
            &[&detector_iri],
        )
        .await;
}
