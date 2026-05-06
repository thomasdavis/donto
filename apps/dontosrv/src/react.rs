//! Reaction endpoints — Alexandria §3.2 (endorse, dispute, cite, supersede).
//!
//!   POST /react                 — attach a reaction to a statement
//!   GET  /reactions/:statement  — enumerate current reactions
//!
//! These exist so Dontopedia (and any other product layer) can expose the
//! folk-sonomic side of donto — "I agree with this", "I disagree", "I cite
//! this elsewhere" — without writing raw statements. The donto_client
//! Rust methods already own the SQL; we're just HTTP-shaping them.

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use donto_client::model::{Reaction, ReactionKind};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ReactReq {
    pub source: Uuid,
    /// "endorses" | "rejects" | "cites" | "supersedes"
    pub kind: String,
    #[serde(default)]
    pub object_iri: Option<String>,
    #[serde(default = "default_context")]
    pub context: String,
    #[serde(default)]
    pub actor: Option<String>,
}

fn default_context() -> String {
    "donto:anonymous".to_string()
}

#[derive(Debug, Serialize)]
pub struct ReactResp {
    pub reaction_id: Uuid,
}

pub async fn react(State(s): State<Arc<AppState>>, Json(req): Json<ReactReq>) -> impl IntoResponse {
    let kind = match ReactionKind::parse(&req.kind) {
        Some(k) => k,
        None => {
            return Json(json!({ "error": format!("bad kind: {:?}", req.kind) })).into_response();
        }
    };
    match s
        .client
        .react(
            req.source,
            kind,
            req.object_iri.as_deref(),
            &req.context,
            req.actor.as_deref(),
        )
        .await
    {
        Ok(id) => Json(ReactResp { reaction_id: id }).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Serialize)]
pub struct ReactionOut {
    pub reaction_id: Uuid,
    pub kind: String,
    pub object_iri: Option<String>,
    pub context: String,
    pub polarity: String,
}

#[derive(Debug, Serialize)]
pub struct ReactionsResp {
    pub reactions: Vec<ReactionOut>,
    pub counts: CountsByKind,
}

#[derive(Debug, Default, Serialize)]
pub struct CountsByKind {
    pub endorses: u32,
    pub rejects: u32,
    pub cites: u32,
    pub supersedes: u32,
}

pub async fn list_reactions(
    State(s): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match s.client.reactions_for(id).await {
        Ok(rows) => {
            let counts = summarise(&rows);
            let reactions = rows
                .into_iter()
                .map(|r| ReactionOut {
                    reaction_id: r.reaction_id,
                    kind: r.kind.as_str().to_string(),
                    object_iri: r.object_iri,
                    context: r.context,
                    polarity: r.polarity.as_str().to_string(),
                })
                .collect();
            Json(ReactionsResp { reactions, counts }).into_response()
        }
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

fn summarise(rows: &[Reaction]) -> CountsByKind {
    let mut c = CountsByKind::default();
    for r in rows {
        match r.kind {
            ReactionKind::Endorses => c.endorses += 1,
            ReactionKind::Rejects => c.rejects += 1,
            ReactionKind::Cites => c.cites += 1,
            ReactionKind::Supersedes => c.supersedes += 1,
        }
    }
    c
}
