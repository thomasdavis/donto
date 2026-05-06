//! v1000 integration: end-to-end flow exercising multiple kernels.
//!
//! Source registration with policy (M0/M1) → claim with modality and
//! extraction level (M2) → predicate minting (M3) → review decision
//! (M4) → frame creation with roles (M2) → release manifest (M7).
//!
//! These tests intentionally cross migration boundaries to catch
//! interactions between subsystems.

use donto_client::{Object, StatementInput};
use serde_json::json;

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

#[tokio::test]
async fn end_to_end_extraction_workflow() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("e2e-flow");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "e2e-flow").await;

    // 1. Register source with policy.
    let src_iri = format!("src:{prefix}/grammar.pdf");
    let _doc_id: uuid::Uuid = c
        .query_one(
            "select donto_register_source_v1000($1, 'pdf', 'policy:default/public')",
            &[&src_iri],
        )
        .await
        .unwrap()
        .get(0);

    // 2. Mint a candidate predicate.
    let pred = format!("ex:{prefix}/hasMorpheme");
    c.query_one(
        "select donto_mint_predicate_candidate($1, 'hasMorpheme', \
            'Subject lexeme contains object morpheme.', \
            'Lexeme', 'Morpheme', 'linguistics', $2, $3, 'donto-native', \
            'tester', null, null)",
        &[
            &pred,
            &json!([{"subject": "ex:lexeme/foo", "object": "ex:morph/-ngka"}]),
            &json!([]),
        ],
    )
    .await
    .unwrap();

    // 3. Approve the predicate.
    let approved: bool = c
        .query_one("select donto_approve_predicate($1, 'reviewer:1')", &[&pred])
        .await
        .unwrap()
        .get(0);
    assert!(approved);

    // 4. Assert a claim using the predicate.
    let stmt = client
        .assert(
            &StatementInput::new(
                format!("ex:{prefix}/lexeme/foo"),
                pred.clone(),
                Object::iri(format!("ex:{prefix}/morph/-ngka")),
            )
            .with_context(&ctx_iri),
        )
        .await
        .unwrap();

    // 5. Decorate with modality, extraction level, multivalue confidence.
    c.execute("select donto_set_modality($1, 'descriptive')", &[&stmt])
        .await
        .unwrap();
    c.execute(
        "select donto_set_extraction_level($1, 'source_generalization')",
        &[&stmt],
    )
    .await
    .unwrap();
    c.execute("select donto_set_confidence($1, 0.78)", &[&stmt])
        .await
        .unwrap();
    c.execute("select donto_set_calibrated_confidence($1, 0.82)", &[&stmt])
        .await
        .unwrap();

    // 6. Reviewer accepts the claim.
    let _rev: uuid::Uuid = c
        .query_one(
            "select donto_record_review('claim', $1, 'accept', 'reviewer:1', \
                'evidence in §3.4', null, 0.9::double precision, null, null, '{}'::jsonb)",
            &[&stmt.to_string()],
        )
        .await
        .unwrap()
        .get(0);

    // 7. Create a paradigm-cell frame referencing the lexeme.
    let frame_id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('paradigm_cell', $1, 'demo')",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "select donto_add_frame_role($1, 'lexeme', 'entity', $2, null)",
        &[&frame_id, &format!("ex:{prefix}/lexeme/foo")],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'features', 'literal', null, $2)",
        &[&frame_id, &json!({"case": "LOC"})],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'form', 'literal', null, $2)",
        &[&frame_id, &json!({"text": "foo-ngka"})],
    )
    .await
    .unwrap();

    // 8. Build a release manifest.
    let release_name = format!("e2e-{}", uuid::Uuid::new_v4().simple());
    let release_id: uuid::Uuid = c
        .query_one(
            "insert into donto_dataset_release (release_name, release_version, query_spec, \
                source_manifest, output_formats) values ($1, '0.1.0', $2, $3, '{donto-jsonl,cldf}'::text[]) \
             returning release_id",
            &[
                &release_name,
                &json!({"context": ctx_iri}),
                &json!([{"source_iri": src_iri}]),
            ],
        )
        .await
        .unwrap()
        .get(0);

    let sealed: bool = c
        .query_one("select donto_seal_release($1, 'release-bot')", &[&release_id])
        .await
        .unwrap()
        .get(0);
    assert!(sealed);

    // 9. Verify the release-summary view reflects it.
    let row = c
        .query_one(
            "select release_name from donto_release_summary($1)",
            &[&release_id],
        )
        .await
        .unwrap();
    let n: String = row.get(0);
    assert_eq!(n, release_name);

    // 10. Verify event log captured the workflow.
    let events: i64 = c
        .query_one(
            "select count(*) from donto_event_log \
             where target_kind in ('predicate_descriptor', 'review_decision', \
                                   'frame', 'frame_role', 'release') \
             and occurred_at > now() - interval '5 minutes'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(events >= 5, "workflow emitted >= 5 events; got {events}");
}

