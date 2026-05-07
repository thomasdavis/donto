use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterDocReq {
    pub iri: String,
    #[serde(default = "default_media_type")]
    pub media_type: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

fn default_media_type() -> String {
    "text/plain".to_string()
}

pub async fn register(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RegisterDocReq>,
) -> impl IntoResponse {
    match s
        .client
        .ensure_document(
            &req.iri,
            &req.media_type,
            req.label.as_deref(),
            req.source_url.as_deref(),
            req.language.as_deref(),
        )
        .await
    {
        Ok(id) => Json(json!({ "document_id": id, "iri": req.iri })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct AddRevisionReq {
    pub document_id: Uuid,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub parser_version: Option<String>,
}

pub async fn add_revision(
    State(s): State<Arc<AppState>>,
    Json(req): Json<AddRevisionReq>,
) -> impl IntoResponse {
    match s
        .client
        .add_revision(
            req.document_id,
            req.body.as_deref(),
            None,
            req.parser_version.as_deref(),
        )
        .await
    {
        Ok(id) => Json(json!({ "revision_id": id })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

/// Source registration that requires `source_kind` and `policy_iri`
/// (PRD I2 — no source without policy). Use this for new code paths;
/// the legacy `/documents/register` is kept for backwards compatibility
/// but is deprecated.
#[derive(Debug, Deserialize)]
pub struct RegisterSourceWithPolicyReq {
    pub iri: String,
    pub source_kind: String,
    pub policy_iri: String,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

pub async fn register_with_policy(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RegisterSourceWithPolicyReq>,
) -> impl IntoResponse {
    match s
        .client
        .register_source(
            &req.iri,
            &req.source_kind,
            &req.policy_iri,
            req.media_type.as_deref(),
            req.label.as_deref(),
            req.source_url.as_deref(),
            req.language.as_deref(),
        )
        .await
    {
        Ok(id) => Json(json!({
            "document_id": id,
            "iri": req.iri,
            "policy_iri": req.policy_iri,
        }))
        .into_response(),
        Err(e) => Json(json!({
            "error": "register_source_failed",
            "detail": e.to_string(),
        }))
        .into_response(),
    }
}
