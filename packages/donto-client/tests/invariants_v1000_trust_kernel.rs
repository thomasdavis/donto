//! v1000 Trust Kernel invariants.
//!
//! Migrations 0111 (policy_capsule) + 0112 (attestation) introduce
//! the policy/attestation/audit foundation. These tests pin the
//! contract: default-restricted, max-restriction inheritance,
//! attestation overrides only with rationale, revocation immediate.

use uuid::Uuid;

mod common;
use common::{cleanup_prefix, connect, tag};

#[tokio::test]
async fn default_policies_are_seeded() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    let n: i64 = c
        .query_one(
            "select count(*) from donto_policy_capsule where policy_iri like 'policy:default/%'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        n >= 4,
        "expected at least 4 default policies (public, restricted_pending_review, community_restricted, private_research); got {n}"
    );
}

#[tokio::test]
async fn unassigned_target_falls_back_to_restricted() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let _ = tag("trust-fallback");

    // Unassigned target: ask for read_content.
    let allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'read_content')",
            &[&"src:does-not-exist"],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        !allowed,
        "unassigned target must default to restricted (PRD I2 fail-closed)"
    );

    // read_metadata should still be allowed under the default-restricted policy.
    let meta_allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'read_metadata')",
            &[&"src:does-not-exist"],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        meta_allowed,
        "unassigned target metadata read should be allowed (default-restricted exposes metadata)"
    );
}

#[tokio::test]
async fn assignment_to_public_policy_allows_read() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("trust-public");
    cleanup_prefix(&client, &prefix).await;

    let target = format!("{prefix}/doc/1");

    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/public', 'test')",
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
    assert!(allowed, "public policy must allow read_content");

    let train_allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'train_model')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        !train_allowed,
        "public policy must not allow train_model by default (separate action)"
    );
}

#[tokio::test]
async fn max_restriction_inheritance_when_two_policies_collide() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("trust-max-restrict");
    cleanup_prefix(&client, &prefix).await;

    let target = format!("{prefix}/doc/1");

    // Assign both public and community-restricted to the same target.
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/public', 'test')",
        &[&target],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'test')",
        &[&target],
    )
    .await
    .unwrap();

    // read_content is permitted by public but not by community_restricted.
    // Max restriction → false.
    let allowed: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'read_content')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        !allowed,
        "max-restriction inheritance: a single restrictive policy must veto"
    );
}

#[tokio::test]
async fn attestation_can_override_denial_for_specific_holder() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("trust-attest");
    cleanup_prefix(&client, &prefix).await;

    let target = format!("{prefix}/doc/1");
    let holder = format!("{prefix}/agent/researcher-1");

    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'test')",
        &[&target],
    )
    .await
    .unwrap();

    // Without an attestation, this holder cannot read.
    let denied: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'read_content')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!denied, "default-deny without attestation");

    // Issue an attestation that grants read_content under the community policy.
    let attestation_id: Uuid = c
        .query_one(
            "select donto_issue_attestation($1, 'system', 'policy:default/community_restricted', \
                array['read_content']::text[], 'community_curation', \
                'Reviewer authorised by community council on 2026-05-07.', null, null)",
            &[&holder],
        )
        .await
        .unwrap()
        .get(0);

    let now_allowed: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'read_content')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(now_allowed, "attestation must grant the holder the action");

    // Revoke the attestation; access denied immediately.
    c.execute(
        "select donto_revoke_attestation($1, 'system', 'test')",
        &[&attestation_id],
    )
    .await
    .unwrap();

    let denied_again: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'read_content')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        !denied_again,
        "revocation must take effect immediately for new authorisation checks"
    );
}

#[tokio::test]
async fn attestation_requires_rationale() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("trust-rationale");
    let holder = format!("{prefix}/agent/x");

    let res = c
        .execute(
            "select donto_issue_attestation($1, 'system', 'policy:default/public', \
                array['read_content']::text[], 'audit', '', null, null)",
            &[&holder],
        )
        .await;
    assert!(
        res.is_err(),
        "empty rationale must be rejected (audit requirement)"
    );
}

#[tokio::test]
async fn anchor_kind_validator_enforces_required_keys() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    let valid: bool = c
        .query_one(
            "select donto_validate_anchor_locator('char_span', '{\"start\": 10, \"end\": 25}'::jsonb)",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid, "valid char_span locator must validate");

    let missing_end: bool = c
        .query_one(
            "select donto_validate_anchor_locator('char_span', '{\"start\": 10}'::jsonb)",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        !missing_end,
        "char_span without end key must fail validation"
    );

    let media_valid: bool = c
        .query_one(
            "select donto_validate_anchor_locator('media_time', \
                '{\"start_ms\": 0, \"end_ms\": 1500}'::jsonb)",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(media_valid, "media_time with start_ms+end_ms must validate");
}

#[tokio::test]
async fn unknown_anchor_kind_raises() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .query_one(
            "select donto_validate_anchor_locator('not_a_real_kind', '{}'::jsonb)",
            &[],
        )
        .await;
    assert!(res.is_err(), "unknown anchor kind must raise an exception");
}
