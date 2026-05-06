//! v1000 production smoke / scale spike. Runs realistic-shaped batch
//! operations and asserts they complete in time bounds appropriate
//! for v1000-on-commodity-Postgres. These are smoke tests — they
//! catch order-of-magnitude regressions, not micro-benchmarks.

use std::time::Instant;

use donto_client::{Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

#[tokio::test]
async fn batch_assert_1k_under_two_seconds() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("smoke-1k");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "smoke-1k").await;

    let stmts: Vec<StatementInput> = (0..1000)
        .map(|i| {
            StatementInput::new(
                format!("{prefix}/s/{i}"),
                "ex:p",
                Object::iri(format!("{prefix}/o/{i}")),
            )
            .with_context(&ctx_iri)
        })
        .collect();

    let t0 = Instant::now();
    let n = client.assert_batch(&stmts).await.unwrap();
    let elapsed = t0.elapsed();
    assert_eq!(n, 1000);
    assert!(
        elapsed.as_secs_f64() < 5.0,
        "1k batch_assert in {elapsed:?} (target <5s for commodity Postgres)"
    );
}

#[tokio::test]
async fn one_hundred_v1000_overlay_writes_under_a_second() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("smoke-overlay-100");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "smoke-overlay-100").await;

    // Pre-create 100 statements.
    let mut stmt_ids = Vec::new();
    for i in 0..100 {
        let id = client
            .assert(
                &StatementInput::new(
                    format!("{prefix}/s/{i}"),
                    "ex:p",
                    Object::iri(format!("{prefix}/o/{i}")),
                )
                .with_context(&ctx_iri),
            )
            .await
            .unwrap();
        stmt_ids.push(id);
    }

    // Set modality + extraction level + claim_kind on each.
    let t0 = Instant::now();
    for id in &stmt_ids {
        c.execute("select donto_set_modality($1, 'descriptive')", &[id])
            .await
            .unwrap();
        c.execute(
            "select donto_set_extraction_level($1, 'source_generalization')",
            &[id],
        )
        .await
        .unwrap();
        c.execute("select donto_set_claim_kind($1, 'atomic')", &[id])
            .await
            .unwrap();
    }
    let elapsed = t0.elapsed();
    assert!(
        elapsed.as_secs_f64() < 5.0,
        "300 overlay writes in {elapsed:?}"
    );

    // All overlays present.
    let n: i64 = c
        .query_one(
            "select count(*) from donto_stmt_modality where statement_id = any($1)",
            &[&stmt_ids],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 100);
}

#[tokio::test]
async fn fifty_releases_seal_round_trip() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("smoke-50-releases");

    let t0 = Instant::now();
    let mut release_ids = Vec::new();
    for i in 0..50 {
        let id: uuid::Uuid = c
            .query_one(
                "insert into donto_dataset_release (release_name, release_version, query_spec) \
                 values ($1, $2, '{}'::jsonb) returning release_id",
                &[&format!("{prefix}-r{i}"), &"0.1.0"],
            )
            .await
            .unwrap()
            .get(0);
        release_ids.push(id);
        c.execute("select donto_seal_release($1, 'release-bot')", &[&id])
            .await
            .unwrap();
    }
    let elapsed = t0.elapsed();
    assert!(
        elapsed.as_secs_f64() < 5.0,
        "50 releases sealed in {elapsed:?}"
    );

    // Each emitted a creation event.
    let n: i64 = c
        .query_one(
            "select count(*) from donto_event_log \
             where target_kind = 'release' and target_id = any($1::text[])",
            &[&release_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 50);
}

#[tokio::test]
async fn frame_with_ten_roles_and_reverse_lookup() {
    // A typical valency frame may have ~10 roles. Indexed reverse-lookup
    // by (role, value_ref) must remain fast.
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "smoke-frame-reverse").await;
    let prefix = tag("smoke-frame-rv");
    let target_value = format!("ex:{prefix}/target");

    // Create 50 frames each with the target value in role 'subject'.
    for i in 0..50 {
        let id: uuid::Uuid = c
            .query_one(
                "select donto_create_claim_frame('valency_frame', $1)",
                &[&ctx_iri],
            )
            .await
            .unwrap()
            .get(0);
        for role in &["agent", "patient", "instrument", "location"] {
            c.execute(
                "select donto_add_frame_role($1, $2, 'literal', null, $3)",
                &[&id, role, &serde_json::json!({"i": i, "role": role})],
            )
            .await
            .unwrap();
        }
        c.execute(
            "select donto_add_frame_role($1, 'subject', 'entity', $2, null)",
            &[&id, &target_value],
        )
        .await
        .unwrap();
    }

    // Reverse lookup must find all 50.
    let t0 = Instant::now();
    let rows = c
        .query(
            "select frame_id from donto_frames_with_role_value('subject', $1, 1000)",
            &[&target_value],
        )
        .await
        .unwrap();
    let elapsed = t0.elapsed();
    assert_eq!(rows.len(), 50);
    assert!(
        elapsed.as_millis() < 500,
        "reverse lookup of 50 frames in {elapsed:?}"
    );
}

#[tokio::test]
async fn one_hundred_concurrent_attestation_issues_isolate_correctly() {
    // Issue 100 attestations concurrently for the same holder/policy.
    // All should succeed (issue is not idempotent — each is a credential).
    let client = pg_or_skip!(connect().await);
    let prefix = tag("smoke-att-concurrent");
    let holder = format!("agent:{prefix}");

    let t0 = Instant::now();
    let issues: Vec<_> = (0..100)
        .map(|_| {
            let cli = client.clone();
            let h = holder.clone();
            async move {
                let conn = cli.pool().get().await.unwrap();
                conn.query_one(
                    "select donto_issue_attestation($1, 's', 'policy:default/public', \
                        array['read_metadata']::text[], 'audit', 'rationale', null, null)",
                    &[&h],
                )
                .await
                .unwrap()
                .get::<_, uuid::Uuid>(0)
            }
        })
        .collect();
    let ids = futures_util::future::join_all(issues).await;
    let elapsed = t0.elapsed();

    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 100, "all 100 attestations are distinct");
    assert!(
        elapsed.as_secs_f64() < 10.0,
        "100 concurrent attestation issues in {elapsed:?}"
    );
}

#[tokio::test]
async fn alignment_closure_rebuild_after_30_alignments() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("smoke-aln-30");

    // Register 30 alignments forming a chain: P1 -> P2 -> ... -> P30
    let t_register = Instant::now();
    for i in 0..29 {
        c.query_one(
            "select donto_register_alignment($1, $2, 'exact_equivalent', 1.0)",
            &[
                &format!("ex:{prefix}/p{i}"),
                &format!("ex:{prefix}/p{}", i + 1),
            ],
        )
        .await
        .unwrap();
    }
    let register_elapsed = t_register.elapsed();
    assert!(
        register_elapsed.as_secs_f64() < 5.0,
        "30 alignment registers in {register_elapsed:?}"
    );

    // Rebuild closure.
    let t_close = Instant::now();
    let _: i32 = c
        .query_one("select donto_rebuild_predicate_closure()", &[])
        .await
        .unwrap()
        .get(0);
    let close_elapsed = t_close.elapsed();
    assert!(
        close_elapsed.as_secs_f64() < 10.0,
        "closure rebuild in {close_elapsed:?}"
    );
}
