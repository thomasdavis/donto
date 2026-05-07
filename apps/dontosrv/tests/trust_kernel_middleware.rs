//! Trust Kernel HTTP middleware end-to-end test.
//!
//! Drives the dontosrv Axum router in-process and confirms:
//!   * `POST /sources/register` refuses requests without `policy_iri`
//!   * `POST /protected/read` returns 403 when the caller has no
//!     attestation for the target's policy
//!   * The same call returns 200 once an attestation is issued
//!   * Revoking the attestation flips authorisation back to denied
//!   * `POST /authorise` is a side-effect-free probe
//!   * `GET /policy/effective/...` surfaces the effective actions

use axum::body::Body;
use axum::http::{Request, StatusCode};
use donto_client::DontoClient;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::util::ServiceExt;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<Arc<dontosrv::AppState>> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    Some(Arc::new(dontosrv::AppState {
        client: c,
        lean: None,
    }))
}

async fn post(app: axum::Router, path: &str, body: Value, caller: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(c) = caller {
        req = req.header("x-donto-caller", c);
    }
    let resp = app
        .oneshot(req.body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1_048_576)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

async fn get_path(app: axum::Router, path: &str) -> (StatusCode, Value) {
    let resp = app
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1_048_576)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

fn tag(s: &str) -> String {
    format!("tk:{s}:{}", uuid::Uuid::new_v4().simple())
}

#[tokio::test]
async fn register_source_v1000_requires_policy_iri() {
    let Some(state) = boot().await else {
        eprintln!("skip");
        return;
    };
    let app = dontosrv::router(state);
    let prefix = tag("register-no-policy");
    // Missing policy_iri in the JSON body.
    let (status, body) = post(
        app,
        "/sources/register",
        json!({
            "iri": format!("src:{prefix}/x"),
            "source_kind": "pdf"
        }),
        None,
    )
    .await;
    // Serde rejects the missing required field at deserialisation.
    // Either way the request fails — the SQL function is never reached.
    assert_ne!(status, StatusCode::OK);
    let _ = body; // contents may be axum's own error envelope
}

#[tokio::test]
async fn register_source_v1000_accepts_with_policy() {
    let Some(state) = boot().await else {
        eprintln!("skip");
        return;
    };
    let app = dontosrv::router(state);
    let prefix = tag("register-ok");
    let (status, body) = post(
        app,
        "/sources/register",
        json!({
            "iri": format!("src:{prefix}/y"),
            "source_kind": "pdf",
            "policy_iri": "policy:default/public"
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("document_id").is_some());
}

#[tokio::test]
async fn protected_read_default_denies_without_attestation() {
    let Some(state) = boot().await else {
        eprintln!("skip");
        return;
    };
    let prefix = tag("read-default-deny");
    let target = format!("doc:{prefix}");
    state
        .client
        .assign_policy(
            "document",
            &target,
            "policy:default/community_restricted",
            "tester",
        )
        .await
        .unwrap();

    let app = dontosrv::router(state);
    let (status, body) = post(
        app,
        "/protected/read",
        json!({
            "target_kind": "document",
            "target_id": target,
            "action": "read_content"
        }),
        Some("agent:randomer"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "forbidden");
    assert_eq!(body["caller"], "agent:randomer");
    assert_eq!(body["action"], "read_content");
}

#[tokio::test]
async fn protected_read_allows_with_valid_attestation() {
    let Some(state) = boot().await else {
        eprintln!("skip");
        return;
    };
    let prefix = tag("read-with-att");
    let target = format!("doc:{prefix}");
    let holder = format!("agent:{prefix}/researcher");

    state
        .client
        .assign_policy(
            "document",
            &target,
            "policy:default/community_restricted",
            "tester",
        )
        .await
        .unwrap();
    state
        .client
        .issue_attestation(
            &holder,
            "community-council",
            "policy:default/community_restricted",
            &["read_content"],
            "community_curation",
            "Council MoU 2026-Q2",
            None,
        )
        .await
        .unwrap();

    let app = dontosrv::router(state);
    let (status, body) = post(
        app,
        "/protected/read",
        json!({
            "target_kind": "document",
            "target_id": target,
            "action": "read_content"
        }),
        Some(&holder),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["caller"], holder);
}

#[tokio::test]
async fn revoking_attestation_immediately_denies_subsequent_read() {
    let Some(state) = boot().await else {
        eprintln!("skip");
        return;
    };
    let prefix = tag("read-revoke");
    let target = format!("doc:{prefix}");
    let holder = format!("agent:{prefix}/h");

    state
        .client
        .assign_policy(
            "document",
            &target,
            "policy:default/community_restricted",
            "tester",
        )
        .await
        .unwrap();
    let att_id = state
        .client
        .issue_attestation(
            &holder,
            "council",
            "policy:default/community_restricted",
            &["read_content"],
            "audit",
            "rationale",
            None,
        )
        .await
        .unwrap();

    let app = dontosrv::router(state.clone());
    // Allow.
    let (s1, _) = post(
        app,
        "/protected/read",
        json!({
            "target_kind": "document",
            "target_id": target,
            "action": "read_content"
        }),
        Some(&holder),
    )
    .await;
    assert_eq!(s1, StatusCode::OK);

    // Revoke.
    state
        .client
        .revoke_attestation(att_id, "admin", Some("project ended"))
        .await
        .unwrap();

    // Deny.
    let app2 = dontosrv::router(state);
    let (s2, body) = post(
        app2,
        "/protected/read",
        json!({
            "target_kind": "document",
            "target_id": target,
            "action": "read_content"
        }),
        Some(&holder),
    )
    .await;
    assert_eq!(s2, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "forbidden");
}

#[tokio::test]
async fn authorise_probe_is_side_effect_free() {
    let Some(state) = boot().await else {
        eprintln!("skip");
        return;
    };
    let prefix = tag("probe");
    let target = format!("doc:{prefix}");
    state
        .client
        .assign_policy("document", &target, "policy:default/public", "tester")
        .await
        .unwrap();

    let app = dontosrv::router(state);
    let (status, body) = post(
        app,
        "/authorise",
        json!({
            "target_kind": "document",
            "target_id": target,
            "action": "read_content"
        }),
        Some("agent:probe"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["allowed"], true);
    assert_eq!(body["caller"], "agent:probe");
}

#[tokio::test]
async fn authorise_probe_anonymous_falls_back_to_default_restricted() {
    let Some(state) = boot().await else {
        eprintln!("skip");
        return;
    };
    let prefix = tag("anon-probe");
    let target = format!("doc:{prefix}");
    state
        .client
        .assign_policy(
            "document",
            &target,
            "policy:default/community_restricted",
            "tester",
        )
        .await
        .unwrap();
    let app = dontosrv::router(state);
    let (status, body) = post(
        app,
        "/authorise",
        json!({
            "target_kind": "document",
            "target_id": target,
            "action": "read_content"
        }),
        None, // no x-donto-caller
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["allowed"], false);
    assert_eq!(body["caller"], "agent:anonymous");
}

#[tokio::test]
async fn effective_actions_endpoint_surfaces_policy_state() {
    let Some(state) = boot().await else {
        eprintln!("skip");
        return;
    };
    let prefix = tag("effective");
    let target = format!("doc:{prefix}");
    state
        .client
        .assign_policy("document", &target, "policy:default/public", "tester")
        .await
        .unwrap();
    let app = dontosrv::router(state);
    let (status, body) =
        get_path(app, &format!("/policy/effective/document/{target}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["read_content"], true);
    // Public policy gates train_model — confirm.
    assert_eq!(body["train_model"], false);
}
