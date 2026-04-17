//! End-to-end: parse DontoQL/SPARQL → evaluate against live db → check rows.

use donto_client::{DontoClient, Object, Polarity, StatementInput};
use donto_query::{evaluate, parse_dontoql, parse_sparql, Term};

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn setup() -> Option<(DontoClient, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let prefix = format!("test:e2e_q:{}", uuid::Uuid::new_v4().simple());
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .ok()?;
    // Cleanup on entry just in case prefix collides.
    let conn = c.pool().get().await.ok()?;
    conn.execute(
        "delete from donto_statement where context like $1",
        &[&format!("{prefix}%")],
    )
    .await
    .ok();
    Some((c, ctx))
}

#[tokio::test]
async fn dontoql_basic_pattern_returns_rows() {
    let Some((c, ctx)) = setup().await else {
        eprintln!("skip");
        return;
    };

    c.assert(
        &StatementInput::new("ex:alice", "ex:knows", Object::iri("ex:bob")).with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new("ex:alice", "ex:knows", Object::iri("ex:carol")).with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(&StatementInput::new("ex:bob", "ex:knows", Object::iri("ex:dave")).with_context(&ctx))
        .await
        .unwrap();

    let mut q = parse_dontoql(
        r#"
        MATCH ?x ex:knows ?y
        POLARITY asserted
        PROJECT ?x, ?y
    "#,
    )
    .unwrap();
    // Inject scope to isolate the test from other contexts.
    q.scope = Some(donto_client::ContextScope::just(&ctx));

    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 3);
    for r in &rows {
        assert!(r.0.contains_key("x") && r.0.contains_key("y"));
    }
}

#[tokio::test]
async fn dontoql_join_on_shared_var() {
    let Some((c, ctx)) = setup().await else {
        eprintln!("skip");
        return;
    };
    c.assert(
        &StatementInput::new("ex:alice", "ex:knows", Object::iri("ex:bob")).with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new("ex:bob", "ex:knows", Object::iri("ex:carol")).with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new(
            "ex:carol",
            "ex:age",
            Object::lit(donto_client::Literal::integer(40)),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();

    let mut q = parse_dontoql(
        r#"
        MATCH ?a ex:knows ?b, ?b ex:knows ?c, ?c ex:age ?n
        PROJECT ?a, ?n
    "#,
    )
    .unwrap();
    q.scope = Some(donto_client::ContextScope::just(&ctx));
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0["a"], Term::Iri("ex:alice".into()));
}

#[tokio::test]
async fn sparql_basic_select_round_trips() {
    let Some((c, ctx)) = setup().await else {
        eprintln!("skip");
        return;
    };
    c.assert(
        &StatementInput::new(
            "http://example.org/alice",
            "http://example.org/knows",
            Object::iri("http://example.org/bob"),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();

    let mut q = parse_sparql(
        r#"
        PREFIX ex: <http://example.org/>
        SELECT ?x ?y WHERE { ?x ex:knows ?y . }
    "#,
    )
    .unwrap();
    q.scope = Some(donto_client::ContextScope::just(&ctx));
    q.polarity = Some(Polarity::Asserted);
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0["x"], Term::Iri("http://example.org/alice".into()));
    assert_eq!(rows[0].0["y"], Term::Iri("http://example.org/bob".into()));
}

#[tokio::test]
async fn dontoql_filter_neq_drops_rows() {
    let Some((c, ctx)) = setup().await else {
        eprintln!("skip");
        return;
    };
    c.assert(
        &StatementInput::new(
            "ex:a",
            "ex:name",
            Object::lit(donto_client::Literal::string("Alice")),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new(
            "ex:a",
            "ex:name",
            Object::lit(donto_client::Literal::string("Mallory")),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();

    let mut q = parse_dontoql(
        r#"
        MATCH ?s ex:name ?n
        FILTER ?n != "Mallory"
        PROJECT ?n
    "#,
    )
    .unwrap();
    q.scope = Some(donto_client::ContextScope::just(&ctx));
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn dontoql_limit_offset() {
    let Some((c, ctx)) = setup().await else {
        eprintln!("skip");
        return;
    };
    for i in 0..5 {
        c.assert(
            &StatementInput::new(format!("ex:s{i}"), "ex:p", Object::iri(format!("ex:o{i}")))
                .with_context(&ctx),
        )
        .await
        .unwrap();
    }
    let mut q = parse_dontoql(
        r#"
        MATCH ?s ex:p ?o
        PROJECT ?s
        LIMIT 2
        OFFSET 1
    "#,
    )
    .unwrap();
    q.scope = Some(donto_client::ContextScope::just(&ctx));
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 2);
}
