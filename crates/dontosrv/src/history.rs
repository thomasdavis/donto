//! Subject-history endpoint for the donto-faces visualisation layer.
//!
//! GET /history/:subject  → JSON array of every statement (open or closed,
//! every context, every polarity) about that subject. The visualisation
//! renders this directly into the Stratigraph / Rashomon / Probe lenses.
//!
//! This is intentionally a wider read than `donto_match`'s default — it
//! returns retracted rows. The Stratigraph cannot render retraction without
//! seeing the closed tx_time intervals.

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::AppState;

#[derive(Debug, Serialize)]
pub struct HistoryRow {
    pub statement_id: uuid::Uuid,
    pub subject:   String,
    pub predicate: String,
    pub object_iri: Option<String>,
    pub object_lit: Option<Value>,
    pub context:   String,
    pub polarity:  String,
    pub maturity:  i32,
    pub valid_lo:  Option<chrono::NaiveDate>,
    pub valid_hi:  Option<chrono::NaiveDate>,
    pub tx_lo:     chrono::DateTime<chrono::Utc>,
    pub tx_hi:     Option<chrono::DateTime<chrono::Utc>>,
    /// statement_ids this row's content was derived from (lineage table).
    pub lineage:   Vec<uuid::Uuid>,
}

pub async fn handle(
    State(state): State<Arc<AppState>>,
    Path(subject): Path<String>,
) -> impl IntoResponse {
    let pool = state.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    let rows = match conn.query(
        "select s.statement_id, s.subject, s.predicate, s.object_iri, s.object_lit,
                s.context,
                donto_polarity(s.flags), donto_maturity(s.flags),
                lower(s.valid_time), upper(s.valid_time),
                lower(s.tx_time),    upper(s.tx_time),
                coalesce(
                    (select array_agg(source_stmt) from donto_stmt_lineage l
                      where l.statement_id = s.statement_id),
                    '{}'::uuid[]) as lineage
           from donto_statement s
          where s.subject = $1
          order by lower(s.tx_time) asc",
        &[&subject],
    ).await {
        Ok(rs) => rs,
        Err(e) => return Json(json!({"error": format!("history query: {e}")})).into_response(),
    };

    let out: Vec<HistoryRow> = rows.into_iter().map(|r| HistoryRow {
        statement_id: r.get(0),
        subject:    r.get(1),
        predicate:  r.get(2),
        object_iri: r.get(3),
        object_lit: r.get(4),
        context:    r.get(5),
        polarity:   r.get(6),
        maturity:   r.get(7),
        valid_lo:   r.get(8),
        valid_hi:   r.get(9),
        tx_lo:      r.get(10),
        tx_hi:      r.get(11),
        lineage:    r.get(12),
    }).collect();

    Json(json!({
        "subject": subject,
        "count":   out.len(),
        "rows":    out,
    })).into_response()
}

/// GET /subjects → list of distinct subjects, with row counts. Used by the
/// faces UI to populate its picker on first load.
pub async fn list_subjects(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let pool = state.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    let rows = conn.query(
        "select subject, count(*)::int as n
           from donto_statement
          group by subject
          having count(*) > 1
          order by n desc
          limit 50",
        &[],
    ).await.unwrap_or_default();
    let subs: Vec<Value> = rows.iter().map(|r| {
        let s: String = r.get(0);
        let n: i32 = r.get(1);
        json!({"subject": s, "count": n})
    }).collect();
    Json(json!({"subjects": subs})).into_response()
}
