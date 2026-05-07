//! Write-side HTTP handlers: `/contexts/ensure`, `/assert`, `/assert/batch`,
//! `/retract`.
//!
//! These exist so product surfaces (Dontopedia, external ingestion pipelines)
//! have a first-class HTTP path in. The pre-existing read routes were enough
//! for Faces; anything that produces new statements needs these.
//!
//! Semantics follow the SQL surface directly — we do not re-interpret the
//! data model here. If you find yourself wanting to, add the behaviour to a
//! SQL function and call it. Dontosrv is a thin shell.

use axum::{extract::State, response::IntoResponse, Json};
use chrono::NaiveDate;
use donto_client::model::{Literal, Object, Polarity, StatementInput};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct EnsureContextReq {
    pub iri: String,
    /// "source" | "hypothesis" | "derived" | "snapshot" | "user" | …
    /// Free string: the SQL function owns validation.
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub parent: Option<String>,
}

fn default_kind() -> String {
    "source".to_string()
}
fn default_mode() -> String {
    "permissive".to_string()
}

pub async fn ensure_context(
    State(s): State<Arc<AppState>>,
    Json(req): Json<EnsureContextReq>,
) -> impl IntoResponse {
    match s
        .client
        .ensure_context(&req.iri, &req.kind, &req.mode, req.parent.as_deref())
        .await
    {
        Ok(()) => Json(json!({ "iri": req.iri, "ok": true })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct AssertReq {
    pub subject: String,
    pub predicate: String,
    #[serde(default)]
    pub object_iri: Option<String>,
    #[serde(default)]
    pub object_lit: Option<Literal>,
    #[serde(default = "default_context")]
    pub context: String,
    #[serde(default = "default_polarity")]
    pub polarity: String,
    #[serde(default)]
    pub maturity: u8,
    #[serde(default)]
    pub valid_from: Option<NaiveDate>,
    #[serde(default)]
    pub valid_to: Option<NaiveDate>,
}

fn default_context() -> String {
    "donto:anonymous".to_string()
}
fn default_polarity() -> String {
    "asserted".to_string()
}

#[derive(Debug, Serialize)]
pub struct AssertResp {
    pub statement_id: Uuid,
}

pub async fn assert(
    State(s): State<Arc<AppState>>,
    Json(req): Json<AssertReq>,
) -> impl IntoResponse {
    let object = match (req.object_iri, req.object_lit) {
        (Some(iri), None) => Object::Iri(iri),
        (None, Some(lit)) => Object::Literal(lit),
        (Some(_), Some(_)) => {
            return Json(json!({
                "error": "supply exactly one of object_iri or object_lit"
            }))
            .into_response();
        }
        (None, None) => {
            return Json(json!({
                "error": "one of object_iri or object_lit is required"
            }))
            .into_response();
        }
    };

    let polarity = match Polarity::parse(&req.polarity) {
        Some(p) => p,
        None => {
            return Json(json!({ "error": format!("bad polarity {:?}", req.polarity) }))
                .into_response();
        }
    };

    let input = StatementInput::new(req.subject, req.predicate, object)
        .with_context(req.context)
        .with_polarity(polarity)
        .with_maturity(req.maturity)
        .with_valid(req.valid_from, req.valid_to);

    match s.client.assert(&input).await {
        Ok(id) => Json(AssertResp { statement_id: id }).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct AssertBatchReq {
    pub statements: Vec<AssertReq>,
}

pub async fn assert_batch(
    State(s): State<Arc<AppState>>,
    Json(req): Json<AssertBatchReq>,
) -> impl IntoResponse {
    let mut inputs: Vec<StatementInput> = Vec::with_capacity(req.statements.len());
    for (i, r) in req.statements.into_iter().enumerate() {
        let object = match (r.object_iri, r.object_lit) {
            (Some(iri), None) => Object::Iri(iri),
            (None, Some(lit)) => Object::Literal(lit),
            _ => {
                return Json(json!({
                    "error": format!("statement[{i}]: supply exactly one of object_iri / object_lit")
                }))
                .into_response();
            }
        };
        let polarity = match Polarity::parse(&r.polarity) {
            Some(p) => p,
            None => {
                return Json(json!({
                    "error": format!("statement[{i}]: bad polarity {:?}", r.polarity)
                }))
                .into_response();
            }
        };
        inputs.push(
            StatementInput::new(r.subject, r.predicate, object)
                .with_context(r.context)
                .with_polarity(polarity)
                .with_maturity(r.maturity)
                .with_valid(r.valid_from, r.valid_to),
        );
    }

    match s.client.assert_batch(&inputs).await {
        Ok(n) => Json(json!({ "inserted": n })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct RetractReq {
    pub statement_id: Uuid,
}

pub async fn retract(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RetractReq>,
) -> impl IntoResponse {
    match s.client.retract(req.statement_id).await {
        Ok(ok) => {
            Json(json!({ "statement_id": req.statement_id, "retracted": ok })).into_response()
        }
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}
