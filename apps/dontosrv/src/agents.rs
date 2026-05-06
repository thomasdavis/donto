use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterAgentReq {
    pub iri: String,
    #[serde(default = "default_custom")]
    pub agent_type: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
}

fn default_custom() -> String {
    "custom".to_string()
}

pub async fn register(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RegisterAgentReq>,
) -> impl IntoResponse {
    match s
        .client
        .ensure_agent(
            &req.iri,
            &req.agent_type,
            req.label.as_deref(),
            req.model_id.as_deref(),
        )
        .await
    {
        Ok(id) => Json(json!({ "agent_id": id, "iri": req.iri })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct BindContextReq {
    pub agent_id: Uuid,
    pub context: String,
    #[serde(default = "default_owner")]
    pub role: String,
}

fn default_owner() -> String {
    "owner".to_string()
}

pub async fn bind_context(
    State(s): State<Arc<AppState>>,
    Json(req): Json<BindContextReq>,
) -> impl IntoResponse {
    match s
        .client
        .bind_agent_context(req.agent_id, &req.context, &req.role)
        .await
    {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}
