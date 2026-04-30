//! Materialized predicate closure index invariants (migration 0051).
//!
//! `donto_rebuild_predicate_closure` flattens `donto_predicate_alignment` into
//! a (predicate_iri, equivalent_iri, relation, swap_direction, confidence) row
//! per match. exact_equivalent / inverse_equivalent / close_match are
//! bidirectional; sub_property_of is upward only. Self-identity rows exist for
//! every active or implicit predicate.

mod common;

use common::{connect, rebuild_closure_with_retry, tag};
use donto_client::AlignmentRelation;

async fn register_predicate(client: &donto_client::DontoClient, iri: &str) {
    let c = client.pool().get().await.unwrap();
    c.execute(
        "select donto_register_predicate($1, null, null, null, null, null, null, null)",
        &[&iri],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn exact_equivalent_appears_bidirectionally() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pc-exact");

    let a = format!("{prefix}/bornIn");
    let b = format!("{prefix}/wasBornIn");
    register_predicate(&client, &a).await;
    register_predicate(&client, &b).await;

    client
        .register_alignment(
            &a,
            &b,
            AlignmentRelation::ExactEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    rebuild_closure_with_retry(&client).await;

    let c = client.pool().get().await.unwrap();
    // a → b
    let n_ab: i64 = c.query_one(
        "select count(*) from donto_predicate_closure \
         where predicate_iri = $1 and equivalent_iri = $2 \
           and relation = 'exact_equivalent' and not swap_direction",
        &[&a, &b],
    ).await.unwrap().get(0);
    assert_eq!(n_ab, 1, "exact_equivalent must propagate a → b");

    // b → a
    let n_ba: i64 = c.query_one(
        "select count(*) from donto_predicate_closure \
         where predicate_iri = $1 and equivalent_iri = $2 \
           and relation = 'exact_equivalent' and not swap_direction",
        &[&b, &a],
    ).await.unwrap().get(0);
    assert_eq!(n_ba, 1, "exact_equivalent must propagate b → a");
}

#[tokio::test]
async fn inverse_equivalent_sets_swap_direction() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pc-inv");

    let parent = format!("{prefix}/parentOf");
    let child = format!("{prefix}/childOf");
    register_predicate(&client, &parent).await;
    register_predicate(&client, &child).await;

    client
        .register_alignment(
            &parent,
            &child,
            AlignmentRelation::InverseEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    rebuild_closure_with_retry(&client).await;

    let c = client.pool().get().await.unwrap();
    let swap_pc: bool = c.query_one(
        "select swap_direction from donto_predicate_closure \
         where predicate_iri = $1 and equivalent_iri = $2 \
           and relation = 'inverse_equivalent'",
        &[&parent, &child],
    ).await.unwrap().get(0);
    assert!(swap_pc, "inverse_equivalent must set swap_direction = true");

    let swap_cp: bool = c.query_one(
        "select swap_direction from donto_predicate_closure \
         where predicate_iri = $1 and equivalent_iri = $2 \
           and relation = 'inverse_equivalent'",
        &[&child, &parent],
    ).await.unwrap().get(0);
    assert!(swap_cp, "inverse_equivalent must be bidirectional with swap");
}

#[tokio::test]
async fn sub_property_of_propagates_upward_only() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pc-sub");

    // sub_property_of is asymmetric: a query for the broader predicate matches
    // statements asserted with the narrower one, not vice versa. The closure
    // (migration 0051) keys on the alignment's target_iri and expands to its
    // source_iri.
    let narrow = format!("{prefix}/parentOf");
    let broad = format!("{prefix}/relatedTo");
    register_predicate(&client, &narrow).await;
    register_predicate(&client, &broad).await;

    client
        .register_alignment(
            &narrow,
            &broad,
            AlignmentRelation::SubPropertyOf,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    rebuild_closure_with_retry(&client).await;

    let c = client.pool().get().await.unwrap();

    // broad → narrow (the upward-only edge) must exist.
    let upward: i64 = c.query_one(
        "select count(*) from donto_predicate_closure \
         where predicate_iri = $1 and equivalent_iri = $2 and relation = 'sub_property_of'",
        &[&broad, &narrow],
    ).await.unwrap().get(0);
    assert_eq!(upward, 1, "sub_property_of must propagate the upward edge");

    // narrow → broad (with relation = sub_property_of) must NOT exist.
    let downward: i64 = c.query_one(
        "select count(*) from donto_predicate_closure \
         where predicate_iri = $1 and equivalent_iri = $2 and relation = 'sub_property_of'",
        &[&narrow, &broad],
    ).await.unwrap().get(0);
    assert_eq!(
        downward, 0,
        "sub_property_of must not propagate downward"
    );
}

#[tokio::test]
async fn close_match_above_threshold_only() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pc-close");

    let high_a = format!("{prefix}/highA");
    let high_b = format!("{prefix}/highB");
    let low_a = format!("{prefix}/lowA");
    let low_b = format!("{prefix}/lowB");
    register_predicate(&client, &high_a).await;
    register_predicate(&client, &high_b).await;
    register_predicate(&client, &low_a).await;
    register_predicate(&client, &low_b).await;

    // Above floor (0.8) — propagates.
    client
        .register_alignment(
            &high_a,
            &high_b,
            AlignmentRelation::CloseMatch,
            0.9,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    // Below floor — does NOT propagate.
    client
        .register_alignment(
            &low_a,
            &low_b,
            AlignmentRelation::CloseMatch,
            0.5,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    rebuild_closure_with_retry(&client).await;

    let c = client.pool().get().await.unwrap();
    let high: i64 = c.query_one(
        "select count(*) from donto_predicate_closure \
         where predicate_iri = $1 and equivalent_iri = $2 and relation = 'close_match'",
        &[&high_a, &high_b],
    ).await.unwrap().get(0);
    assert_eq!(high, 1, "close_match above threshold must appear");

    let low: i64 = c.query_one(
        "select count(*) from donto_predicate_closure \
         where predicate_iri = $1 and equivalent_iri = $2 and relation = 'close_match'",
        &[&low_a, &low_b],
    ).await.unwrap().get(0);
    assert_eq!(low, 0, "close_match below threshold must NOT appear");
}

#[tokio::test]
async fn rebuild_is_idempotent() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pc-idem");

    let a = format!("{prefix}/a");
    let b = format!("{prefix}/b");
    register_predicate(&client, &a).await;
    register_predicate(&client, &b).await;
    client
        .register_alignment(
            &a,
            &b,
            AlignmentRelation::ExactEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // Tests run in parallel so the *total* row count can drift between
    // rebuilds. Pin idempotence to rows scoped to this test's prefix.
    let pattern = format!("{prefix}%");
    let c = client.pool().get().await.unwrap();

    rebuild_closure_with_retry(&client).await;
    let n1: i64 = c
        .query_one(
            "select count(*) from donto_predicate_closure \
             where predicate_iri like $1 or equivalent_iri like $1",
            &[&pattern],
        )
        .await
        .unwrap()
        .get(0);

    rebuild_closure_with_retry(&client).await;
    let n2: i64 = c
        .query_one(
            "select count(*) from donto_predicate_closure \
             where predicate_iri like $1 or equivalent_iri like $1",
            &[&pattern],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        n1, n2,
        "rebuild must be idempotent for a stable subset of alignments"
    );
    assert!(n1 > 0, "must include at least the registered alignment");
}

#[tokio::test]
async fn self_identity_rows_exist_for_active_predicates() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pc-self");

    let p = format!("{prefix}/somePred");
    register_predicate(&client, &p).await;
    rebuild_closure_with_retry(&client).await;

    let c = client.pool().get().await.unwrap();
    let n: i64 = c
        .query_one(
            "select count(*) from donto_predicate_closure \
             where predicate_iri = $1 and equivalent_iri = $1 and relation = 'self'",
            &[&p],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1, "self-identity row required for every active predicate");
}
