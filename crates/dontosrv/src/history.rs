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

/// Query parameters for `/history/:subject`. All optional; designed so the
/// UI can ship a tiny initial render and let the user filter to drill in.
#[derive(Debug, serde::Deserialize, Default)]
pub struct HistoryParams {
    /// Cap on the number of rows shipped. Default 2000, max 20000.
    #[serde(default)]
    pub limit:     Option<i64>,
    /// Restrict to a single context.
    #[serde(default)]
    pub context:   Option<String>,
    /// Restrict to a single predicate.
    #[serde(default)]
    pub predicate: Option<String>,
    /// Lower bound on `valid_time` (ISO date), inclusive.
    #[serde(default)]
    pub from:      Option<chrono::NaiveDate>,
    /// Upper bound on `valid_time` (ISO date), inclusive.
    #[serde(default)]
    pub to:        Option<chrono::NaiveDate>,
    /// Include retracted rows? Default true (the visualisation needs them).
    #[serde(default)]
    pub include_retracted: Option<bool>,
}

pub async fn handle(
    State(state): State<Arc<AppState>>,
    Path(subject): Path<String>,
    axum::extract::Query(p): axum::extract::Query<HistoryParams>,
) -> impl IntoResponse {
    let pool = state.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };

    let limit  = p.limit.unwrap_or(2000).clamp(1, 20_000);
    let include_retracted = p.include_retracted.unwrap_or(true);

    // Total count (cheap; subject is indexed) so the UI knows whether the
    // result was truncated.
    let total: i64 = conn.query_one(
        "select count(*)::bigint from donto_statement
          where subject = $1
            and ($2::boolean or upper(tx_time) is null)
            and ($3::text is null or context = $3)
            and ($4::text is null or predicate = $4)",
        &[&subject, &include_retracted, &p.context, &p.predicate],
    ).await.map(|r| r.get(0)).unwrap_or(0);

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
            and ($2::boolean or upper(s.tx_time) is null)
            and ($3::text is null or s.context = $3)
            and ($4::text is null or s.predicate = $4)
            and ($5::date is null or upper(s.valid_time) is null
                 or upper(s.valid_time) > $5)
            and ($6::date is null or lower(s.valid_time) is null
                 or lower(s.valid_time) <= $6)
          order by lower(s.tx_time) asc
          limit $7",
        &[&subject, &include_retracted, &p.context, &p.predicate,
          &p.from, &p.to, &limit],
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
        "subject":    subject,
        "count":      out.len(),
        "total":      total,
        "truncated":  (out.len() as i64) < total,
        "limit":      limit,
        "filters":    {
            "context":   p.context,
            "predicate": p.predicate,
            "from":      p.from,
            "to":        p.to,
            "include_retracted": include_retracted,
        },
        "rows":       out,
    })).into_response()
}

/// GET /subjects → recently-touched subjects, with row counts.
///
/// A naive `SELECT subject, count(*) FROM donto_statement GROUP BY subject`
/// is O(table) — minutes on a 25M-row deployment. We instead read
/// `donto_audit` for the recent window, find distinct subjects, and look up
/// their row counts via the indexed (subject, predicate, object_iri) btree.
/// Bounded by audit-window size, not table size.
pub async fn list_subjects(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let pool = state.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };

    // Distinct subjects touched in the last 30 days, capped at 5000 audit
    // rows. Both bounds keep this O(small).
    let recent = match conn.query(
        "with recent_audit as (
             select statement_id from donto_audit
              where at > now() - interval '30 days'
              order by at desc
              limit 5000
         )
         select distinct s.subject
           from recent_audit ra
           join donto_statement s on s.statement_id = ra.statement_id
           order by 1
           limit 50",
        &[],
    ).await {
        Ok(rs) => rs,
        Err(e) => return Json(json!({"error": format!("/subjects audit scan: {e}")})).into_response(),
    };

    // Per-subject row count via the indexed (subject, predicate, object_iri)
    // btree — O(log n) per subject.
    let mut subs: Vec<Value> = Vec::with_capacity(recent.len());
    for r in &recent {
        let s: String = r.get(0);
        let n: i64 = match conn.query_one(
            "select count(*)::bigint from donto_statement where subject = $1",
            &[&s],
        ).await {
            Ok(row) => row.get(0),
            Err(_)  => 0,
        };
        subs.push(json!({"subject": s, "count": n}));
    }
    subs.sort_by(|a, b|
        b["count"].as_i64().unwrap_or(0).cmp(&a["count"].as_i64().unwrap_or(0))
    );

    Json(json!({"subjects": subs})).into_response()
}

