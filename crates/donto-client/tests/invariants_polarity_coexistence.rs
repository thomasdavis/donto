//! Polarity coexistence (PRD §3 principle 1, §6).
//!
//! Donto is paraconsistent: asserted, negated, absent, and unknown all
//! coexist at the atom for the same (subject, predicate, object, context).
//! The store never arbitrates. These tests pin that explicitly so any
//! future "helpful" deduplication sees them fail.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};

#[tokio::test]
async fn asserted_and_negated_for_same_triple_both_persist() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "pol_asserted_neg").await;

    let id_a = c
        .assert(&StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    let id_b = c
        .assert(
            &StatementInput::new("ex:a", "ex:p", Object::iri("ex:b"))
                .with_context(&ctx)
                .with_polarity(Polarity::Negated),
        )
        .await
        .unwrap();
    assert_ne!(id_a, id_b);

    let asserted = c
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
    let negated = c
        .match_pattern(
            Some("ex:a"),
            Some("ex:p"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Negated),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(asserted.len(), 1);
    assert_eq!(negated.len(), 1);
    assert_eq!(asserted[0].statement_id, id_a);
    assert_eq!(negated[0].statement_id, id_b);
}

#[tokio::test]
async fn all_four_polarities_coexist_on_the_same_triple() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "pol_all_four").await;

    let mut ids = Vec::new();
    for p in [
        Polarity::Asserted,
        Polarity::Negated,
        Polarity::Absent,
        Polarity::Unknown,
    ] {
        let id = c
            .assert(
                &StatementInput::new("ex:a", "ex:p", Object::iri("ex:b"))
                    .with_context(&ctx)
                    .with_polarity(p),
            )
            .await
            .unwrap();
        ids.push(id);
    }
    // Four distinct physical rows.
    let uniq: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(
        uniq.len(),
        4,
        "four polarities must produce four distinct rows"
    );

    // No polarity filter → all four come back.
    let all = c
        .match_pattern(
            Some("ex:a"),
            Some("ex:p"),
            None,
            Some(&ContextScope::just(&ctx)),
            None, // no polarity filter
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(all.len(), 4);
}

#[tokio::test]
async fn polarity_filter_does_not_leak_other_polarities() {
    // Regression guard for "I asked for asserted; I got everything".
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "pol_filter_leak").await;

    c.assert(&StatementInput::new("ex:x", "ex:p", Object::iri("ex:y")).with_context(&ctx))
        .await
        .unwrap();
    c.assert(
        &StatementInput::new("ex:x", "ex:p", Object::iri("ex:z"))
            .with_context(&ctx)
            .with_polarity(Polarity::Negated),
    )
    .await
    .unwrap();

    for (pol, expected_obj) in [(Polarity::Asserted, "ex:y"), (Polarity::Negated, "ex:z")] {
        let rows = c
            .match_pattern(
                Some("ex:x"),
                Some("ex:p"),
                None,
                Some(&ContextScope::just(&ctx)),
                Some(pol),
                0,
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "polarity {pol:?} returned {}", rows.len());
        assert_eq!(rows[0].object, Object::iri(expected_obj));
    }
}

#[tokio::test]
async fn same_triple_same_polarity_is_idempotent() {
    // Repeated assertion of the same (s, p, o, ctx, polarity) is a no-op:
    // same statement_id, no new physical row.
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "pol_idem").await;

    let input = StatementInput::new("ex:i", "ex:p", Object::iri("ex:j")).with_context(&ctx);
    let id1 = c.assert(&input).await.unwrap();
    let id2 = c.assert(&input).await.unwrap();
    let id3 = c.assert(&input).await.unwrap();
    assert_eq!(id1, id2);
    assert_eq!(id2, id3);

    let conn = c.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_statement \
             where subject='ex:i' and predicate='ex:p' and context=$1",
            &[&ctx],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        n, 1,
        "same content must not create additional physical rows"
    );
}
