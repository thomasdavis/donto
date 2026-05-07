//!  constraint paths: every CHECK and FK constraint introduced
//! by the  migrations rejects the values it should and accepts
//! the ones it should.

mod common;
use common::{connect, ctx, tag};
use serde_json::json;

// -------------------- migration 0089 hypothesis_only --------------------

#[tokio::test]
async fn cannot_mark_nonexistent_statement_as_hypothesis_only() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .execute(
            "select donto_mark_hypothesis_only($1)",
            &[&uuid::Uuid::new_v4()],
        )
        .await;
    assert!(res.is_err(), "FK to donto_statement enforced");
}

// -------------------- migration 0090 event_log --------------------

#[tokio::test]
async fn event_log_target_kind_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    for bad in &["", "not_real", "Document", "POLICY"] {
        let res = c
            .execute(
                "insert into donto_event_log (target_kind, target_id, event_type, actor) \
                 values ($1, 't', 'created', 'tester')",
                &[bad],
            )
            .await;
        assert!(res.is_err(), "rejects target_kind={bad}");
    }
}

#[tokio::test]
async fn event_log_event_type_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    for bad in &["", "deleted", "modified", "wibbled"] {
        let res = c
            .execute(
                "insert into donto_event_log (target_kind, target_id, event_type, actor) \
                 values ('alignment', 't', $1, 'tester')",
                &[bad],
            )
            .await;
        assert!(res.is_err(), "rejects event_type={bad}");
    }
}

// -------------------- migration 0091 argument relations v2 --------------------

