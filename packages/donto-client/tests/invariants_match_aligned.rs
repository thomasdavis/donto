//! Alignment-aware matching invariants (migration 0052).
//!
//! `donto_match_aligned` extends `donto_match` with closure expansion. Each
//! row carries `matched_via` ('direct', 'exact_equivalent', 'inverse_equivalent',
//! 'sub_property_of', 'close_match') and `alignment_confidence`. Inverse
//! equivalents are projected with subject and object swapped so the caller
//! sees rows oriented like the original predicate. `match_strict` skips the
//! closure entirely.

mod common;

use common::{connect, ctx, rebuild_closure_with_retry, tag};
use donto_client::{AlignmentRelation, ContextScope, Object, StatementInput};

async fn register_predicate(client: &donto_client::DontoClient, iri: &str) {
    let c = client.pool().get().await.unwrap();
    c.execute(
        "select donto_register_predicate($1, null, null, null, null, null, null, null)",
        &[&iri],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn exact_equivalent_returns_assertion_via_expansion() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ma-exact").await;
    let prefix = tag("ma-exact");

    let born_in = format!("{prefix}/bornIn");
    let was_born_in = format!("{prefix}/wasBornIn");
    register_predicate(&client, &born_in).await;
    register_predicate(&client, &was_born_in).await;

    // Assert with bornIn.
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/alice"),
                &born_in,
                Object::iri(format!("{prefix}/london")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Align and rebuild closure.
    client
        .register_alignment(
            &born_in,
            &was_born_in,
            AlignmentRelation::ExactEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    rebuild_closure_with_retry(&client).await;

    // Query for wasBornIn — must return the bornIn row via expansion.
    let scope = ContextScope::just(&ctx);
    let rows = client
        .match_aligned(
            None,
            Some(&was_born_in),
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
            true,
            0.5,
        )
        .await
        .unwrap();

    let expanded: Vec<_> = rows
        .iter()
        .filter(|r| r.matched_via == "exact_equivalent")
        .collect();
    assert_eq!(
        expanded.len(),
        1,
        "expected one exact_equivalent expansion, got {rows:?}"
    );
    let r = expanded[0];
    assert_eq!(r.statement.subject, format!("{prefix}/alice"));
    assert_eq!(r.statement.predicate, born_in);
}

#[tokio::test]
async fn inverse_equivalent_swaps_subject_and_object() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ma-inv").await;
    let prefix = tag("ma-inv");

    let born_in = format!("{prefix}/bornIn");
    let birthplace_of = format!("{prefix}/birthplaceOf");
    register_predicate(&client, &born_in).await;
    register_predicate(&client, &birthplace_of).await;

    // Assert (london, birthplaceOf, alice).
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/london"),
                &birthplace_of,
                Object::iri(format!("{prefix}/alice")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    client
        .register_alignment(
            &born_in,
            &birthplace_of,
            AlignmentRelation::InverseEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    rebuild_closure_with_retry(&client).await;

    // Query (alice, bornIn, ?) — must return swapped row from inverse.
    let scope = ContextScope::just(&ctx);
    let rows = client
        .match_aligned(
            Some(&format!("{prefix}/alice")),
            Some(&born_in),
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
            true,
            0.5,
        )
        .await
        .unwrap();

    let inv: Vec<_> = rows
        .iter()
        .filter(|r| r.matched_via == "inverse_equivalent")
        .collect();
    assert_eq!(
        inv.len(),
        1,
        "expected one inverse_equivalent match, got {rows:?}"
    );
    let r = inv[0];
    // After swap-back the caller sees (alice, ?, london).
    assert_eq!(r.statement.subject, format!("{prefix}/alice"));
    match &r.statement.object {
        Object::Iri(o) => assert_eq!(o, &format!("{prefix}/london")),
        Object::Literal(_) => panic!("expected IRI object after swap"),
    }
}

#[tokio::test]
async fn no_expansion_returns_only_direct() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ma-direct").await;
    let prefix = tag("ma-direct");

    let a = format!("{prefix}/a");
    let b = format!("{prefix}/b");
    register_predicate(&client, &a).await;
    register_predicate(&client, &b).await;

    client
        .assert(
            &StatementInput::new(format!("{prefix}/x"), &a, Object::iri(format!("{prefix}/y")))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    client
        .register_alignment(
            &a,
            &b,
            AlignmentRelation::ExactEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    rebuild_closure_with_retry(&client).await;

    // Query for b with expansion off — must NOT see a's assertion.
    let scope = ContextScope::just(&ctx);
    let rows = client
        .match_aligned(
            None,
            Some(&b),
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
            false, // expand off
            1.0,
        )
        .await
        .unwrap();
    assert!(
        rows.iter().all(|r| r.matched_via == "direct"),
        "expansion=false must only return direct rows, got {rows:?}"
    );
    // Specifically, no rows for predicate b (since the assertion was on a).
    assert_eq!(
        rows.len(),
        0,
        "expansion=false on b with no direct match must be empty"
    );
}

#[tokio::test]
async fn min_alignment_confidence_filters_low_confidence() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ma-conf").await;
    let prefix = tag("ma-conf");

    let a = format!("{prefix}/a");
    let b = format!("{prefix}/b");
    register_predicate(&client, &a).await;
    register_predicate(&client, &b).await;

    client
        .assert(
            &StatementInput::new(format!("{prefix}/x"), &a, Object::iri(format!("{prefix}/y")))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    // close_match at 0.85 — passes default 0.8 closure floor and 0.8 query
    // floor, but fails a 0.95 query floor.
    client
        .register_alignment(
            &a,
            &b,
            AlignmentRelation::CloseMatch,
            0.85,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    rebuild_closure_with_retry(&client).await;

    let scope = ContextScope::just(&ctx);

    // With low floor — expansion visible.
    let permissive = client
        .match_aligned(
            None,
            Some(&b),
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
            true,
            0.8,
        )
        .await
        .unwrap();
    let permissive_close: Vec<_> = permissive
        .iter()
        .filter(|r| r.matched_via == "close_match")
        .collect();
    assert_eq!(
        permissive_close.len(),
        1,
        "low floor must surface close_match"
    );

    // With high floor — expansion filtered out.
    let strict_floor = client
        .match_aligned(
            None,
            Some(&b),
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
            true,
            0.95,
        )
        .await
        .unwrap();
    let strict_close: Vec<_> = strict_floor
        .iter()
        .filter(|r| r.matched_via == "close_match")
        .collect();
    assert_eq!(
        strict_close.len(),
        0,
        "high floor must filter low-confidence close_match"
    );
}

#[tokio::test]
async fn match_strict_returns_only_direct() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ma-strict").await;
    let prefix = tag("ma-strict");

    let a = format!("{prefix}/a");
    let b = format!("{prefix}/b");
    register_predicate(&client, &a).await;
    register_predicate(&client, &b).await;

    client
        .assert(
            &StatementInput::new(format!("{prefix}/x"), &a, Object::iri(format!("{prefix}/y")))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    client
        .register_alignment(
            &a,
            &b,
            AlignmentRelation::ExactEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    rebuild_closure_with_retry(&client).await;

    // Query for b strictly — must NOT return the assertion on a.
    let scope = ContextScope::just(&ctx);
    let rows = client
        .match_strict(
            None,
            Some(&b),
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        0,
        "match_strict must not expand via the closure, got {rows:?}"
    );

    // But query for a strictly — must see the direct match.
    let direct = client
        .match_strict(
            None,
            Some(&a),
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(direct.len(), 1, "match_strict must return direct rows");
}
