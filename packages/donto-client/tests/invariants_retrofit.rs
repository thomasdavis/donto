//! Alexandria §3.4 invariants: retrofit ingest.
//!
//!   * valid_time is required (backdating an empty interval is meaningless)
//!   * tx_time is always now() — never backdated
//!   * retrofit_reason is required and queryable

use chrono::NaiveDate;
use donto_client::{Literal, Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

fn stmt(subject: &str, predicate: &str, object: Object, context: &str) -> StatementInput {
    StatementInput::new(subject, predicate, object).with_context(context)
}

#[tokio::test]
async fn retrofit_records_reason_and_preserves_backdated_valid_time() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("retrofit-ok");
    cleanup_prefix(&client, &prefix).await;

    let ctx = ctx(&client, "retrofit-ok").await;
    let subject = format!("{prefix}/article");
    let valid_lo = NaiveDate::from_ymd_opt(2018, 1, 1).unwrap();

    let id = client
        .assert_retrofit(
            &stmt(
                &subject,
                "openworld:flagged_as_biased",
                Object::Literal(Literal::string("yes")),
                &ctx,
            )
            .with_valid(Some(valid_lo), None),
            "compliance review 2026 Q2; predicate did not exist at ingest",
            Some("auditor"),
        )
        .await
        .expect("retrofit assert");

    // The statement's valid_time_from matches the backdated date, but its
    // tx_time_from is ~now (not backdated).
    let pool = client.pool().get().await.unwrap();
    let row = pool
        .query_one(
            "select lower(valid_time), lower(tx_time), now() - lower(tx_time) < interval '1 minute'
             from donto_statement where statement_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let vlo: NaiveDate = row.get(0);
    let tx_is_current: bool = row.get(2);
    assert_eq!(vlo, valid_lo, "valid_time was not backdated");
    assert!(tx_is_current, "tx_time must be now(), never backdated");

    // Reason is queryable via the overlay + view.
    let reason_row = pool
        .query_one(
            "select retrofit_reason from donto_retrofit where statement_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let reason: String = reason_row.get(0);
    assert!(reason.contains("compliance review"));
}

#[tokio::test]
async fn retrofit_requires_valid_time() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("retrofit-nv");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "retrofit-nv").await;

    let subject = format!("{prefix}/x");
    let err = client
        .assert_retrofit(
            &stmt(&subject, "ex:p", Object::iri("ex:o"), &ctx),
            "reason",
            None,
        )
        .await
        .err()
        .expect("retrofit without valid_time must error");
    let msg = format!("{err}");
    assert!(msg.contains("valid_lo") || msg.contains("valid_hi"));
}

#[tokio::test]
async fn retrofit_requires_reason() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("retrofit-nr");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "retrofit-nr").await;

    let subject = format!("{prefix}/x");
    // Bypass the client-side wrapper (which doesn't re-check the reason)
    // and drive the SQL directly to prove the function enforces it.
    let pool = client.pool().get().await.unwrap();
    let err = pool
        .query_one(
            "select donto_assert_retrofit($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
            &[
                &subject,
                &"ex:p",
                &"ex:o",
                &Option::<serde_json::Value>::None,
                &NaiveDate::from_ymd_opt(2020, 1, 1),
                &Option::<NaiveDate>::None,
                &"", // empty reason
                &ctx,
                &"asserted",
                &0_i32,
                &Option::<String>::None,
            ],
        )
        .await
        .err()
        .expect("empty reason must error");
    // tokio_postgres's Display is terse; look at the debug form where the
    // server's message ("retrofit_reason is required") appears.
    let msg = format!("{err:?}");
    assert!(
        msg.contains("retrofit_reason"),
        "expected 'retrofit_reason' in error, got: {msg}"
    );
}
