//! Build a [`ReleaseManifest`] from a [`ReleaseSpec`].
//!
//! The skeleton resolves the release contents by enumerating
//! statements visible in the listed contexts (as of the spec's
//! `as_of` if any), computes a deterministic per-statement SHA-256,
//! evaluates the policy gate, and returns the manifest. The DontoQL
//! query strings are *recorded* in `query_specs` for citation but the
//! statement set is driven by the contexts; this matches the
//! reproducibility property the PRD requires (same data + same spec
//! → same checksums) without depending on evaluator output shape.

use crate::manifest::{
    Citation, LossReport, ReleaseManifest, StatementChecksum,
};
use std::collections::BTreeMap;
use crate::policy::evaluate_policy;
use crate::ReleaseError;
use chrono::{DateTime, Utc};
use donto_client::{ContextScope, DontoClient, Object, Statement};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseSpec {
    /// Caller-supplied stable identifier for this release.
    /// Re-using the same id with the same data must produce the same
    /// `manifest_sha256`.
    pub release_id: String,
    /// DontoQL query strings the release was *meant* to answer. Recorded
    /// in the manifest for citation; not interpreted by the skeleton
    /// builder.
    pub query_specs: Vec<String>,
    /// Contexts whose statements are bundled.
    pub contexts: Vec<String>,
    /// Optional bitemporal lens — read state as it stood at this tx_at.
    pub as_of: Option<DateTime<Utc>>,
    /// Minimum maturity to include (E0 = 0; E2 = 2 = "evidence-supported").
    pub min_maturity: u8,
    /// If true, the policy gate must clear every contributing context for
    /// anonymous read; otherwise the manifest is marked `releasable=false`
    /// and `build_release` returns an error.
    pub require_public: bool,
    pub citation: Citation,
    /// Source manifest — opaque IRIs the caller wants pinned in the
    /// manifest (e.g. document revision IRIs). Sorted before hashing.
    pub source_versions: Vec<String>,
    /// Transformation manifest — opaque IRIs (e.g. extraction-run IRIs).
    /// Sorted before hashing.
    pub transformations: Vec<String>,
    /// Optional pre-computed loss notes from adapter runs that
    /// contributed to this release. Each entry: format → human note.
    /// Folded into the manifest's LossReport.note + per-adapter counts.
    #[serde(default)]
    pub adapter_losses: BTreeMap<String, String>,
    /// If true, the builder queries `donto_document` for the
    /// release's contributing contexts and tries to auto-populate
    /// missing Citation fields (authors from creators, year from
    /// source_date, etc.). Anything the caller supplied via
    /// `citation` takes precedence.
    #[serde(default)]
    pub auto_citation: bool,
}

impl ReleaseSpec {
    pub fn new(release_id: impl Into<String>) -> Self {
        Self {
            release_id: release_id.into(),
            query_specs: vec![],
            contexts: vec![],
            as_of: None,
            min_maturity: 0,
            require_public: false,
            citation: Citation::default(),
            source_versions: vec![],
            transformations: vec![],
            adapter_losses: BTreeMap::new(),
            auto_citation: false,
        }
    }
}

pub async fn build_release(
    client: &DontoClient,
    spec: &ReleaseSpec,
) -> Result<ReleaseManifest, ReleaseError> {
    if spec.contexts.is_empty() {
        return Err(ReleaseError::InvalidSpec(
            "ReleaseSpec.contexts must list at least one context — \
             releases are always scoped"
                .into(),
        ));
    }
    let scope = ContextScope::any_of(spec.contexts.clone());

    let stmts: Vec<Statement> = client
        .match_pattern(
            None,
            None,
            None,
            Some(&scope),
            None,
            spec.min_maturity,
            spec.as_of,
            None,
        )
        .await?;

    let mut checksums: Vec<StatementChecksum> = stmts
        .iter()
        .map(|s| StatementChecksum {
            statement_id: s.statement_id.to_string(),
            sha256: hash_statement(s),
        })
        .collect();
    checksums.sort();

    let contributing_contexts: BTreeSet<String> =
        stmts.iter().map(|s| s.context.clone()).collect();

    let policy_report =
        evaluate_policy(client, &contributing_contexts, spec.require_public).await?;

    if spec.require_public && !policy_report.releasable {
        return Err(ReleaseError::PolicyRefused(policy_report.note));
    }

    let mut sources = spec.source_versions.clone();
    sources.sort();
    let mut transforms = spec.transformations.clone();
    transforms.sort();

    // Build the loss report from any caller-supplied adapter losses.
    let mut loss_report = LossReport::default();
    if !spec.adapter_losses.is_empty() {
        let mut notes = Vec::with_capacity(spec.adapter_losses.len());
        for (adapter, note) in &spec.adapter_losses {
            loss_report
                .adapter_versions
                .insert(adapter.clone(), "v1".into());
            notes.push(format!("[{adapter}] {note}"));
            // Best-effort row-count parsing: if the note contains
            // a leading integer (e.g. "12 rows dropped: …"), accumulate.
            if let Some(num) = note.split_whitespace().next().and_then(|t| t.parse::<u64>().ok()) {
                loss_report.dropped_rows += num;
            }
        }
        notes.sort();
        loss_report.note = notes.join("; ");
    }

    // Optionally auto-populate Citation gaps from contributing
    // documents. Only fills fields the caller left empty.
    let citation = if spec.auto_citation {
        derive_citation_from_documents(client, &contributing_contexts, spec.citation.clone()).await?
    } else {
        spec.citation.clone()
    };

    let mut manifest = ReleaseManifest {
        release_id: spec.release_id.clone(),
        // created_at is *not* part of the canonical hash — see
        // `canonical_bytes`. We freeze it to the unix epoch in the hashed
        // form by skipping it; here we record real wall-clock time for
        // human consumption.
        created_at: Utc::now(),
        query_specs: spec.query_specs.clone(),
        source_versions: sources,
        transformations: transforms,
        statement_checksums: checksums,
        policy_report,
        loss_report,
        citation,
        manifest_sha256: String::new(),
    };

    let canonical = manifest.canonical_bytes()?;
    let mut h = Sha256::new();
    h.update(&canonical);
    manifest.manifest_sha256 = hex::encode(h.finalize());
    Ok(manifest)
}

