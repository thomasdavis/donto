//! Derivation rule handler.
//!
//! Per PRD §17 rules are Lean-authored. Phase 6 ships built-in rules that
//! demonstrate the protocol against real data:
//!   * `builtin:transitive/<predicate>` — transitive closure (e.g.
//!     parentOf+ → ancestorOf).
//!   * `builtin:inverse/<predicate>/<inverse>` — inverse expansion.
//!   * `builtin:symmetric/<predicate>` — emit reversed pairs.
//! All built-ins are deterministic and idempotent: re-running with the same
//! inputs is a no-op.

use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct DeriveReq {
    pub rule_iri: String,
    pub scope: serde_json::Value,
    pub into: String,            // output context iri
}

#[derive(Debug, Serialize)]
pub struct DeriveResp {
    pub rule_iri: String,
    pub into: String,
    pub emitted: u64,
    pub source: &'static str,    // "builtin" | "lean" | "cached"
}

pub async fn derive(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeriveReq>,
) -> impl IntoResponse {
    let pool = state.client.pool();

    // Idempotency: fingerprint the inputs and skip if seen.
    let fp = inputs_fingerprint(&req);
    if let Ok(c) = pool.get().await {
        if let Ok(Some(_)) = c.query_opt(
            "select 1 from donto_derivation_report
             where rule_iri = $1 and inputs_fingerprint = $2 limit 1",
            &[&req.rule_iri, &fp],
        ).await {
            return Json(DeriveResp { rule_iri: req.rule_iri.clone(), into: req.into.clone(), emitted: 0, source: "cached" }).into_response();
        }
    }

    // Ensure derivation context. Derivation outputs come from rule code,
    // not user-typed input, so they live in permissive mode — predicate
    // registration is the curator's job, not the rule's.
    if let Err(e) = state.client.ensure_context(&req.into, "derivation", "permissive", None).await {
        return Json(json!({"error": format!("context create: {e}")})).into_response();
    }

    let emitted = match req.rule_iri.as_str() {
        s if s.starts_with("builtin:transitive/") => {
            let pred = s.trim_start_matches("builtin:transitive/").to_string();
            transitive_closure(&state, &pred, &req.scope, &req.into).await
        }
        s if s.starts_with("builtin:inverse/") => {
            let rest = s.trim_start_matches("builtin:inverse/");
            let mut it = rest.splitn(2, '/');
            let p = it.next().unwrap_or("").to_string();
            let inv = it.next().unwrap_or("").to_string();
            inverse_emission(&state, &p, &inv, &req.scope, &req.into).await
        }
        s if s.starts_with("builtin:symmetric/") => {
            let p = s.trim_start_matches("builtin:symmetric/").to_string();
            inverse_emission(&state, &p, &p, &req.scope, &req.into).await
        }
        s if s.starts_with("lean:") => {
            return Json(json!({"error":"sidecar_unavailable","detail":"Lean rule engine wired in Phase 6+"})).into_response();
        }
        _ => return Json(json!({"error": "unknown_rule_iri", "rule_iri": req.rule_iri})).into_response(),
    };

    if let Ok(c) = pool.get().await {
        let _ = c.execute(
            "insert into donto_derivation_report (rule_iri, inputs_fingerprint, scope, into_ctx, emitted_count)
             values ($1, $2, $3, $4, $5)",
            &[&req.rule_iri, &fp, &req.scope, &req.into, &(emitted as i64)],
        ).await;
    }

    Json(DeriveResp { rule_iri: req.rule_iri, into: req.into, emitted, source: "builtin" }).into_response()
}

fn inputs_fingerprint(req: &DeriveReq) -> Vec<u8> {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(req.rule_iri.as_bytes());
    h.update(serde_json::to_vec(&req.scope).unwrap_or_default());
    h.update(req.into.as_bytes());
    h.finalize().to_vec()
}

async fn transitive_closure(state: &AppState, predicate: &str, scope: &serde_json::Value, into: &str) -> u64 {
    let c = match state.client.pool().get().await { Ok(c) => c, Err(_) => return 0 };
    // Compute closure entirely in SQL — far cheaper than streaming statements.
    let rows = c.query(
        "with recursive
            scope_ctx as (select context_iri from donto_resolve_scope($1::jsonb)),
            edges as (
                select subject, object_iri, statement_id
                from donto_statement s
                where s.predicate = $2 and upper(s.tx_time) is null
                  and s.context in (select context_iri from scope_ctx)
                  and donto_polarity(s.flags) = 'asserted'
                  and s.object_iri is not null
            ),
            closure(a, b, evidence) as (
                select subject, object_iri, array[statement_id]::uuid[] from edges
                union
                select c.a, e.object_iri, c.evidence || e.statement_id
                from closure c
                join edges e on c.b = e.subject
                where not (e.statement_id = any(c.evidence))   -- cycle break
            )
            select a, b, evidence from closure",
        &[scope, &predicate],
    ).await.unwrap_or_default();

    let mut emitted = 0u64;
    for r in &rows {
        let a: String = r.get(0);
        let b: String = r.get(1);
        let evidence: Vec<uuid::Uuid> = r.get(2);
        // The transitive predicate name is convention: append `+`.
        let new_pred = format!("{predicate}+");
        if let Ok(id) = state.client.assert(&donto_client::StatementInput::new(
            a, &new_pred, donto_client::Object::iri(b),
        ).with_context(into).with_maturity(3)).await {
            // Lineage.
            let _ = c.execute(
                "insert into donto_stmt_lineage (statement_id, source_stmt) select $1, unnest($2::uuid[])
                 on conflict do nothing",
                &[&id, &evidence],
            ).await;
            emitted += 1;
        }
    }
    emitted
}

async fn inverse_emission(state: &AppState, predicate: &str, inverse: &str, scope: &serde_json::Value, into: &str) -> u64 {
    let c = match state.client.pool().get().await { Ok(c) => c, Err(_) => return 0 };
    let rows = c.query(
        "select s.statement_id, s.subject, s.object_iri
         from donto_statement s, donto_resolve_scope($1::jsonb) sc
         where s.predicate = $2 and upper(s.tx_time) is null and s.context = sc.context_iri
           and donto_polarity(s.flags) = 'asserted' and s.object_iri is not null",
        &[scope, &predicate],
    ).await.unwrap_or_default();
    let mut emitted = 0u64;
    for r in &rows {
        let src: uuid::Uuid = r.get(0);
        let s: String = r.get(1);
        let o: String = r.get(2);
        if let Ok(id) = state.client.assert(&donto_client::StatementInput::new(
            o, inverse, donto_client::Object::iri(s),
        ).with_context(into).with_maturity(3)).await {
            let _ = c.execute(
                "insert into donto_stmt_lineage (statement_id, source_stmt) values ($1, $2)
                 on conflict do nothing",
                &[&id, &src],
            ).await;
            emitted += 1;
        }
    }
    emitted
}
