//! DontoQL v2 — integration tests for the PRD §11 delta clauses.
//!
//! Covers AS_OF, MODALITY, EXTRACTION_LEVEL, and ordering FILTER ops
//! end-to-end against a live database; and verifies that the
//! deferred clauses (POLICY ALLOWS, SCHEMA_LENS, EXPANDS_FROM,
//! ORDER BY contradiction_pressure) parse but produce a structured
//! `Unsupported` error rather than silently misbehaving.

use chrono::{Duration, Utc};
use donto_client::{DontoClient, Object, StatementInput};
use donto_query::{evaluate, parse_dontoql, EvalError};

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let prefix = format!("test:dql2:{}", uuid::Uuid::new_v4().simple());
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .ok()?;
    Some((c, ctx, prefix))
}

/// Delete every statement under the test's context so subsequent
/// time-windowed analyzers (donto-analytics tests) don't pick the
/// rows up as conflicts. Must be called at the end of each test —
/// async Drop doesn't exist, so we do it explicitly.
async fn cleanup(c: &DontoClient, ctx: &str) {
    let Ok(conn) = c.pool().get().await else {
        return;
    };
    let _ = conn
        .execute("delete from donto_statement where context = $1", &[&ctx])
        .await;
    let _ = conn
        .execute("delete from donto_context where iri = $1", &[&ctx])
        .await;
}

// -----------------------------------------------------------------
// AS_OF — bitemporal time travel
// -----------------------------------------------------------------

#[tokio::test]
async fn as_of_returns_state_before_retraction() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };

    let subj = format!("{}/alice", ctx);
    // Assert at t0; retract at t1; AS_OF t0 should still see it,
    // current query should not.
    let stmt_id = c
        .assert(&StatementInput::new(&subj, "ex:knows", Object::iri("ex:bob")).with_context(&ctx))
        .await
        .unwrap();
    let t0 = Utc::now();
    // Tiny delay so tx_lower < t0 + epsilon.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    c.retract(stmt_id).await.unwrap();
    let _t1 = Utc::now();

    // Current state — no rows.
    let q_now = parse_dontoql(&format!(
        "MATCH ?s ex:knows ?o SCOPE include {ctx}",
        ctx = ctx
    ))
    .unwrap();
    let rows_now = evaluate(&c, &q_now).await.unwrap();
    assert_eq!(rows_now.len(), 0, "current state should have no rows");

    // AS_OF before the retract — row visible.
    let q_past = parse_dontoql(&format!(
        r#"MATCH ?s ex:knows ?o SCOPE include {ctx}
           AS_OF "{ts}""#,
        ctx = ctx,
        ts = t0.to_rfc3339()
    ))
    .unwrap();
    let rows_past = evaluate(&c, &q_past).await.unwrap();
    assert_eq!(
        rows_past.len(),
        1,
        "AS_OF before retract should see the original assertion"
    );

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn as_of_two_word_form_works() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let subj = format!("{}/x", ctx);
    c.assert(&StatementInput::new(&subj, "ex:p", Object::iri("ex:o")).with_context(&ctx))
        .await
        .unwrap();
    let now_plus = (Utc::now() + Duration::seconds(60)).to_rfc3339();
    let q = parse_dontoql(&format!(
        r#"MATCH ?s ex:p ?o SCOPE include {ctx}
           TRANSACTION_TIME AS_OF "{ts}""#,
        ctx = ctx,
        ts = now_plus
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1);

    cleanup(&c, &ctx).await;
}

// -----------------------------------------------------------------
// MODALITY — overlay-table filter
// -----------------------------------------------------------------

#[tokio::test]
async fn modality_filter_drops_unmatched_statements() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // Three assertions, two tagged 'descriptive', one 'inferred'.
    let s1 = c
        .assert(
            &StatementInput::new("ex:a", "ex:p", Object::iri("ex:x")).with_context(&ctx),
        )
        .await
        .unwrap();
    let s2 = c
        .assert(
            &StatementInput::new("ex:b", "ex:p", Object::iri("ex:y")).with_context(&ctx),
        )
        .await
        .unwrap();
    let s3 = c
        .assert(
            &StatementInput::new("ex:c", "ex:p", Object::iri("ex:z")).with_context(&ctx),
        )
        .await
        .unwrap();

    let conn = c.pool().get().await.unwrap();
    conn.execute(
        "select donto_set_modality($1, 'descriptive', 'test')",
        &[&s1],
    )
    .await
    .unwrap();
    conn.execute(
        "select donto_set_modality($1, 'descriptive', 'test')",
        &[&s2],
    )
    .await
    .unwrap();
    conn.execute(
        "select donto_set_modality($1, 'inferred', 'test')",
        &[&s3],
    )
    .await
    .unwrap();

    let q = parse_dontoql(&format!(
        "MATCH ?s ex:p ?o SCOPE include {ctx} MODALITY descriptive",
        ctx = ctx
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 2, "expected the two descriptive rows");

    let q2 = parse_dontoql(&format!(
        "MATCH ?s ex:p ?o SCOPE include {ctx} MODALITY descriptive, inferred",
        ctx = ctx
    ))
    .unwrap();
    let rows2 = evaluate(&c, &q2).await.unwrap();
    assert_eq!(rows2.len(), 3, "union of two modality tags returns all");

    cleanup(&c, &ctx).await;
}

// -----------------------------------------------------------------
// EXTRACTION_LEVEL — overlay-table filter
// -----------------------------------------------------------------

