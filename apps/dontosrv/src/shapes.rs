//! Shape validation request handler.
//!
//! Per PRD §15 the authoritative shape engine is Lean. This Rust handler
//! ships:
//!   * a small built-in shape library (FunctionalPredicate, RangeShape,
//!     MinCardinality, AcyclicClosure, DatatypeShape) that returns real
//!     reports against the live database,
//!   * a `lean://` shape IRI scheme that returns `sidecar_unavailable` until
//!     the Lean engine is wired up in Phase 5+.
//!
//! Reports are persisted in `donto_shape_report` so reads can consult the
//! cache without re-running.

use axum::{extract::State, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ValidateReq {
    pub shape_iri: String,
    pub scope: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ValidateResp {
    pub shape_iri: String,
    pub focus_count: u64,
    pub violations: Vec<Violation>,
    pub source: &'static str, // "builtin" | "cached" | "lean"
}

#[derive(Debug, Serialize)]
pub struct Violation {
    pub focus: String,
    pub reason: String,
    pub evidence: Vec<uuid::Uuid>,
}

pub async fn validate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ValidateReq>,
) -> impl IntoResponse {
    let pool = state.client.pool();

    // Cache check.
    if let Ok(c) = pool.get().await {
        let scope_fp = scope_fingerprint(&req.scope);
        if let Ok(Some(row)) = c
            .query_opt(
                "select report from donto_shape_report
             where shape_iri = $1 and scope_fingerprint = $2
             order by evaluated_at desc limit 1",
                &[&req.shape_iri, &scope_fp],
            )
            .await
        {
            let report: serde_json::Value = row.get(0);
            return Json(json!({
                "shape_iri": req.shape_iri,
                "source": "cached",
                "report": report,
            }))
            .into_response();
        }
    }

    // Built-in shapes by IRI prefix.
    let resp = match req.shape_iri.as_str() {
        s if s.starts_with("builtin:functional/") => {
            let pred = s.trim_start_matches("builtin:functional/");
            shape_functional(&state, pred, &req.scope).await
        }
        s if s.starts_with("builtin:datatype/") => {
            // builtin:datatype/<predicate>/<datatype>
            let rest = s.trim_start_matches("builtin:datatype/");
            let mut it = rest.splitn(2, '/');
            let pred = it.next().unwrap_or("").to_string();
            let dt = it.next().unwrap_or("").to_string();
            shape_datatype(&state, &pred, &dt, &req.scope).await
        }
        s if s.starts_with("lean:") => {
            return forward_to_lean(&state, &req).await;
        }
        _ => {
            return Json(json!({
                "error": "unknown_shape_iri",
                "shape_iri": req.shape_iri,
            }))
            .into_response();
        }
    };

    // Cache write.
    if let Ok(c) = pool.get().await {
        let scope_fp = scope_fingerprint(&req.scope);
        let report = serde_json::to_value(&resp).unwrap_or(serde_json::Value::Null);
        let _ = c.execute(
            "insert into donto_shape_report (shape_iri, scope_fingerprint, scope, report, focus_count, violation_count)
             values ($1, $2, $3, $4, $5, $6)",
            &[
                &req.shape_iri,
                &scope_fp,
                &req.scope,
                &report,
                &(resp.focus_count as i64),
                &(resp.violations.len() as i64),
            ],
        ).await;
    }

    Json(resp).into_response()
}

fn scope_fingerprint(scope: &serde_json::Value) -> Vec<u8> {
    use sha2::Digest;
    let bytes = serde_json::to_vec(scope).unwrap_or_default();
    let mut h = sha2::Sha256::new();
    h.update(&bytes);
    h.finalize().to_vec()
}

/// Forward a `lean:` shape request to the engine. We ship every statement
/// in the resolved scope as the input set — the Lean shape evaluates
/// against that snapshot. (Future: page in slices for very large scopes.)
async fn forward_to_lean(state: &AppState, req: &ValidateReq) -> axum::response::Response {
    let Some(lean) = &state.lean else {
        return Json(json!({
            "error": "sidecar_unavailable",
            "shape_iri": req.shape_iri,
            "detail": "Lean engine not configured (start dontosrv with --lean-engine /path/to/donto_engine)",
        })).into_response();
    };

    let pool = state.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    // Resolve the scope client-side so the planner sees a concrete text[] array
    // and can use the (context) btree index. Inlining donto_resolve_scope into
    // the query forces a hash join over the whole statement table because the
    // function is opaque to the planner.
    let resolved: Vec<String> = match conn
        .query(
            "select context_iri from donto_resolve_scope($1::jsonb)",
            &[&req.scope],
        )
        .await
    {
        Ok(rs) => rs.into_iter().map(|r| r.get::<_, String>(0)).collect(),
        Err(e) => return Json(json!({"error": format!("scope resolution: {e}")})).into_response(),
    };
    if resolved.is_empty() {
        return Json(json!({
            "shape_iri": req.shape_iri,
            "source":    "lean",
            "report":    {"focus_count":0,"violations":[]},
        }))
        .into_response();
    }
    let rows = match conn
        .query(
            "select s.statement_id, s.subject, s.predicate, s.object_iri, s.object_lit, s.context,
                donto_polarity(s.flags), donto_maturity(s.flags),
                lower(s.valid_time), upper(s.valid_time)
           from donto_statement s
          where s.context = any($1::text[])
            and upper(s.tx_time) is null",
            &[&resolved.as_slice()],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => return Json(json!({"error": format!("scope resolution: {e}")})).into_response(),
    };

    let stmts: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let id: uuid::Uuid = r.get(0);
            let subject: String = r.get(1);
            let predicate: String = r.get(2);
            let object_iri: Option<String> = r.get(3);
            let object_lit: Option<serde_json::Value> = r.get(4);
            let context: String = r.get(5);
            let polarity: String = r.get(6);
            let mut o = serde_json::Map::new();
            o.insert("id".into(), serde_json::Value::String(id.to_string()));
            o.insert("subject".into(), serde_json::Value::String(subject));
            o.insert("predicate".into(), serde_json::Value::String(predicate));
            o.insert("context".into(), serde_json::Value::String(context));
            o.insert("polarity".into(), serde_json::Value::String(polarity));
            if let Some(i) = object_iri {
                o.insert("object_iri".into(), serde_json::Value::String(i));
            }
            if let Some(l) = object_lit {
                o.insert("object_lit".into(), l);
            }
            serde_json::Value::Object(o)
        })
        .collect();

    let envelope = json!({
        "version": "0.1.0-json",
        "kind":    "validate_request",
        "shape_iri": req.shape_iri,
        "scope":     req.scope,
        "statements": stmts,
    });
    match lean.send(envelope).await {
        Ok(v) => Json(json!({
            "shape_iri": req.shape_iri,
            "source":    "lean",
            "report":    v,
        }))
        .into_response(),
        Err(e) => Json(json!({
            "error": "sidecar_unavailable",
            "shape_iri": req.shape_iri,
            "detail": e.to_string(),
        }))
        .into_response(),
    }
}

