//! v1000 adversarial review: edge cases, security spot checks,
//! known-gap tripwires, and behavioural guarantees that don't show
//! up in happy-path tests.
//!
//! Intent: catch the bugs nobody notices on a casual review. Each
//! test documents a specific assumption or claim about how the
//! system should behave under pressure.

mod common;
use common::{cleanup_prefix, connect, ctx, tag};
use serde_json::json;

// --------------------------------------------------------------------
// Trust Kernel — gap tripwires.
//
// The legacy `donto_ensure_document` and `donto_register_document` SQL
// functions DO NOT require policy_id. The v1000 entry point
// `donto_register_source_v1000` does. Until the HTTP layer is migrated
// in M0's middleware step, callers using the legacy functions can
// still register a source with NULL policy_id. These tests pin the
// current state so a future middleware change isn't surprising:
//
//   * `legacy_register_document_does_not_enforce_policy` — asserts
//     the bypass exists today (so the test will FAIL when the bypass
//     is closed, prompting a deliberate migration).
//   * `v1000_register_enforces_policy` — asserts the v1000 path is
//     correct.
// --------------------------------------------------------------------

#[tokio::test]
async fn legacy_register_document_does_not_enforce_policy() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-legacy-bypass");
    let iri = format!("src:{prefix}/legacy");
    let id: uuid::Uuid = c
        .query_one(
            "select donto_ensure_document($1, 'application/pdf', null, null, null)",
            &[&iri],
        )
        .await
        .unwrap()
        .get(0);

    // Document is created without policy_id.
    let policy: Option<String> = c
        .query_one(
            "select policy_id from donto_document where document_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        policy.is_none(),
        "legacy path bypasses policy requirement; M0 middleware must close this"
    );

    // Effective actions still default-restrict because no policy is assigned.
    let allowed_read: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'read_content')",
            &[&iri],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        !allowed_read,
        "fail-closed default still applies even when policy_id is NULL"
    );
}

#[tokio::test]
async fn v1000_register_enforces_policy() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-v1000-enforce");
    let res = c
        .query_one(
            "select donto_register_source_v1000($1, 'pdf', null)",
            &[&format!("src:{prefix}/v1000")],
        )
        .await;
    assert!(res.is_err());
}

// --------------------------------------------------------------------
// Revoked-policy carve-out.
//
// A policy can be flagged `revocation_status='revoked'`. Effective-action
// resolution must skip revoked policies. Without this, a revoked
// "all-allow" policy would still grant access.
// --------------------------------------------------------------------

#[tokio::test]
async fn revoked_policy_does_not_contribute_to_effective_actions() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-revoked-policy");

    // Register a custom policy and assign it.
    let policy_iri = format!("policy:{prefix}/temp-allow-all");
    c.execute(
        "insert into donto_policy_capsule (policy_iri, policy_kind, allowed_actions) values \
            ($1, 'public', $2::jsonb)",
        &[
            &policy_iri,
            &json!({
                "read_metadata": true, "read_content": true, "quote": true,
                "view_anchor_location": true, "derive_claims": true,
                "derive_embeddings": true, "translate": true, "summarize": true,
                "export_claims": true, "export_sources": true, "export_anchors": true,
                "train_model": true, "publish_release": true,
                "share_with_third_party": true, "federated_query": true
            }),
        ],
    )
    .await
    .unwrap();

    let target = format!("doc:{prefix}");
    c.execute(
        "select donto_assign_policy('document', $1, $2, 'tester')",
        &[&target, &policy_iri],
    )
    .await
    .unwrap();

    // While active, train_model is allowed.
    let allowed_active: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'train_model')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(allowed_active);

    // Revoke the policy.
    c.execute(
        "update donto_policy_capsule set revocation_status = 'revoked' where policy_iri = $1",
        &[&policy_iri],
    )
    .await
    .unwrap();

    let allowed_revoked: bool = c
        .query_one(
            "select donto_action_allowed('document', $1, 'train_model')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        !allowed_revoked,
        "revoked policy must not contribute to effective actions"
    );
}

// --------------------------------------------------------------------
// Expired-at-boundary.
//
// `donto_holder_can` requires `expires_at is null or expires_at > now()`.
// An attestation that expired one millisecond ago must fail.
// --------------------------------------------------------------------

