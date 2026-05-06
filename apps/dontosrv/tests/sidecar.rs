//! In-process integration test for dontosrv. Spins the router with a real
//! DontoClient and drives requests via tower::ServiceExt.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use donto_client::{DontoClient, Literal, Object, StatementInput};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::util::ServiceExt;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn bootstrap() -> Option<(Arc<dontosrv::AppState>, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let prefix = format!("test:sidecar:{}", uuid::Uuid::new_v4().simple());
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .ok()?;
    let state = Arc::new(dontosrv::AppState {
        client: c,
        lean: None,
    });
    Some((state, ctx))
}

async fn post_json(app: axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1_048_576)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

#[tokio::test]
async fn health_endpoint() {
    let Some((state, _ctx)) = bootstrap().await else {
        eprintln!("skip");
        return;
    };
    let app = dontosrv::router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn sparql_endpoint_runs_query() {
    let Some((state, ctx)) = bootstrap().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;
    c.assert(&StatementInput::new("ex:s", "ex:p", Object::iri("ex:o")).with_context(&ctx))
        .await
        .unwrap();

    // Inject scope by adding a SCOPE preset alias would be cleaner, but for the
    // test we use the inline scope feature in DontoQL.
    let app = dontosrv::router(state.clone());
    let (status, v) = post_json(
        app,
        "/dontoql",
        json!({
            "query": format!("SCOPE include <{ctx}> MATCH ?s ex:p ?o PROJECT ?s, ?o"),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = v.get("rows").and_then(|x| x.as_array()).unwrap();
    assert_eq!(rows.len(), 1, "got: {v}");
}

#[tokio::test]
async fn shape_validate_functional_finds_violations() {
    let Some((state, ctx)) = bootstrap().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;
    c.assert(
        &StatementInput::new("ex:alice", "ex:spouse", Object::iri("ex:bob")).with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new("ex:alice", "ex:spouse", Object::iri("ex:carol")).with_context(&ctx),
    )
    .await
    .unwrap();

    let app = dontosrv::router(state.clone());
    let (status, v) = post_json(
        app,
        "/shapes/validate",
        json!({
            "shape_iri": "builtin:functional/ex:spouse",
            "scope": {"include":[ctx], "include_descendants":true},
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // The cached path may return either an inline ValidateResp OR the cached
    // {report} envelope from the second call. First call always inline.
    if let Some(viols) = v.get("violations").and_then(|x| x.as_array()) {
        assert_eq!(viols.len(), 1);
    } else if let Some(rep) = v.get("report") {
        let viols = rep.get("violations").and_then(|x| x.as_array()).unwrap();
        assert_eq!(viols.len(), 1);
    } else {
        panic!("unexpected response: {v}");
    }
}

#[tokio::test]
async fn rule_derive_transitive_closure_emits_into_context() {
    let Some((state, ctx)) = bootstrap().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;
    c.assert(&StatementInput::new("ex:a", "ex:parent", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    c.assert(&StatementInput::new("ex:b", "ex:parent", Object::iri("ex:c")).with_context(&ctx))
        .await
        .unwrap();
    c.assert(&StatementInput::new("ex:c", "ex:parent", Object::iri("ex:d")).with_context(&ctx))
        .await
        .unwrap();

    let into = format!("ctx:derived:{}", uuid::Uuid::new_v4().simple());
    let app = dontosrv::router(state.clone());
    let (status, v) = post_json(
        app,
        "/rules/derive",
        json!({
            "rule_iri": "builtin:transitive/ex:parent",
            "scope":    {"include":[ctx.clone()]},
            "into":     into,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let emitted = v.get("emitted").and_then(|x| x.as_u64()).unwrap();
    // Closure of a→b→c→d adds (a,c), (a,d), (b,d). Already includes (a,b),
    // (b,c), (c,d) too because the recursive CTE seeds from the edges.
    // Total expected: 6.
    assert_eq!(emitted, 6, "got: {v}");
}

#[tokio::test]
async fn certificate_attach_and_verify_direct() {
    let Some((state, ctx)) = bootstrap().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;
    let id = c
        .assert(
            &StatementInput::new("ex:s", "ex:claim", Object::lit(Literal::string("hi")))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let app = dontosrv::router(state.clone());
    let (st, _v) = post_json(
        app.clone(),
        "/certificates/attach",
        json!({
            "statement_id": id,
            "kind": "direct_assertion",
            "body": {"source": "ex:src/wikipedia"},
        }),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let req = Request::builder()
        .method("POST")
        .uri(format!("/certificates/verify/{id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1_048_576)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v.get("ok").and_then(|x| x.as_bool()), Some(true), "got {v}");
}
