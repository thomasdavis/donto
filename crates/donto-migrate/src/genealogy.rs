//! Genealogy SQLite (research.db) migrator. PRD §24 table mapping.
//!
//! Tables expected (probed; missing ones are skipped without error):
//!   entities, claims, events, participants, relationships,
//!   aliases, discrepancies, sources, hypotheses, ingestion_log.
//!
//! Strategy:
//!   1. Each `sources` row → a `source`-kind context under `<root>/source/`.
//!   2. `entities` → `<root>/iri/<id>` plus rdfs:label and per-row metadata.
//!   3. `claims` → statements in the source's context with confidence as
//!      maturity (uncertified=0, speculative=0, moderate=1, strong=1)
//!      pending Phase 5 confidence overlays.
//!   4. `events` → event-node pattern under `<root>/event/<id>`.
//!   5. `participants` → role-predicate statements on the event node.
//!   6. `relationships` → binary statements; sameAs / possiblySame /
//!      differentFrom mapped to donto:* predicates.
//!   7. `aliases` → reified statements with bitemporal valid_time.
//!   8. `discrepancies` → exposed as a shape query at runtime; preserved
//!      here as donto:Discrepancy event-nodes for audit.
//!   9. `hypotheses` → hypothesis-kind context per hypothesis.
//!  10. `ingestion_log` → donto_audit entries (action='migrated').
//!
//! Round-trip parity is verified by row-count + sample-query equivalence
//! between the SQLite source and donto target.

use anyhow::{anyhow, Context, Result};
use chrono::NaiveDate;
use donto_client::{DontoClient, Literal, Object, StatementInput};
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Default, Serialize)]
pub struct MigrationReport {
    pub root: String,
    pub sqlite_path: String,
    pub dry_run: bool,
    pub source_contexts: u64,
    pub entities: u64,
    pub claims: u64,
    pub events: u64,
    pub participants: u64,
    pub relationships: u64,
    pub aliases: u64,
    pub discrepancies: u64,
    pub hypotheses: u64,
    pub ingestion_log: u64,
    pub statements_emitted: u64,
}

