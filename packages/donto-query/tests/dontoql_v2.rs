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
async fn policy_allows_passes_statements_with_no_evidence_link() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // No evidence_link → policy is silent → keep the row (permissive).
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
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1, "permissive default for no-evidence row");

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn policy_allows_drops_statements_whose_source_denies_action() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // Two statements: one tied to a public source (allows read_metadata),
    // one tied to a restricted source (denies read_metadata).
    let s_pub = c
        .assert(
            &StatementInput::new("ex:a", "ex:p", Object::iri("ex:x")).with_context(&ctx),
        )
        .await
        .unwrap();
    let s_priv = c
        .assert(
            &StatementInput::new("ex:b", "ex:p", Object::iri("ex:y")).with_context(&ctx),
        )
        .await
        .unwrap();

    let conn = c.pool().get().await.unwrap();
    // Two policies and two source documents with respective policies.
    let pub_iri = format!("{ctx}/policy/public", ctx = ctx);
    let priv_iri = format!("{ctx}/policy/private", ctx = ctx);
    conn.execute(
        "insert into donto_policy_capsule (policy_iri, policy_kind, allowed_actions, created_by) \
         values ($1, 'public', \
                 jsonb_build_object('read_metadata', true, 'read_content', true), \
                 'test')",
        &[&pub_iri],
    )
    .await
    .unwrap();
    conn.execute(
        "insert into donto_policy_capsule (policy_iri, policy_kind, allowed_actions, created_by) \
         values ($1, 'private', \
                 jsonb_build_object('read_metadata', false, 'read_content', false), \
                 'test')",
        &[&priv_iri],
    )
    .await
    .unwrap();
    let doc_pub = format!("{ctx}/doc/public", ctx = ctx);
    let doc_priv = format!("{ctx}/doc/private", ctx = ctx);
    let pub_id: uuid::Uuid = conn
        .query_one(
            "insert into donto_document (iri, media_type, policy_id, status) \
             values ($1, 'text/plain', $2, 'registered') returning document_id",
            &[&doc_pub, &pub_iri],
        )
        .await
        .unwrap()
        .get(0);
    let priv_id: uuid::Uuid = conn
        .query_one(
            "insert into donto_document (iri, media_type, policy_id, status) \
             values ($1, 'text/plain', $2, 'registered') returning document_id",
            &[&doc_priv, &priv_iri],
        )
        .await
        .unwrap()
        .get(0);
    // Evidence links.
    conn.execute(
        "insert into donto_evidence_link (statement_id, link_type, target_document_id) \
         values ($1, 'extracted_from', $2)",
        &[&s_pub, &pub_id],
    )
    .await
    .unwrap();
    conn.execute(
        "insert into donto_evidence_link (statement_id, link_type, target_document_id) \
         values ($1, 'extracted_from', $2)",
        &[&s_priv, &priv_id],
    )
    .await
    .unwrap();

    let q = parse_dontoql(&format!(
        "MATCH ?s ex:p ?o SCOPE include {ctx} POLICY ALLOWS read_metadata",
        ctx = ctx
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(
        rows.len(),
        1,
        "private-source statement should be dropped, public-source kept"
    );

    let q_export = parse_dontoql(&format!(
        "MATCH ?s ex:p ?o SCOPE include {ctx} POLICY ALLOWS export_claims",
        ctx = ctx
    ))
    .unwrap();
    let rows_export = evaluate(&c, &q_export).await.unwrap();
    assert_eq!(
        rows_export.len(),
        0,
        "neither public nor private policy permits export_claims"
    );

    // Cleanup the policy/document/links we inserted; cleanup() handles statements + context.
    let _ = conn
        .execute(
            "delete from donto_evidence_link where statement_id = any($1)",
            &[&vec![s_pub, s_priv]],
        )
        .await;
    let _ = conn
        .execute(
            "delete from donto_document where document_id = any($1)",
            &[&vec![pub_id, priv_id]],
        )
        .await;
    let _ = conn
        .execute(
            "delete from donto_policy_capsule where policy_iri = any($1)",
            &[&vec![pub_iri, priv_iri]],
        )
        .await;
    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn policy_allows_unknown_action_errors_cleanly() {
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
        "MATCH ?s ?p ?o SCOPE include {ctx} POLICY ALLOWS frobnicate",
        ctx = ctx
    ))
    .unwrap();
    match evaluate(&c, &q).await {
        Err(EvalError::Unsupported(m)) => assert!(m.contains("unknown action")),
        other => panic!("expected Unsupported for unknown action, got {other:?}"),
    }

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn schema_lens_drops_statements_without_lens_membership() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let lens_iri = format!("{ctx}/lens/linguistics-core", ctx = ctx);
    let s_in = c
        .assert(
            &StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx),
        )
        .await
        .unwrap();
    let _s_out = c
        .assert(
            &StatementInput::new("ex:c", "ex:p", Object::iri("ex:d")).with_context(&ctx),
        )
        .await
        .unwrap();
    let conn = c.pool().get().await.unwrap();
    // The 'schema_lens' kind isn't allowed by donto_context.kind's
    // check constraint (only registered in donto_context_kind);
    // use 'custom' for the lens context, and rely on the
    // statement_context role='schema_lens' for the lens semantics.
    c.ensure_context(&lens_iri, "custom", "permissive", None)
        .await
        .unwrap();
    conn.execute(
        "select donto_add_statement_context($1, $2, 'schema_lens', 'test')",
        &[&s_in, &lens_iri],
    )
    .await
    .unwrap();

    let q = parse_dontoql(&format!(
        "MATCH ?s ex:p ?o SCOPE include {ctx} SCHEMA_LENS {lens}",
        ctx = ctx,
        lens = lens_iri
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1, "only the lens-attached statement survives");

    let _ = conn
        .execute("delete from donto_statement_context where context = $1", &[&lens_iri])
        .await;
    let _ = conn
        .execute("delete from donto_context where iri = $1", &[&lens_iri])
        .await;
    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn expands_from_resolves_concept_to_predicate_set_via_lens() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let concept = format!("{ctx}/concept/case-marking", ctx = ctx);
    let lens = format!("{ctx}/lens/linguistics-core", ctx = ctx);
    let pred_in = format!("{ctx}/predicate/markedBy", ctx = ctx);
    let pred_out = "ex:unrelated-predicate";

    // Two statements: one with pred_in (aligned to concept under the
    // lens), one with pred_out (not aligned).
    c.assert(
        &StatementInput::new("ex:a", &pred_in, Object::iri("ex:x")).with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new("ex:b", pred_out, Object::iri("ex:y")).with_context(&ctx),
    )
    .await
    .unwrap();

    let conn = c.pool().get().await.unwrap();
    // donto_predicate_alignment.scope FKs to donto_context.iri,
    // so the lens has to exist first as a context.
    c.ensure_context(&lens, "custom", "permissive", None)
        .await
        .unwrap();
    // Register the alignment under the lens.
    conn.execute(
        "insert into donto_predicate_alignment \
            (source_iri, target_iri, relation, confidence, scope, safe_for_query_expansion) \
         values ($1, $2, 'sub_property_of', 0.95, $3, true)",
        &[&concept, &pred_in, &lens],
    )
    .await
    .unwrap();

    let q = parse_dontoql(&format!(
        "MATCH ?s ?p ?o SCOPE include {ctx} \
         PREDICATES STRICT \
         EXPANDS_FROM concept {concept} USING schema_lens {lens}",
        ctx = ctx,
        concept = concept,
        lens = lens
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(
        rows.len(),
        1,
        "only the predicate aligned under the lens survives"
    );

    let _ = conn
        .execute(
            "delete from donto_predicate_alignment where scope = $1",
            &[&lens],
        )
        .await;
    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn order_by_contradiction_pressure_orders_attacked_statements_first() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // Two statements; one will receive an attack edge.
    let s_attacked = c
        .assert(
            &StatementInput::new("ex:a", "ex:p", Object::iri("ex:x")).with_context(&ctx),
        )
        .await
        .unwrap();
    let s_clean = c
        .assert(
            &StatementInput::new("ex:b", "ex:p", Object::iri("ex:y")).with_context(&ctx),
        )
        .await
        .unwrap();
    // An attacker statement, and an argument edge that 'rebuts' s_attacked.
    let s_attacker = c
        .assert(
            &StatementInput::new("ex:c", "ex:p", Object::iri("ex:z")).with_context(&ctx),
        )
        .await
        .unwrap();
    let conn = c.pool().get().await.unwrap();
    conn.execute(
        "insert into donto_argument \
            (source_statement_id, target_statement_id, relation, context) \
         values ($1, $2, 'rebuts', $3)",
        &[&s_attacker, &s_attacked, &ctx],
    )
    .await
    .unwrap();

    let q = parse_dontoql(&format!(
        "MATCH ?s ex:p ?o SCOPE include {ctx} \
         ORDER BY contradiction_pressure DESC \
         LIMIT 10",
        ctx = ctx
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    // s_attacker also matches ex:p, but only s_attacked has attack
    // pressure > 0. Both s_clean and s_attacker have pressure 0.
    // So the first row should be s_attacked's binding (?s = ex:a).
    assert!(rows.len() >= 1);
    let first_s = rows[0].0.get("s").map(|t| format!("{t:?}")).unwrap_or_default();
    assert!(
        first_s.contains("ex:a"),
        "first row's ?s should be ex:a (the attacked subject), got {first_s}"
    );

    // ASC reverses: attacked subject sorts last.
    let q_asc = parse_dontoql(&format!(
        "MATCH ?s ex:p ?o SCOPE include {ctx} \
         ORDER BY contradiction_pressure ASC \
         LIMIT 10",
        ctx = ctx
    ))
    .unwrap();
    let rows_asc = evaluate(&c, &q_asc).await.unwrap();
    let last_s = rows_asc
        .last()
        .and_then(|r| r.0.get("s"))
        .map(|t| format!("{t:?}"))
        .unwrap_or_default();
    assert!(
        last_s.contains("ex:a"),
        "ASC ordering should put attacked subject last, got {last_s}"
    );

    let _ = conn
        .execute(
            "delete from donto_argument where source_statement_id = $1",
            &[&s_attacker],
        )
        .await;
    cleanup(&c, &ctx).await;
}

// WITH evidence — attaches evidence_link rows to the result.
#[tokio::test]
async fn with_evidence_none_returns_empty_evidence_vec() {
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
        "MATCH ?s ?p ?o SCOPE include {ctx} WITH evidence = none",
        ctx = ctx
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].1.is_empty(),
        "WITH evidence = none should leave evidence empty"
    );
    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn with_evidence_full_attaches_evidence_link_rows() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let stmt = c
        .assert(
            &StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx),
        )
        .await
        .unwrap();
    let conn = c.pool().get().await.unwrap();
    let doc_iri = format!("{ctx}/doc/citable", ctx = ctx);
    let doc_id: uuid::Uuid = conn
        .query_one(
            "insert into donto_document (iri, media_type, status) \
             values ($1, 'text/plain', 'registered') returning document_id",
            &[&doc_iri],
        )
        .await
        .unwrap()
        .get(0);
    conn.execute(
        "insert into donto_evidence_link \
            (statement_id, link_type, target_document_id, confidence) \
         values ($1, 'extracted_from', $2, 0.9)",
        &[&stmt, &doc_id],
    )
    .await
    .unwrap();

    let q = parse_dontoql(&format!(
        "MATCH ?s ?p ?o SCOPE include {ctx} WITH evidence = full",
        ctx = ctx
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1.len(), 1, "one evidence link attached");
    assert_eq!(rows[0].1[0].link_type, "extracted_from");
    assert_eq!(rows[0].1[0].target_document_iri.as_deref(), Some(doc_iri.as_str()));
    assert!((rows[0].1[0].confidence.unwrap() - 0.9).abs() < 1e-9);

    let _ = conn
        .execute(
            "delete from donto_evidence_link where statement_id = $1",
            &[&stmt],
        )
        .await;
    let _ = conn
        .execute("delete from donto_document where document_id = $1", &[&doc_id])
        .await;
    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn with_evidence_redacted_drops_evidence_when_policy_denies() {
    let Some((c, ctx, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let stmt = c
        .assert(
            &StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx),
        )
        .await
        .unwrap();
    let conn = c.pool().get().await.unwrap();
    let policy_iri = format!("{ctx}/policy/no-anchor", ctx = ctx);
    conn.execute(
        "insert into donto_policy_capsule (policy_iri, policy_kind, allowed_actions, created_by) \
         values ($1, 'private', \
                 jsonb_build_object('view_anchor_location', false), \
                 'test')",
        &[&policy_iri],
    )
    .await
    .unwrap();
    let doc_iri = format!("{ctx}/doc/restricted", ctx = ctx);
    let doc_id: uuid::Uuid = conn
        .query_one(
            "insert into donto_document (iri, media_type, policy_id, status) \
             values ($1, 'text/plain', $2, 'registered') returning document_id",
            &[&doc_iri, &policy_iri],
        )
        .await
        .unwrap()
        .get(0);
    conn.execute(
        "insert into donto_evidence_link \
            (statement_id, link_type, target_document_id) \
         values ($1, 'extracted_from', $2)",
        &[&stmt, &doc_id],
    )
    .await
    .unwrap();

    let q = parse_dontoql(&format!(
        "MATCH ?s ?p ?o SCOPE include {ctx} WITH evidence = redacted_if_required",
        ctx = ctx
    ))
    .unwrap();
    let rows = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows.len(), 1, "the row itself is not dropped by WITH evidence");
    assert!(
        rows[0].1.is_empty(),
        "evidence row is dropped because policy denies view_anchor_location"
    );

    // Now flip the policy to allow → evidence reappears.
    conn.execute(
        "update donto_policy_capsule set \
           allowed_actions = jsonb_build_object('view_anchor_location', true) \
         where policy_iri = $1",
        &[&policy_iri],
    )
    .await
    .unwrap();
    let rows2 = evaluate(&c, &q).await.unwrap();
    assert_eq!(rows2[0].1.len(), 1, "evidence reappears once policy allows");

    let _ = conn
        .execute(
            "delete from donto_evidence_link where statement_id = $1",
            &[&stmt],
        )
        .await;
    let _ = conn
        .execute("delete from donto_document where document_id = $1", &[&doc_id])
        .await;
    let _ = conn
        .execute("delete from donto_policy_capsule where policy_iri = $1", &[&policy_iri])
        .await;
    cleanup(&c, &ctx).await;
}
