use axum::{extract::{Path, State}, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct AssertArgumentReq {
    pub source: Uuid,
    pub target: Uuid,
    pub relation: String,
    #[serde(default = "default_context")]
    pub context: String,
    #[serde(default)]
    pub strength: Option<f64>,
    #[serde(default)]
    pub agent_id: Option<Uuid>,
    #[serde(default)]
    pub evidence: Option<serde_json::Value>,
}

fn default_context() -> String { "donto:anonymous".to_string() }

pub async fn assert_argument(
    State(s): State<Arc<AppState>>,
    Json(req): Json<AssertArgumentReq>,
) -> impl IntoResponse {
    match s.client.assert_argument(
        req.source, req.target, &req.relation, &req.context,
        req.strength, req.agent_id, req.evidence.as_ref(),
    ).await {
        Ok(id) => Json(json!({ "argument_id": id })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

pub async fn arguments_for(
    State(s): State<Arc<AppState>>,
    Path(stmt_id): Path<Uuid>,
) -> impl IntoResponse {
    let c = match s.client.pool().get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({ "error": e.to_string() })).into_response(),
    };
    match c.query(
        "select argument_id, source_statement_id, target_statement_id, \
                relation, strength, context, agent_id \
         from donto_arguments_for($1)", &[&stmt_id]
    ).await {
        Ok(rows) => {
            let args: Vec<serde_json::Value> = rows.iter().map(|r| json!({
                "argument_id": r.get::<_, Uuid>("argument_id").to_string(),
                "source": r.get::<_, Uuid>("source_statement_id").to_string(),
                "target": r.get::<_, Uuid>("target_statement_id").to_string(),
                "relation": r.get::<_, String>("relation"),
                "strength": r.get::<_, Option<f64>>("strength"),
                "context": r.get::<_, String>("context"),
            })).collect();
            Json(json!({ "arguments": args })).into_response()
        }
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

pub async fn contradiction_frontier(
    State(s): State<Arc<AppState>>,
) -> impl IntoResponse {
    let c = match s.client.pool().get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({ "error": e.to_string() })).into_response(),
    };
    match c.query(
        "select statement_id, attack_count, support_count, net_pressure \
         from donto_contradiction_frontier(null)", &[]
    ).await {
        Ok(rows) => {
            let items: Vec<serde_json::Value> = rows.iter().map(|r| json!({
                "statement_id": r.get::<_, Uuid>("statement_id").to_string(),
                "attack_count": r.get::<_, i64>("attack_count"),
                "support_count": r.get::<_, i64>("support_count"),
                "net_pressure": r.get::<_, i64>("net_pressure"),
            })).collect();
            Json(json!({ "frontier": items })).into_response()
        }
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}