/// GET /search?q=<text>  — full-text-ish search by label predicates.
///
/// Looks at rdfs:label / ex:label / ex:name literals where the value
/// matches the query. Returns distinct subjects with a representative
/// label and total row count. Bounded by `limit` (default 25, max 100).
#[derive(Debug, serde::Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(p): axum::extract::Query<SearchParams>,
) -> impl IntoResponse {
    let pool = state.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };
    let q = p.q.trim();
    if q.is_empty() {
        return Json(json!({"matches": [], "q": ""})).into_response();
    }
    let limit = p.limit.unwrap_or(25).clamp(1, 100);
    let needle = format!("%{q}%");

    // Union three sources of hits, distinct on subject:
    //   1. label-like literal match (rdfs:label / ex:label / ex:name / name /
    //      label / title — predicate registry is user-folksonomy so we cast a
    //      wide net)
    //   2. subject-IRI substring match (so a query of "ajax davis" finds
    //      ex:ajax-davis-actor even before any name statement exists)
    //   3. any literal object that matches ilike (cheap fallback; lets a
    //      search for e.g. "IMDb" find subjects cited by IMDb even when the
    //      label statement isn't there yet).
    let rows = match conn.query(
        "with label_hits as (
             select distinct on (subject)
                    subject,
                    object_lit ->> 'v' as label
               from donto_statement
              where predicate in (
                      'rdfs:label','ex:label','ex:name','name','label','title'
                    )
                and object_lit is not null
                and (object_lit ->> 'v') ilike $1
                and upper(tx_time) is null
              order by subject, length(object_lit ->> 'v') asc
              limit 200
         ),
         iri_hits as (
             select distinct subject, null::text as label
               from donto_statement
              where replace(replace(lower(subject), ':', ' '), '-', ' ')
                      ilike replace(replace(lower($1), ':', ' '), '-', ' ')
                and upper(tx_time) is null
              limit 200
         ),
         lit_hits as (
             select distinct subject, null::text as label
               from donto_statement
              where object_lit is not null
                and (object_lit ->> 'v') ilike $1
                and upper(tx_time) is null
              limit 200
         ),
         hits as (
             select subject, label from label_hits
             union
             select subject, label from iri_hits
             union
             select subject, label from lit_hits
         )
         select h.subject,
                (select object_lit ->> 'v'
                   from donto_statement s
                  where s.subject = h.subject
                    and s.predicate in (
                          'rdfs:label','ex:label','ex:name','name','label','title'
                        )
                    and s.object_lit is not null
                    and upper(s.tx_time) is null
                  order by length(s.object_lit ->> 'v') asc
                  limit 1) as label,
                (select count(*)::bigint from donto_statement s
                  where s.subject = h.subject and upper(s.tx_time) is null) as row_count
           from (select distinct subject, max(label) as label from hits group by subject) h
          order by row_count desc, h.subject
          limit $2",
        &[&needle, &limit],
    ).await {
        Ok(rs) => rs,
        Err(e) => return Json(json!({"error": format!("/search: {e}")})).into_response(),
    };

    let matches: Vec<Value> = rows.iter().map(|r| {
        let subject: String = r.get(0);
        let label: Option<String> = r.get(1);
        let n: i64 = r.get(2);
        json!({"subject": subject, "label": label, "count": n})
    }).collect();

    Json(json!({"q": q, "matches": matches})).into_response()
}

