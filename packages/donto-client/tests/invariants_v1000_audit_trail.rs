//! v1000 audit-trail invariants: every state-changing operation on
//! v1000 objects emits an event into donto_event_log with the right
//! target_kind and event_type.

mod common;
use common::{connect, ctx, tag};
use serde_json::json;

async fn count_events(
    c: &deadpool_postgres::Object,
    target_kind: &str,
    target_id: &str,
    event_type: &str,
) -> i64 {
    c.query_one(
        "select count(*) from donto_event_log \
         where target_kind = $1 and target_id = $2 and event_type = $3",
        &[&target_kind, &target_id, &event_type],
    )
    .await
    .unwrap()
    .get(0)
}

#[tokio::test]
async fn frame_create_emits_created_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "audit-frame-create").await;
    let id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('paradigm_cell', $1, null)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        count_events(&c, "frame", &id.to_string(), "created").await,
        1
    );
}

#[tokio::test]
async fn frame_role_emits_created_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "audit-frame-role").await;
    let frame_id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('valency_frame', $1, null)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    let role_id: i64 = c
        .query_one(
            "select donto_add_frame_role($1, 'agent', 'literal', null, $2)",
            &[&frame_id, &json!({"text": "x"})],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        count_events(&c, "frame_role", &role_id.to_string(), "created").await,
        1
    );
}

#[tokio::test]
async fn release_seal_emits_created_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let name = tag("audit-release-seal");
    let id: uuid::Uuid = c
        .query_one(
            "insert into donto_dataset_release (release_name, release_version, query_spec) \
             values ($1, '0.1.0', '{}'::jsonb) returning release_id",
            &[&name],
        )
        .await
        .unwrap()
        .get(0);
    c.execute("select donto_seal_release($1, 'tester')", &[&id])
        .await
        .unwrap();
    assert_eq!(
        count_events(&c, "release", &id.to_string(), "created").await,
        1
    );
}

#[tokio::test]
async fn frame_status_change_emits_typed_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "audit-frame-status").await;
    let id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('clause_type', $1, null)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);

    c.execute(
        "select donto_set_frame_status($1, 'retracted', 'tester')",
        &[&id],
    )
    .await
    .unwrap();
    assert_eq!(
        count_events(&c, "frame", &id.to_string(), "retracted").await,
        1
    );

    c.execute(
        "select donto_set_frame_status($1, 'superseded', 'tester')",
        &[&id],
    )
    .await
    .unwrap();
    assert_eq!(
        count_events(&c, "frame", &id.to_string(), "superseded").await,
        1
    );
}

#[tokio::test]
async fn identity_proposal_status_change_emits_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("audit-id");
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
        "select donto_set_identity_proposal_status($1, 'accepted', 'tester', null)",
        &[&id],
    )
    .await
    .unwrap();
    assert_eq!(
        count_events(&c, "identity_hypothesis", &id.to_string(), "approved").await,
        1
    );

    c.execute(
        "select donto_set_identity_proposal_status($1, 'rejected', 'tester', null)",
        &[&id],
    )
    .await
    .unwrap();
    assert_eq!(
        count_events(&c, "identity_hypothesis", &id.to_string(), "rejected").await,
        1
    );
}

#[tokio::test]
async fn predicate_mint_emits_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("audit-mint");
    let iri = format!("ex:{prefix}/p");

    c.query_one(
        "select donto_mint_predicate_candidate($1, 'lbl', 'def', 'A', 'B', 'd', $2, $3, 'donto-native', 'tester', null, null)",
        &[
            &iri,
            &json!([{"subject": "a", "object": "b"}]),
            &json!([]),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        count_events(&c, "predicate_descriptor", &iri, "created").await,
        1
    );
}

#[tokio::test]
async fn predicate_approve_emits_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("audit-approve");
    let iri = format!("ex:{prefix}/p");

    c.query_one(
        "select donto_mint_predicate_candidate($1, 'lbl', 'def', 'A', 'B', 'd', $2, $3, 'donto-native', 'tester', null, null)",
        &[
            &iri,
            &json!([{"subject": "a", "object": "b"}]),
            &json!([]),
        ],
    )
    .await
    .unwrap();
    c.query_one("select donto_approve_predicate($1, 'reviewer')", &[&iri])
        .await
        .unwrap();
    assert_eq!(
        count_events(&c, "predicate_descriptor", &iri, "approved").await,
        1
    );
}

