//! Release builder skeleton (M7).
//!
//! A donto release is a citable, reproducible bundle of statements
//! materialised from one or more DontoQL queries. The release carries:
//!
//! * the **query spec** (the DontoQL queries that produced it),
//! * **source versions** (every revision the release leans on),
//! * a **transformation manifest** (extraction runs / shape reports / …),
//! * **statement checksums** (deterministic per-statement hashes),
//! * a **policy report** (whether each scope is releasable + which
//!   policies cleared it),
//! * a **loss report** (round-trip fidelity per adapter),
//! * **citation metadata** (DOI-shaped fields).
//!
//! The skeleton in this crate covers the in-process pipeline:
//!
//! 1. [`builder::ReleaseSpec`] declares what to bundle.
//! 2. [`builder::build_release`] resolves the queries via
//!    `donto-query::evaluate`, computes per-statement checksums, runs
//!    the policy gate, and produces a [`manifest::ReleaseManifest`].
//! 3. [`export::write_native_jsonl`] writes a deterministic JSONL of
//!    the manifest.
//!
//! Out of scope for the skeleton (kept as TODOs in the export module):
//! RO-Crate exporter and CLDF exporter — both consume the same
//! [`manifest::ReleaseManifest`] when implemented.

pub mod builder;
pub mod export;
pub mod manifest;
pub mod policy;

pub use builder::{build_release, ReleaseSpec};
pub use export::write_native_jsonl;
pub use manifest::{Citation, LossReport, PolicyReport, ReleaseManifest, StatementChecksum};

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("query error: {0}")]
    Query(#[from] donto_query::EvalError),
    #[error("client error: {0}")]
    Client(#[from] donto_client::Error),
    #[error("dontoql parse error: {0}")]
    Parse(String),
    #[error("policy gate refused release: {0}")]
    PolicyRefused(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialisation error: {0}")]
    Serde(#[from] serde_json::Error),
}