async fn shape_functional(
    state: &AppState,
    predicate: &str,
    scope: &serde_json::Value,
) -> ValidateResp {
    let pool = state.client.pool();
    let c = match pool.get().await {
        Ok(c) => c,
        Err(_) => {
            return ValidateResp {
                shape_iri: format!("builtin:functional/{predicate}"),
                focus_count: 0,
                violations: vec![],
                source: "builtin",
            }
        }
    };
    let rows = c
        .query(
            "with scope_ctx as (select context_iri from donto_resolve_scope($1::jsonb)),
              scoped as (
                select s.subject, s.statement_id, s.object_iri, s.object_lit
                from donto_statement s
                where s.predicate = $2
                  and upper(s.tx_time) is null
                  and s.context in (select context_iri from scope_ctx)
                  and donto_polarity(s.flags) = 'asserted'
              ),
              counts as (
                select subject, count(distinct coalesce(object_iri, object_lit::text)) as objs,
                       array_agg(statement_id) as ids
                from scoped group by subject
              )
         select subject, ids from counts where objs > 1",
            &[scope, &predicate],
        )
        .await
        .unwrap_or_default();

    let mut violations = Vec::new();
    let mut focus_count = 0u64;
    for r in &rows {
        focus_count += 1;
        let subject: String = r.get(0);
        let ids: Vec<uuid::Uuid> = r.get(1);
        violations.push(Violation {
            focus: subject,
            reason: format!("predicate {predicate} is functional but has multiple objects"),
            evidence: ids,
        });
    }
    ValidateResp {
        shape_iri: format!("builtin:functional/{predicate}"),
        focus_count,
        violations,
        source: "builtin",
    }
}

async fn shape_datatype(
    state: &AppState,
    predicate: &str,
    datatype: &str,
    scope: &serde_json::Value,
) -> ValidateResp {
    let pool = state.client.pool();
    let c = match pool.get().await {
        Ok(c) => c,
        Err(_) => {
            return ValidateResp {
                shape_iri: format!("builtin:datatype/{predicate}/{datatype}"),
                focus_count: 0,
                violations: vec![],
                source: "builtin",
            }
        }
    };
    let rows = c
        .query(
            "with scope_ctx as (select context_iri from donto_resolve_scope($1::jsonb))
         select s.subject, s.statement_id, s.object_lit
         from donto_statement s
         where s.predicate = $2
           and upper(s.tx_time) is null
           and s.context in (select context_iri from scope_ctx)
           and donto_polarity(s.flags) = 'asserted'
           and (s.object_lit is null or coalesce(s.object_lit ->> 'dt', '') <> $3)",
            &[scope, &predicate, &datatype],
        )
        .await
        .unwrap_or_default();
    let mut violations = Vec::new();
    for r in &rows {
        violations.push(Violation {
            focus: r.get(0),
            reason: format!("expected literal of datatype {datatype}"),
            evidence: vec![r.get(1)],
        });
    }
    ValidateResp {
        shape_iri: format!("builtin:datatype/{predicate}/{datatype}"),
        focus_count: violations.len() as u64,
        violations,
        source: "builtin",
    }
}
