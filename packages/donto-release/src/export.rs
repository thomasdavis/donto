//! Release export formats.
//!
//! Skeleton implements native JSONL only. RO-Crate and CLDF exporters
//! consume the same [`crate::ReleaseManifest`] when wired in (M7+).

use crate::ReleaseManifest;
use donto_client::{Object, Statement};
use std::collections::{BTreeMap, BTreeSet};
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

/// Write a CLDF directory (the inverse of `donto-ling-cldf`'s
/// importer) from a [`ReleaseManifest`] plus the statements it
/// describes. Produces:
///
///   <crate_dir>/
///     <release_id>-metadata.json   # JSON-LD descriptor
///     languages.csv                # LanguageTable
///     parameters.csv               # ParameterTable
///     codes.csv                    # CodeTable
///     values.csv                   # ValueTable
///
/// The exporter assumes the statements follow the
/// donto-ling-cldf import convention:
///   - Subjects with `rdf:type ling:Language` go to LanguageTable.
///   - Subjects with `rdf:type ling:Code` go to CodeTable
///     (with a `ling:codeFor` edge pointing to the parameter).
///   - Predicates that appear on language→object edges (other
///     than `rdf:type`, `ling:name`, `ling:glottocode`) are
///     ParameterTable rows.
///   - Those edges themselves become ValueTable rows.
///
/// Statements that don't fit any of the above are recorded
/// in the returned `lossy_count` so the caller can decide
/// whether to abort the export.
pub fn write_cldf_release(
    manifest: &ReleaseManifest,
    statements: &[Statement],
    crate_dir: &Path,
) -> Result<CldfExportSummary, crate::ReleaseError> {
    std::fs::create_dir_all(crate_dir)?;

    // 1. Pass: classify statements.
    let mut languages: BTreeMap<String, LangRow> = BTreeMap::new();
    let mut codes: BTreeMap<String, CodeRow> = BTreeMap::new();
    let mut value_stmts: Vec<&Statement> = Vec::new();
    for st in statements {
        match (st.predicate.as_str(), &st.object) {
            ("rdf:type", Object::Iri(o)) if o == "ling:Language" => {
                languages
                    .entry(st.subject.clone())
                    .or_insert_with(|| LangRow::new(&st.subject));
            }
            ("rdf:type", Object::Iri(o)) if o == "ling:Code" => {
                codes
                    .entry(st.subject.clone())
                    .or_insert_with(|| CodeRow::new(&st.subject));
            }
            _ => {}
        }
    }
    // 2. Pass: fold language/code metadata.
    for st in statements {
        match (st.predicate.as_str(), &st.object) {
            ("ling:name", Object::Literal(l)) => {
                if let Some(lang) = languages.get_mut(&st.subject) {
                    lang.name = literal_string(l);
                } else if let Some(code) = codes.get_mut(&st.subject) {
                    code.name = literal_string(l);
                }
            }
            ("ling:glottocode", Object::Literal(l)) => {
                if let Some(lang) = languages.get_mut(&st.subject) {
                    lang.glottocode = literal_string(l);
                }
            }
            ("ling:codeFor", Object::Iri(p)) => {
                if let Some(code) = codes.get_mut(&st.subject) {
                    code.parameter_id = Some(local_part(p));
                }
            }
            _ => {}
        }
    }
    // 3. Pass: collect parameter IDs and value rows.
    let mut parameters: BTreeSet<String> = BTreeSet::new();
    let mut lossy_count: u64 = 0;
    for st in statements {
        // Skip the type/meta predicates we already consumed.
        if matches!(
            st.predicate.as_str(),
            "rdf:type" | "ling:name" | "ling:glottocode" | "ling:codeFor"
        ) {
            continue;
        }
        // Subject must be a Language.
        if !languages.contains_key(&st.subject) {
            lossy_count += 1;
            continue;
        }
        parameters.insert(st.predicate.clone());
        value_stmts.push(st);
    }

    // 4. Write the four TSV/CSV tables. We use CSV (RFC 4180) for
    // maximum tooling compatibility — donto-ling-cldf accepts both.
    let lang_path = crate_dir.join("languages.csv");
    write_csv(
        &lang_path,
        &["ID", "Name", "Glottocode"],
        languages.values().map(|l| {
            vec![
                local_part(&l.iri),
                l.name.clone().unwrap_or_default(),
                l.glottocode.clone().unwrap_or_default(),
            ]
        }),
    )?;

    let param_path = crate_dir.join("parameters.csv");
    write_csv(
        &param_path,
        &["ID", "Name", "Description"],
        parameters.iter().map(|p| {
            vec![local_part(p), local_part(p), String::new()]
        }),
    )?;

    let code_path = crate_dir.join("codes.csv");
    write_csv(
        &code_path,
        &["ID", "Parameter_ID", "Name"],
        codes.values().map(|c| {
            vec![
                local_part(&c.iri),
                c.parameter_id.clone().unwrap_or_default(),
                c.name.clone().unwrap_or_default(),
            ]
        }),
    )?;

    let value_path = crate_dir.join("values.csv");
    let mut value_rows: Vec<Vec<String>> = Vec::with_capacity(value_stmts.len());
    for (i, st) in value_stmts.iter().enumerate() {
        let value_str = match &st.object {
            Object::Iri(i2) => local_part(i2),
            Object::Literal(l) => literal_string(l).unwrap_or_default(),
        };
        value_rows.push(vec![
            format!("v{i}"),
            local_part(&st.subject),
            local_part(&st.predicate),
            value_str,
        ]);
    }
    write_csv(
        &value_path,
        &["ID", "Language_ID", "Parameter_ID", "Value"],
        value_rows.into_iter(),
    )?;

    // 5. Metadata file.
    let metadata_iri = format!("cldf:{}", manifest.release_id);
    let meta = serde_json::json!({
        "@context": ["http://www.w3.org/ns/csvw", {"@language": "en"}],
        "dc:identifier": metadata_iri,
        "dc:title": if manifest.citation.title.is_empty() {
            manifest.release_id.clone()
        } else {
            manifest.citation.title.clone()
        },
        "tables": [
            {"url": "languages.csv",
             "dc:conformsTo": "http://cldf.clld.org/v1.0/terms.rdf#LanguageTable"},
            {"url": "parameters.csv",
             "dc:conformsTo": "http://cldf.clld.org/v1.0/terms.rdf#ParameterTable"},
            {"url": "codes.csv",
             "dc:conformsTo": "http://cldf.clld.org/v1.0/terms.rdf#CodeTable"},
            {"url": "values.csv",
             "dc:conformsTo": "http://cldf.clld.org/v1.0/terms.rdf#ValueTable"}
        ]
    });
    let meta_path = crate_dir.join(format!("{}-metadata.json", safe_basename(&manifest.release_id)));
    std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?)?;

    Ok(CldfExportSummary {
        languages: languages.len() as u64,
        parameters: parameters.len() as u64,
        codes: codes.len() as u64,
        values: value_stmts.len() as u64,
        lossy_count,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CldfExportSummary {
    pub languages: u64,
    pub parameters: u64,
    pub codes: u64,
    pub values: u64,
    /// Statements that didn't fit any of the four canonical CLDF
    /// roles. The export does not refuse — the caller decides.
    pub lossy_count: u64,
}

#[derive(Debug, Clone)]
struct LangRow {
    iri: String,
    name: Option<String>,
    glottocode: Option<String>,
}
impl LangRow {
    fn new(iri: &str) -> Self {
        Self {
            iri: iri.to_string(),
            name: None,
            glottocode: None,
        }
    }
}

#[derive(Debug, Clone)]
struct CodeRow {
    iri: String,
    parameter_id: Option<String>,
    name: Option<String>,
}
impl CodeRow {
    fn new(iri: &str) -> Self {
        Self {
            iri: iri.to_string(),
            parameter_id: None,
            name: None,
        }
    }
}

fn write_csv<I>(
    path: &Path,
    header: &[&str],
    rows: I,
) -> Result<(), crate::ReleaseError>
where
    I: IntoIterator<Item = Vec<String>>,
{
    let mut wtr = csv::Writer::from_path(path).map_err(|e| {
        crate::ReleaseError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    })?;
    wtr.write_record(header).map_err(|e| {
        crate::ReleaseError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    })?;
    for row in rows {
        wtr.write_record(&row).map_err(|e| {
            crate::ReleaseError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        })?;
    }
    wtr.flush().map_err(crate::ReleaseError::Io)?;
    Ok(())
}

fn local_part(iri: &str) -> String {
    iri.rsplit('/').next().unwrap_or(iri).to_string()
}

fn literal_string(l: &donto_client::Literal) -> Option<String> {
    match &l.v {
        serde_json::Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn safe_basename(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

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
