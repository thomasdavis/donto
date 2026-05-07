//! Evidence substrate: proof obligations.
//!
//!   * emit creates an open obligation
//!   * resolve transitions to terminal status
//!   * assign sets the agent and moves to in_progress
//!   * open_obligations filters by type and context
//!   * summary groups by type and status

use donto_client::{Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

#[tokio::test]
async fn emit_and_resolve_lifecycle() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("obl-life");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "obl-life").await;

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let obl_id = client
        .emit_obligation(stmt_id, "needs-coref", &ctx, 5, None, None)
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let row = c
        .query_one(
            "select status, priority, obligation_type from donto_proof_obligation \
             where obligation_id = $1",
            &[&obl_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("status"), "open");
    assert_eq!(row.get::<_, i16>("priority"), 5);
    assert_eq!(row.get::<_, String>("obligation_type"), "needs-coref");

    // Resolve it.
    let resolved = client
        .resolve_obligation(obl_id, None, "resolved")
        .await
        .unwrap();
    assert!(resolved);

    let row = c
        .query_one(
            "select status, resolved_at from donto_proof_obligation \
             where obligation_id = $1",
            &[&obl_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("status"), "resolved");
    assert!(row
        .get::<_, Option<chrono::DateTime<chrono::Utc>>>("resolved_at")
        .is_some());
}

#[tokio::test]
async fn resolve_already_resolved_is_noop() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("obl-noop");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "obl-noop").await;

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let obl_id = client
        .emit_obligation(stmt_id, "needs-source-support", &ctx, 0, None, None)
        .await
        .unwrap();

    client
        .resolve_obligation(obl_id, None, "resolved")
        .await
        .unwrap();
    let second = client
        .resolve_obligation(obl_id, None, "resolved")
        .await
        .unwrap();
    assert!(
        !second,
        "resolving an already-resolved obligation must return false"
    );
}

#[tokio::test]
async fn assign_obligation_to_agent() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("obl-assign");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "obl-assign").await;

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let agent_iri = format!("test:agent/{}", tag("obl-assign"));
    let agent_id = client
        .ensure_agent(&agent_iri, "llm", None, None)
        .await
        .unwrap();

    let obl_id = client
        .emit_obligation(stmt_id, "needs-entity-disambiguation", &ctx, 3, None, None)
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    c.execute(
        "select donto_assign_obligation($1, $2)",
        &[&obl_id, &agent_id],
    )
    .await
    .unwrap();

    let row = c
        .query_one(
            "select status, assigned_agent from donto_proof_obligation \
             where obligation_id = $1",
            &[&obl_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("status"), "in_progress");
    assert_eq!(
        row.get::<_, Option<uuid::Uuid>>("assigned_agent"),
        Some(agent_id)
    );
}

#[tokio::test]
async fn open_obligations_filtered() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("obl-filter");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "obl-filter").await;

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    // Emit three obligations: 2 coref, 1 temporal.
    for t in ["needs-coref", "needs-coref", "needs-temporal-grounding"] {
        client
            .emit_obligation(stmt_id, t, &ctx, 0, None, None)
            .await
            .unwrap();
    }

    let pool = client.pool();
    let c = pool.get().await.unwrap();

    // All open.
    let all: i64 = c
        .query_one(
            "select count(*) from donto_open_obligations(null, $1)",
            &[&ctx],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(all, 3);

    // Filtered by type.
    let coref: i64 = c
        .query_one(
            "select count(*) from donto_open_obligations($1, $2)",
            &[&"needs-coref", &ctx],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(coref, 2);
}

#[tokio::test]
async fn obligation_summary() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("obl-summ");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "obl-summ").await;

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let o1 = client
        .emit_obligation(stmt_id, "needs-coref", &ctx, 0, None, None)
        .await
        .unwrap();
    client
        .emit_obligation(stmt_id, "needs-coref", &ctx, 0, None, None)
        .await
        .unwrap();
    client
        .emit_obligation(stmt_id, "needs-source-support", &ctx, 0, None, None)
        .await
        .unwrap();

    // Resolve one coref.
    client
        .resolve_obligation(o1, None, "resolved")
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let rows = c
        .query(
            "select obligation_type, status, cnt from donto_obligation_summary($1)",
            &[&ctx],
        )
        .await
        .unwrap();

    let find = |typ: &str, status: &str| -> i64 {
        rows.iter()
            .find(|r| {
                r.get::<_, String>("obligation_type") == typ
                    && r.get::<_, String>("status") == status
            })
            .map(|r| r.get::<_, i64>("cnt"))
            .unwrap_or(0)
    };
    assert_eq!(find("needs-coref", "open"), 1);
    assert_eq!(find("needs-coref", "resolved"), 1);
    assert_eq!(find("needs-source-support", "open"), 1);
}

#[tokio::test]
async fn invalid_obligation_type_rejected() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("obl-bad");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "obl-bad").await;

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let err = client
        .emit_obligation(stmt_id, "needs-magic", &ctx, 0, None, None)
        .await
        .err()
        .expect("invalid obligation_type must error");
    assert!(format!("{err:?}").contains("obligation_type"));
}
