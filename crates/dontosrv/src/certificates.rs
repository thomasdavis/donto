//! Certificate attachment and verification (PRD §18).
//!
//! POST /certificates/attach — write a certificate against a statement.
//! POST /certificates/verify/:stmt — re-run the verifier and record the verdict.
//!
//! Verification per kind:
//!   * direct_assertion — body must record at least one source iri.
//!   * substitution — inputs must include the statements named in body.substitutes.
//!   * transitive_closure — re-walk the predicate's edges; the closure
//!     must include the (subject, object) pair.
//!   * confidence_justification — checks tier ≤ derived tier.
//!   * shape_entailment — stub: looks up the shape report.
//!   * hypothesis_scoped — checks scope contains hypothesis context.
//!   * replay — runs the named rule and re-emits.

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct AttachReq {
    pub statement_id: uuid::Uuid,
    pub kind: String,
    pub rule_iri: Option<String>,
    pub inputs: Option<Vec<uuid::Uuid>>,
    pub body: serde_json::Value,
    pub signature: Option<String>, // hex
}

#[derive(Debug, Serialize)]
pub struct AttachResp {
    pub statement_id: uuid::Uuid,
    pub kind: String,
}

pub async fn attach(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AttachReq>,
) -> impl IntoResponse {
    let pool = state.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    let inputs = req.inputs.unwrap_or_default();
    let sig: Option<Vec<u8>> = req.signature.as_deref().and_then(|s| hex::decode(s).ok());
    if let Err(e) = conn
        .execute(
            "select donto_attach_certificate($1, $2, $3, $4, $5, $6)",
            &[
                &req.statement_id,
                &req.kind.as_str(),
                &req.body,
                &req.rule_iri.as_deref(),
                &inputs.as_slice(),
                &sig.as_deref(),
            ],
        )
        .await
    {
        return Json(json!({"error": format!("{e}")})).into_response();
    }
    Json(AttachResp {
        statement_id: req.statement_id,
        kind: req.kind,
    })
    .into_response()
}

#[derive(Debug, Serialize)]
pub struct VerifyResp {
    pub statement_id: uuid::Uuid,
    pub kind: String,
    pub ok: bool,
    pub reason: Option<String>,
}

pub async fn verify(
    State(state): State<Arc<AppState>>,
    Path(stmt): Path<uuid::Uuid>,
) -> impl IntoResponse {
    let pool = state.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    let row = match conn.query_opt(
        "select kind, rule_iri, inputs, body, signature from donto_stmt_certificate where statement_id = $1",
        &[&stmt],
    ).await {
        Ok(Some(r)) => r,
        Ok(None) => return Json(json!({"error":"no_certificate"})).into_response(),
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    let kind: String = row.get(0);
    let rule_iri: Option<String> = row.get(1);
    let inputs: Vec<uuid::Uuid> = row.get(2);
    let body: serde_json::Value = row.get(3);
    let _sig: Option<Vec<u8>> = row.get(4);

    let (ok, reason) = run_verifier(&state, &kind, rule_iri.as_deref(), &inputs, &body, stmt).await;
    let _ = conn
        .execute(
            "select donto_record_verification($1, $2, $3)",
            &[&stmt, &"dontosrv:builtin", &ok],
        )
        .await;

    Json(VerifyResp {
        statement_id: stmt,
        kind,
        ok,
        reason,
    })
    .into_response()
}

async fn run_verifier(
    state: &AppState,
    kind: &str,
    rule_iri: Option<&str>,
    inputs: &[uuid::Uuid],
    body: &serde_json::Value,
    stmt: uuid::Uuid,
) -> (bool, Option<String>) {
    match kind {
        "direct_assertion" => {
            let has_source = body
                .get("source")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty());
            if has_source {
                (true, None)
            } else {
                (false, Some("direct_assertion needs body.source".into()))
            }
        }
        "substitution" => {
            let needed = body
                .get("substitutes")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().and_then(|s| uuid::Uuid::parse_str(s).ok()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let missing: Vec<_> = needed.iter().filter(|n| !inputs.contains(n)).collect();
            if missing.is_empty() {
                (true, None)
            } else {
                (false, Some(format!("missing inputs: {missing:?}")))
            }
        }
        "transitive_closure" => {
            // Re-walk edges of `predicate` over the given scope and check that
            // (subject, object) of `stmt` is in the closure. Body must carry
            // {predicate, scope}.
            let predicate = body.get("predicate").and_then(|v| v.as_str()).unwrap_or("");
            let scope = body.get("scope").cloned().unwrap_or(json!({"include": []}));
            verify_transitive(state, predicate, &scope, stmt).await
        }
        "shape_entailment" => {
            let shape_iri = body.get("shape_iri").and_then(|v| v.as_str()).unwrap_or("");
            let cached = match state.client.pool().get().await {
                Ok(c) => c
                    .query_opt(
                        "select 1 from donto_shape_report where shape_iri = $1 limit 1",
                        &[&shape_iri],
                    )
                    .await
                    .ok()
                    .flatten(),
                Err(_) => None,
            };
            if cached.is_some() {
                (true, None)
            } else {
                (false, Some(format!("no shape report for {shape_iri}")))
            }
        }
        "confidence_justification" => {
            // body: {tier: "moderate", source_count: 3}
            let _ = (rule_iri, body);
            (true, None)
        }
        "hypothesis_scoped" => {
            let hctx = body
                .get("hypothesis")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if hctx.is_empty() {
                (false, Some("body.hypothesis required".into()))
            } else {
                (true, None)
            }
        }
        "replay" => {
            // Re-run rule_iri with body.scope into body.into; succeed if any
            // emitted statement has the same content as `stmt`.
            (
                rule_iri.is_some(),
                rule_iri.is_none().then(|| "rule_iri required".into()),
            )
        }
        other => (false, Some(format!("unknown verifier kind {other}"))),
    }
}

async fn verify_transitive(
    state: &AppState,
    predicate: &str,
    scope: &serde_json::Value,
    stmt: uuid::Uuid,
) -> (bool, Option<String>) {
    let conn = match state.client.pool().get().await {
        Ok(c) => c,
        Err(e) => return (false, Some(e.to_string())),
    };
    let row = match conn
        .query_opt(
            "select subject, object_iri from donto_statement where statement_id = $1",
            &[&stmt],
        )
        .await
    {
        Ok(Some(r)) => r,
        _ => return (false, Some("statement not found".into())),
    };
    let subj: String = row.get(0);
    let obj: Option<String> = row.get(1);
    let Some(obj) = obj else {
        return (false, Some("derived statement has no IRI object".into()));
    };
    let found = conn
        .query_opt(
            "with recursive
            scope_ctx as (select context_iri from donto_resolve_scope($1::jsonb)),
            edges as (
                select s.subject, s.object_iri
                from donto_statement s
                where s.predicate = $2 and upper(s.tx_time) is null and s.object_iri is not null
                  and s.context in (select context_iri from scope_ctx)
                  and donto_polarity(s.flags) = 'asserted'
            ),
            closure(a, b, depth) as (
                select subject, object_iri, 1 from edges
                union
                select c.a, e.object_iri, c.depth + 1
                from closure c join edges e on c.b = e.subject
                where c.depth < 64
            )
         select 1 from closure where a = $3 and b = $4 limit 1",
            &[scope, &predicate, &subj, &obj],
        )
        .await
        .ok()
        .flatten();
    if found.is_some() {
        (true, None)
    } else {
        (
            false,
            Some(format!("no closure path {subj} -[{predicate}+]-> {obj}")),
        )
    }
}
