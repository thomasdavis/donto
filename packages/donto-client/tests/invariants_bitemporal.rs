//! Bitemporal invariants (PRD §3 principle 3, §8).
//!
//! "Bitemporal from the atom. Every statement has valid-time and
//!  transaction-time intervals. Retraction closes transaction-time;
//!  it never deletes."
//!
//! These tests prove no statement is ever physically deleted, that
//! retraction history is always recoverable, that valid-time and
//! transaction-time are independently queryable, and that correction
//! preserves an audit trail.

mod common;

use chrono::{Duration, NaiveDate, Utc};
use donto_client::{ContextScope, Object, Polarity, StatementInput};

#[tokio::test]
async fn retraction_never_deletes_the_row() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "bt_no_delete").await;

    let id = c
        .assert(&StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    assert!(c.retract(id).await.unwrap());

    // Direct row check — the physical row must still exist.
    let conn = c.pool().get().await.unwrap();
    let row = conn
        .query_one(
            "select upper(tx_time) from donto_statement where statement_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let upper: Option<chrono::DateTime<Utc>> = row.get(0);
    assert!(upper.is_some(), "tx_time.upper must be set on retraction");
}

#[tokio::test]
async fn double_retract_is_idempotent_and_silent() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "bt_double_retract").await;

    let id = c
        .assert(&StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    assert!(c.retract(id).await.unwrap(), "first retract returns true");
    assert!(
        !c.retract(id).await.unwrap(),
        "second retract returns false (no row affected)"
    );
    assert!(!c.retract(id).await.unwrap(), "third retract returns false");
}

#[tokio::test]
async fn as_of_anywhere_in_open_window_returns_row() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "bt_as_of").await;
    let prefix = common::tag("bt_as_of");
    let subj = format!("{prefix}/a");

    let id = c
        .assert(&StatementInput::new(&subj, "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();

    // Brief sleep so Utc::now() is unambiguously after the assert in
    // Postgres microsecond resolution.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Sample 5 timestamps spread across an open window: all must see the row.
    for offset_ms in [10i64, 50, 100, 200, 500] {
        let probe = Utc::now() + Duration::milliseconds(offset_ms);
        let n = c
            .match_pattern(
                Some(&subj),
                Some("ex:p"),
                None,
                Some(&ContextScope::just(&ctx)),
                Some(Polarity::Asserted),
                0,
                Some(probe),
                None,
            )
            .await
            .unwrap()
            .len();
        assert_eq!(n, 1, "open-window probe at +{offset_ms}ms must see {id}");
    }
}

#[tokio::test]
async fn as_of_after_retraction_does_not_show_row() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "bt_after_retract").await;
    let prefix = common::tag("bt_after_retract");
    let subj = format!("{prefix}/a");

    let id = c
        .assert(&StatementInput::new(&subj, "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let before_retract = Utc::now();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    c.retract(id).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let after_retract = Utc::now();

    let pre = c
        .match_pattern(
            Some(&subj),
            Some("ex:p"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            Some(before_retract),
            None,
        )
        .await
        .unwrap();
    assert_eq!(pre.len(), 1, "pre-retraction window must see the row");

    let post = c
        .match_pattern(
            Some(&subj),
            Some("ex:p"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            Some(after_retract),
            None,
        )
        .await
        .unwrap();
    assert_eq!(post.len(), 0, "post-retraction window must not see the row");
}

#[tokio::test]
async fn correction_creates_new_row_and_closes_prior() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "bt_correction").await;

    let id_old = c
        .assert(&StatementInput::new("ex:a", "ex:loc", Object::iri("ex:wrong")).with_context(&ctx))
        .await
        .unwrap();
    let id_new = c
        .correct(id_old, None, None, Some(&Object::iri("ex:right")), None)
        .await
        .unwrap();
    assert_ne!(id_old, id_new, "correction must produce a new statement_id");

    // Both rows still exist physically.
    let conn = c.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_statement where statement_id = any($1)",
            &[&[id_old, id_new].as_slice()],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2, "both old and new physical rows must persist");

    // Default reads see only the new.
    let rows = c
        .match_pattern(
            Some("ex:a"),
            Some("ex:loc"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].statement_id, id_new);
    assert_eq!(rows[0].object, Object::Iri("ex:right".into()));
}

#[tokio::test]
async fn valid_time_intervals_are_independent_of_tx_time() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "bt_valid_indep").await;

    // Three life-stages of one entity, all asserted "now" but with valid-time
    // covering past decades.
    let stages = [
        (
            NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
            "ex:youth",
        ),
        (
            NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2010, 1, 1).unwrap(),
            "ex:middle",
        ),
        (
            NaiveDate::from_ymd_opt(2010, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2030, 1, 1).unwrap(),
            "ex:senior",
        ),
    ];
    for (lo, hi, role) in stages {
        c.assert(
            &StatementInput::new("ex:alice", "ex:role", Object::iri(role))
                .with_context(&ctx)
                .with_valid(Some(lo), Some(hi)),
        )
        .await
        .unwrap();
    }

    // Probe each stage.
    let cases = [
        (NaiveDate::from_ymd_opt(1980, 6, 1).unwrap(), "ex:youth"),
        (NaiveDate::from_ymd_opt(2000, 6, 1).unwrap(), "ex:middle"),
        (NaiveDate::from_ymd_opt(2020, 6, 1).unwrap(), "ex:senior"),
    ];
    for (probe, expected) in cases {
        let rows = c
            .match_pattern(
                Some("ex:alice"),
                Some("ex:role"),
                None,
                Some(&ContextScope::just(&ctx)),
                Some(Polarity::Asserted),
                0,
                None,
                Some(probe),
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "exactly one role at {probe}");
        assert_eq!(rows[0].object, Object::Iri(expected.into()));
    }
}

#[tokio::test]
async fn audit_records_assert_and_retract_actions() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "bt_audit").await;

    let id = c
        .assert(&StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    c.retract(id).await.unwrap();

    let conn = c.pool().get().await.unwrap();
    let actions: Vec<String> = conn
        .query(
            "select action from donto_audit where statement_id = $1 order by at",
            &[&id],
        )
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.get::<_, String>(0))
        .collect();
    assert_eq!(
        actions,
        vec!["assert", "retract"],
        "audit log must record both actions in order"
    );
}
