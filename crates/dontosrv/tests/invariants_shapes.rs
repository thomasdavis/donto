//! Shape report invariants (PRD §16, §15 sidecar contract).
//!
//! "Shapes are overlays, not schemas. Shape failure is a report, not an
//!  ingestion error."
//!
//! These tests prove the validator never deletes statements, that reports
//! cache by (shape_iri, scope_fingerprint), and that the cache is consulted
//! before re-execution.

use axum::body::Body;
use axum::http::Request;
use donto_client::{DontoClient, Object, StatementInput};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::util::ServiceExt;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(Arc<dontosrv::AppState>, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let prefix = format!("test:shp:{}", uuid::Uuid::new_v4().simple());
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .ok()?;
    Some((Arc::new(dontosrv::AppState { client: c }), ctx))
}

async fn validate(state: Arc<dontosrv::AppState>, shape: &str, ctx: &str) -> Value {
    let app = dontosrv::router(state);
    let req = Request::builder()
        .method("POST")
        .uri("/shapes/validate")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "shape_iri": shape, "scope": {"include":[ctx]},
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1_048_576)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn validation_never_deletes_or_modifies_statements() {
    let Some((state, ctx)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;
    let id1 = c
        .assert(&StatementInput::new("ex:a", "ex:spouse", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    let id2 = c
        .assert(&StatementInput::new("ex:a", "ex:spouse", Object::iri("ex:c")).with_context(&ctx))
        .await
        .unwrap();

    // Run a violation-producing shape.
    let _ = validate(state.clone(), "builtin:functional/ex:spouse", &ctx).await;

    // Both rows still open.
    for id in [id1, id2] {
        let conn = c.pool().get().await.unwrap();
        let n: i64 = conn.query_one(
            "select count(*) from donto_statement where statement_id = $1 and upper(tx_time) is null",
            &[&id],
        ).await.unwrap().get(0);
        assert_eq!(n, 1, "shape validation must not retract statements");
    }
}

#[tokio::test]
async fn report_cache_short_circuits_repeat_validation() {
    let Some((state, ctx)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;
    c.assert(&StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();

    let v1 = validate(state.clone(), "builtin:functional/ex:p", &ctx).await;
    let v2 = validate(state.clone(), "builtin:functional/ex:p", &ctx).await;
    // First call: source = "builtin"; second: source = "cached".
    assert_eq!(v1.get("source").and_then(|x| x.as_str()), Some("builtin"));
    assert_eq!(
        v2.get("source").and_then(|x| x.as_str()),
        Some("cached"),
        "second identical validation must come from cache: got {v2}"
    );

    // Cache row count = 1 (the cache write happened on first call).
    let conn = c.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_shape_report where shape_iri = $1",
            &[&"builtin:functional/ex:p"],
        )
        .await
        .unwrap()
        .get(0);
    assert!(n >= 1);
}

#[tokio::test]
async fn datatype_shape_finds_mismatched_literals() {
    let Some((state, ctx)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;

    c.assert(
        &StatementInput::new(
            "ex:a",
            "ex:age",
            Object::lit(donto_client::Literal::integer(36)),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new(
            "ex:b",
            "ex:age",
            Object::lit(donto_client::Literal::string("forty")),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new("ex:c", "ex:age", Object::iri("ex:hidden_iri")).with_context(&ctx),
    )
    .await
    .unwrap();

    let v = validate(state.clone(), "builtin:datatype/ex:age/xsd:integer", &ctx).await;
    let viols = v.get("violations").and_then(|x| x.as_array()).unwrap();
    // Expect 2 violations: the string and the IRI.
    assert_eq!(viols.len(), 2, "datatype shape must flag both: {v}");
}

#[tokio::test]
async fn lean_iri_returns_sidecar_unavailable() {
    // PRD §15 operational contract: lean: IRIs go to the Lean engine,
    // which is not yet wired. The endpoint must say so explicitly.
    let Some((state, ctx)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let v = validate(state, "lean:my-shape", &ctx).await;
    assert_eq!(
        v.get("error").and_then(|x| x.as_str()),
        Some("sidecar_unavailable"),
        "lean: shapes must report sidecar_unavailable: {v}"
    );
}
