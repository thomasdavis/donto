//! Correction invariants (PRD §8 "retract + assert with overrides").
//!
//! `donto_correct` is the only sanctioned way to change a fact. It closes
//! the prior row's tx_time and emits a new row carrying the requested
//! overrides; every unspecified field is inherited from the prior row.
//! These tests exercise each axis independently (subject, predicate,
//! polarity, object) and chain corrections to prove the audit trail
//! stays intact across multiple hops.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};

#[tokio::test]
async fn correct_subject_only_inherits_everything_else() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "corr_subj").await;

    let id_old = c
        .assert(
            &StatementInput::new("ex:typo", "ex:knows", Object::iri("ex:bob")).with_context(&ctx),
        )
        .await
        .unwrap();
    let id_new = c
        .correct(id_old, Some("ex:alice"), None, None, None)
        .await
        .unwrap();

    let rows = c
        .match_pattern(
            Some("ex:alice"),
            Some("ex:knows"),
            Some("ex:bob"),
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "corrected row must be reachable under new subject"
    );
    assert_eq!(rows[0].statement_id, id_new);
    assert_eq!(rows[0].predicate, "ex:knows");
    assert_eq!(rows[0].object, Object::iri("ex:bob"));

    // The old subject no longer resolves at current tx_time.
    let stale = c
        .match_pattern(
            Some("ex:typo"),
            None,
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(
        stale.is_empty(),
        "old subject must not resolve after correction"
    );
}

#[tokio::test]
async fn correct_predicate_only_inherits_everything_else() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "corr_pred").await;

    let id_old = c
        .assert(
            &StatementInput::new("ex:a", "ex:employedBy", Object::iri("ex:corp"))
                .with_context(&ctx),
        )
        .await
        .unwrap();
    let id_new = c
        .correct(id_old, None, Some("ex:worksAt"), None, None)
        .await
        .unwrap();
    assert_ne!(id_old, id_new);

    let rows = c
        .match_pattern(
            Some("ex:a"),
            Some("ex:worksAt"),
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
    assert_eq!(rows[0].object, Object::iri("ex:corp"));
}

#[tokio::test]
async fn correct_polarity_flip_preserves_subject_predicate_object() {
    // Flipping polarity from asserted→negated is the cleanest paraconsistent
    // correction: "we said yes; now we say no; both rows persist at their
    // respective tx_times."
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "corr_flip").await;

    let id_old = c
        .assert(
            &StatementInput::new("ex:a", "ex:likes", Object::iri("ex:spinach")).with_context(&ctx),
        )
        .await
        .unwrap();
    let id_new = c
        .correct(id_old, None, None, None, Some(Polarity::Negated))
        .await
        .unwrap();

    // Current read: only the negated row is visible under asserted polarity
    // filter (there is none).
    let asserted = c
        .match_pattern(
            Some("ex:a"),
            Some("ex:likes"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(
        asserted.is_empty(),
        "corrected-to-negated row must not match polarity=asserted"
    );

    let negated = c
        .match_pattern(
            Some("ex:a"),
            Some("ex:likes"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Negated),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(negated.len(), 1);
    assert_eq!(negated[0].statement_id, id_new);
    assert_eq!(negated[0].object, Object::iri("ex:spinach"));

    // Both physical rows still exist (paraconsistency).
    let conn = c.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_statement where statement_id = any($1)",
            &[&[id_old, id_new].as_slice()],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2);
}

#[tokio::test]
async fn chained_corrections_preserve_all_historical_rows() {
    // A→B→C: three physical rows, two of them with a closed tx_time.
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "corr_chain").await;

    let id_a = c
        .assert(&StatementInput::new("ex:s", "ex:p", Object::iri("ex:v1")).with_context(&ctx))
        .await
        .unwrap();
    let id_b = c
        .correct(id_a, None, None, Some(&Object::iri("ex:v2")), None)
        .await
        .unwrap();
    let id_c = c
        .correct(id_b, None, None, Some(&Object::iri("ex:v3")), None)
        .await
        .unwrap();

    assert_ne!(id_a, id_b);
    assert_ne!(id_b, id_c);
    assert_ne!(id_a, id_c);

    let conn = c.pool().get().await.unwrap();
    let rows = conn
        .query(
            "select statement_id, upper(tx_time) from donto_statement \
             where statement_id = any($1) order by lower(tx_time)",
            &[&[id_a, id_b, id_c].as_slice()],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 3, "three physical rows must exist");

    // First two rows are closed; last one is open.
    let closed_a: Option<chrono::DateTime<chrono::Utc>> = rows[0].get(1);
    let closed_b: Option<chrono::DateTime<chrono::Utc>> = rows[1].get(1);
    let open_c: Option<chrono::DateTime<chrono::Utc>> = rows[2].get(1);
    assert!(closed_a.is_some(), "first row in chain must be tx-closed");
    assert!(closed_b.is_some(), "middle row in chain must be tx-closed");
    assert!(open_c.is_none(), "final row in chain must be tx-open");
}

#[tokio::test]
async fn correcting_a_retracted_row_does_not_resurrect_it() {
    // Retract closes the tx_time. Subsequent correct() on the same id should
    // be a no-op or error — not create a new row that silently shadows the
    // retraction.
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "corr_after_retract").await;

    let id = c
        .assert(&StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    assert!(c.retract(id).await.unwrap());

    // Any correction attempt should either error cleanly or return a value
    // that doesn't make the retracted row visible again.
    let outcome = c
        .correct(id, None, None, Some(&Object::iri("ex:c")), None)
        .await;

    // After the call, default (current-tx) reads must still see nothing for
    // the original (a, p, *).
    let visible = c
        .match_pattern(
            Some("ex:a"),
            Some("ex:p"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    match outcome {
        Ok(_) => {
            // If correct() succeeded it must have created a distinct new row
            // under the new object — not revived the retracted one.
            assert!(
                visible.iter().all(|r| r.statement_id != id),
                "retracted statement_id must not re-appear as a current row"
            );
        }
        Err(_) => {
            // If correct() rejected, the prior retraction stands.
            assert!(visible.is_empty());
        }
    }
}