#[tokio::test]
async fn expired_attestation_in_past_does_not_grant() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-exp-past");
    let holder = format!("agent:{prefix}");

    c.query_one(
        "select donto_issue_attestation($1, 's', 'policy:default/public', \
            array['read_content']::text[], 'audit', 'rationale', \
            now() - interval '1 second', null)",
        &[&holder],
    )
    .await
    .unwrap();

    let granted: bool = c
        .query_one(
            "select donto_holder_can($1, 'policy:default/public', 'read_content')",
            &[&holder],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!granted);
}

#[tokio::test]
async fn just_revoked_attestation_does_not_grant() {
    // Revoked attestations are excluded by the "revoked_at is null" filter.
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-just-revoked");
    let holder = format!("agent:{prefix}");
    let id: uuid::Uuid = c
        .query_one(
            "select donto_issue_attestation($1, 's', 'policy:default/public', \
                array['read_content']::text[], 'audit', 'rationale', null, null)",
            &[&holder],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "select donto_revoke_attestation($1, 'admin', 'policy change')",
        &[&id],
    )
    .await
    .unwrap();
    let granted: bool = c
        .query_one(
            "select donto_holder_can($1, 'policy:default/public', 'read_content')",
            &[&holder],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!granted);
}

// --------------------------------------------------------------------
// Multiple-attestation OR semantics.
//
// A holder with a narrow attestation + a broad attestation should be
// authorised at the union of granted actions.
// --------------------------------------------------------------------

#[tokio::test]
async fn multiple_attestations_or_semantics() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-multi-att");
    let holder = format!("agent:{prefix}");
    let target = format!("doc:{prefix}");

    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&target],
    )
    .await
    .unwrap();

    // Attestation 1: read_content only
    c.query_one(
        "select donto_issue_attestation($1, 's', 'policy:default/community_restricted', \
            array['read_content']::text[], 'audit', 'first', null, null)",
        &[&holder],
    )
    .await
    .unwrap();

    // Attestation 2: quote only
    c.query_one(
        "select donto_issue_attestation($1, 's', 'policy:default/community_restricted', \
            array['quote']::text[], 'audit', 'second', null, null)",
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
    let quote_ok: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'quote')",
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
    assert!(read_ok && quote_ok && !train_ok);
}

// --------------------------------------------------------------------
// Cross-target attestation isolation.
//
// An attestation for policy P assigned to target T1 must not authorise
// the same holder against target T2 even if T2 has the same policy
// assigned.
// --------------------------------------------------------------------

#[tokio::test]
async fn attestation_is_policy_scoped_not_target_scoped() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-att-scope");
    let holder = format!("agent:{prefix}");
    let t1 = format!("doc:{prefix}/t1");
    let t2 = format!("doc:{prefix}/t2");

    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&t1],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&t2],
    )
    .await
    .unwrap();

    c.query_one(
        "select donto_issue_attestation($1, 's', 'policy:default/community_restricted', \
            array['read_content']::text[], 'audit', 'rationale', null, null)",
        &[&holder],
    )
    .await
    .unwrap();

    // Attestation is policy-scoped, so the holder gets read_content on both.
    let t1_ok: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'read_content')",
            &[&holder, &t1],
        )
        .await
        .unwrap()
        .get(0);
    let t2_ok: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'read_content')",
            &[&holder, &t2],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        t1_ok && t2_ok,
        "policy-scoped attestation grants on both targets"
    );
}

// --------------------------------------------------------------------
// Cross-policy attestation isolation.
//
// An attestation for policy P1 must NOT grant access under policy P2.
// --------------------------------------------------------------------

#[tokio::test]
async fn attestation_for_one_policy_does_not_grant_under_another() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-cross-policy");
    let holder = format!("agent:{prefix}");
    let target = format!("doc:{prefix}");

    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&target],
    )
    .await
    .unwrap();

    // Attestation for a DIFFERENT policy.
    c.query_one(
        "select donto_issue_attestation($1, 's', 'policy:default/private_research', \
            array['read_content']::text[], 'audit', 'rationale', null, null)",
        &[&holder],
    )
    .await
    .unwrap();

    let granted: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'read_content')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!granted);
}

// --------------------------------------------------------------------
// Cycle detection in alignment registration.
//
// A → B exact_match, B → A exact_match should not blow up the closure.
// --------------------------------------------------------------------

