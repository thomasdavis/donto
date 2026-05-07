//! Release export formats.
//!
//! Skeleton implements native JSONL only. RO-Crate and CLDF exporters
//! consume the same [`crate::ReleaseManifest`] when wired in (M7+).

use crate::ReleaseManifest;
use std::io::Write;
use std::path::Path;

/// Write a release manifest as native JSONL: line 1 is the manifest
/// header (without the per-statement checksums to keep it small), and
/// each subsequent line is one [`crate::StatementChecksum`].
///
/// The file is written deterministically — re-running with the same
/// manifest produces byte-identical bytes.
pub fn write_native_jsonl(
    manifest: &ReleaseManifest,
    path: &Path,
) -> Result<(), crate::ReleaseError> {
    let mut f = std::fs::File::create(path)?;

    let mut header = manifest.clone();
    header.statement_checksums = vec![];
    let header_line = serde_json::to_string(&header)?;
    writeln!(f, "{header_line}")?;

    let mut sorted = manifest.statement_checksums.clone();
    sorted.sort();
    for c in &sorted {
        let line = serde_json::to_string(c)?;
        writeln!(f, "{line}")?;
    }
    Ok(())
}

// TODO(M7+): RO-Crate exporter (NFR-005).
// TODO(M7+): CLDF exporter (NFR-005).
