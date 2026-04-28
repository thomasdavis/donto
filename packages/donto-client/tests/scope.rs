//! Scope inheritance: include_descendants, include_ancestors, exclude.
//! Per PRD §7.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};

#[tokio::test]
async fn descendants_are_included_by_default() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = "test:scope_desc:";
    common::cleanup_prefix(&client, prefix).await;

    let root = format!("{prefix}root");
    let child = format!("{prefix}child");
    let grand = format!("{prefix}grand");
    client
        .ensure_context(&root, "custom", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&child, "custom", "permissive", Some(&root))
        .await
        .unwrap();
    client
        .ensure_context(&grand, "custom", "permissive", Some(&child))
        .await
        .unwrap();

    client
        .assert(&StatementInput::new("ex:s", "ex:p", Object::iri("ex:root")).with_context(&root))
        .await
        .unwrap();
    client
        .assert(&StatementInput::new("ex:s", "ex:p", Object::iri("ex:child")).with_context(&child))
        .await
        .unwrap();
    client
        .assert(&StatementInput::new("ex:s", "ex:p", Object::iri("ex:grand")).with_context(&grand))
        .await
        .unwrap();

    let scope = ContextScope::just(&root);
    let rows = client
        .match_pattern(
            Some("ex:s"),
            Some("ex:p"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 3, "descendants must be visible");
}

#[tokio::test]
async fn exclude_wins_over_include() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = "test:scope_excl:";
    common::cleanup_prefix(&client, prefix).await;

    let root = format!("{prefix}root");
    let child = format!("{prefix}child");
    client
        .ensure_context(&root, "custom", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&child, "custom", "permissive", Some(&root))
        .await
        .unwrap();

    client
        .assert(&StatementInput::new("ex:s", "ex:p", Object::iri("ex:root")).with_context(&root))
        .await
        .unwrap();
    client
        .assert(&StatementInput::new("ex:s", "ex:p", Object::iri("ex:child")).with_context(&child))
        .await
        .unwrap();

    let scope = ContextScope::just(&root).excluding(&child);
    let rows = client
        .match_pattern(
            Some("ex:s"),
            Some("ex:p"),
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
    assert_eq!(rows[0].object, Object::Iri("ex:root".into()));
}

#[tokio::test]
async fn ancestors_opt_in() {
    let client = pg_or_skip!(common::connect().await);
    let prefix = "test:scope_anc:";
    common::cleanup_prefix(&client, prefix).await;

    let root = format!("{prefix}root");
    let child = format!("{prefix}child");
    client
        .ensure_context(&root, "custom", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&child, "custom", "permissive", Some(&root))
        .await
        .unwrap();

    client
        .assert(&StatementInput::new("ex:s", "ex:p", Object::iri("ex:root")).with_context(&root))
        .await
        .unwrap();
    client
        .assert(&StatementInput::new("ex:s", "ex:p", Object::iri("ex:child")).with_context(&child))
        .await
        .unwrap();

    // Default: child only — descendants from `child` is empty.
    let scope = ContextScope::just(&child).without_descendants();
    let rows = client
        .match_pattern(
            Some("ex:s"),
            Some("ex:p"),
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
    assert_eq!(rows[0].object, Object::Iri("ex:child".into()));

    // With ancestors: see root too.
    let scope = ContextScope::just(&child)
        .without_descendants()
        .with_ancestors();
    let rows = client
        .match_pattern(
            Some("ex:s"),
            Some("ex:p"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
}
