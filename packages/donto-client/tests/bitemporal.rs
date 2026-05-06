//! Bitemporal correctness: retraction closes tx_time and is invisible to
//! default reads but visible to as-of reads. Correction preserves history.

mod common;

use chrono::{NaiveDate, Utc};
use donto_client::{ContextScope, Object, Polarity, StatementInput};

#[tokio::test]
async fn retract_closes_tx_time_and_history_is_recoverable() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = common::tag("retract");
    common::cleanup_prefix(&client, &prefix).await;
    let ctx = format!("{prefix}/ctx");
    client
        .ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    let subj = format!("{prefix}/a");
    let s = StatementInput::new(&subj, "ex:p", Object::iri("ex:b")).with_context(&ctx);
    let id = client.assert(&s).await.unwrap();

    // Visible before retraction.
    let before = Utc::now();
    let rows = client
        .match_pattern(
            Some(&subj),
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
    assert_eq!(rows.len(), 1);

    // Retract.
    assert!(client.retract(id).await.unwrap());
    // Idempotent: second retract returns false.
    assert!(!client.retract(id).await.unwrap());

    // Default read: gone.
    let rows = client
        .match_pattern(
            Some(&subj),
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
    assert_eq!(rows.len(), 0);

    // As-of just before the retraction: still there.
    let rows = client
        .match_pattern(
            Some(&subj),
            Some("ex:p"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            Some(before),
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "history must remain queryable");
}

#[tokio::test]
async fn correction_supersedes_prior_belief() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = common::tag("correct");
    common::cleanup_prefix(&client, &prefix).await;
    let ctx = format!("{prefix}/ctx");
    client
        .ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    let subj = format!("{prefix}/a");
    let s = StatementInput::new(&subj, "ex:p", Object::iri("ex:wrong")).with_context(&ctx);
    let id_old = client.assert(&s).await.unwrap();

    let id_new = client
        .correct(id_old, None, None, Some(&Object::iri("ex:right")), None)
        .await
        .unwrap();
    assert_ne!(id_old, id_new);

    // Default read returns the corrected statement.
    let rows = client
        .match_pattern(
            Some(&subj),
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
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].object, Object::Iri("ex:right".into()));
}

#[tokio::test]
async fn valid_time_filter_selects_in_range_only() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = common::tag("valid_time");
    common::cleanup_prefix(&client, &prefix).await;
    let ctx = format!("{prefix}/ctx");
    client
        .ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    let subj = format!("{prefix}/a");
    let in_range = StatementInput::new(&subj, "ex:livedIn", Object::iri("ex:berlin"))
        .with_context(&ctx)
        .with_valid(
            Some(NaiveDate::from_ymd_opt(2010, 1, 1).unwrap()),
            Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
        );
    let out_of_range = StatementInput::new(&subj, "ex:livedIn", Object::iri("ex:paris"))
        .with_context(&ctx)
        .with_valid(
            Some(NaiveDate::from_ymd_opt(2021, 1, 1).unwrap()),
            Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
        );

    client.assert(&in_range).await.unwrap();
    client.assert(&out_of_range).await.unwrap();

    let probe = NaiveDate::from_ymd_opt(2015, 6, 15).unwrap();
    let rows = client
        .match_pattern(
            Some(&subj),
            Some("ex:livedIn"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            Some(probe),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].object, Object::Iri("ex:berlin".into()));

    // No filter: see both.
    let rows = client
        .match_pattern(
            Some(&subj),
            Some("ex:livedIn"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
}