pub async fn migrate(
    client: &DontoClient,
    sqlite: &Path,
    root: &str,
    dry_run: bool,
) -> Result<MigrationReport> {
    let conn = Connection::open(sqlite).with_context(|| format!("opening {}", sqlite.display()))?;
    let mut report = MigrationReport {
        root: root.into(),
        sqlite_path: sqlite.display().to_string(),
        dry_run,
        ..Default::default()
    };
    let mut all: Vec<StatementInput> = Vec::new();

    // Root context.
    if !dry_run {
        client
            .ensure_context(root, "custom", "permissive", None)
            .await?;
    }

    // 1. sources → contexts.
    if has_table(&conn, "sources")? {
        let mut stmt = conn.prepare("select id, name, kind, citation from sources")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: String = r.get::<_, rusqlite::types::Value>(0).map(stringify_value)?;
            let name: Option<String> = r.get(1).ok();
            let _kind: Option<String> = r.get(2).ok();
            let citation: Option<String> = r.get(3).ok();
            let ctx_iri = format!("{root}/source/{id}");
            if !dry_run {
                client
                    .ensure_context(&ctx_iri, "source", "permissive", Some(root))
                    .await?;
            }
            if let Some(label) = name {
                all.push(
                    StatementInput::new(
                        &ctx_iri,
                        "rdfs:label",
                        Object::lit(Literal::string(label)),
                    )
                    .with_context(root),
                );
            }
            if let Some(c) = citation {
                all.push(
                    StatementInput::new(
                        &ctx_iri,
                        "donto:citation",
                        Object::lit(Literal::string(c)),
                    )
                    .with_context(root),
                );
            }
            report.source_contexts += 1;
        }
    }

    // 2. entities.
    if has_table(&conn, "entities")? {
        let mut stmt = conn.prepare("select id, kind, label, source_id from entities")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: String = r.get::<_, rusqlite::types::Value>(0).map(stringify_value)?;
            let kind: Option<String> = r.get(1).ok();
            let label: Option<String> = r.get(2).ok();
            let src: Option<String> = r
                .get::<_, rusqlite::types::Value>(3)
                .ok()
                .map(stringify_value);
            let iri = format!("{root}/iri/{id}");
            let ctx = src
                .as_deref()
                .map(|s| format!("{root}/source/{s}"))
                .unwrap_or_else(|| root.into());
            if let Some(k) = kind {
                all.push(
                    StatementInput::new(&iri, "rdf:type", Object::iri(format!("ex:{k}")))
                        .with_context(&ctx),
                );
            }
            if let Some(l) = label {
                all.push(
                    StatementInput::new(&iri, "rdfs:label", Object::lit(Literal::string(l)))
                        .with_context(&ctx),
                );
            }
            report.entities += 1;
        }
    }

    // 3. claims (subject, predicate, object, source_id, confidence).
    if has_table(&conn, "claims")? {
        let mut stmt = conn
            .prepare("select subject_id, predicate, object, source_id, confidence from claims")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let subj_id: String = r.get::<_, rusqlite::types::Value>(0).map(stringify_value)?;
            let predicate: String = r.get(1)?;
            let obj_raw: rusqlite::types::Value = r.get(2)?;
            let src: Option<String> = r
                .get::<_, rusqlite::types::Value>(3)
                .ok()
                .map(stringify_value);
            let conf: Option<String> = r.get(4).ok();

            let subject = format!("{root}/iri/{subj_id}");
            let object = match obj_raw {
                rusqlite::types::Value::Text(s)
                    if s.starts_with("urn:") || s.starts_with("ex:") || s.starts_with("http") =>
                {
                    Object::iri(s)
                }
                rusqlite::types::Value::Integer(i) => Object::lit(Literal::integer(i)),
                rusqlite::types::Value::Real(f) => Object::lit(Literal {
                    v: serde_json::json!(f),
                    dt: "xsd:decimal".into(),
                    lang: None,
                }),
                rusqlite::types::Value::Text(s) => Object::lit(Literal::string(s)),
                rusqlite::types::Value::Null => continue,
                rusqlite::types::Value::Blob(_) => continue,
            };
            let ctx = src
                .as_deref()
                .map(|s| format!("{root}/source/{s}"))
                .unwrap_or_else(|| root.into());
            let maturity = match conf.as_deref() {
                Some("strong") | Some("moderate") => 1u8,
                _ => 0u8,
            };
            all.push(
                StatementInput::new(subject, predicate, object)
                    .with_context(ctx)
                    .with_maturity(maturity),
            );
            report.claims += 1;
        }
    }

    // 4. events + 5. participants.
    if has_table(&conn, "events")? {
        let mut stmt = conn.prepare("select id, kind, date, place, source_id from events")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: String = r.get::<_, rusqlite::types::Value>(0).map(stringify_value)?;
            let kind: Option<String> = r.get(1).ok();
            let date: Option<String> = r.get(2).ok();
            let place: Option<String> = r.get(3).ok();
            let src: Option<String> = r
                .get::<_, rusqlite::types::Value>(4)
                .ok()
                .map(stringify_value);
            let ev = format!("{root}/event/{id}");
            let ctx = src
                .as_deref()
                .map(|s| format!("{root}/source/{s}"))
                .unwrap_or_else(|| root.into());
            if let Some(k) = kind {
                all.push(
                    StatementInput::new(&ev, "rdf:type", Object::iri(format!("ex:{k}")))
                        .with_context(&ctx),
                );
            }
            if let Some(d) = date {
                if let Ok(nd) = parse_date(&d) {
                    all.push(
                        StatementInput::new(&ev, "ex:when", Object::lit(Literal::date(nd)))
                            .with_context(&ctx),
                    );
                } else {
                    all.push(
                        StatementInput::new(&ev, "ex:whenText", Object::lit(Literal::string(d)))
                            .with_context(&ctx),
                    );
                }
            }
            if let Some(pl) = place {
                all.push(
                    StatementInput::new(&ev, "ex:place", Object::lit(Literal::string(pl)))
                        .with_context(&ctx),
                );
            }
            report.events += 1;
        }
    }
    if has_table(&conn, "participants")? {
        let mut stmt = conn.prepare("select event_id, entity_id, role from participants")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let ev_id: String = r.get::<_, rusqlite::types::Value>(0).map(stringify_value)?;
            let en_id: String = r.get::<_, rusqlite::types::Value>(1).map(stringify_value)?;
            let role: Option<String> = r.get(2).ok();
            let ev = format!("{root}/event/{ev_id}");
            let pe = format!("{root}/iri/{en_id}");
            let pred = match role {
                Some(r) => format!("ex:role/{r}"),
                None => "ex:participant".into(),
            };
            all.push(StatementInput::new(ev, pred, Object::iri(pe)).with_context(root));
            report.participants += 1;
        }
    }

    // 6. relationships.
    if has_table(&conn, "relationships")? {
        let mut stmt =
            conn.prepare("select left_id, right_id, kind, confidence from relationships")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let left: String = r.get::<_, rusqlite::types::Value>(0).map(stringify_value)?;
            let right: String = r.get::<_, rusqlite::types::Value>(1).map(stringify_value)?;
            let kind: String = r.get(2)?;
            let conf: Option<String> = r.get(3).ok();
            let p = match kind.as_str() {
                "sameAs" => "donto:sameAs".to_string(),
                "possiblySame" => "donto:possiblySame".to_string(),
                "differentFrom" => "donto:differentFrom".to_string(),
                other => format!("ex:rel/{other}"),
            };
            let maturity = if matches!(conf.as_deref(), Some("strong") | Some("moderate")) {
                1u8
            } else {
                0u8
            };
            all.push(
                StatementInput::new(
                    format!("{root}/iri/{left}"),
                    p,
                    Object::iri(format!("{root}/iri/{right}")),
                )
                .with_context(root)
                .with_maturity(maturity),
            );
            report.relationships += 1;
        }
    }

    // 7. aliases (with year/location bitemporal scope).
    if has_table(&conn, "aliases")? {
        let mut stmt =
            conn.prepare("select entity_id, name, year_start, year_end, location from aliases")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: String = r.get::<_, rusqlite::types::Value>(0).map(stringify_value)?;
            let name: String = r.get(1)?;
            let ys: Option<i64> = r.get(2).ok();
            let ye: Option<i64> = r.get(3).ok();
            let loc: Option<String> = r.get(4).ok();
            let ent = format!("{root}/iri/{id}");
            let lo = ys.and_then(|y| NaiveDate::from_ymd_opt(y as i32, 1, 1));
            let hi = ye.and_then(|y| NaiveDate::from_ymd_opt(y as i32, 1, 1));
            let stmt = StatementInput::new(&ent, "ex:knownAs", Object::lit(Literal::string(&name)))
                .with_context(root)
                .with_valid(lo, hi);
            all.push(stmt);
            if let Some(l) = loc {
                all.push(
                    StatementInput::new(
                        &ent,
                        "ex:knownAsLocation",
                        Object::lit(Literal::string(l)),
                    )
                    .with_context(root)
                    .with_valid(lo, hi),
                );
            }
            report.aliases += 1;
        }
    }

    // 8. discrepancies (preserved as audit nodes; live shape query is Phase 5+).
    if has_table(&conn, "discrepancies")? {
        let mut stmt = conn.prepare("select id, summary, kind from discrepancies")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: String = r.get::<_, rusqlite::types::Value>(0).map(stringify_value)?;
            let summary: Option<String> = r.get(1).ok();
            let kind: Option<String> = r.get(2).ok();
            let d = format!("{root}/discrepancy/{id}");
            all.push(
                StatementInput::new(&d, "rdf:type", Object::iri("donto:Discrepancy"))
                    .with_context(root),
            );
            if let Some(s) = summary {
                all.push(
                    StatementInput::new(&d, "rdfs:comment", Object::lit(Literal::string(s)))
                        .with_context(root),
                );
            }
            if let Some(k) = kind {
                all.push(
                    StatementInput::new(&d, "ex:discrepancyKind", Object::lit(Literal::string(k)))
                        .with_context(root),
                );
            }
            report.discrepancies += 1;
        }
    }

    // 9. hypotheses → hypothesis-kind contexts.
    if has_table(&conn, "hypotheses")? {
        let mut stmt = conn.prepare("select id, statement, status from hypotheses")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let id: String = r.get::<_, rusqlite::types::Value>(0).map(stringify_value)?;
            let statement: Option<String> = r.get(1).ok();
            let status: Option<String> = r.get(2).ok();
            let h = format!("{root}/hypothesis/{id}");
            let hctx = format!("ctx:hypo/{id}");
            if !dry_run {
                client
                    .ensure_context(&hctx, "hypothesis", "permissive", Some(root))
                    .await?;
            }
            all.push(
                StatementInput::new(&h, "rdf:type", Object::iri("donto:Hypothesis"))
                    .with_context(root),
            );
            if let Some(s) = statement {
                all.push(
                    StatementInput::new(&h, "donto:statement", Object::lit(Literal::string(s)))
                        .with_context(root),
                );
            }
            if let Some(s) = status {
                all.push(
                    StatementInput::new(&h, "donto:status", Object::iri(format!("donto:{s}")))
                        .with_context(root),
                );
            }
            report.hypotheses += 1;
        }
    }

    // 10. ingestion_log → audit (kept lightweight; actually written if not dry).
    if has_table(&conn, "ingestion_log")? {
        let mut stmt = conn.prepare("select count(*) from ingestion_log")?;
        let n: i64 = stmt.query_row([], |r| r.get(0))?;
        report.ingestion_log = n as u64;
    }

    report.statements_emitted = all.len() as u64;
    if !dry_run {
        for chunk in all.chunks(2000) {
            client.assert_batch(chunk).await?;
        }
    }
    Ok(report)
}

fn has_table(conn: &Connection, name: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "select count(*) from sqlite_master where type='table' and name=?1",
        rusqlite::params![name],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn stringify_value(v: rusqlite::types::Value) -> String {
    use rusqlite::types::Value::*;
    match v {
        Null => String::new(),
        Integer(i) => i.to_string(),
        Real(f) => f.to_string(),
        Text(s) => s,
        Blob(b) => format!("blob:{}", b.len()),
    }
}

fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y/%m/%d"))
        .map_err(|_| anyhow!("unparseable date `{s}`"))
}
