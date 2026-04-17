//! Context invariants (PRD §3 principle 2, §7).
//!
//! "Every statement has a context. Default `donto:anonymous` exists, but
//!  the slot is never empty."
//!
//! Plus: scope inheritance rules, exclude semantics, ancestor walks,
//! permissive vs curated mode enforcement.

mod common;

use donto_client::{ContextScope, Object, StatementInput};

#[tokio::test]
async fn default_context_exists_after_migrate() {
    let c = pg_or_skip!(common::connect().await);
    let conn = c.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_context where iri = 'donto:anonymous'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1, "donto:anonymous must exist post-migrate");
}

#[tokio::test]
async fn descendants_are_visible_from_root_scope() {
    let c = pg_or_skip!(common::connect().await);
    let prefix = common::tag("ctx_desc");
    let root = format!("{prefix}/root");
    let child = format!("{prefix}/child");
    let grand = format!("{prefix}/grand");
    c.ensure_context(&root, "custom", "permissive", None)
        .await
        .unwrap();
    c.ensure_context(&child, "custom", "permissive", Some(&root))
        .await
        .unwrap();
    c.ensure_context(&grand, "custom", "permissive", Some(&child))
        .await
        .unwrap();

    for ctx in [&root, &child, &grand] {
        c.assert(&StatementInput::new("ex:s", "ex:p", Object::iri(ctx)).with_context(ctx))
            .await
            .unwrap();
    }
    let n = c
        .match_pattern(
            Some("ex:s"),
            Some("ex:p"),
            None,
            Some(&ContextScope::just(&root)),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap()
        .len();
    assert_eq!(n, 3, "root scope must see itself, child, grandchild");
}

#[tokio::test]
async fn exclude_wins_over_descendant_inclusion() {
    let c = pg_or_skip!(common::connect().await);
    let prefix = common::tag("ctx_excl");
    let root = format!("{prefix}/root");
    let child = format!("{prefix}/child");
    let grand = format!("{prefix}/grand");
    c.ensure_context(&root, "custom", "permissive", None)
        .await
        .unwrap();
    c.ensure_context(&child, "custom", "permissive", Some(&root))
        .await
        .unwrap();
    c.ensure_context(&grand, "custom", "permissive", Some(&child))
        .await
        .unwrap();

    for ctx in [&root, &child, &grand] {
        c.assert(&StatementInput::new("ex:s", "ex:p", Object::iri(ctx)).with_context(ctx))
            .await
            .unwrap();
    }

    let scope = ContextScope::just(&root).excluding(&child);
    let rows = c
        .match_pattern(
            Some("ex:s"),
            Some("ex:p"),
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    // child excluded, but grand is still a descendant of root and not
    // excluded, so it should be visible. Per PRD §7 the exclude is a
    // membership filter not a subtree-prune.
    let objs: Vec<String> = rows
        .into_iter()
        .map(|r| match r.object {
            Object::Iri(s) => s,
            _ => unreachable!(),
        })
        .collect();
    assert!(objs.contains(&root), "root must be visible");
    assert!(!objs.contains(&child), "child must be excluded");
    assert!(
        objs.contains(&grand),
        "grandchild must be visible (excludes are per-context, not subtree prunes)"
    );
}

#[tokio::test]
async fn ancestor_walk_optional_off_by_default() {
    let c = pg_or_skip!(common::connect().await);
    let prefix = common::tag("ctx_anc");
    let root = format!("{prefix}/root");
    let child = format!("{prefix}/child");
    c.ensure_context(&root, "custom", "permissive", None)
        .await
        .unwrap();
    c.ensure_context(&child, "custom", "permissive", Some(&root))
        .await
        .unwrap();

    c.assert(&StatementInput::new("ex:s", "ex:p", Object::iri("ex:from_root")).with_context(&root))
        .await
        .unwrap();
    c.assert(
        &StatementInput::new("ex:s", "ex:p", Object::iri("ex:from_child")).with_context(&child),
    )
    .await
    .unwrap();

    // Default scope from child without descendants and without ancestors —
    // sees only child.
    let scope = ContextScope::just(&child).without_descendants();
    let n = c
        .match_pattern(
            Some("ex:s"),
            Some("ex:p"),
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap()
        .len();
    assert_eq!(n, 1);

    let with_anc = ContextScope::just(&child)
        .without_descendants()
        .with_ancestors();
    let n = c
        .match_pattern(
            Some("ex:s"),
            Some("ex:p"),
            None,
            Some(&with_anc),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap()
        .len();
    assert_eq!(n, 2, "ancestors must surface root once enabled");
}

#[tokio::test]
async fn no_assert_with_null_context() {
    // PRD §3 principle 2: "every statement has a context."
    let c = pg_or_skip!(common::connect().await);
    let conn = c.pool().get().await.unwrap();
    let r = conn
        .execute(
            "select donto_assert('ex:a','ex:p','ex:b',null,null,'asserted',0,null,null,null)",
            &[],
        )
        .await;
    assert!(r.is_err(), "asserting with null context must error");
}

#[tokio::test]
async fn curated_context_rejects_unregistered_predicate() {
    // PRD §4: curated contexts require registered predicates.
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::curated_ctx(&c, "ctx_curated_reject").await;

    // Asserting an unregistered predicate into a curated context must fail.
    let conn = c.pool().get().await.unwrap();
    let r = conn.execute(
        "select donto_assert('ex:a','ex:zzz_unregistered','ex:b',null,$1,'asserted',0,null,null,null)",
        &[&ctx.as_str()],
    ).await;
    assert!(
        r.is_err(),
        "curated context must reject unregistered predicates"
    );
}

#[tokio::test]
async fn curated_context_accepts_registered_predicate() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::curated_ctx(&c, "ctx_curated_accept").await;
    let conn = c.pool().get().await.unwrap();

    conn.execute(
        "select donto_register_predicate('ex:knownPredicate','known',null,null,null,null,null,null)",
        &[],
    ).await.unwrap();
    conn.execute(
        "select donto_assert('ex:a','ex:knownPredicate','ex:b',null,$1,'asserted',0,null,null,null)",
        &[&ctx.as_str()],
    ).await.expect("curated context must accept registered predicate");
}
