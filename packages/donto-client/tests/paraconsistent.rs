//! Paraconsistency: contradictory statements coexist in shared scope without
//! erroring; default reads return both. Negated/absent are hidden by default
//! and surfaced only on opt-in. Per PRD §3 principle 1 and §6 truth table.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};

#[tokio::test]
async fn contradictions_coexist() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = "test:para:";
    common::cleanup_prefix(&client, prefix).await;
    let src_a = format!("{prefix}srcA");
    let src_b = format!("{prefix}srcB");
    client
        .ensure_context(&src_a, "source", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&src_b, "source", "permissive", None)
        .await
        .unwrap();

    client
        .assert(
            &StatementInput::new(
                "ex:alice",
                "ex:birthYear",
                Object::lit(donto_client::Literal::integer(1899)),
            )
            .with_context(&src_a),
        )
        .await
        .unwrap();
    client
        .assert(
            &StatementInput::new(
                "ex:alice",
                "ex:birthYear",
                Object::lit(donto_client::Literal::integer(1925)),
            )
            .with_context(&src_b),
        )
        .await
        .unwrap();

    // Joint scope: both visible, no error.
    let mut scope = ContextScope::just(&src_a);
    scope.include.push(src_b.clone());
    let rows = client
        .match_pattern(
            Some("ex:alice"),
            Some("ex:birthYear"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "both contradictory statements coexist");
}

#[tokio::test]
async fn negated_hidden_by_default() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = "test:neg:";
    common::cleanup_prefix(&client, prefix).await;
    let ctx = format!("{prefix}ctx");
    client
        .ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    client
        .assert(
            &StatementInput::new("ex:alice", "ex:member", Object::iri("ex:club"))
                .with_context(&ctx)
                .with_polarity(Polarity::Negated),
        )
        .await
        .unwrap();

    let scope = ContextScope::just(&ctx);

    // Default polarity = asserted → hidden.
    let rows = client
        .match_pattern(
            Some("ex:alice"),
            Some("ex:member"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(
        rows.is_empty(),
        "negated statement must be hidden by default"
    );

    // Opt-in: see it.
    let rows = client
        .match_pattern(
            Some("ex:alice"),
            Some("ex:member"),
            None,
            Some(&scope),
            Some(Polarity::Negated),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}
