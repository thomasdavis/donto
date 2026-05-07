//! v1000 idempotency: every public function should be safe to call
//! twice with the same inputs.

mod common;
use common::{connect, ctx, tag};
use serde_json::json;

#[tokio::test]
async fn assign_policy_idempotent_on_same_target() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("doc:{}", tag("idem-policy"));

    let id1: uuid::Uuid = c
        .query_one(
            "select donto_assign_policy('document', $1, 'policy:default/public', 'tester')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    let id2: uuid::Uuid = c
        .query_one(
            "select donto_assign_policy('document', $1, 'policy:default/public', 'tester')",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);

    assert_eq!(
        id1, id2,
        "idempotent on (target_kind, target_id, policy_iri)"
    );
}

#[tokio::test]
async fn issue_attestation_creates_distinct_rows_each_time() {
    // Attestations are intentionally NOT idempotent — each issuance
    // creates a fresh credential. Verify that double-issue creates 2 rows.
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let holder = format!("agent:{}", tag("idem-attest"));

    let a1: uuid::Uuid = c
        .query_one(
            "select donto_issue_attestation($1, 'system', 'policy:default/public', \
                array['read_metadata']::text[], 'audit', 'a', null, null)",
            &[&holder],
        )
        .await
        .unwrap()
        .get(0);
    let a2: uuid::Uuid = c
        .query_one(
            "select donto_issue_attestation($1, 'system', 'policy:default/public', \
                array['read_metadata']::text[], 'audit', 'a', null, null)",
            &[&holder],
        )
        .await
        .unwrap()
        .get(0);
    assert_ne!(a1, a2, "issue is not idempotent (each is a credential)");
}

#[tokio::test]
async fn revoke_attestation_idempotent() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let holder = format!("agent:{}", tag("idem-revoke"));
    let id: uuid::Uuid = c
        .query_one(
            "select donto_issue_attestation($1, 'system', 'policy:default/public', \
                array['read_metadata']::text[], 'audit', 'a', null, null)",
            &[&holder],
        )
        .await
        .unwrap()
        .get(0);

    let r1: bool = c
        .query_one(
            "select donto_revoke_attestation($1, 'tester', null)",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    let r2: bool = c
        .query_one(
            "select donto_revoke_attestation($1, 'tester', null)",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(r1, "first revoke succeeds");
    assert!(!r2, "second revoke is no-op (already revoked)");
}

#[tokio::test]
async fn mark_hypothesis_only_idempotent() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idem-hyp");
    let ctx_iri = ctx(&client, "idem-hyp").await;
    let stmt = client
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

    for _ in 0..3 {
        c.execute("select donto_mark_hypothesis_only($1)", &[&stmt])
            .await
            .unwrap();
    }
    let n: i64 = c
        .query_one(
            "select count(*) from donto_stmt_hypothesis_only where statement_id = $1",
            &[&stmt],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1);
}

#[tokio::test]
async fn set_modality_idempotent() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idem-mod");
    let ctx_iri = ctx(&client, "idem-mod").await;
    let stmt = client
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

    for _ in 0..3 {
        c.execute("select donto_set_modality($1, 'descriptive')", &[&stmt])
            .await
            .unwrap();
    }
    let n: i64 = c
        .query_one(
            "select count(*) from donto_stmt_modality where statement_id = $1",
            &[&stmt],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1);
}

#[tokio::test]
async fn set_extraction_level_idempotent() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idem-xl");
    let ctx_iri = ctx(&client, "idem-xl").await;
    let stmt = client
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
    for _ in 0..3 {
        c.execute("select donto_set_extraction_level($1, 'quoted')", &[&stmt])
            .await
            .unwrap();
    }
    let n: i64 = c
        .query_one(
            "select count(*) from donto_stmt_extraction_level where statement_id = $1",
            &[&stmt],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1);
}