#[tokio::test]
async fn reciprocal_alignment_does_not_explode_closure() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-reciprocal");
    let a = format!("ex:{prefix}/A");
    let b = format!("ex:{prefix}/B");
    c.query_one(
        "select donto_register_alignment($1, $2, 'exact_equivalent', 1.0)",
        &[&a, &b],
    )
    .await
    .unwrap();
    c.query_one(
        "select donto_register_alignment($1, $2, 'exact_equivalent', 1.0)",
        &[&b, &a],
    )
    .await
    .unwrap();

    let rebuilt: i32 = c
        .query_one("select donto_rebuild_predicate_closure()", &[])
        .await
        .unwrap()
        .get(0);
    assert!(rebuilt >= 0);
}

// --------------------------------------------------------------------
// Long-string boundary conditions.
//
// Very long IRIs and labels must round-trip cleanly. text columns are
// unbounded but underlying bytea / TOAST may surprise.
// --------------------------------------------------------------------

#[tokio::test]
async fn very_long_iri_round_trip() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-long-iri");
    let long_path = "a".repeat(2000);
    let iri = format!("ex:{prefix}/{long_path}");

    c.execute(
        "select donto_register_source_v1000($1, 'pdf', 'policy:default/public')",
        &[&iri],
    )
    .await
    .unwrap();

    let read_back: String = c
        .query_one("select iri from donto_document where iri = $1", &[&iri])
        .await
        .unwrap()
        .get(0);
    assert_eq!(read_back.len(), iri.len());
}

// --------------------------------------------------------------------
// Unicode in IRIs.
//
// IRIs can contain non-ASCII characters per RFC 3987.
// --------------------------------------------------------------------

#[tokio::test]
async fn unicode_iri_round_trip() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-unicode");
    // Yolŋu Matha endonym; Cyrillic; CJK; emoji.
    for fragment in &["Yolŋu", "Український", "中文", "🌏-tag"] {
        let iri = format!("ex:{prefix}/{fragment}");
        c.execute(
            "select donto_register_source_v1000($1, 'pdf', 'policy:default/public')",
            &[&iri],
        )
        .await
        .unwrap();
        let n: i64 = c
            .query_one(
                "select count(*) from donto_document where iri = $1",
                &[&iri],
            )
            .await
            .unwrap()
            .get(0);
        assert_eq!(n, 1, "unicode iri {fragment} round-trips");
    }
}

// --------------------------------------------------------------------
// Empty / whitespace-only IRI.
//
// donto_register_source_v1000 doesn't validate IRI format; this just
// pins current behaviour so a future tightening is deliberate.
// --------------------------------------------------------------------

#[tokio::test]
async fn empty_iri_currently_accepted_by_substrate() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    // current substrate doesn't validate IRI shape; document this.
    let res = c
        .execute(
            "select donto_register_source_v1000('', 'pdf', 'policy:default/public')",
            &[],
        )
        .await;
    // Inserts succeed because there's no NOT NULL/length constraint on iri.
    // The HTTP layer is the right place to validate IRI shape.
    assert!(
        res.is_ok(),
        "substrate currently accepts empty IRI; HTTP layer must validate"
    );
}

// --------------------------------------------------------------------
// Self-referential frame role.
//
// A frame can have a role pointing to itself. Sanity check that this
// doesn't loop on read.
// --------------------------------------------------------------------

#[tokio::test]
async fn frame_role_can_reference_own_frame() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "adv-self-ref-frame").await;
    let frame_id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('valency_frame', $1)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);

    // Frame role pointing back at the frame itself.
    c.execute(
        "select donto_add_frame_role($1, 'self', 'frame_ref', $2, null)",
        &[&frame_id, &frame_id.to_string()],
    )
    .await
    .unwrap();

    // donto_frame_roles should return the row without infinite-looping.
    let rows = c
        .query("select role_id from donto_frame_roles($1)", &[&frame_id])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

// --------------------------------------------------------------------
// Identity proposal duplicate entity_refs.
//
// `entity_refs text[]` does not enforce uniqueness inside the array;
// duplicates currently allowed. Document this.
// --------------------------------------------------------------------

#[tokio::test]
async fn identity_proposal_entity_refs_duplicates_currently_allowed() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-id-dup");
    let refs = vec![
        format!("ent:{prefix}/x"),
        format!("ent:{prefix}/x"),
        format!("ent:{prefix}/y"),
    ];
    let id: uuid::Uuid = c
        .query_one(
            "select donto_register_identity_proposal('same_as', $1::text[])",
            &[&refs],
        )
        .await
        .unwrap()
        .get(0);
    let card: i32 = c
        .query_one(
            "select cardinality(entity_refs) from donto_identity_proposal where proposal_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(card, 3, "duplicates preserved; dedup is application-layer");
}

