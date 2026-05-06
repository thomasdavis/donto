use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct EmitReq {
    pub statement_id: Uuid,
    pub obligation_type: String,
    #[serde(default = "default_context")]
    pub context: String,
    #[serde(default)]
    pub priority: Option<i16>,
    #[serde(default)]
    pub detail: Option<serde_json::Value>,
    #[serde(default)]
    pub assigned_agent: Option<Uuid>,
}

fn default_context() -> String {
    "donto:anonymous".to_string()
}

pub async fn emit(State(s): State<Arc<AppState>>, Json(req): Json<EmitReq>) -> impl IntoResponse {
    match s
        .client
        .emit_obligation(
            req.statement_id,
            &req.obligation_type,
            &req.context,
            req.priority.unwrap_or(0),
            req.detail.as_ref(),
            req.assigned_agent,
        )
        .await
    {
        Ok(id) => Json(json!({ "obligation_id": id })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ResolveReq {
    pub obligation_id: Uuid,
    #[serde(default)]
    pub resolved_by: Option<Uuid>,
    #[serde(default = "default_resolved")]
    pub status: String,
}

fn default_resolved() -> String {
    "resolved".to_string()
}

pub async fn resolve(
    State(s): State<Arc<AppState>>,
    Json(req): Json<ResolveReq>,
) -> impl IntoResponse {
    match s
        .client
        .resolve_obligation(req.obligation_id, req.resolved_by, &req.status)
        .await
    {
        Ok(ok) => Json(json!({ "resolved": ok })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ListReq {
    #[serde(default)]
    pub obligation_type: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
}

fn default_limit() -> i32 {
    100
}

pub async fn list_open(
    State(s): State<Arc<AppState>>,
    Json(req): Json<ListReq>,
) -> impl IntoResponse {
    let c = match s.client.pool().get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({ "error": e.to_string() })).into_response(),
    };
    match c
        .query(
            "select obligation_id, statement_id, obligation_type, priority, \
                context, assigned_agent, detail, created_at \
         from donto_open_obligations($1, $2, $3)",
            &[&req.obligation_type, &req.context, &req.limit],
        )
        .await
    {
        Ok(rows) => {
            let items: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "obligation_id": r.get::<_, Uuid>("obligation_id").to_string(),
                        "statement_id": r.get::<_, Option<Uuid>>("statement_id"),
                        "obligation_type": r.get::<_, String>("obligation_type"),
                        "priority": r.get::<_, i16>("priority"),
                        "context": r.get::<_, String>("context"),
                        "assigned_agent": r.get::<_, Option<Uuid>>("assigned_agent"),
                    })
                })
                .collect();
            Json(json!({ "obligations": items })).into_response()
        }
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

pub async fn summary(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    let c = match s.client.pool().get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({ "error": e.to_string() })).into_response(),
    };
    match c
        .query(
            "select obligation_type, status, cnt from donto_obligation_summary(null)",
            &[],
        )
        .await
    {
        Ok(rows) => {
            let items: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "obligation_type": r.get::<_, String>("obligation_type"),
                        "status": r.get::<_, String>("status"),
                        "count": r.get::<_, i64>("cnt"),
                    })
                })
                .collect();
            Json(json!({ "summary": items })).into_response()
        }
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}