/// Auto-populate gaps in a [`Citation`] from the contributing
/// documents. Reads `donto_document.creators` (JSONB array of
/// strings or `{name: …}` objects) and `source_date` (JSONB,
/// expected to contain `year` or an RFC-3339 timestamp) to fill
/// the manifest's `authors`/`year` if the caller left them empty.
/// Non-empty caller fields are never overwritten.
async fn derive_citation_from_documents(
    client: &DontoClient,
    contexts: &BTreeSet<String>,
    base: Citation,
) -> Result<Citation, ReleaseError> {
    let mut citation = base;
    if !citation.authors.is_empty() && citation.year.is_some() {
        return Ok(citation);
    }
    if contexts.is_empty() {
        return Ok(citation);
    }
    let ctx_vec: Vec<String> = contexts.iter().cloned().collect();
    let conn = client
        .pool()
        .get()
        .await
        .map_err(|e| ReleaseError::Client(donto_client::Error::Pool(e)))?;
    // donto_document.iri matches donto_statement.context for source-kind contexts;
    // when they don't match (most non-source contexts), this just returns 0 rows.
    let rows = conn
        .query(
            "select creators, source_date \
             from donto_document \
             where iri = any($1::text[])",
            &[&ctx_vec],
        )
        .await
        .map_err(|e| ReleaseError::Client(donto_client::Error::Postgres(e)))?;

    let mut all_authors: Vec<String> = Vec::new();
    let mut min_year: Option<i32> = None;
    for r in rows {
        let creators: serde_json::Value = r.try_get(0).unwrap_or(serde_json::json!([]));
        let source_date: Option<serde_json::Value> = r.try_get(1).ok();
        if let serde_json::Value::Array(items) = creators {
            for item in items {
                let name = match item {
                    serde_json::Value::String(s) => Some(s),
                    serde_json::Value::Object(o) => o
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    _ => None,
                };
                if let Some(n) = name {
                    if !n.is_empty() && !all_authors.contains(&n) {
                        all_authors.push(n);
                    }
                }
            }
        }
        if let Some(sd) = source_date {
            // Accept either {"year": 2026} or a ISO-8601 string.
            let year = sd
                .get("year")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .or_else(|| {
                    sd.as_str()
                        .and_then(|s| s.get(0..4))
                        .and_then(|s| s.parse::<i32>().ok())
                });
            if let Some(y) = year {
                min_year = Some(min_year.map_or(y, |m| m.min(y)));
            }
        }
    }

    if citation.authors.is_empty() && !all_authors.is_empty() {
        all_authors.sort();
        citation.authors = all_authors;
    }
    if citation.year.is_none() {
        citation.year = min_year;
    }
    Ok(citation)
}

/// Canonical hash of a statement. Encodes the fields that define
/// "this fact at this transaction time": id, s/p/o, context, polarity,
/// maturity, valid range, tx_lo. tx_hi is intentionally excluded so a
/// release built before vs after a retraction can be detected by the
/// statement set, not by mutation of an existing line.
fn hash_statement(s: &Statement) -> String {
    let mut h = Sha256::new();
    h.update(s.statement_id.to_string().as_bytes());
    h.update(b"\x1f");
    h.update(s.subject.as_bytes());
    h.update(b"\x1f");
    h.update(s.predicate.as_bytes());
    h.update(b"\x1f");
    match &s.object {
        Object::Iri(i) => {
            h.update(b"iri:");
            h.update(i.as_bytes());
        }
        Object::Literal(l) => {
            h.update(b"lit:");
            h.update(l.v.to_string().as_bytes());
            h.update(b"^^");
            h.update(l.dt.as_bytes());
            h.update(b"@");
            h.update(l.lang.as_deref().unwrap_or("").as_bytes());
        }
    }
    h.update(b"\x1f");
    h.update(s.context.as_bytes());
    h.update(b"\x1f");
    h.update(s.polarity.as_str().as_bytes());
    h.update(b"\x1f");
    h.update([s.maturity]);
    h.update(b"\x1f");
    h.update(
        s.valid_lo
            .map(|d| d.to_string())
            .unwrap_or_default()
            .as_bytes(),
    );
    h.update(b"\x1f");
    h.update(
        s.valid_hi
            .map(|d| d.to_string())
            .unwrap_or_default()
            .as_bytes(),
    );
    h.update(b"\x1f");
    h.update(s.tx_lo.to_rfc3339().as_bytes());
    hex::encode(h.finalize())
}