/// GET /statement/:id  — everything about one statement.
///
/// Returns the row itself + lineage (both directions), audit-log entries,
/// and certificate (if attached). Used by the click-an-item detail drawer
/// in donto-faces; also useful from a terminal.
pub async fn statement_detail(
    State(state): State<Arc<AppState>>,
    Path(id_str): Path<String>,
) -> impl IntoResponse {
    let id = match uuid::Uuid::parse_str(&id_str) {
        Ok(u) => u,
        Err(_) => return Json(json!({"error": "not a uuid", "id": id_str})).into_response(),
    };
    let pool = state.client.pool();
    let conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };

    // 1. The statement itself.
    let row = match conn.query_opt(
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
          where s.statement_id = $1",
        &[&id],
    ).await {
        Ok(Some(r)) => r,
        Ok(None)    => return Json(json!({"error": "not found", "id": id_str})).into_response(),
        Err(e)      => return Json(json!({"error": format!("statement query: {e}")})).into_response(),
    };

    let row_json = json!({
        "statement_id": row.get::<_, uuid::Uuid>(0),
        "subject":      row.get::<_, String>(1),
        "predicate":    row.get::<_, String>(2),
        "object_iri":   row.get::<_, Option<String>>(3),
        "object_lit":   row.get::<_, Option<Value>>(4),
        "context":      row.get::<_, String>(5),
        "polarity":     row.get::<_, String>(6),
        "maturity":     row.get::<_, i32>(7),
        "valid_lo":     row.get::<_, Option<chrono::NaiveDate>>(8),
        "valid_hi":     row.get::<_, Option<chrono::NaiveDate>>(9),
        "tx_lo":        row.get::<_, chrono::DateTime<chrono::Utc>>(10),
        "tx_hi":        row.get::<_, Option<chrono::DateTime<chrono::Utc>>>(11),
        "lineage":      row.get::<_, Vec<uuid::Uuid>>(12),
    });

    // 2. Lineage sources (full statement rows for each id this row was
    //    derived from). Indexed lookup, fast.
    let sources: Vec<Value> = match conn.query(
        "select s.statement_id, s.subject, s.predicate, s.object_iri, s.object_lit,
                s.context, donto_polarity(s.flags), donto_maturity(s.flags),
                lower(s.valid_time), upper(s.valid_time),
                lower(s.tx_time),    upper(s.tx_time)
           from donto_stmt_lineage l
           join donto_statement   s on s.statement_id = l.source_stmt
          where l.statement_id = $1",
        &[&id],
    ).await {
        Ok(rs) => rs.iter().map(brief_row).collect(),
        Err(_) => vec![],
    };

    // 3. Reverse lineage — statements that were derived FROM this one.
    let derived: Vec<Value> = match conn.query(
        "select s.statement_id, s.subject, s.predicate, s.object_iri, s.object_lit,
                s.context, donto_polarity(s.flags), donto_maturity(s.flags),
                lower(s.valid_time), upper(s.valid_time),
                lower(s.tx_time),    upper(s.tx_time)
           from donto_stmt_lineage l
           join donto_statement   s on s.statement_id = l.statement_id
          where l.source_stmt = $1
          limit 50",
        &[&id],
    ).await {
        Ok(rs) => rs.iter().map(brief_row).collect(),
        Err(_) => vec![],
    };

    // 4. Audit log entries for this statement.
    let audit: Vec<Value> = match conn.query(
        "select at, actor, action, detail
           from donto_audit
          where statement_id = $1
          order by at asc",
        &[&id],
    ).await {
        Ok(rs) => rs.iter().map(|r| json!({
            "at":     r.get::<_, chrono::DateTime<chrono::Utc>>(0),
            "actor":  r.get::<_, Option<String>>(1),
            "action": r.get::<_, String>(2),
            "detail": r.get::<_, Value>(3),
        })).collect(),
        Err(_) => vec![],
    };

    // 5. Certificate (if any).
    let certificate = match conn.query_opt(
        "select kind, rule_iri, inputs, body, signature, produced_at,
                verified_at, verifier, verified_ok
           from donto_stmt_certificate
          where statement_id = $1",
        &[&id],
    ).await {
        Ok(Some(r)) => Some(json!({
            "kind":        r.get::<_, String>(0),
            "rule_iri":    r.get::<_, Option<String>>(1),
            "inputs":      r.get::<_, Vec<uuid::Uuid>>(2),
            "body":        r.get::<_, Value>(3),
            "signature":   r.get::<_, Option<Vec<u8>>>(4).map(hex::encode),
            "produced_at": r.get::<_, chrono::DateTime<chrono::Utc>>(5),
            "verified_at": r.get::<_, Option<chrono::DateTime<chrono::Utc>>>(6),
            "verifier":    r.get::<_, Option<String>>(7),
            "verified_ok": r.get::<_, Option<bool>>(8),
        })),
        _ => None,
    };

    // 6. Sibling statements: same subject + same predicate, all polarities,
    //    all contexts. Useful for "what else has been said about this?"
    let siblings: Vec<Value> = match conn.query(
        "select statement_id, subject, predicate, object_iri, object_lit,
                context, donto_polarity(flags), donto_maturity(flags),
                lower(valid_time), upper(valid_time),
                lower(tx_time),    upper(tx_time)
           from donto_statement
          where subject   = $1
            and predicate = $2
            and statement_id <> $3
          order by lower(tx_time) desc
          limit 50",
        &[&row_json["subject"].as_str().unwrap_or(""),
          &row_json["predicate"].as_str().unwrap_or(""),
          &id],
    ).await {
        Ok(rs) => rs.iter().map(brief_row).collect(),
        Err(_) => vec![],
    };

    Json(json!({
        "statement":   row_json,
        "lineage":     {"sources": sources, "derived": derived},
        "audit":       audit,
        "certificate": certificate,
        "siblings":    siblings,
    })).into_response()
}

/// Brief row encoder (no lineage column — saves a join per row).
fn brief_row(r: &tokio_postgres::Row) -> Value {
    json!({
        "statement_id": r.get::<_, uuid::Uuid>(0),
        "subject":      r.get::<_, String>(1),
        "predicate":    r.get::<_, String>(2),
        "object_iri":   r.get::<_, Option<String>>(3),
        "object_lit":   r.get::<_, Option<Value>>(4),
        "context":      r.get::<_, String>(5),
        "polarity":     r.get::<_, String>(6),
        "maturity":     r.get::<_, i32>(7),
        "valid_lo":     r.get::<_, Option<chrono::NaiveDate>>(8),
        "valid_hi":     r.get::<_, Option<chrono::NaiveDate>>(9),
        "tx_lo":        r.get::<_, chrono::DateTime<chrono::Utc>>(10),
        "tx_hi":        r.get::<_, Option<chrono::DateTime<chrono::Utc>>>(11),
    })
}
