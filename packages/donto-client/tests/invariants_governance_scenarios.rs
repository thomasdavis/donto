//!  governance scenarios: end-to-end flows exercising policy +
//! attestation + audit + access enforcement together.

mod common;
use common::{connect, tag};

async fn make_target(c: &deadpool_postgres::Object, prefix: &str) -> String {
    let target = format!("doc:{prefix}");
    c.execute(
        "select donto_register_source($1, 'pdf', 'policy:default/restricted_pending_review')",
        &[&target],
    )
    .await
    .unwrap();
    target
}

#[tokio::test]
async fn restricted_pending_review_blocks_read_content() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-rpr-block");
    let target = make_target(&c, &prefix).await;

    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/restricted_pending_review', 'tester')",
        &[&target],
    )
    .await
    .unwrap();

    let allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'read_content')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!allowed);
}

#[tokio::test]
async fn restricted_pending_review_allows_metadata() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-rpr-meta");
    let target = make_target(&c, &prefix).await;

    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/restricted_pending_review', 'tester')",
        &[&target],
    )
    .await
    .unwrap();

    let allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'read_metadata')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        allowed,
        "metadata read allowed even under restricted-pending"
    );
}

#[tokio::test]
async fn private_research_blocks_export() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-priv-export");
    let target = make_target(&c, &prefix).await;
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/private_research', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    let allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'export_claims')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!allowed);
}

#[tokio::test]
async fn private_research_allows_derive_claims() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-priv-derive");
    let target = make_target(&c, &prefix).await;
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/private_research', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    let allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'derive_claims')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(allowed);
}

#[tokio::test]
async fn community_restricted_blocks_train_model() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-comm-train");
    let target = make_target(&c, &prefix).await;
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    let allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'train_model')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!allowed);
}

#[tokio::test]
async fn public_blocks_train_model_by_default() {
    // Public policy still gates train_model — read != train.
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-pub-train");
    let target = make_target(&c, &prefix).await;
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/public', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    let allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'train_model')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!allowed);
}

#[tokio::test]
async fn attestation_grants_specific_action() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-att-grant");
    let target = make_target(&c, &prefix).await;
    let holder = format!("agent:{prefix}");
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    c.query_one(
        "select donto_issue_attestation($1, 'community-council', 'policy:default/community_restricted', \
            array['read_content']::text[], 'community_curation', 'curator approved', null, null)",
        &[&holder],
    )
    .await
    .unwrap();
    let read_ok: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'read_content')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get(0);
    let train_ok: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'train_model')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(read_ok, "attestation grants read_content");
    assert!(!train_ok, "attestation does not grant train_model");
}

#[tokio::test]
async fn attestation_all_action_grants_everything() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-att-all");
    let target = make_target(&c, &prefix).await;
    let holder = format!("agent:{prefix}");
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    c.query_one(
        "select donto_issue_attestation($1, 'community', 'policy:default/community_restricted', \
            array['all']::text[], 'community_curation', 'full access', null, null)",
        &[&holder],
    )
    .await
    .unwrap();
    for action in &["read_content", "quote", "train_model", "export_claims"] {
        let ok: bool = c
            .query_one(
                "select donto_authorise($1, 'document', $2, $3)",
                &[&holder, &target, action],
            )
            .await
            .unwrap()
            .get(0);
        assert!(ok, "all-action attestation grants {action}");
    }
}

#[tokio::test]
async fn revocation_immediate_for_new_checks() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-revoke");
    let target = make_target(&c, &prefix).await;
    let holder = format!("agent:{prefix}");
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    let att: uuid::Uuid = c
        .query_one(
            "select donto_issue_attestation($1, 's', 'policy:default/community_restricted', \
                array['read_content']::text[], 'audit', 'a', null, null)",
            &[&holder],
        )
        .await
        .unwrap()
        .get(0);
    assert!(c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'read_content')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get::<_, bool>(0));
    c.execute(
        "select donto_revoke_attestation($1, 'admin', null)",
        &[&att],
    )
    .await
    .unwrap();
    assert!(!c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'read_content')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get::<_, bool>(0));
}

#[tokio::test]
async fn expired_attestation_does_not_grant() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-att-exp");
    let target = make_target(&c, &prefix).await;
    let holder = format!("agent:{prefix}");
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    // Issue with expires_at in the past
    c.query_one(
        "select donto_issue_attestation($1, 's', 'policy:default/community_restricted', \
            array['read_content']::text[], 'audit', 'a', '2020-01-01T00:00:00Z'::timestamptz, null)",
        &[&holder],
    )
    .await
    .unwrap();
    let allowed: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'read_content')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!allowed, "expired attestation should not grant");
}

#[tokio::test]
async fn max_restriction_two_policies() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-max-restrict");
    let target = make_target(&c, &prefix).await;
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/public', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/private_research', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    // public allows export_claims; private_research does not. Max-restriction wins.
    let allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'export_claims')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!allowed);
}

#[tokio::test]
async fn no_policy_assignment_falls_through_to_default_restricted() {
    // When a target has NO policy assigned at all, donto_effective_actions
    // returns the default restricted policy's actions.
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("doc:{}", tag("gov-no-policy"));
    let res: serde_json::Value = c
        .query_one("select donto_effective_actions('document', $1)", &[&target])
        .await
        .unwrap()
        .get(0);
    assert_eq!(res["read_metadata"], true);
    assert_eq!(res["read_content"], false);
    assert_eq!(res["train_model"], false);
    assert_eq!(res["publish_release"], false);
}

#[tokio::test]
async fn audit_log_includes_assignment() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-audit-assign");
    let target = make_target(&c, &prefix).await;
    let assign_id: uuid::Uuid = c
        .query_one(
            "select donto_assign_policy('document', $1, 'policy:default/public', 'auditor')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    let n: i64 = c
        .query_one(
            "select count(*) from donto_event_log \
             where target_kind = 'access_assignment' and target_id = $1::text and actor = 'auditor'",
            &[&assign_id.to_string()],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1);
}

#[tokio::test]
async fn rationale_required_for_attestation() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-rationale");
    // SQL trim() defaults to spaces only, so a tab-only string would pass
    // the constraint. Audit-grade rationale validation should be enforced
    // at the API layer; the SQL CHECK is a defence-in-depth.
    for bad in &["", "   "] {
        let res = c
            .query_one(
                "select donto_issue_attestation($1, 's', 'policy:default/public', \
                    array['read_metadata']::text[], 'audit', $2, null, null)",
                &[&format!("agent:{prefix}"), bad],
            )
            .await;
        assert!(res.is_err(), "rejects rationale={bad:?}");
    }
}

#[tokio::test]
async fn community_restricted_blocks_share_with_third_party() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("gov-share");
    let target = make_target(&c, &prefix).await;
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&target],
    )
    .await
    .unwrap();
    let allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'share_with_third_party')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!allowed);
}
