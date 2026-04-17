//! Snapshot determinism + hypothesis scoping (PRD §8 snapshots, §20 hypotheses).
//!
//! Snapshots capture a frozen membership set; subsequent retractions don't
//! affect what the snapshot shows. Hypothesis contexts let counterfactuals
//! be queried without polluting the curated view.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};
use serde_json::json;

#[tokio::test]
async fn snapshot_membership_frozen_under_retraction() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "snap_freeze").await;
    let snap_iri = format!("ctx:snap/{}", uuid::Uuid::new_v4().simple());

    let id = c.assert(&StatementInput::new("ex:a","ex:p",Object::iri("ex:b"))
        .with_context(&ctx)).await.unwrap();

    let conn = c.pool().get().await.unwrap();
    conn.execute(
        "select donto_snapshot_create($1, $2::jsonb, 'test')",
        &[&snap_iri.as_str(), &json!({"include":[ctx.clone()]})],
    ).await.unwrap();

    // Retract — current view loses the row, snapshot keeps it.
    c.retract(id).await.unwrap();

    let live = c.match_pattern(Some("ex:a"), Some("ex:p"), None,
        Some(&ContextScope::just(&ctx)), Some(Polarity::Asserted), 0, None, None,
    ).await.unwrap().len();
    assert_eq!(live, 0, "live view loses row after retraction");

    let n: i64 = conn.query_one(
        "select count(*) from donto_match_in_snapshot($1, 'ex:a', 'ex:p', null, 'asserted', 0)",
        &[&snap_iri.as_str()],
    ).await.unwrap().get(0);
    assert_eq!(n, 1, "snapshot must keep the row visible");
}

#[tokio::test]
async fn snapshot_member_count_matches_membership() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "snap_count").await;
    let snap_iri = format!("ctx:snap/{}", uuid::Uuid::new_v4().simple());

    for i in 0..7 {
        c.assert(&StatementInput::new(
            format!("ex:s/{i}"), "ex:p", Object::iri(format!("ex:o/{i}"))
        ).with_context(&ctx)).await.unwrap();
    }
    let conn = c.pool().get().await.unwrap();
    conn.execute("select donto_snapshot_create($1, $2::jsonb, null)",
        &[&snap_iri.as_str(), &json!({"include":[ctx.clone()]})]
    ).await.unwrap();

    let stored: i32 = conn.query_one(
        "select member_count from donto_snapshot where iri = $1",
        &[&snap_iri.as_str()],
    ).await.unwrap().get(0);
    assert_eq!(stored, 7, "member_count must match captured rows");
}

#[tokio::test]
async fn hypothesis_context_isolates_counterfactuals() {
    let c = pg_or_skip!(common::connect().await);
    let curated = common::tag("hyp_curated");
    let curated_root = format!("{curated}/curated");
    let hypo_ctx = format!("{curated}/hypo");

    c.ensure_context(&curated_root, "custom",     "permissive", None).await.unwrap();
    c.ensure_context(&hypo_ctx,     "hypothesis", "permissive", Some(&curated_root)).await.unwrap();

    // Curated fact: alice and alice_2 are different IRIs.
    c.assert(&StatementInput::new("ex:alice", "ex:type", Object::iri("ex:Person"))
        .with_context(&curated_root)).await.unwrap();
    c.assert(&StatementInput::new("ex:alice_2", "ex:type", Object::iri("ex:Person"))
        .with_context(&curated_root)).await.unwrap();

    // Counterfactual: under this hypothesis they are the same person.
    c.assert(&StatementInput::new("ex:alice", "donto:sameAs", Object::iri("ex:alice_2"))
        .with_context(&hypo_ctx)).await.unwrap();

    // Curated-only scope: no sameAs.
    let n_curated = c.match_pattern(None, Some("donto:sameAs"), None,
        Some(&ContextScope::just(&curated_root).excluding(&hypo_ctx)),
        Some(Polarity::Asserted), 0, None, None).await.unwrap().len();
    assert_eq!(n_curated, 0, "curated view must not see hypothesis assertions");

    // Hypothesis scope: sees both curated facts (via ancestors) and the sameAs.
    let mut sc = ContextScope::just(&hypo_ctx);
    sc.include_ancestors = true;
    let any_pred = c.match_pattern(None, Some("donto:sameAs"), None,
        Some(&sc), Some(Polarity::Asserted), 0, None, None).await.unwrap();
    assert_eq!(any_pred.len(), 1, "hypothesis scope must see the sameAs assertion");
}

#[tokio::test]
async fn sameas_is_non_monotonic() {
    // PRD §10: donto:sameAs is context-scoped and non-monotonic.
    // Adding a context that excludes the assertion withdraws the identity.
    let c = pg_or_skip!(common::connect().await);
    let prefix = common::tag("sameas_nonmono");
    let ctx_with    = format!("{prefix}/with");
    let ctx_without = format!("{prefix}/without");
    c.ensure_context(&ctx_with,    "custom", "permissive", None).await.unwrap();
    c.ensure_context(&ctx_without, "custom", "permissive", None).await.unwrap();

    c.assert(&StatementInput::new("ex:alice","donto:sameAs",Object::iri("ex:alice_2"))
        .with_context(&ctx_with)).await.unwrap();

    // With the context: visible.
    let n = c.match_pattern(None, Some("donto:sameAs"), None,
        Some(&ContextScope::just(&ctx_with)), Some(Polarity::Asserted), 0, None, None,
    ).await.unwrap().len();
    assert_eq!(n, 1);

    // Without the context: gone.
    let n = c.match_pattern(None, Some("donto:sameAs"), None,
        Some(&ContextScope::just(&ctx_without)), Some(Polarity::Asserted), 0, None, None,
    ).await.unwrap().len();
    assert_eq!(n, 0, "sameAs withdrawn when its context is not in scope");
}
