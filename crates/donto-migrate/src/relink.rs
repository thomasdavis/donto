//! Additive re-link migrator for the genealogy `research.db`.
//!
//! The original `genealogy` migrator (in `genealogy.rs`) preserved only a
//! thin slice of the SQLite schema — most provenance was dropped. This
//! pass walks the same database again and imports EVERYTHING that was
//! missing, as new subjects, without touching any existing donto rows.
//! `donto_assert` dedupes by content hash, so re-running is safe.
//!
//! New subjects emitted, all under `<root>`:
//!
//!   <root>/document/<id>          — every documents row
//!   <root>/chunk/<id>             — every chunks row (with content excerpt)
//!   <root>/claim/<id>             — every claims row with full provenance
//!   <root>/discrepancy/<id>       — every discrepancies row with resolution
//!   <root>/hypothesis/<id>        — full hypothesis fields
//!   <root>/event/<id>             — re-link with precision / location / description
//!   <root>/relationship/<id>      — re-link with valid_from/until / created_at
//!   <root>/alias/<id>             — alias as its own subject (linked back to entity)
//!   <root>/extraction-cost/<id>   — token_usage rows
//!   <root>/ingestion/<id>         — ingestion_log rows
//!   <root>/person/<id>            — persons rows (small)
//!
//! Strict no-data-loss: nothing is deleted, nothing is updated. Existing
//! rows in donto stay verbatim.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use donto_client::{DontoClient, Literal, Object, Polarity, StatementInput};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Default, Serialize)]
pub struct RelinkReport {
    pub root:        String,
    pub sqlite_path: String,
    pub elapsed_ms:  u64,
    pub statements_emitted: u64,
    pub by_table:    BTreeMap<String, u64>,
}

/// Maximum chunk-content length to embed inline (in chars). Avoids 100KB
/// statements; full chunk text stays available in research.db itself.
const CHUNK_TEXT_MAX: usize = 1500;
const TEXT_SPAN_MAX:  usize = 800;
/// Batch size for assert_batch — server-side prepared statements run one
/// at a time inside, so smaller batches keep memory bounded.
const BATCH:          usize = 1000;

