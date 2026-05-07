//! dontosrv as a library: routes, state, sidecar handlers. The `dontosrv`
//! binary is a thin wrapper around [`router`].

#![warn(missing_debug_implementations, rust_2018_idioms)]

pub mod agents;
pub mod alignment;
pub mod arguments;
pub mod auth;
pub mod browse;
pub mod certificates;
pub mod dir;
pub mod documents;
pub mod evidence;
pub mod history;
pub mod ingest;
pub mod lean;
pub mod obligations;
pub mod policy;
pub mod react;
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
    pub lean: Option<lean::LeanClient>,
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
        .route("/search",   get(history::search))
        .route("/history/:subject", get(history::handle))
        .route("/statement/:id",    get(history::statement_detail))
        .route("/contexts",         get(browse::list_contexts))
        .route("/predicates",       get(browse::list_predicates))
        .route("/contexts/ensure",  post(ingest::ensure_context))
        .route("/assert",           post(ingest::assert))
        .route("/assert/batch",     post(ingest::assert_batch))
        .route("/retract",          post(ingest::retract))
        .route("/react",            post(react::react))
        .route("/reactions/:id",    get(react::list_reactions))
        // Evidence substrate
        .route("/documents/register",     post(documents::register))
        .route("/sources/register",       post(documents::register_v1000))
        .route("/documents/revision",     post(documents::add_revision))
        // Trust Kernel (M0)
        .route("/policy/assign",          post(policy::assign))
        .route("/policy/effective/:target_kind/:target_id", get(policy::effective_actions))
        .route("/attestations",           post(policy::issue_attestation))
        .route("/attestations/:id/revoke", post(policy::revoke_attestation))
        .route("/authorise",              post(policy::authorise_probe))
        .route("/protected/read",         post(policy::protected_read))
        .route("/evidence/link/span",     post(evidence::link_span))
        .route("/evidence/:stmt",         get(evidence::evidence_for))
        .route("/agents/register",        post(agents::register))
        .route("/agents/bind",            post(agents::bind_context))
        .route("/arguments/assert",       post(arguments::assert_argument))
        .route("/arguments/:stmt",        get(arguments::arguments_for))
        .route("/arguments/frontier",     get(arguments::contradiction_frontier))
        .route("/obligations/emit",       post(obligations::emit))
        .route("/obligations/resolve",    post(obligations::resolve))
        .route("/obligations/open",       post(obligations::list_open))
        .route("/obligations/summary",    get(obligations::summary))
        .route("/claim/:id",              get(claim_card))
        // Predicate alignment layer (PAL)
        .route("/alignment/register",       post(alignment::register))
        .route("/alignment/retract",        post(alignment::retract))
        .route("/alignment/rebuild-closure",post(alignment::rebuild_closure))
        .route("/alignment/runs/start",     post(alignment::start_run))
        .route("/alignment/runs/complete",  post(alignment::complete_run))
        .route("/descriptors/upsert",       post(alignment::upsert_descriptor))
        .route("/descriptors/nearest",      post(alignment::nearest_predicates))
        .route("/shadow/materialize",       post(alignment::materialize_shadow))
        .route("/shadow/rebuild",           post(alignment::rebuild_shadows))
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
    headers.insert("access-control-allow-origin", "*".parse().unwrap());
    headers.insert(
        "access-control-allow-methods",
        "GET, POST, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        "access-control-allow-headers",
        "content-type".parse().unwrap(),
    );
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

async fn claim_card(
    State(s): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> impl IntoResponse {
    let c = match s.client.pool().get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    match c.query_one("select donto_claim_card($1)", &[&id]).await {
        Ok(row) => {
            let card: Option<serde_json::Value> = row.get(0);
            match card {
                Some(v) => Json(v).into_response(),
                None => Json(json!({"error": "statement not found"})).into_response(),
            }
        }
        Err(e) => Json(json!({"error": e.to_string()})).into_response(),
    }
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
