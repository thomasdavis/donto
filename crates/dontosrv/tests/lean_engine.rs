//! End-to-end Lean engine test.
//!
//! Spawns the real `donto_engine` Lean binary as a child process, sends a
//! `validate_request` for the `lean:builtin/parent-child-age-gap` shape,
//! confirms the report comes back with the violations the Lean code
//! computed.
//!
//! Test self-skips if either Postgres or the Lean engine is unavailable.
//! Engine path resolution:
//!   1. $DONTO_LEAN_ENGINE
//!   2. <repo>/lean/.lake/build/bin/donto_engine (default for `cargo test`
//!      invoked from the repo root after `cd lean && lake build`)

use donto_client::{DontoClient, Object, Polarity, StatementInput};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

fn engine_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("DONTO_LEAN_ENGINE") {
        let p = PathBuf::from(p);
        return p.exists().then_some(p);
    }
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); p.pop();   // crates/dontosrv/ → repo root
    p.push("lean/.lake/build/bin/donto_engine");
    p.exists().then_some(p)
}

async fn boot() -> Option<(Arc<dontosrv::AppState>, String)> {
    let bin = engine_path()?;
    let client = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = client.pool().get().await.ok()?;
    client.migrate().await.ok()?;

    let lean = match dontosrv::lean::LeanClient::try_spawn(bin.to_str()).await {
        Ok(Some(c)) => c,
        Ok(None)    => { eprintln!("lean: spawn returned None"); return None; }
        Err(e)      => { eprintln!("lean: spawn failed: {e}"); return None; }
    };
    let prefix = format!("test:lean:{}", uuid::Uuid::new_v4().simple());
    let ctx = format!("{prefix}/ctx");
    client.ensure_context(&ctx, "custom", "permissive", None).await.ok()?;
    Some((Arc::new(dontosrv::AppState { client, lean: Some(lean) }), ctx))
}

async fn validate_via_http(state: Arc<dontosrv::AppState>, shape: &str, ctx: &str) -> Value {
    use axum::body::Body;
    use axum::http::Request;
    use tower::util::ServiceExt;
    let app = dontosrv::router(state);
    let req = Request::builder().method("POST").uri("/shapes/validate")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "shape_iri": shape, "scope": {"include":[ctx]},
        })).unwrap())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn lean_parent_child_age_gap_finds_violation() {
    let Some((state, ctx)) = boot().await else { eprintln!("skip"); return; };
    let c = &state.client;

    // Set up a clearly-violating parent/child pair: 5 year gap.
    c.assert(&StatementInput::new("ex:alice", "ex:parentOf", Object::iri("ex:bob"))
        .with_context(&ctx)).await.unwrap();
    c.assert(&StatementInput::new("ex:alice", "ex:birthYear",
        Object::lit(donto_client::Literal::integer(1850))).with_context(&ctx)).await.unwrap();
    c.assert(&StatementInput::new("ex:bob", "ex:birthYear",
        Object::lit(donto_client::Literal::integer(1855))).with_context(&ctx)).await.unwrap();

    let v = validate_via_http(state.clone(), "lean:builtin/parent-child-age-gap", &ctx).await;
    assert_eq!(v.get("source").and_then(|x| x.as_str()), Some("lean"),
        "report should be sourced from the Lean engine: {v}");
    let report = v.get("report").expect("envelope must include `report`");
    let viols  = report.get("violations").and_then(|x| x.as_array())
        .expect("report must include `violations`");
    assert_eq!(viols.len(), 1, "got: {v}");
    let r = viols[0].get("reason").and_then(|x| x.as_str()).unwrap_or("");
    assert!(r.contains("only 5y older"), "wrong reason: {r}");
}

#[tokio::test]
async fn lean_parent_child_age_gap_passes_reasonable_pair() {
    let Some((state, ctx)) = boot().await else { eprintln!("skip"); return; };
    let c = &state.client;

    // Reasonable: 25 year gap.
    c.assert(&StatementInput::new("ex:p1", "ex:parentOf", Object::iri("ex:c1"))
        .with_context(&ctx)).await.unwrap();
    c.assert(&StatementInput::new("ex:p1", "ex:birthYear",
        Object::lit(donto_client::Literal::integer(1900))).with_context(&ctx)).await.unwrap();
    c.assert(&StatementInput::new("ex:c1", "ex:birthYear",
        Object::lit(donto_client::Literal::integer(1925))).with_context(&ctx)).await.unwrap();

    let v = validate_via_http(state.clone(), "lean:builtin/parent-child-age-gap", &ctx).await;
    let report = v.get("report").expect("report present");
    let viols  = report.get("violations").and_then(|x| x.as_array()).unwrap();
    assert_eq!(viols.len(), 0, "reasonable 25y gap should produce no violations: {v}");
}

#[tokio::test]
async fn lean_engine_is_required_for_lean_iris() {
    // Without an engine wired, the same shape iri returns sidecar_unavailable.
    let client = match DontoClient::from_dsn(&dsn()) {
        Ok(c) => c, Err(_) => { eprintln!("skip (no postgres)"); return; }
    };
    let _ = match client.pool().get().await { Ok(c) => c, Err(_) => { eprintln!("skip"); return; } };
    client.migrate().await.unwrap();
    let prefix = format!("test:lean:noengine:{}", uuid::Uuid::new_v4().simple());
    let ctx = format!("{prefix}/ctx");
    client.ensure_context(&ctx, "custom", "permissive", None).await.unwrap();
    let state = Arc::new(dontosrv::AppState { client, lean: None });

    let v = validate_via_http(state, "lean:builtin/parent-child-age-gap", &ctx).await;
    assert_eq!(v.get("error").and_then(|x| x.as_str()), Some("sidecar_unavailable"),
        "without --lean-engine, lean: shapes must report sidecar_unavailable: {v}");
}