#[tokio::test]
async fn predicate_deprecate_emits_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("audit-deprecate");
    let iri = format!("ex:{prefix}/p");

    c.query_one(
        "select donto_mint_predicate_candidate($1, 'lbl', 'def', 'A', 'B', 'd', $2, $3, 'donto-native', 'tester', null, null)",
        &[
            &iri,
            &json!([{"subject": "a", "object": "b"}]),
            &json!([]),
        ],
    )
    .await
    .unwrap();
    c.query_one(
        "select donto_deprecate_predicate($1, 'reviewer', 'duplicate')",
        &[&iri],
    )
    .await
    .unwrap();
    assert_eq!(
        count_events(&c, "predicate_descriptor", &iri, "updated").await,
        1
    );
}

#[tokio::test]
async fn policy_assignment_emits_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("audit-policy");
    let target = format!("doc:{prefix}");
    let id: uuid::Uuid = c
        .query_one(
            "select donto_assign_policy('document', $1, 'policy:default/public', 'tester')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        count_events(&c, "access_assignment", &id.to_string(), "created").await,
        1
    );
}

#[tokio::test]
async fn attestation_issue_emits_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("audit-attest");
    let id: uuid::Uuid = c
        .query_one(
            "select donto_issue_attestation($1, 'system', 'policy:default/public', \
                array['read_metadata']::text[], 'audit', 'audit', null, null)",
            &[&format!("agent:{prefix}")],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        count_events(&c, "attestation", &id.to_string(), "created").await,
        1
    );
}

#[tokio::test]
async fn attestation_revoke_emits_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("audit-revoke");
    let id: uuid::Uuid = c
        .query_one(
            "select donto_issue_attestation($1, 'system', 'policy:default/public', \
                array['read_metadata']::text[], 'audit', 'audit', null, null)",
            &[&format!("agent:{prefix}")],
        )
        .await
        .unwrap()
        .get(0);
    c.query_one(
        "select donto_revoke_attestation($1, 'tester', 'no longer needed')",
        &[&id],
    )
    .await
    .unwrap();
    assert_eq!(
        count_events(&c, "attestation", &id.to_string(), "revoked").await,
        1
    );
}

#[tokio::test]
async fn review_decision_emits_typed_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("clm:{}", tag("audit-rev"));
    let id: uuid::Uuid = c
        .query_one(
            "select donto_record_review('claim', $1, 'accept', 'r:1', 'good', null, null, null, null, '{}'::jsonb)",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        count_events(&c, "review_decision", &id.to_string(), "approved").await,
        1
    );
}

#[tokio::test]
async fn review_decision_reject_emits_rejected_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("clm:{}", tag("audit-rev-rej"));
    let id: uuid::Uuid = c
        .query_one(
            "select donto_record_review('claim', $1, 'reject', 'r:1', 'no evidence', null, null, null, null, '{}'::jsonb)",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        count_events(&c, "review_decision", &id.to_string(), "rejected").await,
        1
    );
}

#[tokio::test]
async fn audit_event_payload_carries_target_metadata() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("doc:{}", tag("audit-pl"));
    let assign_id: uuid::Uuid = c
        .query_one(
            "select donto_assign_policy('document', $1, 'policy:default/public', 'tester')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    let payload: serde_json::Value = c
        .query_one(
            "select payload from donto_event_log \
             where target_kind = 'access_assignment' and target_id = $1::text",
            &[&assign_id.to_string()],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(payload["target_kind"], "document");
    assert_eq!(payload["target_id"], target);
    assert_eq!(payload["policy_iri"], "policy:default/public");
}

#[tokio::test]
async fn audit_history_orders_descending() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("aln:{}", tag("audit-hist"));
    for ev in &["created", "updated", "approved"] {
        c.query_one(
            "select donto_emit_event('alignment', $1, $2, 'tester', '{}'::jsonb, null, null)",
            &[&target, ev],
        )
        .await
        .unwrap();
    }
    let rows = c
        .query(
            "select event_type from donto_event_history('alignment', $1, 10)",
            &[&target],
        )
        .await
        .unwrap();
    let kinds: Vec<String> = rows.iter().map(|r| r.get(0)).collect();
    assert_eq!(kinds, vec!["approved", "updated", "created"]);
}
