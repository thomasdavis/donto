//! v1000 frames + frame_role + frame_type_registry
//! (migrations 0105, 0106, 0116).

mod common;
use common::{connect, ctx, tag};

#[tokio::test]
async fn create_frame_round_trip() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "frame-rt").await;
    let id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('paradigm_cell', $1, 'demo cell')",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    let row = c
        .query_one(
            "select frame_type, primary_context, status from donto_claim_frame where frame_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let (ft, pctx, st): (String, String, String) = (row.get(0), row.get(1), row.get(2));
    assert_eq!(ft, "paradigm_cell");
    assert_eq!(pctx, ctx_iri);
    assert_eq!(st, "active");
}

#[tokio::test]
async fn frame_status_transitions_emit_events() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "frame-status").await;
    let id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('valency_frame', $1, null)",
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

    let n: i64 = c
        .query_one(
            "select count(*) from donto_event_log \
             where target_kind = 'frame' and target_id = $1::text",
            &[&id.to_string()],
        )
        .await
        .unwrap()
        .get(0);
    assert!(n >= 2, "create + retract events");
}

#[tokio::test]
async fn frame_status_check_constraint() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "frame-bad").await;
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

#[tokio::test]
async fn frame_roles_indexed_by_role_name() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "frame-roles-idx").await;
    let id: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('interlinear_example', $1)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);

    c.execute(
        "select donto_add_frame_role($1, 'vernacular', 'literal', null, '\"-ngka\"'::jsonb)",
        &[&id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'gloss', 'literal', null, '\"LOC\"'::jsonb)",
        &[&id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'translation', 'literal', null, '\"locative\"'::jsonb)",
        &[&id],
    )
    .await
    .unwrap();

    let rows = c
        .query(
            "select role from donto_frame_roles($1)",
            &[&id],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn frame_role_value_present_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "frame-bad-role").await;
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
            "insert into donto_frame_role (frame_id, role, value_kind) values ($1, 'agent', 'entity')",
            &[&id],
        )
        .await;
    assert!(
        res.is_err(),
        "value_ref or value_literal must be present for non-expression kinds"
    );
}

#[tokio::test]
async fn reverse_lookup_finds_frames_by_role_value() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "frame-rev-lookup").await;
    let value_ref = format!("ent:{}/erg-noun", tag("rev"));

    for _ in 0..3 {
        let id: uuid::Uuid = c
            .query_one(
                "select donto_create_claim_frame('argument_marking_pattern', $1)",
                &[&ctx_iri],
            )
            .await
            .unwrap()
            .get(0);
        c.execute(
            "select donto_add_frame_role($1, 'subject', 'entity', $2, null)",
            &[&id, &value_ref],
        )
        .await
        .unwrap();
    }

    let rows = c
        .query(
            "select frame_id from donto_frames_with_role_value('subject', $1, 100)",
            &[&value_ref],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
}

// -------------------- frame_type_registry (0116) -------------------- //

#[tokio::test]
async fn frame_types_seeded() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let n: i64 = c
        .query_one("select count(*) from donto_frame_type", &[])
        .await
        .unwrap()
        .get(0);
    assert!(
        n >= 24,
        "expected 18 linguistic + 6 cross-domain frame types, got {n}"
    );
}

#[tokio::test]
async fn frame_type_validate_required_roles() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    let valid: bool = c
        .query_one(
            "select donto_validate_frame_roles('paradigm_cell', \
                array['lexeme','features','form']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid);

    let missing: bool = c
        .query_one(
            "select donto_validate_frame_roles('paradigm_cell', \
                array['lexeme','features']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!missing, "missing required role 'form'");

    let unknown: bool = c
        .query_one(
            "select donto_validate_frame_roles('not_a_frame_type', array['x']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!unknown, "unknown frame type returns false");
}

#[tokio::test]
async fn cross_domain_frame_types_present() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    for ft in &[
        "diagnosis", "legal_precedent", "experiment_result",
        "clinical_observation", "schema_mapping",
        "access_policy_inheritance",
    ] {
        let n: i64 = c
            .query_one("select count(*) from donto_frame_type where frame_type = $1", &[ft])
            .await
            .unwrap()
            .get(0);
        assert_eq!(n, 1, "frame type {ft} present");
    }
}
