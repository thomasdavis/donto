//! dontosrv as a library: routes, state, sidecar handlers. The `dontosrv`
//! binary is a thin wrapper around [`router`].

#![warn(missing_debug_implementations, rust_2018_idioms)]

pub mod certificates;
pub mod dir;
pub mod history;
pub mod lean;
pub mod rules;
pub mod shapes;

use axum::{
    extract::{Json, State},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use donto_client::DontoClient;
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct AppState {
    pub client: DontoClient,
    /// Optional Lean engine. `None` means the binary wasn't configured;
    /// `lean:` shape IRIs return `sidecar_unavailable` in that case
    /// (PRD §15 sidecar contract).
    pub lean:   Option<lean::LeanClient>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/sparql", post(sparql))
        .route("/dontoql", post(dontoql))
        .route("/dir", post(dir::handle))
        .route("/shapes/validate", post(shapes::validate))
        .route("/rules/derive", post(rules::derive))
        .route("/certificates/attach", post(certificates::attach))
        .route("/certificates/verify/:stmt", post(certificates::verify))
        .route("/subjects", get(history::list_subjects))
        .route("/history/:subject", get(history::handle))
        .layer(axum::middleware::from_fn(cors))
        .with_state(state)
}

/// Permissive CORS so the Next.js dev server (apps/faces, port 3000) can hit
/// dontosrv (port 7878) during development. Faces is read-only and surfaces
/// only what dontosrv chooses to expose.
async fn cors(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if req.method() == axum::http::Method::OPTIONS {
        let mut resp = axum::response::Response::new(axum::body::Body::empty());
        cors_headers(resp.headers_mut());
        return resp;
    }
    let mut resp = next.run(req).await;
    cors_headers(resp.headers_mut());
    resp
}

fn cors_headers(headers: &mut axum::http::HeaderMap) {
    headers.insert("access-control-allow-origin",  "*".parse().unwrap());
    headers.insert("access-control-allow-methods", "GET, POST, OPTIONS".parse().unwrap());
    headers.insert("access-control-allow-headers", "content-type".parse().unwrap());
}

async fn health() -> &'static str {
    "ok"
}

async fn version() -> impl IntoResponse {
    Json(json!({
        "service": "dontosrv",
        "version": env!("CARGO_PKG_VERSION"),
        "dir":     dir::DIR_VERSION,
    }))
}

#[derive(Debug, serde::Deserialize)]
pub struct QueryReq {
    pub query: String,
    pub scope_preset: Option<String>,
}

async fn sparql(State(s): State<Arc<AppState>>, Json(req): Json<QueryReq>) -> impl IntoResponse {
    let q = match donto_query::parse_sparql(&req.query) {
        Ok(q) => q,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    run(&s.client, q, req.scope_preset).await
}

async fn dontoql(State(s): State<Arc<AppState>>, Json(req): Json<QueryReq>) -> impl IntoResponse {
    let q = match donto_query::parse_dontoql(&req.query) {
        Ok(q) => q,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    run(&s.client, q, req.scope_preset).await
}

async fn run(
    client: &DontoClient,
    mut q: donto_query::Query,
    preset: Option<String>,
) -> axum::response::Response {
    if let Some(p) = preset {
        q.scope_preset = Some(p);
    }
    match donto_query::evaluate(client, &q).await {
        Ok(rows) => Json(json!({"rows": rows})).into_response(),
        Err(e) => Json(json!({"error": e.to_string()})).into_response(),
    }
}
