//! CLDF table parser. Reads each TSV (or CSV) referenced by the
//! metadata and projects it into the four canonical row shapes the
//! importer cares about.

use crate::metadata::{CldfMetadata, TableKind};
use crate::ImportError;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Language {
    pub id: String,
    pub iri: String,
    pub name: Option<String>,
    pub glottocode: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub id: String,
    pub iri: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Code {
    pub id: String,
    pub iri: String,
    pub parameter_id: Option<String>,
    pub parameter_iri: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Value {
    pub id: String,
    pub language_id: String,
    pub parameter_id: String,
    pub value: Option<String>,
}

#[derive(Debug, Default)]
pub struct ParsedTables {
    pub languages: Vec<Language>,
    pub parameters: Vec<Parameter>,
    pub codes: Vec<Code>,
    pub values: Vec<Value>,
    /// Lookup: CLDF ID → donto IRI for each entity kind.
    pub lang_index: HashMap<String, String>,
    pub param_index: HashMap<String, String>,
    pub code_index: HashMap<String, String>,
}

impl ParsedTables {
    pub fn language_iri(&self, id: &str) -> Option<String> {
        self.lang_index.get(id).cloned()
    }
    pub fn parameter_iri(&self, id: &str) -> Option<String> {
        self.param_index.get(id).cloned()
    }
    pub fn code_iri(&self, id: &str) -> Option<String> {
        self.code_index.get(id).cloned()
    }
}

pub fn parse_all(dir: &Path, meta: &CldfMetadata) -> Result<ParsedTables, ImportError> {
    let mut out = ParsedTables::default();
    // Prefix derived from the dataset IRI (or fallback to `cldf:`).
    let dataset_prefix = meta
        .dataset_iri()
        .map(|i| {
            // Normalise — drop trailing slash, ensure it ends with `/`.
            let i = i.trim_end_matches('/');
            format!("{i}/")
        })
        .unwrap_or_else(|| "cldf:".into());

    for table in &meta.tables {
        let kind = table.kind();
        let url = match &table.url {
            Some(u) => u,
            None => continue,
        };
        let path = dir.join(url);
        if !path.exists() {
            continue;
        }
        match kind {
            TableKind::Language => {
                for row in read_rows(&path)? {
                    let id = row.get("ID").map(String::as_str).unwrap_or("").to_string();
                    if id.is_empty() {
                        continue;
                    }
                    let iri = format!("{dataset_prefix}lang/{id}");
                    out.lang_index.insert(id.clone(), iri.clone());
                    out.languages.push(Language {
                        id,
                        iri,
                        name: row.get("Name").cloned(),
                        glottocode: row.get("Glottocode").cloned(),
                    });
                }
            }
            TableKind::Parameter => {
                for row in read_rows(&path)? {
                    let id = row.get("ID").cloned().unwrap_or_default();
                    if id.is_empty() {
                        continue;
                    }
                    let iri = format!("{dataset_prefix}param/{id}");
                    out.param_index.insert(id.clone(), iri.clone());
                    out.parameters.push(Parameter {
                        id,
                        iri,
                        name: row.get("Name").cloned(),
                        description: row.get("Description").cloned(),
                    });
                }
            }
            TableKind::Code => {
                for row in read_rows(&path)? {
                    let id = row.get("ID").cloned().unwrap_or_default();
                    if id.is_empty() {
                        continue;
                    }
                    let iri = format!("{dataset_prefix}code/{id}");
                    out.code_index.insert(id.clone(), iri.clone());
                    let parameter_id = row.get("Parameter_ID").cloned();
                    let parameter_iri = parameter_id
                        .as_deref()
                        .and_then(|p| out.param_index.get(p).cloned());
                    out.codes.push(Code {
                        id,
                        iri,
                        parameter_id,
                        parameter_iri,
                        name: row.get("Name").cloned(),
                    });
                }
            }
            TableKind::Value => {
                for row in read_rows(&path)? {
                    let id = row.get("ID").cloned().unwrap_or_default();
                    if id.is_empty() {
                        continue;
                    }
                    let language_id = row.get("Language_ID").cloned().unwrap_or_default();
                    let parameter_id = row.get("Parameter_ID").cloned().unwrap_or_default();
                    let value = row.get("Value").cloned();
                    out.values.push(Value {
                        id,
                        language_id,
                        parameter_id,
                        value,
                    });
                }
            }
            // Other / Unknown — recorded as a loss in lib.rs after
            // the four canonical tables run.
            _ => {}
        }
    }
    Ok(out)
}

/// Read a CLDF TSV/CSV into row maps. CLDF v1.0 default is
/// TSV-with-RFC4180-escape; CSV is also legal. We auto-detect by
/// extension.
fn read_rows(path: &Path) -> Result<Vec<HashMap<String, String>>, ImportError> {
    let delim = if path.extension().and_then(|s| s.to_str()) == Some("tsv") {
        b'\t'
    } else {
        b','
    };
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .has_headers(true)
        .from_path(path)?;
    let headers = rdr.headers()?.clone();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        let row: HashMap<String, String> = headers
            .iter()
            .zip(rec.iter())
            .map(|(h, v)| (h.to_string(), v.to_string()))
            .collect();
        out.push(row);
    }
    Ok(out)
}
