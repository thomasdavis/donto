//!  workflow surfaces: obligation kinds v2 (0113),
//! review decisions (0114), context multi-parent (0107),
//! query v2 metadata (0115).

mod common;
use common::{connect, ctx, tag};

// -------------------- obligation kinds v2 (0113) -------------------- //

#[tokio::test]
async fn extended_obligation_kinds_accepted() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ob-v2-kinds").await;

    for kind in &[
        "needs_evidence",
        "needs_policy",
        "needs_review",
        "needs_identity_resolution",
        "needs_alignment_review",
        "needs_anchor_repair",
        "needs_contradiction_review",
        "needs_formal_validation",
        "needs_community_authority",
    ] {
        c.query_one(
            "select donto_emit_obligation(null, $1, $2, 0::smallint, null, null)",
            &[kind, &ctx_iri],
        )
        .await
        .unwrap_or_else(|e| panic!("kind {kind}: {e}"));
    }
}

#[tokio::test]
async fn v0_obligation_kinds_still_accepted() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ob-v0-kinds").await;
    for kind in &[
        "needs-coref",
        "needs-temporal-grounding",
        "needs-source-support",
        "needs-human-review",
    ] {
        c.query_one(
            "select donto_emit_obligation(null, $1, $2, 0::smallint, null, null)",
            &[kind, &ctx_iri],
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn obligation_status_blocked_accepted() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ob-blocked").await;

    let id: uuid::Uuid = c
        .query_one(
            "select donto_emit_obligation(null, 'needs_review', $1, 0::smallint, null, null)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);

    c.execute(
        "update donto_proof_obligation set status = 'blocked' where obligation_id = $1",
        &[&id],
    )
    .await
    .unwrap();

    let s: String = c
        .query_one(
            "select status from donto_proof_obligation where obligation_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(s, "blocked");
}

#[tokio::test]
async fn obligation_kind_view_lists_nine() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let n: i64 = c
        .query_one("select count(*) from donto_v_obligation_kind", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 9);
}

// -------------------- review decision (0114) -------------------- //

#[tokio::test]
async fn record_review_round_trip() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("rev-rt");
    let target = format!("clm:{prefix}/x");

    let id: uuid::Uuid = c
        .query_one(
            "select donto_record_review('claim', $1, 'accept', 'reviewer:1', \
                'evidence is solid', null, 0.95::double precision, null, null, '{}'::jsonb)",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);

    let row = c
        .query_one(
            "select decision, reviewer_id, rationale from donto_review_decision \
             where review_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let (d, r, rat): (String, String, String) = (row.get(0), row.get(1), row.get(2));
    assert_eq!(d, "accept");
    assert_eq!(r, "reviewer:1");
    assert_eq!(rat, "evidence is solid");
}

#[tokio::test]
async fn empty_rationale_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .query_one(
            "select donto_record_review('claim', 'clm:test', 'accept', 'r:1', '   ', \
                null, null, null, null, '{}'::jsonb)",
            &[],
        )
        .await;
    assert!(res.is_err(), "empty/whitespace rationale rejected");
}

#[tokio::test]
async fn invalid_decision_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .query_one(
            "select donto_record_review('claim', 'clm:test', 'morphologically_correct', \
                'r:1', 'why', null, null, null, null, '{}'::jsonb)",
            &[],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn invalid_target_type_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .query_one(
            "select donto_record_review('mythology', 'm:test', 'accept', \
                'r:1', 'why', null, null, null, null, '{}'::jsonb)",
            &[],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn latest_review_returns_most_recent() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("clm:{}/x", tag("rev-latest"));

    let r1: uuid::Uuid = c
        .query_one(
            "select donto_record_review('claim', $1, 'qualify', 'r:1', 'first take', \
                null, null, null, null, '{}'::jsonb)",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    let r2: uuid::Uuid = c
        .query_one(
            "select donto_record_review('claim', $1, 'accept', 'r:2', 'second take', \
                null, null, null, null, '{}'::jsonb)",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);

    let latest: uuid::Uuid = c
        .query_one("select donto_latest_review('claim', $1)", &[&target])
        .await
        .unwrap()
        .get(0);
    assert_eq!(latest, r2, "most recent decision wins");
    assert_ne!(latest, r1);
}

#[tokio::test]
async fn review_event_emitted() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("clm:{}/x", tag("rev-event"));

    let id: uuid::Uuid = c
        .query_one(
            "select donto_record_review('claim', $1, 'reject', 'r:1', 'no evidence', \
                null, null, null, null, '{}'::jsonb)",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);

    let n: i64 = c
        .query_one(
            "select count(*) from donto_event_log where target_kind = 'review_decision' \
             and target_id = $1::text",
            &[&id.to_string()],
        )
        .await
        .unwrap()
        .get(0);
    assert!(n >= 1);
}

// -------------------- context multi-parent (0107) -------------------- //

#[tokio::test]
async fn context_multi_parent_round_trip() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ctx-mp");

    let parent1 = format!("ctx:{prefix}/parent-1");
    let parent2 = format!("ctx:{prefix}/parent-2");
    let child = format!("ctx:{prefix}/child");

    c.execute(
        "select donto_add_context_parent($1, $2, 'inherit')",
        &[&child, &parent1],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_context_parent($1, $2, 'lens')",
        &[&child, &parent2],
    )
    .await
    .unwrap();

    let n: i64 = c
        .query_one(
            "select count(*) from donto_context_parent where context = $1",
            &[&child],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2);
}

#[tokio::test]
async fn context_multi_parent_role_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ctx-mp-bad");
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

#[tokio::test]
async fn context_multi_parent_no_self_loop() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ctx-self");
    let me = format!("ctx:{prefix}/me");
    c.execute("select donto_ensure_context($1)", &[&me])
        .await
        .unwrap();

    let res = c
        .execute(
            "insert into donto_context_parent (context, parent_context) values ($1, $1)",
            &[&me],
        )
        .await;
    assert!(res.is_err(), "self-loop rejected by CHECK");
}

// -------------------- query metadata (0115) -------------------- //

#[tokio::test]
async fn query_clauses_seeded() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let n: i64 = c
        .query_one(
            "select count(*) from donto_query_clause where deprecated_in is null",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(n >= 25, "expected ≥25  query clauses, got {n}");
}

#[tokio::test]
async fn query_clauses_extended_specific_present() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    for clause in &[
        "MODALITY",
        "EXTRACTION_LEVEL",
        "IDENTITY_LENS",
        "SCHEMA_LENS",
        "POLICY_ALLOWS",
        "AS_OF",
    ] {
        let n: i64 = c
            .query_one(
                "select count(*) from donto_query_clause where clause_name = $1",
                &[clause],
            )
            .await
            .unwrap()
            .get(0);
        assert_eq!(n, 1, "clause {clause} present");
    }
}

#[tokio::test]
async fn query_clauses_helper_filters_by_kind() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let rows = c
        .query("select clause_name from donto_query_clauses('policy')", &[])
        .await
        .unwrap();
    assert!(rows.len() >= 2);
    let names: Vec<String> = rows.iter().map(|r| r.get::<_, String>(0)).collect();
    assert!(names.contains(&"POLICY_ALLOWS".to_string()));
}
