//! Integration test for migration 0118_maturity_audit.
//!
//! Verifies that a trigger fires on UPDATE of donto_statement.flags when
//! the maturity bits change, writing a donto_audit row with action='mature',
//! the correct from_e/to_e labels, and the actor from the `donto.actor` GUC.

mod common;

use uuid::Uuid;

/// Helper: direct INSERT into donto_statement bypassing donto_assert.
/// We need raw SQL control over flags to test the trigger without going
/// through the assert helper (which always starts at a chosen maturity).
async fn raw_insert_statement(
    c: &deadpool_postgres::Object,
    subject: &str,
    predicate: &str,
    object_iri: &str,
    context: &str,
    flags: i16,
) -> Uuid {
    c.query_one(
        "insert into donto_statement \
             (subject, predicate, object_iri, context, flags) \
         values ($1, $2, $3, $4, $5) \
         returning statement_id",
        &[&subject, &predicate, &object_iri, &context, &flags],
    )
    .await
    .expect("raw insert")
    .get(0)
}

#[tokio::test]
async fn maturity_update_writes_audit_row_with_guc_actor() {
    let client = pg_or_skip!(common::connect().await);
    let c = client.pool().get().await.unwrap();

    // Unique IRI prefix for per-test isolation.
    let prefix = common::tag("mat-audit");
    let ctx_iri = format!("{prefix}/ctx");
    let subject = format!("{prefix}/alice");
    let predicate = format!("{prefix}/birthYear");

    // Cleanup at test entry (not exit) per CLAUDE.md.
    c.execute(
        "delete from donto_statement where context like $1",
        &[&format!("{prefix}%")],
    )
    .await
    .ok();
    c.execute(
        "delete from donto_context where iri like $1",
        &[&format!("{prefix}%")],
    )
    .await
    .ok();

    // Ensure context.
    c.execute(
        "select donto_ensure_context($1, 'custom', 'permissive', null)",
        &[&ctx_iri],
    )
    .await
    .unwrap();

    // Step 1: Insert at E0 (stored 0, polarity 0 = asserted, flags = 0).
    let stmt_id: Uuid =
        raw_insert_statement(&c, &subject, &predicate, "ex:1900", &ctx_iri, 0i16).await;

    // Step 2: Set donto.actor GUC to a specific actor.
    let actor = "agent:human-curator-1";
    c.execute(&format!("set local donto.actor = '{actor}'"), &[])
        .await
        .unwrap();

    // Step 3: UPDATE flags to E2 (stored 2 << 2 = 8, polarity 0 = asserted).
    // flags = (2 << 2) | 0 = 8
    c.execute(
        "update donto_statement set flags = 8 where statement_id = $1",
        &[&stmt_id],
    )
    .await
    .unwrap();

    // Step 4: Assert one audit row was written with the expected fields.
    let rows = c
        .query(
            "select action, actor, \
                    detail->>'from_e' as from_e, \
                    detail->>'to_e' as to_e \
             from donto_audit \
             where statement_id = $1 and action = 'mature' \
             order by audit_id",
            &[&stmt_id],
        )
        .await
        .unwrap();

    assert_eq!(
        rows.len(),
        1,
        "expected exactly 1 'mature' audit row, got {}",
        rows.len()
    );

    let action: String = rows[0].get("action");
    let got_actor: Option<String> = rows[0].get("actor");
    let from_e: Option<String> = rows[0].get("from_e");
    let to_e: Option<String> = rows[0].get("to_e");

    assert_eq!(action, "mature");
    assert_eq!(
        got_actor.as_deref(),
        Some(actor),
        "actor must come from donto.actor GUC"
    );
    assert_eq!(from_e.as_deref(), Some("E0"), "from_e must be E0");
    assert_eq!(to_e.as_deref(), Some("E2"), "to_e must be E2");
}

#[tokio::test]
async fn no_audit_row_when_maturity_unchanged() {
    let client = pg_or_skip!(common::connect().await);
    let c = client.pool().get().await.unwrap();

    let prefix = common::tag("mat-audit-noop");
    let ctx_iri = format!("{prefix}/ctx");
    let subject = format!("{prefix}/bob");

    c.execute(
        "delete from donto_statement where context like $1",
        &[&format!("{prefix}%")],
    )
    .await
    .ok();
    c.execute(
        "delete from donto_context where iri like $1",
        &[&format!("{prefix}%")],
    )
    .await
    .ok();

    c.execute(
        "select donto_ensure_context($1, 'custom', 'permissive', null)",
        &[&ctx_iri],
    )
    .await
    .unwrap();

    // Insert at flags=0 (E0, asserted).
    let stmt_id: Uuid = raw_insert_statement(&c, &subject, "ex:p", "ex:o", &ctx_iri, 0i16).await;

    // Update only the polarity bits (negated=1), maturity stays 0 → no audit.
    c.execute(
        "update donto_statement set flags = 1 where statement_id = $1",
        &[&stmt_id],
    )
    .await
    .unwrap();

    let count: i64 = c
        .query_one(
            "select count(*) from donto_audit where statement_id = $1 and action = 'mature'",
            &[&stmt_id],
        )
        .await
        .unwrap()
        .get(0);

    assert_eq!(
        count, 0,
        "no audit row expected when maturity bits unchanged"
    );
}

#[tokio::test]
async fn actor_defaults_to_system_when_guc_not_set() {
    let client = pg_or_skip!(common::connect().await);
    let c = client.pool().get().await.unwrap();

    let prefix = common::tag("mat-audit-sys");
    let ctx_iri = format!("{prefix}/ctx");
    let subject = format!("{prefix}/carol");

    c.execute(
        "delete from donto_statement where context like $1",
        &[&format!("{prefix}%")],
    )
    .await
    .ok();
    c.execute(
        "delete from donto_context where iri like $1",
        &[&format!("{prefix}%")],
    )
    .await
    .ok();

    c.execute(
        "select donto_ensure_context($1, 'custom', 'permissive', null)",
        &[&ctx_iri],
    )
    .await
    .unwrap();

    // Ensure GUC is not set (reset it explicitly).
    c.execute("reset donto.actor", &[]).await.ok();

    let stmt_id: Uuid = raw_insert_statement(&c, &subject, "ex:p", "ex:o", &ctx_iri, 0i16).await;

    // Promote to E1 (flags = 4).
    c.execute(
        "update donto_statement set flags = 4 where statement_id = $1",
        &[&stmt_id],
    )
    .await
    .unwrap();

    let actor: Option<String> = c
        .query_one(
            "select actor from donto_audit where statement_id = $1 and action = 'mature'",
            &[&stmt_id],
        )
        .await
        .unwrap()
        .get(0);

    assert_eq!(
        actor.as_deref(),
        Some("system"),
        "actor must default to 'system' when GUC is unset"
    );
}
