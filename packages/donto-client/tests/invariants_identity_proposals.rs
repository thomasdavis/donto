//!  identity layer (migrations 0093 + 0109): identity proposals
//! and clustering-hypothesis extensions.

mod common;
use common::{connect, tag};

#[tokio::test]
async fn proposal_kinds_accepted() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idp-kinds");

    for kind in &[
        "same_as",
        "different_from",
        "broader_than",
        "narrower_than",
        "split_candidate",
        "merge_candidate",
        "successor_of",
        "alias_of",
    ] {
        let refs = vec![
            format!("ent:{prefix}/{kind}/a"),
            format!("ent:{prefix}/{kind}/b"),
        ];
        c.query_one(
            "select donto_register_identity_proposal($1, $2::text[])",
            &[kind, &refs],
        )
        .await
        .unwrap_or_else(|e| panic!("kind {kind} should be accepted: {e}"));
    }
}

#[tokio::test]
async fn proposal_invalid_kind_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idp-bad");
    let refs = vec![format!("ent:{prefix}/a"), format!("ent:{prefix}/b")];
    let res = c
        .query_one(
            "select donto_register_identity_proposal($1, $2::text[])",
            &[&"morphologically_equivalent", &refs],
        )
        .await;
    assert!(res.is_err(), "unknown hypothesis_kind must be rejected");
}

#[tokio::test]
async fn proposal_requires_at_least_two_entities() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idp-card");
    let one = vec![format!("ent:{prefix}/just-one")];
    let res = c
        .query_one(
            "select donto_register_identity_proposal('same_as', $1::text[])",
            &[&one],
        )
        .await;
    assert!(
        res.is_err(),
        "card(entity_refs) >= 2 must reject single-element"
    );
}

#[tokio::test]
async fn proposal_status_transitions_recorded() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idp-status");
    let refs = vec![format!("ent:{prefix}/a"), format!("ent:{prefix}/b")];

    let id: uuid::Uuid = c
        .query_one(
            "select donto_register_identity_proposal('same_as', $1::text[])",
            &[&refs],
        )
        .await
        .unwrap()
        .get(0);

    c.execute(
        "select donto_set_identity_proposal_status($1, 'accepted', 'reviewer-1', 'looks good')",
        &[&id],
    )
    .await
    .unwrap();

    let status: String = c
        .query_one(
            "select status from donto_identity_proposal where proposal_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(status, "accepted");

    // Event log records the transition.
    let n: i64 = c
        .query_one(
            "select count(*) from donto_event_log \
             where target_kind = 'identity_hypothesis' and target_id = $1::text",
            &[&id.to_string()],
        )
        .await
        .unwrap()
        .get(0);
    assert!(n >= 2, "creation + status change events recorded");
}

#[tokio::test]
async fn cluster_hypothesis_v2_method_default() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    let method: String = c
        .query_one(
            "select method from donto_identity_hypothesis where name = 'strict'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    // Pre-existing rows may default to 'rule' after migration.
    assert!(
        [
            "rule",
            "human",
            "model",
            "registry_match",
            "cross_source_evidence",
            "mixed"
        ]
        .contains(&method.as_str()),
        "method on existing strict hypothesis is in  enum: {method}"
    );
}

#[tokio::test]
async fn cluster_hypothesis_v2_register_with_extended_metadata() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let name = format!("cluster-{}", uuid::Uuid::new_v4().simple());

    let id: i64 = c
        .query_one(
            "select donto_register_clustering_hypothesis($1, $2, 0.85, 0.05, $3, $4, null, '{}'::jsonb)",
            &[&name, &" test cluster", &"human", &"council:test"],
        )
        .await
        .unwrap()
        .get(0);
    assert!(id > 0);

    let row = c
        .query_one(
            "select method, authority from donto_identity_hypothesis where hypothesis_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let (m, a): (String, Option<String>) = (row.get(0), row.get(1));
    assert_eq!(m, "human");
    assert_eq!(a.as_deref(), Some("council:test"));
}
