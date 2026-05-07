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
        }
    }
}

pub async fn build_release(
    client: &DontoClient,
    spec: &ReleaseSpec,
) -> Result<ReleaseManifest, ReleaseError> {
    let scope = if spec.contexts.is_empty() {
        None
    } else {
        Some(ContextScope::any_of(spec.contexts.clone()))
    };

    let stmts: Vec<Statement> = client
        .match_pattern(
            None,
            None,
            None,
            scope.as_ref(),
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
        loss_report: LossReport::default(),
        citation: spec.citation.clone(),
        manifest_sha256: String::new(),
    };

    let canonical = manifest.canonical_bytes()?;
    let mut h = Sha256::new();
    h.update(&canonical);
    manifest.manifest_sha256 = hex::encode(h.finalize());
    Ok(manifest)
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
    h.update(format!("{:?}", s.polarity).as_bytes());
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
