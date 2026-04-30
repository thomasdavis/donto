//! Predicate Alignment Layer HTTP handlers.
//!
//! Mirrors `donto_client::DontoClient` alignment methods through HTTP. See
//! migrations 0048-0056 for the underlying SQL surface; this module is a thin
//! shell over those SQL functions, just like [`crate::ingest`].

use axum::{extract::State, response::IntoResponse, Json};
use chrono::NaiveDate;
use donto_client::AlignmentRelation;
use serde::Deserialize;
use serde_json::{json, Value as JsonVal};
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterAlignmentReq {
    pub source: String,
    pub target: String,
    /// One of: exact_equivalent, inverse_equivalent, sub_property_of,
    /// close_match, decomposition, not_equivalent.
    pub relation: String,
    pub confidence: f64,
    #[serde(default)]
    pub valid_lo: Option<NaiveDate>,
    #[serde(default)]
    pub valid_hi: Option<NaiveDate>,
    #[serde(default)]
    pub run_id: Option<Uuid>,
    #[serde(default)]
    pub provenance: Option<JsonVal>,
    #[serde(default)]
    pub actor: Option<String>,
}

pub async fn register(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RegisterAlignmentReq>,
) -> impl IntoResponse {
    let relation = match AlignmentRelation::parse(&req.relation) {
        Some(r) => r,
        None => {
            return Json(json!({ "error": format!("unknown relation {:?}", req.relation) }))
                .into_response();
        }
    };
    match s
        .client
        .register_alignment(
            &req.source,
            &req.target,
            relation,
            req.confidence,
            req.valid_lo,
            req.valid_hi,
            req.run_id,
            req.provenance.as_ref(),
            req.actor.as_deref(),
        )
        .await
    {
        Ok(id) => Json(json!({ "alignment_id": id })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct RetractAlignmentReq {
    pub alignment_id: Uuid,
}

pub async fn retract(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RetractAlignmentReq>,
) -> impl IntoResponse {
    match s.client.retract_alignment(req.alignment_id).await {
        Ok(ok) => {
            Json(json!({ "alignment_id": req.alignment_id, "retracted": ok })).into_response()
        }
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

pub async fn rebuild_closure(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    match s.client.rebuild_predicate_closure().await {
        Ok(n) => Json(json!({ "rows": n })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct StartRunReq {
    pub run_type: String,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub config: Option<JsonVal>,
    #[serde(default)]
    pub source_predicates: Option<Vec<String>>,
}

pub async fn start_run(
    State(s): State<Arc<AppState>>,
    Json(req): Json<StartRunReq>,
) -> impl IntoResponse {
    match s
        .client
        .start_alignment_run(
            &req.run_type,
            req.model_id.as_deref(),
            req.config.as_ref(),
            req.source_predicates.as_deref(),
        )
        .await
    {
        Ok(id) => Json(json!({ "run_id": id })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct CompleteRunReq {
    pub run_id: Uuid,
    pub status: String,
    #[serde(default)]
    pub proposed: Option<i32>,
    #[serde(default)]
    pub accepted: Option<i32>,
    #[serde(default)]
    pub rejected: Option<i32>,
}

pub async fn complete_run(
    State(s): State<Arc<AppState>>,
    Json(req): Json<CompleteRunReq>,
) -> impl IntoResponse {
    match s
        .client
        .complete_alignment_run(
            req.run_id,
            &req.status,
            req.proposed,
            req.accepted,
            req.rejected,
        )
        .await
    {
        Ok(()) => Json(json!({ "run_id": req.run_id, "ok": true })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpsertDescriptorReq {
    pub iri: String,
    pub label: String,
    #[serde(default)]
    pub gloss: Option<String>,
    #[serde(default)]
    pub subject_type: Option<String>,
    #[serde(default)]
    pub object_type: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub embedding_model: Option<String>,
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
}

pub async fn upsert_descriptor(
    State(s): State<Arc<AppState>>,
    Json(req): Json<UpsertDescriptorReq>,
) -> impl IntoResponse {
    match s
        .client
        .upsert_descriptor(
            &req.iri,
            &req.label,
            req.gloss.as_deref(),
            req.subject_type.as_deref(),
            req.object_type.as_deref(),
            req.domain.as_deref(),
            req.embedding_model.as_deref(),
            req.embedding.as_deref(),
        )
        .await
    {
        Ok(iri) => Json(json!({ "iri": iri })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct NearestPredicatesReq {
    pub embedding: Vec<f32>,
    pub model_id: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
}

fn default_limit() -> i32 {
    10
}

pub async fn nearest_predicates(
    State(s): State<Arc<AppState>>,
    Json(req): Json<NearestPredicatesReq>,
) -> impl IntoResponse {
    match s
        .client
        .nearest_predicates(&req.embedding, &req.model_id, req.domain.as_deref(), req.limit)
        .await
    {
        Ok(rows) => Json(json!({ "candidates": rows })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct MaterializeShadowReq {
    pub statement_id: Uuid,
}

pub async fn materialize_shadow(
    State(s): State<Arc<AppState>>,
    Json(req): Json<MaterializeShadowReq>,
) -> impl IntoResponse {
    match s.client.materialize_shadow(req.statement_id).await {
        Ok(id) => Json(json!({ "statement_id": req.statement_id, "shadow_id": id }))
            .into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct RebuildShadowsReq {
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub limit: Option<i32>,
}

pub async fn rebuild_shadows(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RebuildShadowsReq>,
) -> impl IntoResponse {
    match s
        .client
        .rebuild_shadows(req.context.as_deref(), req.limit)
        .await
    {
        Ok(n) => Json(json!({ "rebuilt": n })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}
