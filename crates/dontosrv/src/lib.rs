//! dontosrv as a library: routes, state, sidecar handlers. The `dontosrv`
//! binary is a thin wrapper around [`router`].

#![warn(missing_debug_implementations, rust_2018_idioms)]

pub mod dir;
pub mod shapes;
pub mod rules;
pub mod certificates;

use axum::{Router, routing::{get, post}, extract::{State, Json}, response::IntoResponse};
use donto_client::DontoClient;
use serde_json::json;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub client: DontoClient,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health",  get(health))
        .route("/version", get(version))
        .route("/sparql",  post(sparql))
        .route("/dontoql", post(dontoql))
        .route("/dir",     post(dir::handle))
        .route("/shapes/validate",     post(shapes::validate))
        .route("/rules/derive",        post(rules::derive))
        .route("/certificates/attach", post(certificates::attach))
        .route("/certificates/verify/:stmt", post(certificates::verify))
        .with_state(state)
}

async fn health() -> &'static str { "ok" }

async fn version() -> impl IntoResponse {
    Json(json!({
        "service": "dontosrv",
        "version": env!("CARGO_PKG_VERSION"),
        "dir":     dir::DIR_VERSION,
    }))
}

#[derive(Debug, serde::Deserialize)]
pub struct QueryReq { pub query: String, pub scope_preset: Option<String> }

async fn sparql(State(s): State<Arc<AppState>>, Json(req): Json<QueryReq>) -> impl IntoResponse {
    let q = match donto_query::parse_sparql(&req.query) {
        Ok(q) => q, Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    run(&s.client, q, req.scope_preset).await
}

async fn dontoql(State(s): State<Arc<AppState>>, Json(req): Json<QueryReq>) -> impl IntoResponse {
    let q = match donto_query::parse_dontoql(&req.query) {
        Ok(q) => q, Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    run(&s.client, q, req.scope_preset).await
}

async fn run(client: &DontoClient, mut q: donto_query::Query, preset: Option<String>) -> axum::response::Response {
    if let Some(p) = preset { q.scope_preset = Some(p); }
    match donto_query::evaluate(client, &q).await {
        Ok(rows) => Json(json!({"rows": rows})).into_response(),
        Err(e)   => Json(json!({"error": e.to_string()})).into_response(),
    }
}