pub async fn relink(
    client: &DontoClient,
    sqlite: &Path,
    root: &str,
) -> Result<RelinkReport> {
    let conn = Connection::open(sqlite)
        .with_context(|| format!("opening {}", sqlite.display()))?;
    let mut report = RelinkReport {
        root: root.into(),
        sqlite_path: sqlite.display().to_string(),
        ..Default::default()
    };
    let started = Instant::now();

    client.ensure_context(root, "custom", "permissive", None).await?;

    let mut buf: Vec<StatementInput> = Vec::with_capacity(BATCH);
    macro_rules! flush {
        ($report:expr, $client:expr, $buf:expr) => {{
            if !$buf.is_empty() {
                let n = $client.assert_batch(&$buf).await?;
                $report.statements_emitted += n as u64;
                $buf.clear();
            }
        }};
    }

    // ── documents ─────────────────────────────────────────────────────────
    if has_table(&conn, "documents")? {
        eprintln!("documents…");
        let mut q = conn.prepare(
            "select id, path, title, doc_type, created, content_hash, tags from documents")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:           i64    = r.get(0)?;
            let path:         String = r.get(1)?;
            let title:        Option<String> = r.get(2).ok();
            let doc_type:     Option<String> = r.get(3).ok();
            let created:      Option<String> = r.get(4).ok();
            let content_hash: Option<String> = r.get(5).ok();
            let tags:         Option<String> = r.get(6).ok();
            let iri = format!("{root}/document/{id}");
            buf.push(s(&iri, "rdf:type", obj_iri("ex:Document"), root, None));
            buf.push(s(&iri, "donto:path", obj_str(&path), root, None));
            if let Some(t)   = title        { buf.push(s(&iri, "rdfs:label",        obj_str(&t),  root, None)); }
            if let Some(t)   = doc_type     { buf.push(s(&iri, "donto:docType",     obj_str(&t),  root, None)); }
            if let Some(c)   = created.as_deref() {
                let d = parse_date(c);
                buf.push(s(&iri, "donto:created", obj_str(c), root, d));
            }
            if let Some(h)   = content_hash { buf.push(s(&iri, "donto:contentHash", obj_str(&h),  root, None)); }
            if let Some(t)   = tags         { if !t.is_empty() && t != "[]" {
                buf.push(s(&iri, "donto:tags", obj_str(&t), root, None));
            }}
            n += 1;
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("documents".into(), n);
        eprintln!("  {n} documents");
    }

    // ── chunks ────────────────────────────────────────────────────────────
    if has_table(&conn, "chunks")? {
        eprintln!("chunks…");
        let mut q = conn.prepare(
            "select id, document_id, chunk_index, content from chunks")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:    i64 = r.get(0)?;
            let doc:   i64 = r.get(1)?;
            let idx:   i64 = r.get(2)?;
            let content: String = r.get(3)?;
            let iri = format!("{root}/chunk/{id}");
            buf.push(s(&iri, "rdf:type",      obj_iri("ex:Chunk"), root, None));
            buf.push(s(&iri, "donto:document", obj_iri(format!("{root}/document/{doc}")), root, None));
            buf.push(s(&iri, "donto:chunkIndex", Object::lit(Literal::integer(idx)), root, None));
            let excerpt = truncate(&content, CHUNK_TEXT_MAX);
            buf.push(s(&iri, "donto:content", obj_str(&excerpt), root, None));
            if content.len() > excerpt.len() {
                buf.push(s(&iri, "donto:contentTruncated", obj_int(content.chars().count() as i64), root, None));
            }
            n += 1;
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("chunks".into(), n);
        eprintln!("  {n} chunks");
    }

    // ── claims (the big one) ──────────────────────────────────────────────
    if has_table(&conn, "claims")? {
        eprintln!("claims (full provenance)…");
        let mut q = conn.prepare("
            select id, subject_id, predicate, object_value, object_entity_id,
                   source_id, chunk_id, text_span, confidence,
                   valid_from, valid_until, superseded_by,
                   extraction_model, extraction_timestamp
              from claims")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:               String  = r.get(0)?;
            let subject_id:       String  = r.get(1)?;
            let predicate:        String  = r.get(2)?;
            let object_value:     Option<String> = r.get(3).ok();
            let object_entity_id: Option<String> = r.get(4).ok();
            let source_id:        String  = r.get(5)?;
            let chunk_id:         Option<i64> = r.get(6).ok();
            let text_span:        Option<String> = r.get(7).ok();
            let confidence:       String  = r.get(8)?;
            let valid_from:       Option<String> = r.get(9).ok();
            let valid_until:      Option<String> = r.get(10).ok();
            let superseded_by:    Option<String> = r.get(11).ok();
            let extraction_model: Option<String> = r.get(12).ok();
            let extraction_ts:    Option<String> = r.get(13).ok();

            let iri = format!("{root}/claim/{id}");
            let valid_lo = valid_from.as_deref().and_then(parse_date);
            let valid_hi = valid_until.as_deref().and_then(parse_date);
            let valid = (valid_lo, valid_hi);

            buf.push(s_v(&iri, "rdf:type", obj_iri("ex:Claim"), root, valid));
            buf.push(s_v(&iri, "donto:claimSubject",   obj_iri(format!("{root}/iri/{subject_id}")), root, valid));
            buf.push(s_v(&iri, "donto:claimPredicate", obj_str(&predicate), root, valid));
            if let Some(v) = object_value.as_deref() {
                buf.push(s_v(&iri, "donto:claimObjectValue", obj_str(v), root, valid));
            }
            if let Some(e) = object_entity_id.as_deref() {
                buf.push(s_v(&iri, "donto:claimObjectEntity", obj_iri(format!("{root}/iri/{e}")), root, valid));
            }
            buf.push(s_v(&iri, "donto:sourceContext",
                obj_iri(format!("{root}/source/{source_id}")), root, valid));
            if let Some(cid) = chunk_id {
                buf.push(s_v(&iri, "donto:evidenceChunk",
                    obj_iri(format!("{root}/chunk/{cid}")), root, valid));
            }
            if let Some(ts) = text_span.as_deref() {
                let trimmed = truncate(ts, TEXT_SPAN_MAX);
                buf.push(s_v(&iri, "donto:textSpan", obj_str(&trimmed), root, valid));
            }
            buf.push(s_v(&iri, "donto:confidence", obj_str(&confidence), root, valid));
            if let Some(m) = extraction_model.as_deref() {
                buf.push(s_v(&iri, "donto:extractedBy", obj_str(m), root, valid));
            }
            if let Some(ts) = extraction_ts.as_deref() {
                buf.push(s_v(&iri, "donto:extractedAt", obj_str(ts), root, valid));
            }
            if let Some(sb) = superseded_by.as_deref() {
                buf.push(s_v(&iri, "donto:supersededBy",
                    obj_iri(format!("{root}/claim/{sb}")), root, valid));
            }
            n += 1;
            if n % 50_000 == 0 { eprintln!("  …{n} claims"); }
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("claims".into(), n);
        eprintln!("  {n} claims");
    }

    // ── discrepancies (full resolution fields) ────────────────────────────
    if has_table(&conn, "discrepancies")? {
        eprintln!("discrepancies…");
        let mut q = conn.prepare("
            select id, claim_a_id, claim_b_id, subject_id, predicate,
                   resolution, resolution_rule, preferred_claim_id, status,
                   created_at
              from discrepancies")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:        String = r.get(0)?;
            let a:         String = r.get(1)?;
            let b:         String = r.get(2)?;
            let subject:   String = r.get(3)?;
            let predicate: String = r.get(4)?;
            let resolution:        Option<String> = r.get(5).ok();
            let resolution_rule:   Option<String> = r.get(6).ok();
            let preferred_claim:   Option<String> = r.get(7).ok();
            let status:    String = r.get(8)?;
            let created_at:Option<String> = r.get(9).ok();
            let iri = format!("{root}/discrepancy/{id}");
            buf.push(s(&iri, "rdf:type", obj_iri("donto:Discrepancy"), root, None));
            buf.push(s(&iri, "donto:claimA",   obj_iri(format!("{root}/claim/{a}")), root, None));
            buf.push(s(&iri, "donto:claimB",   obj_iri(format!("{root}/claim/{b}")), root, None));
            buf.push(s(&iri, "donto:onSubject",   obj_iri(format!("{root}/iri/{subject}")), root, None));
            buf.push(s(&iri, "donto:onPredicate", obj_str(&predicate), root, None));
            buf.push(s(&iri, "donto:status",     obj_str(&status),    root, None));
            if let Some(r) = resolution.as_deref()      { buf.push(s(&iri, "donto:resolution",     obj_str(r), root, None)); }
            if let Some(r) = resolution_rule.as_deref() { buf.push(s(&iri, "donto:resolutionRule", obj_str(r), root, None)); }
            if let Some(p) = preferred_claim.as_deref() { buf.push(s(&iri, "donto:preferredClaim", obj_iri(format!("{root}/claim/{p}")), root, None)); }
            if let Some(c) = created_at.as_deref()      { buf.push(s(&iri, "donto:createdAt",      obj_str(c), root, None)); }
            n += 1;
            if n % 50_000 == 0 { eprintln!("  …{n} discrepancies"); }
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("discrepancies".into(), n);
        eprintln!("  {n} discrepancies");
    }

    // ── hypotheses (full fields) ──────────────────────────────────────────
    if has_table(&conn, "hypotheses")? {
        eprintln!("hypotheses (full)…");
        let mut q = conn.prepare("
            select id, statement, status, evidence_for, evidence_against,
                   decisive_record, resolved_date from hypotheses")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:        String = r.get(0)?;
            let stmt:      String = r.get(1)?;
            let status:    String = r.get(2)?;
            let ev_for:    Option<String> = r.get(3).ok();
            let ev_against:Option<String> = r.get(4).ok();
            let decisive:  Option<String> = r.get(5).ok();
            let resolved:  Option<String> = r.get(6).ok();
            let iri = format!("{root}/hypothesis-full/{id}");
            buf.push(s(&iri, "rdf:type", obj_iri("donto:Hypothesis"), root, None));
            buf.push(s(&iri, "donto:statement", obj_str(&stmt), root, None));
            buf.push(s(&iri, "donto:status",    obj_str(&status), root, None));
            if let Some(v) = ev_for.as_deref()     { buf.push(s(&iri, "donto:evidenceFor",     obj_str(v), root, None)); }
            if let Some(v) = ev_against.as_deref() { buf.push(s(&iri, "donto:evidenceAgainst", obj_str(v), root, None)); }
            if let Some(v) = decisive.as_deref()   { buf.push(s(&iri, "donto:decisiveRecord",  obj_str(v), root, None)); }
            if let Some(v) = resolved.as_deref()   {
                let d = parse_date(v);
                buf.push(s_v(&iri, "donto:resolvedDate", obj_str(v), root, (d, None)));
            }
            n += 1;
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("hypotheses".into(), n);
        eprintln!("  {n} hypotheses");
    }

    // ── relationships (full fields, as standalone subjects) ───────────────
    if has_table(&conn, "relationships")? {
        eprintln!("relationships (full)…");
        let mut q = conn.prepare("
            select id, type, entity_a_id, entity_b_id, source_id,
                   confidence, valid_from, valid_until, created_at
              from relationships")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:    String = r.get(0)?;
            let typ:   String = r.get(1)?;
            let a:     String = r.get(2)?;
            let b:     String = r.get(3)?;
            let src:   Option<String> = r.get(4).ok();
            let conf:  Option<String> = r.get(5).ok();
            let v_from:Option<String> = r.get(6).ok();
            let v_until:Option<String> = r.get(7).ok();
            let created:Option<String> = r.get(8).ok();
            let valid = (
                v_from.as_deref().and_then(parse_date),
                v_until.as_deref().and_then(parse_date),
            );
            let iri = format!("{root}/relationship/{id}");
            buf.push(s_v(&iri, "rdf:type", obj_iri(format!("ex:rel/{typ}")), root, valid));
            buf.push(s_v(&iri, "donto:entityA", obj_iri(format!("{root}/iri/{a}")), root, valid));
            buf.push(s_v(&iri, "donto:entityB", obj_iri(format!("{root}/iri/{b}")), root, valid));
            if let Some(s2) = src.as_deref()   { buf.push(s_v(&iri, "donto:sourceContext", obj_iri(format!("{root}/source/{s2}")), root, valid)); }
            if let Some(c)  = conf.as_deref()  { buf.push(s_v(&iri, "donto:confidence", obj_str(c), root, valid)); }
            if let Some(c)  = created.as_deref() { buf.push(s_v(&iri, "donto:createdAt", obj_str(c), root, valid)); }
            n += 1;
            if n % 50_000 == 0 { eprintln!("  …{n} relationships"); }
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("relationships".into(), n);
        eprintln!("  {n} relationships");
    }

    // ── events (full fields) ──────────────────────────────────────────────
    if has_table(&conn, "events")? {
        eprintln!("events (full fields)…");
        let mut q = conn.prepare("
            select id, type, date_value, date_precision, location_id,
                   description, source_id, created_at from events")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:        String = r.get(0)?;
            let typ:       String = r.get(1)?;
            let date:      Option<String> = r.get(2).ok();
            let precision: Option<String> = r.get(3).ok();
            let loc:       Option<String> = r.get(4).ok();
            let desc:      Option<String> = r.get(5).ok();
            let src:       Option<String> = r.get(6).ok();
            let created:   Option<String> = r.get(7).ok();
            let iri = format!("{root}/event-full/{id}");
            let d = date.as_deref().and_then(parse_date);
            buf.push(s_v(&iri, "rdf:type", obj_iri(format!("ex:{typ}")), root, (d, None)));
            if let Some(dv) = date.as_deref()      { buf.push(s_v(&iri, "donto:dateValue",     obj_str(dv), root, (d, None))); }
            if let Some(p)  = precision.as_deref() { buf.push(s_v(&iri, "donto:datePrecision", obj_str(p),  root, (d, None))); }
            if let Some(l)  = loc.as_deref()       { buf.push(s_v(&iri, "donto:location",      obj_iri(format!("{root}/iri/{l}")), root, (d, None))); }
            if let Some(de) = desc.as_deref()      { buf.push(s_v(&iri, "donto:description",   obj_str(de), root, (d, None))); }
            if let Some(s2) = src.as_deref()       { buf.push(s_v(&iri, "donto:sourceContext", obj_iri(format!("{root}/source/{s2}")), root, (d, None))); }
            if let Some(c)  = created.as_deref()   { buf.push(s_v(&iri, "donto:createdAt",     obj_str(c), root, (d, None))); }
            n += 1;
            if n % 50_000 == 0 { eprintln!("  …{n} events"); }
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("events".into(), n);
        eprintln!("  {n} events");
    }

    // ── aliases (as own subjects with source link) ────────────────────────
    if has_table(&conn, "aliases")? {
        eprintln!("aliases (with source link)…");
        let mut q = conn.prepare(
            "select id, entity_id, name_form, year, location, source_id from aliases")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:        i64    = r.get(0)?;
            let entity:    String = r.get(1)?;
            let name:      String = r.get(2)?;
            let year:      Option<i64> = r.get(3).ok();
            let location:  Option<String> = r.get(4).ok();
            let src:       Option<String> = r.get(5).ok();
            let iri = format!("{root}/alias/{id}");
            let valid = (
                year.and_then(|y| NaiveDate::from_ymd_opt(y as i32, 1, 1)),
                year.and_then(|y| NaiveDate::from_ymd_opt((y + 1) as i32, 1, 1)),
            );
            buf.push(s_v(&iri, "rdf:type", obj_iri("ex:Alias"), root, valid));
            buf.push(s_v(&iri, "donto:entity",   obj_iri(format!("{root}/iri/{entity}")), root, valid));
            buf.push(s_v(&iri, "donto:nameForm", obj_str(&name), root, valid));
            if let Some(l) = location.as_deref() { buf.push(s_v(&iri, "donto:location", obj_str(l), root, valid)); }
            if let Some(s2) = src.as_deref()     { buf.push(s_v(&iri, "donto:sourceContext", obj_iri(format!("{root}/source/{s2}")), root, valid)); }
            n += 1;
            if n % 100_000 == 0 { eprintln!("  …{n} aliases"); }
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("aliases".into(), n);
        eprintln!("  {n} aliases");
    }

    // ── token_usage ───────────────────────────────────────────────────────
    if has_table(&conn, "token_usage")? {
        eprintln!("token_usage…");
        let mut q = conn.prepare(
            "select id, file_path, model, chunks, input_tokens, output_tokens,
                    total_tokens, cost_usd, created_at from token_usage")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:i64 = r.get(0)?;
            let path:String = r.get(1)?;
            let model:String = r.get(2)?;
            let chunks:i64 = r.get(3)?;
            let inp:i64 = r.get(4)?;
            let out:i64 = r.get(5)?;
            let tot:i64 = r.get(6)?;
            let cost:f64 = r.get(7)?;
            let created:Option<String> = r.get(8).ok();
            let iri = format!("{root}/extraction-cost/{id}");
            buf.push(s(&iri, "rdf:type", obj_iri("ex:ExtractionCost"), root, None));
            buf.push(s(&iri, "donto:filePath",     obj_str(&path), root, None));
            buf.push(s(&iri, "donto:model",        obj_str(&model), root, None));
            buf.push(s(&iri, "donto:chunks",       obj_int(chunks), root, None));
            buf.push(s(&iri, "donto:inputTokens",  obj_int(inp), root, None));
            buf.push(s(&iri, "donto:outputTokens", obj_int(out), root, None));
            buf.push(s(&iri, "donto:totalTokens",  obj_int(tot), root, None));
            buf.push(s(&iri, "donto:costUsd",      obj_decimal(cost), root, None));
            if let Some(c) = created.as_deref() { buf.push(s(&iri, "donto:createdAt", obj_str(c), root, None)); }
            n += 1;
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("token_usage".into(), n);
        eprintln!("  {n} token_usage");
    }

    // ── ingestion_log ─────────────────────────────────────────────────────
    if has_table(&conn, "ingestion_log")? {
        eprintln!("ingestion_log…");
        let mut q = conn.prepare(
            "select id, file_path, file_hash, timestamp, entities_extracted,
                    events_extracted, claims_extracted, relationships_extracted,
                    status, notes from ingestion_log")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:i64 = r.get(0)?;
            let path:String = r.get(1)?;
            let hash:String = r.get(2)?;
            let ts:String = r.get(3)?;
            let ent:i64 = r.get(4)?;
            let ev:i64 = r.get(5)?;
            let cl:i64 = r.get(6)?;
            let rel:i64 = r.get(7)?;
            let status:Option<String> = r.get(8).ok();
            let notes:Option<String> = r.get(9).ok();
            let iri = format!("{root}/ingestion/{id}");
            buf.push(s(&iri, "rdf:type", obj_iri("ex:IngestionRun"), root, None));
            buf.push(s(&iri, "donto:filePath",   obj_str(&path), root, None));
            buf.push(s(&iri, "donto:fileHash",   obj_str(&hash), root, None));
            buf.push(s(&iri, "donto:timestamp",  obj_str(&ts), root, None));
            buf.push(s(&iri, "donto:entitiesExtracted",      obj_int(ent), root, None));
            buf.push(s(&iri, "donto:eventsExtracted",        obj_int(ev),  root, None));
            buf.push(s(&iri, "donto:claimsExtracted",        obj_int(cl),  root, None));
            buf.push(s(&iri, "donto:relationshipsExtracted", obj_int(rel), root, None));
            if let Some(st) = status.as_deref() { buf.push(s(&iri, "donto:status", obj_str(st), root, None)); }
            if let Some(n2) = notes.as_deref()  { buf.push(s(&iri, "donto:notes",  obj_str(n2), root, None)); }
            n += 1;
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("ingestion_log".into(), n);
        eprintln!("  {n} ingestion_log");
    }

    // ── persons ───────────────────────────────────────────────────────────
    if has_table(&conn, "persons")? {
        eprintln!("persons…");
        let mut q = conn.prepare(
            "select id, name, confidence, birth_year_est, death_year_est, notes from persons")?;
        let mut rows = q.query([])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let id:String = r.get(0)?;
            let name:String = r.get(1)?;
            let conf:Option<String> = r.get(2).ok();
            let by:Option<i64> = r.get(3).ok();
            let dy:Option<i64> = r.get(4).ok();
            let notes:Option<String> = r.get(5).ok();
            let iri = format!("{root}/person/{id}");
            buf.push(s(&iri, "rdf:type", obj_iri("ex:Person"), root, None));
            buf.push(s(&iri, "rdfs:label", obj_str(&name), root, None));
            if let Some(c)  = conf.as_deref() { buf.push(s(&iri, "donto:confidence", obj_str(c), root, None)); }
            if let Some(y)  = by              { buf.push(s(&iri, "donto:birthYearEst", obj_int(y), root, None)); }
            if let Some(y)  = dy              { buf.push(s(&iri, "donto:deathYearEst", obj_int(y), root, None)); }
            if let Some(n2) = notes.as_deref(){ buf.push(s(&iri, "donto:notes",      obj_str(n2), root, None)); }
            n += 1;
            if buf.len() >= BATCH { flush!(report, client, buf); }
        }
        flush!(report, client, buf);
        report.by_table.insert("persons".into(), n);
        eprintln!("  {n} persons");
    }

    flush!(report, client, buf);
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(report)
}

// ── Tiny helpers ─────────────────────────────────────────────────────────

fn s(subj: &str, pred: &str, o: Object, ctx: &str, valid_lo: Option<NaiveDate>) -> StatementInput {
    let mut si = StatementInput::new(subj, pred, o)
        .with_context(ctx)
        .with_polarity(Polarity::Asserted);
    if let Some(d) = valid_lo { si = si.with_valid(Some(d), None); }
    si
}
fn s_v(subj: &str, pred: &str, o: Object, ctx: &str, valid: (Option<NaiveDate>, Option<NaiveDate>)) -> StatementInput {
    StatementInput::new(subj, pred, o)
        .with_context(ctx)
        .with_polarity(Polarity::Asserted)
        .with_valid(valid.0, valid.1)
}
fn obj_str(s: &str) -> Object { Object::lit(Literal::string(scrub(s))) }
/// Strip NUL bytes (jsonb-text rejects `\u0000`) and any other isolated
/// surrogate halves. Genealogy text_span values occasionally contain stray
/// NULs from upstream OCR / PDF extraction.
fn scrub(s: &str) -> String {
    if !s.contains('\0') && !s.chars().any(|c| (c as u32) < 0x20 && c != '\n' && c != '\r' && c != '\t') {
        return s.to_string();
    }
    s.chars()
        .filter(|&c| c != '\0' && (c >= ' ' || c == '\n' || c == '\r' || c == '\t'))
        .collect()
}
fn obj_int(n: i64) -> Object { Object::lit(Literal::integer(n)) }
fn obj_decimal(f: f64) -> Object {
    Object::lit(Literal {
        v: serde_json::json!(f),
        dt: "xsd:decimal".into(),
        lang: None,
    })
}
fn obj_iri(s: impl Into<String>) -> Object { Object::iri(s.into()) }
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() }
    else {
        let cut = s.char_indices().nth(max).map(|(i, _)| i).unwrap_or(s.len());
        format!("{}…", &s[..cut])
    }
}
fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
        .or_else(|| NaiveDate::parse_from_str(s, "%Y/%m/%d").ok())
        .or_else(|| {
            // Some dates are timestamps; try the date prefix.
            if s.len() >= 10 { NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok() } else { None }
        })
}
fn has_table(conn: &Connection, name: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "select count(*) from sqlite_master where type='table' and name=?1",
        rusqlite::params![name], |r| r.get(0))?;
    Ok(n > 0)
}
