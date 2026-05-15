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

/// Write a minimal RO-Crate (`ro-crate-metadata.json` per
/// <https://www.researchobject.org/ro-crate/1.1/>) describing the
/// release. Three nodes by default plus any extras the caller passes:
///
///   * the implicit `./` root dataset, typed `Dataset`, carrying
///     citation metadata;
///   * `manifest.jsonl`, typed `File`;
///   * `ro-crate-metadata.json` itself (the metadata file descriptor
///     required by the RO-Crate spec).
///
/// Deterministic — re-running with the same manifest produces
/// byte-identical metadata.
pub fn write_ro_crate_metadata(
    manifest: &ReleaseManifest,
    crate_dir: &Path,
    extra_files: &[(&str, &str)],
) -> Result<(), crate::ReleaseError> {
    use serde_json::json;

    let title = if manifest.citation.title.is_empty() {
        manifest.release_id.clone()
    } else {
        manifest.citation.title.clone()
    };

    let mut graph = Vec::new();

    graph.push(json!({
        "@id": "ro-crate-metadata.json",
        "@type": "CreativeWork",
        "conformsTo": { "@id": "https://w3id.org/ro/crate/1.1" },
        "about": { "@id": "./" },
    }));

    let mut root = json!({
        "@id": "./",
        "@type": "Dataset",
        "name": title,
        "datePublished": "1970-01-01T00:00:00Z",
        "identifier": manifest.release_id,
        "hasPart": [],
    });
    let mut parts: Vec<serde_json::Value> = Vec::new();
    parts.push(json!({"@id": "manifest.jsonl"}));
    for (name, _) in extra_files {
        parts.push(json!({"@id": name}));
    }
    root["hasPart"] = serde_json::Value::Array(parts);
    if !manifest.citation.authors.is_empty() {
        let authors: Vec<serde_json::Value> = manifest
            .citation
            .authors
            .iter()
            .map(|a| json!({"@type": "Person", "name": a}))
            .collect();
        root["author"] = serde_json::Value::Array(authors);
    }
    if let Some(license) = &manifest.citation.license {
        root["license"] = serde_json::Value::String(license.clone());
    }
    if let Some(doi) = &manifest.citation.doi {
        root["identifier"] = serde_json::Value::String(doi.clone());
    }
    if let Some(publisher) = &manifest.citation.publisher {
        root["publisher"] = json!({"@type": "Organization", "name": publisher});
    }
    if let Some(version) = &manifest.citation.version {
        root["version"] = serde_json::Value::String(version.clone());
    }
    graph.push(root);

    graph.push(json!({
        "@id": "manifest.jsonl",
        "@type": "File",
        "name": "Release manifest (donto native JSONL)",
        "encodingFormat": "application/x-ndjson",
        "sha256": manifest.manifest_sha256,
    }));

    for (name, encoding) in extra_files {
        graph.push(json!({
            "@id": name,
            "@type": "File",
            "encodingFormat": encoding,
        }));
    }

    let doc = json!({
        "@context": "https://w3id.org/ro/crate/1.1/context",
        "@graph": graph,
    });

    std::fs::create_dir_all(crate_dir)?;
    let metadata_path = crate_dir.join("ro-crate-metadata.json");
    let mut f = std::fs::File::create(&metadata_path)?;
    let bytes = sorted_pretty(&doc)?;
    f.write_all(&bytes)?;
    Ok(())
}

/// Pretty-print with object keys sorted lexicographically. Helper
/// for [`write_ro_crate_metadata`].
fn sorted_pretty(v: &serde_json::Value) -> Result<Vec<u8>, serde_json::Error> {
    fn sort(v: &serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys {
                    out.insert(k.clone(), sort(&map[k]));
                }
                serde_json::Value::Object(out)
            }
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(sort).collect())
            }
            other => other.clone(),
        }
    }
    serde_json::to_vec_pretty(&sort(v))
}

