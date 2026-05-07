//! The release manifest data model.
//!
//! Everything that goes into the manifest serialises to JSON in a
//! stable, key-sorted form so the resulting `manifest_sha256` is
//! reproducible: re-running an unchanged release on the same data
//! produces an identical hash.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One per-statement checksum line. Kept tiny on purpose: the goal is
/// to detect divergence between two manifests, not to re-derive the
/// statement.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct StatementChecksum {
    /// Statement UUID (stringified for stable JSON ordering).
    pub statement_id: String,
    /// SHA-256 over the canonical statement encoding.
    pub sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyReport {
    /// Whether the release is releasable as a whole.
    pub releasable: bool,
    /// Per-context decisions: context IRI → cleared|blocked + reason.
    pub decisions: BTreeMap<String, PolicyDecision>,
    /// Free-form note from the policy gate (e.g. which policies were checked).
    pub note: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub cleared: bool,
    pub policy_iri: Option<String>,
    pub reason: String,
}

/// Loss report aggregated from adapters consumed during the build.
/// Skeleton-shape only — adapters wire their own loss fields in M5/M6.
#[derive(Debug, Clone, Eq, PartialEq, Default, Serialize, Deserialize)]
pub struct LossReport {
    pub adapter_versions: BTreeMap<String, String>,
    pub dropped_predicates: Vec<String>,
    pub dropped_rows: u64,
    pub note: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Default, Serialize, Deserialize)]
pub struct Citation {
    pub title: String,
    pub authors: Vec<String>,
    pub doi: Option<String>,
    pub publisher: Option<String>,
    pub license: Option<String>,
    pub version: Option<String>,
    pub year: Option<i32>,
}

/// The full release manifest. JSON-stable so two builds over the same
/// underlying data produce byte-identical bytes (and therefore hashes).
///
/// `manifest_sha256` is computed over the JSON of the manifest with
/// `manifest_sha256` set to the empty string — see
/// [`ReleaseManifest::canonical_bytes`]. That keeps the field in the
/// final document while leaving the hash itself reproducible.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub release_id: String,
    pub created_at: DateTime<Utc>,
    pub query_specs: Vec<String>,
    pub source_versions: Vec<String>,
    pub transformations: Vec<String>,
    pub statement_checksums: Vec<StatementChecksum>,
    pub policy_report: PolicyReport,
    pub loss_report: LossReport,
    pub citation: Citation,
    pub manifest_sha256: String,
}

impl ReleaseManifest {
    /// Canonical bytes used to hash and to write to disk: pretty-printed
    /// JSON with `manifest_sha256` blanked, `created_at` pinned to the
    /// unix epoch, and statement_checksums pre-sorted. Wall-clock time
    /// is preserved on the in-memory struct so consumers can read it,
    /// but excluded from the hash so re-runs over the same data
    /// reproduce.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut copy = self.clone();
        copy.statement_checksums.sort();
        copy.manifest_sha256 = String::new();
        copy.created_at = DateTime::<Utc>::from_timestamp(0, 0).expect("epoch is valid");
        serde_json::to_vec_pretty(&copy)
    }
}
