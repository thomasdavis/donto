//! Maturity ladder invariants (PRD §2).
//!
//! "Every statement in donto sits at one of five maturity levels.
//!  Levels are recorded per statement per context, not globally."
//!
//! These tests pin the encoding (5 valid levels, packed into flags),
//! prove the same statement can carry different maturity in different
//! contexts, and prove the `min_maturity` filter works.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};

#[tokio::test]
async fn maturity_levels_0_through_4_round_trip() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "mat_round_trip").await;

    for level in 0u8..=4 {
        c.assert(&StatementInput::new(
            format!("ex:s/{level}"), "ex:p", Object::iri("ex:o"),
        ).with_context(&ctx).with_maturity(level)).await.unwrap();
    }

    let scope = ContextScope::just(&ctx);
    for level in 0u8..=4 {
        let rows = c.match_pattern(
            Some(&format!("ex:s/{level}")), Some("ex:p"), None,
            Some(&scope), Some(Polarity::Asserted), 0, None, None,
        ).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].maturity, level);
    }
}

#[tokio::test]
async fn min_maturity_filter_rejects_lower() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "mat_filter").await;

    for level in 0u8..=4 {
        c.assert(&StatementInput::new(
            format!("ex:s/{level}"), "ex:p", Object::iri("ex:o"),
        ).with_context(&ctx).with_maturity(level)).await.unwrap();
    }

    let scope = ContextScope::just(&ctx);
    for floor in 0u8..=4 {
        let n = c.match_pattern(None, Some("ex:p"), None,
            Some(&scope), Some(Polarity::Asserted), floor, None, None,
        ).await.unwrap().len();
        let expected = (5u8 - floor) as usize;
        assert_eq!(n, expected,
            "min_maturity={floor} must return {expected} rows, got {n}");
    }
}

#[tokio::test]
async fn same_subject_different_maturity_in_different_contexts() {
    let c = pg_or_skip!(common::connect().await);
    let prefix = common::tag("mat_per_ctx");
    let raw    = format!("{prefix}/raw");
    let curated = format!("{prefix}/curated");
    c.ensure_context(&raw,     "source",   "permissive", None).await.unwrap();
    c.ensure_context(&curated, "snapshot", "curated",    None).await.unwrap();

    // Same content, different maturity per context. Curated context requires
    // the predicate be registered.
    let conn = c.pool().get().await.unwrap();
    conn.execute(
        "select donto_register_predicate('ex:fact', 'fact', null, null, null, null, null, null)",
        &[],
    ).await.unwrap();

    c.assert(&StatementInput::new("ex:e","ex:fact",Object::iri("ex:v"))
        .with_context(&raw).with_maturity(0)).await.unwrap();
    c.assert(&StatementInput::new("ex:e","ex:fact",Object::iri("ex:v"))
        .with_context(&curated).with_maturity(3)).await.unwrap();

    let mut sc = ContextScope::just(&raw);
    sc.include.push(curated.clone());
    let rows = c.match_pattern(Some("ex:e"), Some("ex:fact"), None,
        Some(&sc), Some(Polarity::Asserted), 0, None, None).await.unwrap();
    assert_eq!(rows.len(), 2);

    let by_ctx: std::collections::HashMap<&str, u8> = rows.iter()
        .map(|r| (r.context.as_str(), r.maturity)).collect();
    assert_eq!(by_ctx[raw.as_str()], 0);
    assert_eq!(by_ctx[curated.as_str()], 3);
}

#[tokio::test]
async fn flag_packing_is_dense_and_lossless() {
    // Hammer the packer with every legal (polarity, maturity) combination.
    let c = pg_or_skip!(common::connect().await);
    let conn = c.pool().get().await.unwrap();

    for pol in ["asserted","negated","absent","unknown"] {
        for mat in 0i32..=4 {
            let f: i16 = conn.query_one(
                "select donto_pack_flags($1, $2)",
                &[&pol, &mat],
            ).await.unwrap().get(0);
            let polarity_back: String = conn.query_one(
                "select donto_polarity($1)", &[&f],
            ).await.unwrap().get(0);
            let maturity_back: i32 = conn.query_one(
                "select donto_maturity($1)", &[&f],
            ).await.unwrap().get(0);
            assert_eq!(polarity_back, pol);
            assert_eq!(maturity_back, mat);
        }
    }
}
