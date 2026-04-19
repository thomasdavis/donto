//! Read-only browsing endpoints for aggregate views:
//!
//!   GET /predicates  → [{ predicate, count }]
//!   GET /contexts    → [{ context, kind, mode, count }]
//!
//! These aren't in the original Faces scope but the Dontopedia product
//! wants predicate and context registries as first-class lenses. The
//! queries are kept O(small) — both use the indexed grouping over
//! `donto_statement`, and both cap at 500 rows.

use axum::{extract::State, response::IntoResponse, Json};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

#[derive(Debug, Serialize)]
pub struct PredicateRow {
    pub predicate: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct PredicatesResponse {
    pub predicates: Vec<PredicateRow>,
}

pub async fn list_predicates(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    let pool = s.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({ "error": e.to_string() })).into_response(),
    };
    let rows = match conn
        .query(
            "select predicate, count(*)::bigint
               from donto_statement
              where tx_hi is null
              group by predicate
              order by count(*) desc
              limit 500",
            &[],
        )
        .await
    {
        Ok(rs) => rs,
        Err(e) => {
            return Json(json!({ "error": format!("/predicates: {e}") })).into_response();
        }
    };
    let out = PredicatesResponse {
        predicates: rows
            .into_iter()
            .map(|r| PredicateRow {
                predicate: r.get::<_, String>(0),
                count: r.get::<_, i64>(1),
            })
            .collect(),
    };
    Json(out).into_response()
}

#[derive(Debug, Serialize)]
pub struct ContextRow {
    pub context: String,
    pub kind: String,
    pub mode: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct ContextsResponse {
    pub contexts: Vec<ContextRow>,
}

pub async fn list_contexts(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    let pool = s.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({ "error": e.to_string() })).into_response(),
    };
    // Join `donto_context` with statement counts so empty contexts still
    // show up in the registry. LEFT JOIN + coalesce keeps count 0 for
    // never-used contexts.
    let rows = match conn
        .query(
            "select c.iri,
                    c.kind,
                    c.mode,
                    coalesce(sc.n, 0)::bigint
               from donto_context c
          left join (
                    select context, count(*)::bigint as n
                      from donto_statement
                     where tx_hi is null
                     group by context
               ) sc on sc.context = c.iri
              order by coalesce(sc.n, 0) desc, c.iri
              limit 500",
            &[],
        )
        .await
    {
        Ok(rs) => rs,
        Err(e) => {
            return Json(json!({ "error": format!("/contexts: {e}") })).into_response();
        }
    };
    let out = ContextsResponse {
        contexts: rows
            .into_iter()
            .map(|r| ContextRow {
                context: r.get::<_, String>(0),
                kind: r.get::<_, String>(1),
                mode: r.get::<_, String>(2),
                count: r.get::<_, i64>(3),
            })
            .collect(),
    };
    Json(out).into_response()
}
