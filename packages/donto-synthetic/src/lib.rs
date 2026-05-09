//! donto-synthetic library — exposes generator and rng modules for integration tests.

pub mod generator;
pub mod rng;

/// Absolute path to the `anomalies.json` sidecar file written by the generator.
///
/// The path is resolved at compile time relative to this crate's `Cargo.toml`,
/// so it is stable even when called from another crate's test binary (e.g.
/// `packages/donto-analytics/tests/test_rule_duration.rs`). The returned
/// `PathBuf` always points to `<crate-root>/anomalies.json` regardless of the
/// process's working directory at runtime.
pub fn anomalies_json_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("anomalies.json")
}