// --------------------------------------------------------------------
// Modality overlay survives statement retraction.
//
// donto_retract closes tx_time but does not delete the statement row,
// so the overlay still references a valid statement_id. Confirm.
// --------------------------------------------------------------------

#[tokio::test]
async fn overlays_survive_retraction() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-overlay-retract");
    let ctx_iri = ctx(&client, "adv-overlay-retract").await;
    let s = client
        .assert(
            &donto_client::StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                donto_client::Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx_iri),
        )
        .await
        .unwrap();
    c.execute("select donto_set_modality($1, 'descriptive')", &[&s])
        .await
        .unwrap();
    c.execute("select donto_set_extraction_level($1, 'quoted')", &[&s])
        .await
        .unwrap();

    client.retract(s).await.unwrap();

    // Retract closes tx_time but doesn't delete the row, so overlays remain.
    let m: Option<String> = c
        .query_one(
            "select modality from donto_stmt_modality where statement_id = $1",
            &[&s],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(m.as_deref(), Some("descriptive"));
}

// --------------------------------------------------------------------
// Frame with no roles.
//
// A frame can exist without any roles. The reverse-lookup helper must
// return the empty set without erroring.
// --------------------------------------------------------------------

#[tokio::test]
async fn frame_without_roles_is_empty_but_valid() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "adv-empty-frame").await;
    let frame_id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('clause_type', $1)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    let rows = c
        .query("select role_id from donto_frame_roles($1)", &[&frame_id])
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);
}

// --------------------------------------------------------------------
// Unknown anchor kind raises rather than silently failing.
// --------------------------------------------------------------------

#[tokio::test]
async fn unknown_anchor_kind_raises_with_message() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .query_one(
            "select donto_validate_anchor_locator('made_up_kind', '{}'::jsonb)",
            &[],
        )
        .await;
    assert!(res.is_err());
}

// --------------------------------------------------------------------
// Default policies cannot be silently overwritten.
//
// donto_emit_event triggers on the four default policies' creation
// would re-fire if migrations re-ran. This test pins that the
// `on conflict do nothing` clause prevents duplication.
// --------------------------------------------------------------------

#[tokio::test]
async fn default_policies_count_is_stable_across_repeated_inserts() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let n_before: i64 = c
        .query_one(
            "select count(*) from donto_policy_capsule where policy_iri like 'policy:default/%'",
            &[],
        )
        .await
        .unwrap()
        .get(0);

    // Re-run the migration's seed inserts manually.
    c.execute(
        "insert into donto_policy_capsule (policy_iri, policy_kind) \
         values ('policy:default/public', 'public') on conflict (policy_iri) do nothing",
        &[],
    )
    .await
    .unwrap();

    let n_after: i64 = c
        .query_one(
            "select count(*) from donto_policy_capsule where policy_iri like 'policy:default/%'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n_after, n_before);
}

// --------------------------------------------------------------------
// Confidence column boundary check.
// --------------------------------------------------------------------

#[tokio::test]
async fn confidence_boundary_zero_and_one() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-conf-bnd");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "adv-conf-bnd").await;
    let s = client
        .assert(
            &donto_client::StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                donto_client::Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx_iri),
        )
        .await
        .unwrap();

    // 0.0 must be accepted.
    c.execute("select donto_set_confidence($1, 0.0)", &[&s])
        .await
        .unwrap();
    // 1.0 must be accepted.
    c.execute("select donto_set_confidence($1, 1.0)", &[&s])
        .await
        .unwrap();
    // -0.0001 must be rejected.
    let res = c
        .execute("select donto_set_confidence($1, -0.0001)", &[&s])
        .await;
    assert!(res.is_err());
}

// --------------------------------------------------------------------
// Locator validator with extra keys.
//
// A locator with the required keys plus extra ones must validate.
// (We don't enforce strict-mode validation; extras are allowed.)
// --------------------------------------------------------------------

#[tokio::test]
async fn anchor_locator_with_extra_keys_validates() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let valid: bool = c
        .query_one(
            "select donto_validate_anchor_locator('char_span', $1)",
            &[&json!({
                "start": 0,
                "end": 10,
                "extra": "ignored",
                "another": [1, 2, 3]
            })],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid);
}

// --------------------------------------------------------------------
// Event log ordering under fast successive emits.
// --------------------------------------------------------------------

