//! Alexandria §3.5: shape reports as first-class attached annotations.
//!
//!   * attaching does not modify the underlying statement
//!   * a new verdict for the same (stmt, shape) closes the prior annotation
//!   * idempotent: attaching the same verdict twice doesn't churn tx_time
//!   * has_shape_verdict reflects the current open annotation only
//!   * user-submitted flags (context = user ctx) coexist with sidecar
//!     reports (context = sidecar ctx)

use donto_client::{Object, ShapeVerdict, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

fn stmt(subject: &str, predicate: &str, object: Object, context: &str) -> StatementInput {
    StatementInput::new(subject, predicate, object).with_context(context)
}

#[tokio::test]
async fn attach_does_not_mutate_statement() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("sr-noop");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "sr-noop").await;

    let subject = format!("{prefix}/s");
    let id = client
        .assert(&stmt(&subject, "ex:p", Object::iri("ex:o"), &ctx))
        .await
        .unwrap();

    // Content hash of the statement before + after attaching a violate report.
    let pool = client.pool().get().await.unwrap();
    let row_before = pool
        .query_one(
            "select content_hash, upper(tx_time) from donto_statement where statement_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let hash_before: Vec<u8> = row_before.get(0);
    let tx_hi_before: Option<chrono::DateTime<chrono::Utc>> = row_before.get(1);

    client
        .attach_shape_report(id, "builtin:functional", ShapeVerdict::Violate, &ctx, None)
        .await
        .unwrap();

    let row_after = pool
        .query_one(
            "select content_hash, upper(tx_time) from donto_statement where statement_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let hash_after: Vec<u8> = row_after.get(0);
    let tx_hi_after: Option<chrono::DateTime<chrono::Utc>> = row_after.get(1);
    assert_eq!(hash_before, hash_after, "content_hash must not change");
    assert_eq!(tx_hi_before, tx_hi_after, "tx_time must not change");
    assert!(tx_hi_after.is_none(), "statement must remain open");
}

#[tokio::test]
async fn verdict_replacement_closes_prior() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("sr-repl");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "sr-repl").await;

    let subject = format!("{prefix}/s");
    let id = client
        .assert(&stmt(&subject, "ex:p", Object::iri("ex:o"), &ctx))
        .await
        .unwrap();

    client
        .attach_shape_report(id, "builtin:datatype", ShapeVerdict::Warn, &ctx, None)
        .await
        .unwrap();
    client
        .attach_shape_report(id, "builtin:datatype", ShapeVerdict::Violate, &ctx, None)
        .await
        .unwrap();

    assert!(
        !client
            .has_shape_verdict(id, ShapeVerdict::Warn, Some("builtin:datatype"))
            .await
            .unwrap()
    );
    assert!(
        client
            .has_shape_verdict(id, ShapeVerdict::Violate, Some("builtin:datatype"))
            .await
            .unwrap()
    );

    // History is still there: two rows exist, one closed.
    let pool = client.pool().get().await.unwrap();
    let total: i64 = pool
        .query_one(
            "select count(*) from donto_stmt_shape_annotation where statement_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(total, 2, "both the warn and the violate rows must persist");
}

#[tokio::test]
async fn attach_is_idempotent() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("sr-idem");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "sr-idem").await;

    let subject = format!("{prefix}/s");
    let id = client
        .assert(&stmt(&subject, "ex:p", Object::iri("ex:o"), &ctx))
        .await
        .unwrap();

    let a1 = client
        .attach_shape_report(id, "ex:shape", ShapeVerdict::Pass, &ctx, None)
        .await
        .unwrap();
    let a2 = client
        .attach_shape_report(id, "ex:shape", ShapeVerdict::Pass, &ctx, None)
        .await
        .unwrap();
    assert_eq!(a1, a2, "re-attaching the same verdict must not churn rows");
}

#[tokio::test]
async fn user_flag_and_sidecar_report_coexist() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("sr-coex");
    cleanup_prefix(&client, &prefix).await;

    let user_ctx = format!("{prefix}/user/alice");
    let sidecar_ctx = format!("{prefix}/sidecar/lean");
    client
        .ensure_context(&user_ctx, "user", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&sidecar_ctx, "derivation", "permissive", None)
        .await
        .unwrap();

    let subject = format!("{prefix}/s");
    let id = client
        .assert(&stmt(&subject, "ex:p", Object::iri("ex:o"), &user_ctx))
        .await
        .unwrap();

    // Two different shapes from two different contexts.
    client
        .attach_shape_report(
            id,
            "user:flag/racist",
            ShapeVerdict::Violate,
            &user_ctx,
            None,
        )
        .await
        .unwrap();
    client
        .attach_shape_report(
            id,
            "builtin:functional",
            ShapeVerdict::Pass,
            &sidecar_ctx,
            None,
        )
        .await
        .unwrap();

    assert!(
        client
            .has_shape_verdict(id, ShapeVerdict::Violate, Some("user:flag/racist"))
            .await
            .unwrap()
    );
    assert!(
        client
            .has_shape_verdict(id, ShapeVerdict::Pass, Some("builtin:functional"))
            .await
            .unwrap()
    );
    // No violate under the sidecar context.
    assert!(
        !client
            .has_shape_verdict(id, ShapeVerdict::Violate, Some("builtin:functional"))
            .await
            .unwrap()
    );
}