#[tokio::test]
async fn extraction_level_filter_drops_unmatched_statements() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let s1 = c
        .assert(
            &StatementInput::new("ex:a", "ex:p", Object::iri("ex:x")).with_context(&ctx),
        )
        .await
        .unwrap();
    let s2 = c
        .assert(
            &StatementInput::new("ex:b", "ex:p", Object::iri("ex:y")).with_context(&ctx),
        )
        .await
        .unwrap();

    let conn = c.pool().get().await.unwrap();
    conn.execute(
        "select donto_set_extraction_level($1, 'quoted', 'test')",
        &[&s1],
    )
    .await
    .unwrap();
    conn.execute(
        "select donto_set_extraction_level($1, 'manual_entry', 'test')",
        &[&s2],
    )
    .await
    .unwrap();

    let q = parse_dontoql(&format!(
        "MATCH ?s ex:p ?o SCOPE include {ctx} EXTRACTION_LEVEL quoted",
        ctx = ctx
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1);

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn extraction_level_filter_drops_when_no_overlay_row() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // No overlay row at all: filter should drop this statement
    // since EXTRACTION_LEVEL implies an explicit value match.
    c.assert(
        &StatementInput::new("ex:a", "ex:p", Object::iri("ex:x")).with_context(&ctx),
    )
    .await
    .unwrap();

    let q = parse_dontoql(&format!(
        "MATCH ?s ex:p ?o SCOPE include {ctx} EXTRACTION_LEVEL quoted",
        ctx = ctx
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 0);

    cleanup(&c, &ctx).await;
}

// -----------------------------------------------------------------
// FILTER — newly-parser-accessible ordering operators
// -----------------------------------------------------------------

#[tokio::test]
async fn filter_numeric_gt_works_against_literal_objects() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // Literal-valued statements: age(alice)=30, age(bob)=20.
    c.assert(
        &StatementInput::new("ex:alice", "ex:age", Object::Literal(literal_int(30)))
            .with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new("ex:bob", "ex:age", Object::Literal(literal_int(20)))
            .with_context(&ctx),
    )
    .await
    .unwrap();

    let q = parse_dontoql(&format!(
        "MATCH ?s ex:age ?n SCOPE include {ctx} FILTER ?n > 25",
        ctx = ctx
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1, "only alice (age 30) > 25");

    cleanup(&c, &ctx).await;
}

fn literal_int(n: i64) -> donto_client::Literal {
    donto_client::Literal {
        v: serde_json::json!(n),
        dt: "xsd:integer".into(),
        lang: None,
    }
}

// -----------------------------------------------------------------
// Deferred clauses — must fail cleanly, not silently
// -----------------------------------------------------------------

#[tokio::test]
async fn policy_allows_returns_unsupported() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    c.assert(
        &StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx),
    )
    .await
    .unwrap();
    let q = parse_dontoql(&format!(
        "MATCH ?s ?p ?o SCOPE include {ctx} POLICY ALLOWS read_metadata",
        ctx = ctx
    ))
    .unwrap();
    match evaluate(&c, &q).await {
        Err(EvalError::Unsupported(m)) => assert!(m.contains("POLICY ALLOWS")),
        other => panic!("expected Unsupported, got {other:?}"),
    }

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn schema_lens_returns_unsupported() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    c.assert(
        &StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx),
    )
    .await
    .unwrap();
    let q = parse_dontoql(&format!(
        "MATCH ?s ?p ?o SCOPE include {ctx} SCHEMA_LENS ex:lens-1",
        ctx = ctx
    ))
    .unwrap();
    match evaluate(&c, &q).await {
        Err(EvalError::Unsupported(m)) => assert!(m.contains("SCHEMA_LENS")),
        other => panic!("expected Unsupported, got {other:?}"),
    }

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn expands_from_returns_unsupported() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    c.assert(
        &StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx),
    )
    .await
    .unwrap();
    let q = parse_dontoql(&format!(
        "MATCH ?s ?p ?o SCOPE include {ctx} \
         EXPANDS_FROM concept ex:case_marking \
         USING schema_lens ex:linguistics-core",
        ctx = ctx
    ))
    .unwrap();
    match evaluate(&c, &q).await {
        Err(EvalError::Unsupported(m)) => assert!(m.contains("EXPANDS_FROM")),
        other => panic!("expected Unsupported, got {other:?}"),
    }

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn order_by_contradiction_pressure_returns_unsupported() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    c.assert(
        &StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx),
    )
    .await
    .unwrap();
    let q = parse_dontoql(&format!(
        "MATCH ?s ?p ?o SCOPE include {ctx} ORDER BY contradiction_pressure DESC",
        ctx = ctx
    ))
    .unwrap();
    match evaluate(&c, &q).await {
        Err(EvalError::Unsupported(m)) => assert!(m.contains("contradiction_pressure")),
        other => panic!("expected Unsupported, got {other:?}"),
    }

    cleanup(&c, &ctx).await;
}

// WITH evidence is non-fatal — it's recorded but doesn't change row shape today.
#[tokio::test]
async fn with_evidence_clause_does_not_error() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    c.assert(
        &StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx),
    )
    .await
    .unwrap();
    let q = parse_dontoql(&format!(
        "MATCH ?s ?p ?o SCOPE include {ctx} WITH evidence = redacted_if_required",
        ctx = ctx
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1);

    cleanup(&c, &ctx).await;
}