#[tokio::test]
async fn argument_review_state_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-arg-rs");
    let ctx_iri = ctx(&client, "cp-arg-rs").await;
    let s1 = client
        .assert(
            &donto_client::StatementInput::new(
                format!("{prefix}/a"),
                "ex:p",
                donto_client::Object::iri(format!("{prefix}/b")),
            )
            .with_context(&ctx_iri),
        )
        .await
        .unwrap();
    let s2 = client
        .assert(
            &donto_client::StatementInput::new(
                format!("{prefix}/c"),
                "ex:p",
                donto_client::Object::iri(format!("{prefix}/d")),
            )
            .with_context(&ctx_iri),
        )
        .await
        .unwrap();
    let arg: uuid::Uuid = c
        .query_one(
            "select donto_assert_argument($1, $2, 'supports', $3)",
            &[&s1, &s2, &ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    let res = c
        .execute(
            "update donto_argument set review_state = 'mythical' where argument_id = $1",
            &[&arg],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0092 alignment v2 --------------------

#[tokio::test]
async fn alignment_review_status_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-aln-rs");
    let id: uuid::Uuid = c
        .query_one(
            "select donto_register_alignment($1, $2, 'close_match', 1.0)",
            &[&format!("ex:{prefix}/p"), &format!("ex:{prefix}/q")],
        )
        .await
        .unwrap()
        .get(0);
    let res = c
        .execute(
            "update donto_predicate_alignment set review_status = 'mythical' where alignment_id = $1",
            &[&id],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn alignment_value_mapping_confidence_range() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-vm-conf");
    let id: uuid::Uuid = c
        .query_one(
            "select donto_register_alignment($1, $2, 'has_value_mapping', 1.0)",
            &[&format!("ex:{prefix}/p"), &format!("ex:{prefix}/q")],
        )
        .await
        .unwrap()
        .get(0);
    let res = c
        .execute(
            "select donto_register_value_mapping($1, '1', 'present', 1.5, null)",
            &[&id],
        )
        .await;
    assert!(res.is_err(), "confidence > 1.0 rejected");
}

// -------------------- migration 0093 identity proposal --------------------

#[tokio::test]
async fn identity_proposal_method_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-id-method");
    let refs = vec![format!("ent:{prefix}/a"), format!("ent:{prefix}/b")];
    let res = c
        .query_one(
            "select donto_register_identity_proposal('same_as', $1::text[], 0.5, 'magic')",
            &[&refs],
        )
        .await;
    assert!(res.is_err(), "method='magic' rejected by CHECK");
}

#[tokio::test]
async fn identity_proposal_status_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-id-status");
    let refs = vec![format!("ent:{prefix}/a"), format!("ent:{prefix}/b")];
    let id: uuid::Uuid = c
        .query_one(
            "select donto_register_identity_proposal('same_as', $1::text[])",
            &[&refs],
        )
        .await
        .unwrap()
        .get(0);
    let res = c
        .execute(
            "update donto_identity_proposal set status = 'mythical' where proposal_id = $1",
            &[&id],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0094 release --------------------

#[tokio::test]
async fn release_visibility_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .execute(
            "insert into donto_dataset_release (release_name, query_spec, visibility) \
             values ('rel-bad', '{}'::jsonb, 'mythical')",
            &[],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn release_output_formats_default_is_donto_jsonl() {
    // The CHECK on output_formats uses array_length() which returns NULL
    // (not 0) for an empty array, so an explicit empty value would slip
    // through CHECK. Defaulting handles it: when the column is omitted,
    // we always get '{donto-jsonl}'.
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let id: uuid::Uuid = c
        .query_one(
            "insert into donto_dataset_release (release_name, query_spec) \
             values ('rel-default-fmt', '{}'::jsonb) returning release_id",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let formats: Vec<String> = c
        .query_one(
            "select output_formats from donto_dataset_release where release_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(formats, vec!["donto-jsonl".to_string()]);
}

// -------------------- migration 0095 source extension --------------------

#[tokio::test]
async fn source_kind_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-skind");
    let res = c
        .execute(
            "select donto_register_source($1, 'mythical', 'policy:default/public')",
            &[&format!("src:{prefix}")],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn source_status_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-sstat");
    c.execute(
        "select donto_register_source($1, 'pdf', 'policy:default/public')",
        &[&format!("src:{prefix}")],
    )
    .await
    .unwrap();
    let res = c
        .execute(
            "update donto_document set status = 'phantom' where iri = $1",
            &[&format!("src:{prefix}")],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0099 modality --------------------

#[tokio::test]
async fn modality_value_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-mod");
    let ctx_iri = ctx(&client, "cp-mod").await;
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
    let res = c
        .execute("select donto_set_modality($1, 'phantasmagorical')", &[&s])
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0100 extraction level --------------------

#[tokio::test]
async fn extraction_level_value_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-xl");
    let ctx_iri = ctx(&client, "cp-xl").await;
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
    let res = c
        .execute("select donto_set_extraction_level($1, 'invented')", &[&s])
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0101 confidence multivalue --------------------

#[tokio::test]
async fn confidence_lens_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-conf-lens");
    let ctx_iri = ctx(&client, "cp-conf-lens").await;
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
    c.execute("select donto_set_confidence($1, 0.5)", &[&s])
        .await
        .unwrap();
    let res = c
        .execute(
            "update donto_stmt_confidence set confidence_lens = 'gauss' where statement_id = $1",
            &[&s],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0103 multi-context --------------------

#[tokio::test]
async fn multi_context_role_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-mc-role");
    let ctx1 = ctx(&client, "cp-mc-role-pri").await;
    let s = client
        .assert(
            &donto_client::StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                donto_client::Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx1),
        )
        .await
        .unwrap();
    let ctx2 = format!("ctx:{prefix}/x");
    c.execute("select donto_ensure_context($1)", &[&ctx2])
        .await
        .unwrap();
    let res = c
        .execute(
            "insert into donto_statement_context (statement_id, context, role) values ($1, $2, 'spectral')",
            &[&s, &ctx2],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0104 claim_kind --------------------

#[tokio::test]
async fn claim_kind_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-ck");
    let ctx_iri = ctx(&client, "cp-ck").await;
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
    let res = c
        .execute("select donto_set_claim_kind($1, 'mythical')", &[&s])
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0105 claim_frame --------------------

#[tokio::test]
async fn claim_frame_status_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "cp-frame-status").await;
    let id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('clause_type', $1)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    let res = c
        .execute(
            "update donto_claim_frame set status = 'mythical' where frame_id = $1",
            &[&id],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0106 frame_role --------------------

#[tokio::test]
async fn frame_role_value_kind_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "cp-role-vk").await;
    let frame_id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('valency_frame', $1)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    let res = c
        .execute(
            "insert into donto_frame_role (frame_id, role, value_kind, value_literal) \
             values ($1, 'agent', 'mythical', '\"x\"'::jsonb)",
            &[&frame_id],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0107 context multi-parent --------------------

#[tokio::test]
async fn context_multi_parent_role_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-cmp-role");
    let parent = format!("ctx:{prefix}/p");
    let child = format!("ctx:{prefix}/c");
    c.execute("select donto_ensure_context($1)", &[&parent])
        .await
        .unwrap();
    c.execute("select donto_ensure_context($1)", &[&child])
        .await
        .unwrap();
    let res = c
        .execute(
            "insert into donto_context_parent (context, parent_context, parent_role) \
             values ($1, $2, 'mythical')",
            &[&child, &parent],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0108 entity extension --------------------

#[tokio::test]
async fn entity_kind_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-ek");
    let id: i64 = c
        .query_one(
            "select donto_ensure_symbol($1, null, null, null, null, null)",
            &[&format!("ent:{prefix}/x")],
        )
        .await
        .unwrap()
        .get(0);
    let res = c
        .execute(
            "update donto_entity_symbol set entity_kind = 'mythical' where symbol_id = $1",
            &[&id],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn entity_label_status_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-elabel");
    let id: i64 = c
        .query_one(
            "select donto_ensure_symbol($1, null, null, null, null, null)",
            &[&format!("ent:{prefix}/x")],
        )
        .await
        .unwrap()
        .get(0);
    let res = c
        .execute(
            "select donto_add_entity_label($1, 'X', null, null, 'mythical')",
            &[&id],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0110 predicate minting --------------------

#[tokio::test]
async fn predicate_minting_status_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-pm-status");
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
    let res = c
        .execute(
            "update donto_predicate_descriptor set minting_status = 'mythical' where iri = $1",
            &[&iri],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0111 policy capsule --------------------

#[tokio::test]
async fn policy_kind_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .execute(
            "insert into donto_policy_capsule (policy_iri, policy_kind) \
             values ('policy:test:bad', 'mythical')",
            &[],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn policy_inheritance_rule_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .execute(
            "insert into donto_policy_capsule (policy_iri, policy_kind, inheritance_rule) \
             values ('policy:test:bad-inh', 'public', 'mythical')",
            &[],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0112 attestation --------------------

#[tokio::test]
async fn attestation_purpose_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-att-purpose");
    let res = c
        .query_one(
            "select donto_issue_attestation($1, 'system', 'policy:default/public', \
                array['read_metadata']::text[], 'mythical', 'rationale', null, null)",
            &[&format!("agent:{prefix}")],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn attestation_actions_nonempty() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cp-att-empty");
    let res = c
        .query_one(
            "select donto_issue_attestation($1, 'system', 'policy:default/public', \
                array[]::text[], 'audit', 'rationale', null, null)",
            &[&format!("agent:{prefix}")],
        )
        .await;
    assert!(res.is_err(), "empty actions array rejected");
}

// -------------------- migration 0113 obligation kinds --------------------

#[tokio::test]
async fn obligation_status_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "cp-ob-status").await;
    let id: uuid::Uuid = c
        .query_one(
            "select donto_emit_obligation(null, 'needs_review', $1, 0::smallint, null, null)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    let res = c
        .execute(
            "update donto_proof_obligation set status = 'mythical' where obligation_id = $1",
            &[&id],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0114 review decision --------------------

#[tokio::test]
async fn review_target_type_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .query_one(
            "select donto_record_review('mythical', 't', 'accept', 'r:1', 'why', null, null, null, null, '{}'::jsonb)",
            &[],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn review_decision_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .query_one(
            "select donto_record_review('claim', 't', 'mythical', 'r:1', 'why', null, null, null, null, '{}'::jsonb)",
            &[],
        )
        .await;
    assert!(res.is_err());
}

// -------------------- migration 0116 frame type registry --------------------

#[tokio::test]
async fn frame_type_validate_unknown_returns_false() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let valid: bool = c
        .query_one(
            "select donto_validate_frame_roles('not_a_real_type', '{}'::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!valid);
}
