//! Paraconsistency invariants (PRD §3 principle 1, §6).
//!
//! "The store holds mutually inconsistent statements. Consistency is a
//! query, not a constraint."
//!
//! These tests prove donto never refuses or coerces contradictions; that
//! contradictory statements coexist under any number of contexts; and that
//! the polarity table behaves exactly as specified.

mod common;

use donto_client::{ContextScope, Literal, Object, Polarity, StatementInput};

#[tokio::test]
async fn three_way_disagreement_about_birth_year_coexists() {
    let c = pg_or_skip!(common::connect().await);
    let tag = common::tag("para_3way");
    let (s_a, s_b, s_c) = (
        format!("{tag}/srcA"), format!("{tag}/srcB"), format!("{tag}/srcC"),
    );
    for s in [&s_a, &s_b, &s_c] {
        c.ensure_context(s, "source", "permissive", None).await.unwrap();
    }

    let claims: [(&str, i64); 3] = [(&s_a, 1899), (&s_b, 1900), (&s_c, 1925)];
    for (ctx, year) in claims {
        c.assert(&StatementInput::new(
            "ex:alice", "ex:birthYear", Object::lit(Literal::integer(year)))
            .with_context(ctx)).await.unwrap();
    }

    // Joint scope — all three visible, donto does not pick a winner.
    let mut sc = ContextScope::just(&s_a);
    sc.include.extend([s_b.clone(), s_c.clone()]);
    let rows = c.match_pattern(
        Some("ex:alice"), Some("ex:birthYear"), None,
        Some(&sc), Some(Polarity::Asserted), 0, None, None,
    ).await.unwrap();
    assert_eq!(rows.len(), 3, "three contradictory statements must all surface");

    // Per-source scope — each source sees only its own.
    for (ctx, _) in claims {
        let one = c.match_pattern(
            Some("ex:alice"), Some("ex:birthYear"), None,
            Some(&ContextScope::just(ctx)),
            Some(Polarity::Asserted), 0, None, None,
        ).await.unwrap();
        assert_eq!(one.len(), 1, "scope {ctx} must isolate its claim");
    }
}

#[tokio::test]
async fn assert_and_negate_in_same_context_both_persist() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "para_assert_negate").await;

    c.assert(&StatementInput::new("ex:alice","ex:member",Object::iri("ex:club"))
        .with_context(&ctx)).await.unwrap();
    c.assert(&StatementInput::new("ex:alice","ex:member",Object::iri("ex:club"))
        .with_context(&ctx).with_polarity(Polarity::Negated)).await.unwrap();

    let scope = ContextScope::just(&ctx);
    // Default polarity 'asserted' returns the positive only.
    let pos = c.match_pattern(Some("ex:alice"), Some("ex:member"), None,
        Some(&scope), Some(Polarity::Asserted), 0, None, None).await.unwrap();
    assert_eq!(pos.len(), 1);

    // Polarity = negated returns the negative only.
    let neg = c.match_pattern(Some("ex:alice"), Some("ex:member"), None,
        Some(&scope), Some(Polarity::Negated), 0, None, None).await.unwrap();
    assert_eq!(neg.len(), 1);

    // No polarity filter (None) returns both — the paraconsistent reality.
    let all = c.match_pattern(Some("ex:alice"), Some("ex:member"), None,
        Some(&scope), None, 0, None, None).await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn polarity_table_invariants_exhaustive() {
    // Per PRD §6 query-form table: default queries match `asserted` only.
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "para_polarity_table").await;

    for pol in [Polarity::Asserted, Polarity::Negated, Polarity::Absent, Polarity::Unknown] {
        c.assert(&StatementInput::new(
            format!("ex:s_{}", pol.as_str()), "ex:p", Object::iri("ex:o"),
        ).with_context(&ctx).with_polarity(pol)).await.unwrap();
    }

    let scope = ContextScope::just(&ctx);

    // Default — asserted only.
    let n_default = c.match_pattern(None, Some("ex:p"), None,
        Some(&scope), Some(Polarity::Asserted), 0, None, None).await.unwrap().len();
    assert_eq!(n_default, 1);

    // Each polarity returns its single instance.
    for pol in [Polarity::Asserted, Polarity::Negated, Polarity::Absent, Polarity::Unknown] {
        let n = c.match_pattern(None, Some("ex:p"), None,
            Some(&scope), Some(pol), 0, None, None).await.unwrap().len();
        assert_eq!(n, 1, "expected 1 row for polarity {}", pol.as_str());
    }

    // Total across polarities = 4 (any-of via None).
    let n_any = c.match_pattern(None, Some("ex:p"), None,
        Some(&scope), None, 0, None, None).await.unwrap().len();
    assert_eq!(n_any, 4);
}

#[tokio::test]
async fn idempotent_re_assert_does_not_double_count() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "para_idem").await;

    let s = StatementInput::new("ex:alice","ex:knows",Object::iri("ex:bob"))
        .with_context(&ctx);

    let mut ids = std::collections::HashSet::new();
    for _ in 0..50 { ids.insert(c.assert(&s).await.unwrap()); }
    assert_eq!(ids.len(), 1, "50 identical asserts must collapse to one statement_id");

    let n = c.match_pattern(Some("ex:alice"), Some("ex:knows"), None,
        Some(&ContextScope::just(&ctx)), Some(Polarity::Asserted), 0, None, None).await.unwrap().len();
    assert_eq!(n, 1);
}
