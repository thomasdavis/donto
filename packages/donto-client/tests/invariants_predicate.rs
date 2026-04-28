//! Predicate registry invariants (PRD §3 principle 4, §9).
//!
//! "Open-world predicates. The predicate space grows at runtime. Aliases
//!  and canonicals are first-class."
//!
//! Aliases are single-hop only (§9). Implicit registration on first use
//! in a permissive context. Curated contexts require active registration.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};

#[tokio::test]
async fn implicit_registration_in_permissive_context() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "pred_implicit").await;
    let unique = format!("ex:p_{}", uuid::Uuid::new_v4().simple());

    c.assert(&StatementInput::new("ex:s", &unique, Object::iri("ex:o")).with_context(&ctx))
        .await
        .unwrap();

    let conn = c.pool().get().await.unwrap();
    let row = conn
        .query_one(
            "select status from donto_predicate where iri = $1",
            &[&unique.as_str()],
        )
        .await
        .unwrap();
    let status: String = row.get(0);
    assert_eq!(
        status, "implicit",
        "first-use must record predicate with status='implicit'"
    );
}

#[tokio::test]
async fn explicit_registration_promotes_implicit_to_active() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "pred_promote").await;
    let pred = format!("ex:p_{}", uuid::Uuid::new_v4().simple());

    // First use: implicit.
    c.assert(&StatementInput::new("ex:s", &pred, Object::iri("ex:o")).with_context(&ctx))
        .await
        .unwrap();

    // Then formally register.
    let conn = c.pool().get().await.unwrap();
    conn.execute(
        "select donto_register_predicate($1, 'my pred', null, null, null, null, null, null)",
        &[&pred.as_str()],
    )
    .await
    .unwrap();

    let row = conn
        .query_one(
            "select status, label from donto_predicate where iri = $1",
            &[&pred.as_str()],
        )
        .await
        .unwrap();
    let status: String = row.get(0);
    let label: Option<String> = row.get(1);
    assert_eq!(status, "active");
    assert_eq!(label.as_deref(), Some("my pred"));
}

#[tokio::test]
async fn alias_chain_rejected() {
    // PRD §9: alias chains forbidden. Cannot point one alias at another alias.
    let c = pg_or_skip!(common::connect().await);
    let conn = c.pool().get().await.unwrap();

    let canon = format!("ex:canon_{}", uuid::Uuid::new_v4().simple());
    let alias_a = format!("{canon}_a");
    let alias_b = format!("{canon}_b");

    conn.execute(
        "select donto_register_predicate($1,null,null,null,null,null,null,null)",
        &[&canon.as_str()],
    )
    .await
    .unwrap();
    conn.execute(
        "select donto_register_predicate($1,null,null,$2,null,null,null,null)",
        &[&alias_a.as_str(), &canon.as_str()],
    )
    .await
    .unwrap();
    let r = conn
        .execute(
            "select donto_register_predicate($1,null,null,$2,null,null,null,null)",
            &[&alias_b.as_str(), &alias_a.as_str()],
        )
        .await;
    assert!(r.is_err(), "registering an alias of an alias must error");
}

#[tokio::test]
async fn canonical_lookup_resolves_alias() {
    let c = pg_or_skip!(common::connect().await);
    let conn = c.pool().get().await.unwrap();

    let canon = format!("ex:canon_{}", uuid::Uuid::new_v4().simple());
    let alias = format!("{canon}_alias");
    conn.execute(
        "select donto_register_predicate($1,null,null,null,null,null,null,null)",
        &[&canon.as_str()],
    )
    .await
    .unwrap();
    conn.execute(
        "select donto_register_predicate($1,null,null,$2,null,null,null,null)",
        &[&alias.as_str(), &canon.as_str()],
    )
    .await
    .unwrap();

    let r1: String = conn
        .query_one("select donto_canonical_predicate($1)", &[&alias.as_str()])
        .await
        .unwrap()
        .get(0);
    assert_eq!(r1, canon, "alias must resolve to canonical");

    let r2: String = conn
        .query_one("select donto_canonical_predicate($1)", &[&canon.as_str()])
        .await
        .unwrap()
        .get(0);
    assert_eq!(r2, canon, "canonical resolves to itself");

    let unknown = format!("ex:never_seen_{}", uuid::Uuid::new_v4().simple());
    let r3: String = conn
        .query_one("select donto_canonical_predicate($1)", &[&unknown.as_str()])
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        r3, unknown,
        "unregistered predicate is its own canonical (open-world)"
    );
}

#[tokio::test]
async fn match_canonical_unifies_alias_siblings() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "pred_unify").await;
    let conn = c.pool().get().await.unwrap();

    let canon = format!("ex:rel_{}", uuid::Uuid::new_v4().simple());
    let alias_a = format!("{canon}_a");
    let alias_b = format!("{canon}_b");

    conn.execute(
        "select donto_register_predicate($1,null,null,null,null,null,null,null)",
        &[&canon.as_str()],
    )
    .await
    .unwrap();
    conn.execute(
        "select donto_register_predicate($1,null,null,$2,null,null,null,null)",
        &[&alias_a.as_str(), &canon.as_str()],
    )
    .await
    .unwrap();
    conn.execute(
        "select donto_register_predicate($1,null,null,$2,null,null,null,null)",
        &[&alias_b.as_str(), &canon.as_str()],
    )
    .await
    .unwrap();

    // Insert one statement under each form.
    for p in [&canon, &alias_a, &alias_b] {
        c.assert(&StatementInput::new("ex:s", p, Object::iri("ex:o")).with_context(&ctx))
            .await
            .unwrap();
    }

    // donto_match (raw) returns only the asked predicate.
    let n = c
        .match_pattern(
            Some("ex:s"),
            Some(&canon),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap()
        .len();
    assert_eq!(n, 1, "raw match returns the literal predicate only");

    // donto_match_canonical returns the alias siblings too.
    let scope_json = serde_json::json!({"include":[ctx.clone()]});
    let n: i64 = conn.query_one(
        "select count(*) from donto_match_canonical('ex:s', $1, null, $2::jsonb, 'asserted', 0)",
        &[&canon.as_str(), &scope_json],
    ).await.unwrap().get(0);
    assert_eq!(n, 3, "canonical match must surface all alias siblings");
}