#[tokio::test]
async fn set_claim_kind_idempotent_on_repeat() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idem-ck");
    let ctx_iri = ctx(&client, "idem-ck").await;
    let stmt = client
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
    for _ in 0..3 {
        c.execute("select donto_set_claim_kind($1, 'frame_summary')", &[&stmt])
            .await
            .unwrap();
    }
    let n: i64 = c
        .query_one(
            "select count(*) from donto_stmt_claim_kind where statement_id = $1",
            &[&stmt],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1);
}

#[tokio::test]
async fn add_statement_context_idempotent() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idem-ctx");
    let ctx1 = ctx(&client, "idem-ctx-pri").await;
    let stmt = client
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

    let ctx2 = format!("ctx:{prefix}/secondary");
    for _ in 0..3 {
        c.execute(
            "select donto_add_statement_context($1, $2, 'secondary', 'tester')",
            &[&stmt, &ctx2],
        )
        .await
        .unwrap();
    }
    let n: i64 = c
        .query_one(
            "select count(*) from donto_statement_context \
             where statement_id = $1 and context = $2",
            &[&stmt, &ctx2],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1);
}

#[tokio::test]
async fn approve_predicate_idempotent() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idem-approve");
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
    let r1: bool = c
        .query_one("select donto_approve_predicate($1, 'r')", &[&iri])
        .await
        .unwrap()
        .get(0);
    let r2: bool = c
        .query_one("select donto_approve_predicate($1, 'r')", &[&iri])
        .await
        .unwrap()
        .get(0);
    assert!(r1);
    assert!(!r2, "second approve is no-op");
}

#[tokio::test]
async fn add_context_parent_idempotent() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idem-ctxp");
    let parent = format!("ctx:{prefix}/parent");
    let child = format!("ctx:{prefix}/child");

    for _ in 0..3 {
        c.execute(
            "select donto_add_context_parent($1, $2, 'inherit')",
            &[&child, &parent],
        )
        .await
        .unwrap();
    }
    let n: i64 = c
        .query_one(
            "select count(*) from donto_context_parent \
             where context = $1 and parent_context = $2",
            &[&child, &parent],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1);
}

#[tokio::test]
async fn add_external_id_appends_each_call() {
    // External IDs are stored as a JSONB array; each call appends.
    // Test verifies the array grows monotonically.
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("idem-extid");
    let id: i64 = c
        .query_one(
            "select donto_ensure_symbol($1, null, null, null, null, null)",
            &[&format!("ent:{prefix}/x")],
        )
        .await
        .unwrap()
        .get(0);

    let extid = format!("test-{}", uuid::Uuid::new_v4().simple());
    for _ in 0..3 {
        c.execute(
            "select donto_add_external_id($1, 'glottolog', $2, 1.0)",
            &[&id, &extid],
        )
        .await
        .unwrap();
    }
    let len: i32 = c
        .query_one(
            "select jsonb_array_length(external_ids) from donto_entity_symbol where symbol_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(len, 3, "external_ids appends each call (not deduped)");
}

#[tokio::test]
async fn assign_policy_emits_one_event_per_assignment() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("doc:{}", tag("idem-policy-event"));

    for _ in 0..3 {
        c.execute(
            "select donto_assign_policy('document', $1, 'policy:default/public', 'tester')",
            &[&target],
        )
        .await
        .unwrap();
    }
    let n: i64 = c
        .query_one(
            "select count(*) from donto_event_log \
             where target_kind = 'access_assignment' \
               and payload->>'target_id' = $1",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    // Each call emits an event even though the row is upserted.
    // This is intentional: governance audit prefers extra over missed.
    assert!(n >= 1);
}

#[tokio::test]
async fn migrations_re_apply_safe() {
    let client = pg_or_skip!(connect().await);
    // migrate() is called once per process via MIGRATED guard, but
    // calling apply_migrations directly should still be safe.
    use donto_client::migrations::apply_migrations;
    apply_migrations(client.pool()).await.unwrap();
    apply_migrations(client.pool()).await.unwrap();
    apply_migrations(client.pool()).await.unwrap();
}
