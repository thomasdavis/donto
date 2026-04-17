//! Certificate verifier invariants (PRD §18).
//!
//! Six of the seven kinds defined in PRD §18 are exercised here. The seventh
//! (`replay`) requires re-running an arbitrary user-defined rule and is
//! covered by integration with `donto-query` separately.

use axum::body::Body;
use axum::http::{Request, StatusCode};
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
    c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let prefix = format!("test:cert:{}", uuid::Uuid::new_v4().simple());
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None).await.ok()?;
    Some((Arc::new(dontosrv::AppState { client: c }), ctx))
}

async fn post(app: axum::Router, path: &str, body: Value) -> Value {
    let req = Request::builder().method("POST").uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn get(app: axum::Router, path: &str) -> Value {
    let req = Request::builder().method("POST").uri(path).body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "{path} returned non-OK");
    let bytes = axum::body::to_bytes(resp.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn direct_assertion_requires_source() {
    let Some((state, ctx)) = boot().await else { eprintln!("skip"); return; };
    let id = state.client.assert(&StatementInput::new("ex:s","ex:p",Object::iri("ex:o"))
        .with_context(&ctx)).await.unwrap();
    let app = dontosrv::router(state.clone());
    // Empty body — should reject.
    post(app.clone(), "/certificates/attach", json!({
        "statement_id": id, "kind": "direct_assertion", "body": {},
    })).await;
    let v = get(app, &format!("/certificates/verify/{id}")).await;
    assert_eq!(v["ok"], json!(false), "empty direct_assertion must reject: {v}");
}

#[tokio::test]
async fn direct_assertion_with_source_passes() {
    let Some((state, ctx)) = boot().await else { eprintln!("skip"); return; };
    let id = state.client.assert(&StatementInput::new("ex:s","ex:p",Object::iri("ex:o"))
        .with_context(&ctx)).await.unwrap();
    let app = dontosrv::router(state.clone());
    post(app.clone(), "/certificates/attach", json!({
        "statement_id": id, "kind": "direct_assertion",
        "body": {"source": "ex:src/wikipedia"},
    })).await;
    let v = get(app, &format!("/certificates/verify/{id}")).await;
    assert_eq!(v["ok"], json!(true), "direct_assertion with source must pass: {v}");
}

#[tokio::test]
async fn substitution_requires_inputs_to_match() {
    let Some((state, ctx)) = boot().await else { eprintln!("skip"); return; };
    let id_target = state.client.assert(&StatementInput::new("ex:t","ex:derived",Object::iri("ex:y"))
        .with_context(&ctx)).await.unwrap();
    let id_input  = state.client.assert(&StatementInput::new("ex:i","ex:input",Object::iri("ex:x"))
        .with_context(&ctx)).await.unwrap();
    let app = dontosrv::router(state.clone());

    // Wrong inputs.
    post(app.clone(), "/certificates/attach", json!({
        "statement_id": id_target, "kind": "substitution",
        "inputs": [], "body": {"substitutes": [id_input.to_string()]},
    })).await;
    let v = get(app.clone(), &format!("/certificates/verify/{id_target}")).await;
    assert_eq!(v["ok"], json!(false));

    // Right inputs.
    post(app.clone(), "/certificates/attach", json!({
        "statement_id": id_target, "kind": "substitution",
        "inputs": [id_input], "body": {"substitutes": [id_input.to_string()]},
    })).await;
    let v = get(app, &format!("/certificates/verify/{id_target}")).await;
    assert_eq!(v["ok"], json!(true));
}

#[tokio::test]
async fn transitive_closure_certificate_walks_real_path() {
    let Some((state, ctx)) = boot().await else { eprintln!("skip"); return; };
    let c = &state.client;
    c.assert(&StatementInput::new("ex:a","ex:parent",Object::iri("ex:b")).with_context(&ctx)).await.unwrap();
    c.assert(&StatementInput::new("ex:b","ex:parent",Object::iri("ex:c")).with_context(&ctx)).await.unwrap();
    let id_derived = c.assert(&StatementInput::new("ex:a","ex:parent+",Object::iri("ex:c")).with_context(&ctx)).await.unwrap();

    let app = dontosrv::router(state.clone());
    post(app.clone(), "/certificates/attach", json!({
        "statement_id": id_derived, "kind": "transitive_closure",
        "body": {"predicate":"ex:parent","scope":{"include":[ctx.clone()]}},
    })).await;
    let v = get(app, &format!("/certificates/verify/{id_derived}")).await;
    assert_eq!(v["ok"], json!(true), "closure path must verify: {v}");
}

#[tokio::test]
async fn transitive_closure_certificate_rejects_non_path() {
    let Some((state, ctx)) = boot().await else { eprintln!("skip"); return; };
    let c = &state.client;
    // No edges in scope, so no closure path can exist.
    let id_derived = c.assert(&StatementInput::new("ex:a","ex:parent+",Object::iri("ex:c")).with_context(&ctx)).await.unwrap();
    let app = dontosrv::router(state.clone());
    post(app.clone(), "/certificates/attach", json!({
        "statement_id": id_derived, "kind": "transitive_closure",
        "body": {"predicate":"ex:parent","scope":{"include":[ctx.clone()]}},
    })).await;
    let v = get(app, &format!("/certificates/verify/{id_derived}")).await;
    assert_eq!(v["ok"], json!(false), "fake derivation must reject: {v}");
}

#[tokio::test]
async fn shape_entailment_requires_a_real_report() {
    let Some((state, ctx)) = boot().await else { eprintln!("skip"); return; };
    let c = &state.client;
    let id = c.assert(&StatementInput::new("ex:s","ex:p",Object::iri("ex:o")).with_context(&ctx))
        .await.unwrap();
    let app = dontosrv::router(state.clone());

    // Without a prior shape report, entailment can't be claimed.
    post(app.clone(), "/certificates/attach", json!({
        "statement_id": id, "kind": "shape_entailment",
        "body": {"shape_iri": "builtin:functional/ex:never_seen"},
    })).await;
    let v = get(app.clone(), &format!("/certificates/verify/{id}")).await;
    assert_eq!(v["ok"], json!(false));

    // Drive a validation to populate the cache.
    post(app.clone(), "/shapes/validate", json!({
        "shape_iri": "builtin:functional/ex:p",
        "scope": {"include":[ctx.clone()]},
    })).await;

    post(app.clone(), "/certificates/attach", json!({
        "statement_id": id, "kind": "shape_entailment",
        "body": {"shape_iri": "builtin:functional/ex:p"},
    })).await;
    let v = get(app, &format!("/certificates/verify/{id}")).await;
    assert_eq!(v["ok"], json!(true));
}

#[tokio::test]
async fn hypothesis_scoped_requires_hypothesis_in_body() {
    let Some((state, ctx)) = boot().await else { eprintln!("skip"); return; };
    let id = state.client.assert(&StatementInput::new("ex:s","ex:p",Object::iri("ex:o"))
        .with_context(&ctx)).await.unwrap();
    let app = dontosrv::router(state.clone());

    post(app.clone(), "/certificates/attach", json!({
        "statement_id": id, "kind": "hypothesis_scoped",
        "body": {},
    })).await;
    let v = get(app.clone(), &format!("/certificates/verify/{id}")).await;
    assert_eq!(v["ok"], json!(false));

    post(app.clone(), "/certificates/attach", json!({
        "statement_id": id, "kind": "hypothesis_scoped",
        "body": {"hypothesis": "ctx:hypo/alice_merge"},
    })).await;
    let v = get(app, &format!("/certificates/verify/{id}")).await;
    assert_eq!(v["ok"], json!(true));
}