// TODO(M7+): CLDF exporter (NFR-005).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{
        Citation, LossReport, PolicyDecision, PolicyReport, ReleaseManifest, StatementChecksum,
    };
    use chrono::DateTime;
    use std::collections::BTreeMap;

    fn fixture_manifest() -> ReleaseManifest {
        let mut decisions = BTreeMap::new();
        decisions.insert(
            "ctx:demo".into(),
            PolicyDecision {
                cleared: true,
                policy_iri: Some("policy:default/public".into()),
                reason: "test".into(),
            },
        );
        ReleaseManifest {
            release_id: "test/ro-crate-skeleton".into(),
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            query_specs: vec!["MATCH ?s ?p ?o LIMIT 1".into()],
            source_versions: vec![],
            transformations: vec![],
            statement_checksums: vec![StatementChecksum {
                statement_id: "00000000-0000-0000-0000-000000000001".into(),
                sha256: "aa".into(),
            }],
            policy_report: PolicyReport {
                releasable: true,
                decisions,
                note: "fixture".into(),
            },
            loss_report: LossReport {
                adapter_versions: BTreeMap::new(),
                dropped_predicates: vec![],
                dropped_rows: 0,
                note: "no losses".into(),
            },
            citation: Citation {
                title: "Test Release".into(),
                authors: vec!["Alice".into(), "Bob".into()],
                doi: Some("10.5555/test".into()),
                publisher: Some("Test Publisher".into()),
                license: Some("CC-BY-4.0".into()),
                version: Some("0.1.0".into()),
                year: Some(2026),
            },
            manifest_sha256: "deadbeef".into(),
        }
    }

    #[test]
    fn writes_ro_crate_metadata_json() {
        let m = fixture_manifest();
        let tmp = tempfile::tempdir().unwrap();
        write_ro_crate_metadata(&m, tmp.path(), &[]).unwrap();
        let body = std::fs::read_to_string(tmp.path().join("ro-crate-metadata.json")).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(doc["@context"], "https://w3id.org/ro/crate/1.1/context");
        let graph = doc["@graph"].as_array().unwrap();
        assert!(graph.iter().any(|n| n["@id"] == "ro-crate-metadata.json"));
        assert!(graph.iter().any(|n| n["@id"] == "./"));
        assert!(graph.iter().any(|n| n["@id"] == "manifest.jsonl"));
    }

    #[test]
    fn deterministic_bytes_across_runs() {
        let m = fixture_manifest();
        let tmp_a = tempfile::tempdir().unwrap();
        let tmp_b = tempfile::tempdir().unwrap();
        write_ro_crate_metadata(&m, tmp_a.path(), &[]).unwrap();
        write_ro_crate_metadata(&m, tmp_b.path(), &[]).unwrap();
        let a = std::fs::read(tmp_a.path().join("ro-crate-metadata.json")).unwrap();
        let b = std::fs::read(tmp_b.path().join("ro-crate-metadata.json")).unwrap();
        assert_eq!(a, b, "two runs over the same manifest must be byte-identical");
    }

    #[test]
    fn extra_files_appear_in_graph_and_root_haspart() {
        let m = fixture_manifest();
        let tmp = tempfile::tempdir().unwrap();
        write_ro_crate_metadata(
            &m,
            tmp.path(),
            &[
                ("envelope.json", "application/json"),
                ("cldf-export.zip", "application/zip"),
            ],
        )
        .unwrap();
        let body = std::fs::read_to_string(tmp.path().join("ro-crate-metadata.json")).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&body).unwrap();
        let graph = doc["@graph"].as_array().unwrap();
        assert!(graph.iter().any(|n| n["@id"] == "envelope.json"));
        assert!(graph.iter().any(|n| n["@id"] == "cldf-export.zip"));
        let root = graph.iter().find(|n| n["@id"] == "./").unwrap();
        let parts: Vec<&serde_json::Value> =
            root["hasPart"].as_array().unwrap().iter().collect();
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn citation_fields_round_trip_into_graph() {
        let m = fixture_manifest();
        let tmp = tempfile::tempdir().unwrap();
        write_ro_crate_metadata(&m, tmp.path(), &[]).unwrap();
        let body = std::fs::read_to_string(tmp.path().join("ro-crate-metadata.json")).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&body).unwrap();
        let root = doc["@graph"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["@id"] == "./")
            .unwrap();
        assert_eq!(root["name"], "Test Release");
        assert_eq!(root["identifier"], "10.5555/test");
        assert_eq!(root["license"], "CC-BY-4.0");
        assert_eq!(root["version"], "0.1.0");
        let authors = root["author"].as_array().unwrap();
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0]["name"], "Alice");
    }
}
