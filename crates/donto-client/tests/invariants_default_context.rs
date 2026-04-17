//! Default-context invariants (PRD §3 principle 3, §7).
//!
//! "Every statement has a context. Default is `donto:anonymous`. The slot
//!  is never empty."
//!
//! A caller that doesn't set a context should not silently lose the row
//! nor fail. These tests exercise the default-binding path and prove the
//! anonymous context participates in scope resolution like any other.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};

const ANON: &str = "donto:anonymous";

#[tokio::test]
async fn default_context_is_donto_anonymous() {
    let c = pg_or_skip!(common::connect().await);

    // Build a statement without calling .with_context(). Uniqueness is
    // enforced by a subject+predicate prefix so we can clean up just our row.
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let subject = format!("ex:def_ctx:{tag}");
    let predicate = "ex:default";
    let input = StatementInput::new(subject.clone(), predicate, Object::iri("ex:yes"));
    assert_eq!(
        input.context, ANON,
        "StatementInput default is donto:anonymous"
    );

    c.assert(&input).await.unwrap();

    let rows = c
        .match_pattern(
            Some(&subject),
            Some(predicate),
            None,
            Some(&ContextScope::just(ANON)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "default-context row must be visible under anonymous"
    );
    assert_eq!(rows[0].context, ANON);

    // Cleanup.
    let conn = c.pool().get().await.unwrap();
    conn.execute(
        "delete from donto_statement where subject = $1 and predicate = $2 and context = $3",
        &[&subject, &predicate, &ANON],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn anonymous_context_exists_in_registry() {
    // The donto_context registry must carry a row for donto:anonymous after
    // migrations run — every insert assumes the FK is satisfiable.
    let c = pg_or_skip!(common::connect().await);
    let conn = c.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_context where iri = $1",
            &[&ANON],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1, "donto:anonymous must be registered at migration time");
}

#[tokio::test]
async fn scope_anywhere_includes_anonymous() {
    let c = pg_or_skip!(common::connect().await);

    let tag = uuid::Uuid::new_v4().simple().to_string();
    let subject = format!("ex:anon_scope:{tag}");
    let predicate = "ex:flag";

    c.assert(&StatementInput::new(
        &subject,
        predicate,
        Object::iri("ex:y"),
    ))
    .await
    .unwrap();

    let rows = c
        .match_pattern(
            Some(&subject),
            Some(predicate),
            None,
            Some(&ContextScope::anywhere()),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "anywhere scope must include donto:anonymous");

    let conn = c.pool().get().await.unwrap();
    conn.execute(
        "delete from donto_statement where subject = $1",
        &[&subject],
    )
    .await
    .unwrap();
}
