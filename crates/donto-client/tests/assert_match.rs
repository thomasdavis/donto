//! Tests for assert + match round-trip and idempotency.

mod common;

use donto_client::{ContextScope, Literal, Object, Polarity, StatementInput};

#[tokio::test]
async fn assert_then_match() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = "test:assert_match:";
    common::cleanup_prefix(&client, prefix).await;

    let ctx = format!("{prefix}ctx");
    client
        .ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    let s = StatementInput::new("ex:alice", "ex:knows", Object::iri("ex:bob")).with_context(&ctx);

    let id1 = client.assert(&s).await.unwrap();
    let id2 = client.assert(&s).await.unwrap();
    assert_eq!(id1, id2, "idempotent re-assert");

    let scope = ContextScope::just(&ctx);
    let rows = client
        .match_pattern(
            Some("ex:alice"),
            Some("ex:knows"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].subject, "ex:alice");
    assert_eq!(rows[0].predicate, "ex:knows");
    assert_eq!(rows[0].object, Object::Iri("ex:bob".into()));
    assert_eq!(rows[0].polarity, Polarity::Asserted);
    assert_eq!(rows[0].context, ctx);
}

#[tokio::test]
async fn literal_object_round_trips() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = "test:literal:";
    common::cleanup_prefix(&client, prefix).await;
    let ctx = format!("{prefix}ctx");
    client
        .ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    let s = StatementInput::new("ex:alice", "ex:age", Object::lit(Literal::integer(36)))
        .with_context(&ctx);
    client.assert(&s).await.unwrap();

    let scope = ContextScope::just(&ctx);
    let rows = client
        .match_pattern(
            Some("ex:alice"),
            Some("ex:age"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    match &rows[0].object {
        Object::Literal(l) => {
            assert_eq!(l.dt, "xsd:integer");
            assert_eq!(l.v, serde_json::json!(36));
        }
        other => panic!("expected literal, got {:?}", other),
    }
}

#[tokio::test]
async fn batch_assert_counts() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = "test:batch:";
    common::cleanup_prefix(&client, prefix).await;
    let ctx = format!("{prefix}ctx");
    client
        .ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    let inputs: Vec<StatementInput> = (0..50)
        .map(|i| {
            StatementInput::new(
                format!("ex:s/{i}"),
                "ex:p",
                Object::iri(format!("ex:o/{i}")),
            )
            .with_context(&ctx)
        })
        .collect();

    let n = client.assert_batch(&inputs).await.unwrap();
    assert_eq!(n, 50);

    let rows = client
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
    assert_eq!(rows.len(), 50);
}