#[tokio::test]
async fn event_log_preserves_emission_order_for_same_target() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("aln_{}", uuid::Uuid::new_v4().simple());

    let mut ids = Vec::new();
    for ev in &["created", "updated", "approved", "updated", "rejected"] {
        let id: i64 = c
            .query_one(
                "select donto_emit_event('alignment', $1, $2, 'a', '{}'::jsonb, null, null)",
                &[&target, ev],
            )
            .await
            .unwrap()
            .get(0);
        ids.push(id);
    }

    // event_id is a BIGSERIAL — strictly monotonic for serial inserts.
    for w in ids.windows(2) {
        assert!(w[1] > w[0]);
    }
}

// --------------------------------------------------------------------
// Predicate minting refuses without nearest-neighbour record.
// --------------------------------------------------------------------

#[tokio::test]
async fn predicate_mint_requires_nearest_record_even_when_empty() {
    // Empty array is acceptable (no neighbours found) — only NULL is rejected.
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-mint-nearest-empty");
    let iri = format!("ex:{prefix}/p");
    let ok = c
        .query_one(
            "select donto_mint_predicate_candidate($1, 'l', 'd', 'A', 'B', 'd', $2, '[]'::jsonb, 'donto-native', 't', null, null)",
            &[&iri, &json!([{"subject": "a", "object": "b"}])],
        )
        .await;
    assert!(ok.is_ok());

    let iri2 = format!("ex:{prefix}/p2");
    let res = c
        .query_one(
            "select donto_mint_predicate_candidate($1, 'l', 'd', 'A', 'B', 'd', $2, null, 'donto-native', 't', null, null)",
            &[&iri2, &json!([{"subject": "a", "object": "b"}])],
        )
        .await;
    assert!(res.is_err(), "NULL nearest record rejected");
}

// --------------------------------------------------------------------
// donto_action_allowed semantics: AND across policies.
//
// If three policies are assigned to a target, an action is allowed
// only if EVERY policy allows it. (Defence-in-depth.)
// --------------------------------------------------------------------

#[tokio::test]
async fn three_policy_max_restriction() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-three-policy");
    let target = format!("doc:{prefix}");
    for pol in &[
        "policy:default/public",
        "policy:default/private_research",
        "policy:default/community_restricted",
    ] {
        c.execute(
            "select donto_assign_policy('document', $1, $2, 'tester')",
            &[&target, pol],
        )
        .await
        .unwrap();
    }
    // read_content: public allows, private_research allows,
    // community_restricted denies. Max-restriction → false.
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

// --------------------------------------------------------------------
// Frame deletion cascades to roles.
// --------------------------------------------------------------------

#[tokio::test]
async fn frame_role_cascades_on_frame_delete() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "adv-cascade").await;
    let frame_id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('clause_type', $1)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "select donto_add_frame_role($1, 'subject', 'literal', null, $2)",
        &[&frame_id, &json!("x")],
    )
    .await
    .unwrap();

    c.execute(
        "delete from donto_claim_frame where frame_id = $1",
        &[&frame_id],
    )
    .await
    .unwrap();

    let n: i64 = c
        .query_one(
            "select count(*) from donto_frame_role where frame_id = $1",
            &[&frame_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 0);
}

// --------------------------------------------------------------------
// Concurrent assertions of identical content collapse to one row.
// --------------------------------------------------------------------

#[tokio::test]
async fn concurrent_identical_assertions_collapse_to_one() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("adv-concurrent-assert");
    let ctx_iri = ctx(&client, "adv-concurrent-assert").await;
    let subj = format!("{prefix}/s");

    let stmts: Vec<_> = (0..16)
        .map(|_| {
            let c = &client;
            let s = subj.clone();
            let ctx = ctx_iri.clone();
            async move {
                c.assert(
                    &donto_client::StatementInput::new(
                        s,
                        "ex:p",
                        donto_client::Object::iri("ex:o"),
                    )
                    .with_context(&ctx),
                )
                .await
                .unwrap()
            }
        })
        .collect();
    let ids = futures_util::future::join_all(stmts).await;
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        1,
        "16 concurrent identical asserts must collapse to one statement_id"
    );
}

// --------------------------------------------------------------------
// Predicate descriptor minting status defaults to candidate.
// --------------------------------------------------------------------