#[tokio::test]
async fn restricted_source_blocks_export_action() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("e2e-restrict");

    let src_iri = format!("src:{prefix}/restricted.eaf");
    c.execute(
        "select donto_register_source_v1000($1, 'audio', 'policy:default/community_restricted')",
        &[&src_iri],
    )
    .await
    .unwrap();

    c.execute(
        "select donto_assign_policy('document', $1, 'policy:default/community_restricted', 'tester')",
        &[&src_iri],
    )
    .await
    .unwrap();

    // Without attestation, no holder can export.
    let allowed: bool = c
        .query_one(
            "select donto_authorise('agent:random-researcher', 'document', $1, 'export_claims')",
            &[&src_iri],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!allowed, "fail-closed by default for export_claims");

    // train_model is also blocked even with a per-policy attestation
    // unless that attestation explicitly grants train_model.
    c.query_one(
        "select donto_issue_attestation($1, 'system', 'policy:default/community_restricted', \
            array['read_metadata']::text[], 'audit', \
            'Read-only access for community audit.', null, null)",
        &[&"agent:auditor"],
    )
    .await
    .unwrap();

    let train_allowed: bool = c
        .query_one(
            "select donto_authorise('agent:auditor', 'document', $1, 'train_model')",
            &[&src_iri],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        !train_allowed,
        "attestation grants only listed actions; train_model not granted"
    );
}

#[tokio::test]
async fn frame_with_required_roles_validates() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "e2e-frame-validate").await;

    let frame_id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('interlinear_example', $1)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "select donto_add_frame_role($1, 'vernacular', 'literal', null, $2)",
        &[&frame_id, &json!({"text": "wungar"})],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'gloss', 'literal', null, $2)",
        &[&frame_id, &json!({"text": "walk"})],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'translation', 'literal', null, $2)",
        &[&frame_id, &json!({"text": "(s)he walks"})],
    )
    .await
    .unwrap();

    let valid: bool = c
        .query_one(
            "select donto_validate_frame_roles('interlinear_example', \
                array['vernacular','gloss','translation']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid, "all required roles present");
}

#[tokio::test]
async fn paraconsistency_with_modality_preserved() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("e2e-paracon");
    cleanup_prefix(&client, &prefix).await;

    let ctx_a = ctx(&client, "e2e-paracon-A").await;
    let ctx_b = ctx(&client, "e2e-paracon-B").await;

    let subj = format!("ex:{prefix}/lang/X");
    let pred = "ex:hasErgative";

    let s1 = client
        .assert(
            &StatementInput::new(subj.clone(), pred, Object::iri("ex:value/yes"))
                .with_context(&ctx_a),
        )
        .await
        .unwrap();
    let s2 = client
        .assert(
            &StatementInput::new(subj.clone(), pred, Object::iri("ex:value/no"))
                .with_context(&ctx_b),
        )
        .await
        .unwrap();

    c.execute("select donto_set_modality($1, 'typological_summary')", &[&s1])
        .await
        .unwrap();
    c.execute("select donto_set_modality($1, 'descriptive')", &[&s2])
        .await
        .unwrap();

    // Both rows survive (paraconsistency invariant).
    let n: i64 = c
        .query_one(
            "select count(*) from donto_statement \
             where subject = $1 and predicate = $2 and upper(tx_time) is null",
            &[&subj, &pred],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2, "both contradictory rows persist");

    // Each carries its own modality.
    let m1: String = c
        .query_one("select donto_get_modality($1)", &[&s1])
        .await
        .unwrap()
        .get(0);
    let m2: String = c
        .query_one("select donto_get_modality($1)", &[&s2])
        .await
        .unwrap()
        .get(0);
    assert_ne!(m1, m2);
}

#[tokio::test]
async fn alignment_safety_flags_constrain_expansion_logically() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("e2e-aln-safety");

    let id: uuid::Uuid = c
        .query_one(
            "select donto_register_alignment($1, $2, 'close_match', 0.6)",
            &[
                &format!("ex:{prefix}/wals_typological"),
                &format!("ex:{prefix}/ud_token_level"),
            ],
        )
        .await
        .unwrap()
        .get(0);

    // Default safety flags forbid logical inference.
    let row = c
        .query_one(
            "select safe_for_query_expansion, safe_for_logical_inference \
             from donto_predicate_alignment where alignment_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let (q, l): (bool, bool) = (row.get(0), row.get(1));
    assert!(q, "query expansion safe by default");
    assert!(
        !l,
        "logical inference NOT safe by default — close_match between typological and token-level is the canonical PRD example"
    );
}

#[tokio::test]
async fn identity_proposal_status_lifecycle() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("e2e-id-life");

    let refs = vec![
        format!("ent:{prefix}/A"),
        format!("ent:{prefix}/B"),
    ];

    let id: uuid::Uuid = c
        .query_one(
            "select donto_register_identity_proposal('merge_candidate', $1::text[])",
            &[&refs],
        )
        .await
        .unwrap()
        .get(0);

    // Lifecycle: candidate → accepted
    c.execute(
        "select donto_set_identity_proposal_status($1, 'accepted', 'reviewer', 'looks fine')",
        &[&id],
    )
    .await
    .unwrap();

    // Lifecycle: accepted → superseded (e.g., a richer hypothesis emerges)
    c.execute(
        "select donto_set_identity_proposal_status($1, 'superseded', 'reviewer', 'replaced by H2')",
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
    assert_eq!(status, "superseded");

    // History recorded in metadata.
    let metadata: serde_json::Value = c
        .query_one(
            "select metadata from donto_identity_proposal where proposal_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    let history = &metadata["status_history"];
    assert_eq!(history.as_array().unwrap().len(), 2);
}
