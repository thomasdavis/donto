//! v1000 / I1: hypothesis-only flag (migration 0089).
//!
//! A statement marked hypothesis_only is allowed to lack evidence
//! but must never be auto-promoted past E1.

use donto_client::{Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

#[tokio::test]
async fn mark_and_test_hypothesis_only() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("hyp-only-mark");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "hyp-only-mark").await;

    let stmt = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    let pre: bool = c
        .query_one("select donto_is_hypothesis_only($1)", &[&stmt])
        .await
        .unwrap()
        .get(0);
    assert!(!pre, "fresh statement is not hypothesis-only");

    c.execute(
        "select donto_mark_hypothesis_only($1, $2, $3)",
        &[&stmt, &"reviewer:a", &"speculative analysis"],
    )
    .await
    .unwrap();

    let post: bool = c
        .query_one("select donto_is_hypothesis_only($1)", &[&stmt])
        .await
        .unwrap()
        .get(0);
    assert!(post, "marked statement is hypothesis-only");
}

#[tokio::test]
async fn mark_is_idempotent() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("hyp-only-idem");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "hyp-only-idem").await;

    let stmt = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    c.execute("select donto_mark_hypothesis_only($1)", &[&stmt])
        .await
        .unwrap();
    c.execute("select donto_mark_hypothesis_only($1)", &[&stmt])
        .await
        .unwrap();

    let n: i64 = c
        .query_one(
            "select count(*) from donto_stmt_hypothesis_only where statement_id = $1",
            &[&stmt],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1, "idempotent on conflict");
}

#[tokio::test]
async fn promotion_gate_blocks_above_e1() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("hyp-only-promote");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "hyp-only-promote").await;

    let stmt = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();
    c.execute("select donto_mark_hypothesis_only($1)", &[&stmt])
        .await
        .unwrap();

    let allowed_e1: bool = c
        .query_one("select donto_can_promote_maturity($1, 1)", &[&stmt])
        .await
        .unwrap()
        .get(0);
    assert!(allowed_e1, "E1 promotion is allowed");

    let allowed_e2: bool = c
        .query_one("select donto_can_promote_maturity($1, 2)", &[&stmt])
        .await
        .unwrap()
        .get(0);
    assert!(!allowed_e2, "E2 promotion is blocked for hypothesis-only");

    let allowed_e3: bool = c
        .query_one("select donto_can_promote_maturity($1, 3)", &[&stmt])
        .await
        .unwrap()
        .get(0);
    assert!(!allowed_e3, "E3 promotion is blocked for hypothesis-only");
}

#[tokio::test]
async fn unmarked_can_promote_freely() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("hyp-only-unmarked");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "hyp-only-unmarked").await;

    let stmt = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    for level in 0..=4 {
        let allowed: bool = c
            .query_one(
                "select donto_can_promote_maturity($1, $2)",
                &[&stmt, &level],
            )
            .await
            .unwrap()
            .get(0);
        assert!(
            allowed,
            "unmarked statement may promote to level {level}"
        );
    }
}

#[tokio::test]
async fn cascades_when_statement_deleted() {
    // Sanity: hypothesis-only overlay is deleted with its statement.
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("hyp-only-cascade");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "hyp-only-cascade").await;

    let stmt = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    c.execute("select donto_mark_hypothesis_only($1)", &[&stmt])
        .await
        .unwrap();

    // Direct delete by content_hash bypassing donto_retract (test-only).
    c.execute(
        "delete from donto_statement where statement_id = $1",
        &[&stmt],
    )
    .await
    .unwrap();

    let n: i64 = c
        .query_one(
            "select count(*) from donto_stmt_hypothesis_only where statement_id = $1",
            &[&stmt],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 0, "ON DELETE CASCADE clears the overlay");
}