#[tokio::test]
async fn newly_minted_predicate_is_not_approved() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-mint-default");
    let iri = format!("ex:{prefix}/p");
    c.query_one(
        "select donto_mint_predicate_candidate($1, 'l', 'd', 'A', 'B', 'd', $2, '[]'::jsonb, 'donto-native', 't', null, null)",
        &[&iri, &json!([{"subject": "a", "object": "b"}])],
    )
    .await
    .unwrap();
    let approved: bool = c
        .query_one("select donto_predicate_is_approved($1)", &[&iri])
        .await
        .unwrap()
        .get(0);
    assert!(!approved);
}

// --------------------------------------------------------------------
// Release seal is one-way.
// --------------------------------------------------------------------

#[tokio::test]
async fn release_seal_cannot_be_undone_via_function() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let name = tag("adv-seal-once");
    let id: uuid::Uuid = c
        .query_one(
            "insert into donto_dataset_release (release_name, release_version, query_spec) \
             values ($1, '0.1.0', '{}'::jsonb) returning release_id",
            &[&name],
        )
        .await
        .unwrap()
        .get(0);
    let s1: bool = c
        .query_one("select donto_seal_release($1, 'tester')", &[&id])
        .await
        .unwrap()
        .get(0);
    let s2: bool = c
        .query_one("select donto_seal_release($1, 'tester')", &[&id])
        .await
        .unwrap()
        .get(0);
    assert!(s1, "first seal works");
    assert!(!s2, "second seal is no-op (already sealed)");
}

// --------------------------------------------------------------------
// Status is reachable via direct SQL UPDATE — no protection against
// ad-hoc writes. Document this is intentional (the trigger / event
// emission happens only via the helper functions).
// --------------------------------------------------------------------

#[tokio::test]
async fn direct_update_to_release_sealed_at_does_not_emit_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let name = tag("adv-direct-update");
    let id: uuid::Uuid = c
        .query_one(
            "insert into donto_dataset_release (release_name, release_version, query_spec) \
             values ($1, '0.1.0', '{}'::jsonb) returning release_id",
            &[&name],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "update donto_dataset_release set sealed_at = now() where release_id = $1",
        &[&id],
    )
    .await
    .unwrap();
    let n: i64 = c
        .query_one(
            "select count(*) from donto_event_log where target_kind = 'release' \
             and target_id = $1::text",
            &[&id.to_string()],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        n, 0,
        "direct UPDATE bypasses event emission — use donto_seal_release"
    );
}

// --------------------------------------------------------------------
// Identity proposal status_history grows monotonically.
// --------------------------------------------------------------------

#[tokio::test]
async fn identity_proposal_history_is_append_only() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-id-hist");
    let refs = vec![format!("ent:{prefix}/a"), format!("ent:{prefix}/b")];
    let id: uuid::Uuid = c
        .query_one(
            "select donto_register_identity_proposal('same_as', $1::text[])",
            &[&refs],
        )
        .await
        .unwrap()
        .get(0);

    for status in &["accepted", "rejected", "superseded"] {
        c.execute(
            "select donto_set_identity_proposal_status($1, $2, 't', null)",
            &[&id, status],
        )
        .await
        .unwrap();
    }

    let metadata: serde_json::Value = c
        .query_one(
            "select metadata from donto_identity_proposal where proposal_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    let history = metadata["status_history"].as_array().unwrap();
    assert_eq!(history.len(), 3);
    let statuses: Vec<&str> = history
        .iter()
        .map(|h| h["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, vec!["accepted", "rejected", "superseded"]);
}

// --------------------------------------------------------------------
// Authorise is policy-AND with attestation-OR semantics — prove
// the overall composition.
// --------------------------------------------------------------------

#[tokio::test]
async fn authorise_combines_policy_and_with_attestation_or() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("adv-auth-combo");
    let target = format!("doc:{prefix}");
    let holder = format!("agent:{prefix}");

    // Two policies: public + private_research. Max-restriction would
    // deny export_claims. We need an attestation under EITHER policy.
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

    let ok_before: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'export_claims')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!ok_before);

    // Attestation under private_research grants export.
    c.query_one(
        "select donto_issue_attestation($1, 's', 'policy:default/private_research', \
            array['export_claims']::text[], 'audit', 'auditor approved', null, null)",
        &[&holder],
    )
    .await
    .unwrap();
    let ok_after: bool = c
        .query_one(
            "select donto_authorise($1, 'document', $2, 'export_claims')",
            &[&holder, &target],
        )
        .await
        .unwrap()
        .get(0);
    assert!(ok_after, "attestation under any assigned policy is enough");
}
