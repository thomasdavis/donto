//! Deep scope-forest resolution (PRD §7).
//!
//! Scopes include/exclude sets plus descendant/ancestor flags. Previous
//! tests cover one- and two-level trees; this one drives a five-level
//! chain and checks that resolution correctly walks the full depth.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};

#[tokio::test]
async fn descendant_resolution_walks_five_levels() {
    let c = pg_or_skip!(common::connect().await);

    let tag = uuid::Uuid::new_v4().simple().to_string();
    let root = format!("ctx:deep:{tag}/L0");
    let names: Vec<String> = (0..5).map(|i| format!("ctx:deep:{tag}/L{i}")).collect();

    // Register the chain L0 ← L1 ← L2 ← L3 ← L4.
    c.ensure_context(&names[0], "custom", "permissive", None)
        .await
        .unwrap();
    for i in 1..5 {
        c.ensure_context(&names[i], "custom", "permissive", Some(&names[i - 1]))
            .await
            .unwrap();
    }

    // Assert a distinct statement at each depth.
    for (i, ctx) in names.iter().enumerate() {
        c.assert(
            &StatementInput::new(
                format!("ex:depth{i}"),
                "ex:level",
                Object::iri(format!("ex:{i}")),
            )
            .with_context(ctx),
        )
        .await
        .unwrap();
    }

    // Scope at L0 with descendants must see all five rows.
    let rows = c
        .match_pattern(
            None,
            Some("ex:level"),
            None,
            Some(&ContextScope::just(&root)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 5, "descendant scope must reach every level");

    // Scope at L0 WITHOUT descendants must see only L0.
    let rows = c
        .match_pattern(
            None,
            Some("ex:level"),
            None,
            Some(&ContextScope::just(&root).without_descendants()),
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
        "no-descendants scope must see only the anchor"
    );
    assert_eq!(rows[0].context, names[0]);

    // Scope at L4 with ancestors must see all five rows.
    let leaf = names.last().unwrap();
    let rows = c
        .match_pattern(
            None,
            Some("ex:level"),
            None,
            Some(&ContextScope::just(leaf).with_ancestors()),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 5, "ancestor scope at leaf must reach root");
}

#[tokio::test]
async fn exclude_is_exact_not_transitive() {
    // Per `donto_resolve_scope` (sql/migrations/0003_functions.sql) the
    // `exclude` list matches contexts exactly: excluding A hides A itself
    // but A's descendants (A1, A2) stay visible under the include-with-
    // descendants scope at the root. This test locks in that contract so
    // future migrations don't silently change it.
    //
    // Tree:
    //   R ← A, R ← B
    //   A ← A1, A ← A2
    //   B ← B1
    // Scope: include=R (with descendants), exclude=A.
    // Visible: R, A1, A2, B, B1.  Hidden: A.
    let c = pg_or_skip!(common::connect().await);

    let tag = uuid::Uuid::new_v4().simple().to_string();
    let r = format!("ctx:excl:{tag}/R");
    let a = format!("ctx:excl:{tag}/A");
    let a1 = format!("ctx:excl:{tag}/A1");
    let a2 = format!("ctx:excl:{tag}/A2");
    let b = format!("ctx:excl:{tag}/B");
    let b1 = format!("ctx:excl:{tag}/B1");

    c.ensure_context(&r, "custom", "permissive", None)
        .await
        .unwrap();
    c.ensure_context(&a, "custom", "permissive", Some(&r))
        .await
        .unwrap();
    c.ensure_context(&a1, "custom", "permissive", Some(&a))
        .await
        .unwrap();
    c.ensure_context(&a2, "custom", "permissive", Some(&a))
        .await
        .unwrap();
    c.ensure_context(&b, "custom", "permissive", Some(&r))
        .await
        .unwrap();
    c.ensure_context(&b1, "custom", "permissive", Some(&b))
        .await
        .unwrap();

    for ctx in [&r, &a, &a1, &a2, &b, &b1] {
        c.assert(
            &StatementInput::new("ex:node", "ex:in", Object::iri(ctx.as_str()))
                .with_context(ctx.as_str()),
        )
        .await
        .unwrap();
    }

    let scope = ContextScope::just(&r).excluding(&a);
    let rows = c
        .match_pattern(
            Some("ex:node"),
            Some("ex:in"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();

    let seen: std::collections::HashSet<String> = rows.iter().map(|r| r.context.clone()).collect();
    assert!(seen.contains(&r), "root must be visible");
    assert!(seen.contains(&b), "sibling subtree root must be visible");
    assert!(
        seen.contains(&b1),
        "sibling subtree descendant must be visible"
    );
    assert!(
        !seen.contains(&a),
        "excluded subtree root (exact-match exclude) must be hidden"
    );
    assert!(
        seen.contains(&a1),
        "descendant of excluded context stays visible — exclude is not transitive"
    );
    assert!(
        seen.contains(&a2),
        "descendant of excluded context stays visible — exclude is not transitive"
    );
}
