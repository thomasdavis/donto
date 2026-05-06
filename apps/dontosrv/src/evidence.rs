use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct LinkEvidenceSpanReq {
    pub statement_id: Uuid,
    pub span_id: Uuid,
    #[serde(default = "default_extracted")]
    pub link_type: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub context: Option<String>,
}

fn default_extracted() -> String {
    "extracted_from".to_string()
}

pub async fn link_span(
    State(s): State<Arc<AppState>>,
    Json(req): Json<LinkEvidenceSpanReq>,
) -> impl IntoResponse {
    match s
        .client
        .link_evidence_span(
            req.statement_id,
            req.span_id,
            &req.link_type,
            req.confidence,
            req.context.as_deref(),
        )
        .await
    {
        Ok(id) => Json(json!({ "link_id": id })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

pub async fn evidence_for(
    State(s): State<Arc<AppState>>,
    Path(stmt_id): Path<Uuid>,
) -> impl IntoResponse {
    let c = match s.client.pool().get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({ "error": e.to_string() })).into_response(),
    };
    match c
        .query(
            "select link_id, link_type, target_document_id, target_revision_id, \
                target_span_id, target_annotation_id, target_run_id, \
                target_statement_id, confidence \
         from donto_evidence_for($1)",
            &[&stmt_id],
        )
        .await
    {
        Ok(rows) => {
            let links: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "link_id": r.get::<_, Uuid>("link_id").to_string(),
                        "link_type": r.get::<_, String>("link_type"),
                        "target_document_id": r.get::<_, Option<Uuid>>("target_document_id"),
                        "target_span_id": r.get::<_, Option<Uuid>>("target_span_id"),
                        "target_run_id": r.get::<_, Option<Uuid>>("target_run_id"),
                        "target_statement_id": r.get::<_, Option<Uuid>>("target_statement_id"),
                        "confidence": r.get::<_, Option<f64>>("confidence"),
                    })
                })
                .collect();
            Json(json!({ "evidence": links })).into_response()
        }
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}
