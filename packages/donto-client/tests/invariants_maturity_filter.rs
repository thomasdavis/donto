//! Maturity-floor invariants (PRD §2, §5).
//!
//! Maturity is a 0..=4 ladder. `match_pattern(min_maturity=k)` must return
//! only rows whose maturity is ≥ k; lower-maturity rows stay in the store
//! but are invisible under that filter.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};

#[tokio::test]
async fn filter_excludes_below_threshold_and_includes_at_or_above() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "mat_threshold").await;

    for m in 0..=4u8 {
        c.assert(
            &StatementInput::new(format!("ex:m{m}"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx)
                .with_maturity(m),
        )
        .await
        .unwrap();
    }

    for k in 0..=4u8 {
        let rows = c
            .match_pattern(
                None,
                Some("ex:p"),
                None,
                Some(&ContextScope::just(&ctx)),
                Some(Polarity::Asserted),
                k,
                None,
                None,
            )
            .await
            .unwrap();
        // 5 - k entries at or above the threshold.
        assert_eq!(
            rows.len(),
            (5 - k as usize),
            "min_maturity={k} expected {} rows, got {}",
            5 - k as usize,
            rows.len()
        );
        for r in &rows {
            assert!(
                r.maturity >= k,
                "row maturity {} < requested floor {k}",
                r.maturity
            );
        }
    }
}

#[tokio::test]
async fn maturity_zero_is_the_permissive_default() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "mat_default").await;

    c.assert(
        &StatementInput::new("ex:low", "ex:p", Object::iri("ex:o"))
            .with_context(&ctx)
            .with_maturity(0),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new("ex:hi", "ex:p", Object::iri("ex:o"))
            .with_context(&ctx)
            .with_maturity(4),
    )
    .await
    .unwrap();

    let rows = c
        .match_pattern(
            None,
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
    assert_eq!(rows.len(), 2, "min_maturity=0 must be fully permissive");
}
