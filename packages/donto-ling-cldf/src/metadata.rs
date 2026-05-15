//! CLDF metadata loader. A CLDF dataset directory has a
//! `<something>-metadata.json` file at its root describing the
//! tables. We pick the first JSON-LD file matching `*metadata.json`.

use crate::ImportError;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// What kind of CLDF object a table holds. We support the four
/// canonical kinds end-to-end; everything else lands in the loss
/// report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    Language,
    Parameter,
    Code,
    Value,
    /// ExampleTable, BorrowingTable, BorrowingTable, CognateTable,
    /// MediaTable, FormTable etc. — recognised but not represented.
    Other(&'static str),
    Unknown,
}

impl TableKind {
    fn from_dc_type(dc_type: &str) -> Self {
        // CLDF metadata uses `dc:conformsTo` URLs like
        // "http://cldf.clld.org/v1.0/terms.rdf#LanguageTable".
        if let Some(suffix) = dc_type.rsplit('#').next() {
            match suffix {
                "LanguageTable" => TableKind::Language,
                "ParameterTable" => TableKind::Parameter,
                "CodeTable" => TableKind::Code,
                "ValueTable" => TableKind::Value,
                "ExampleTable" => TableKind::Other("ExampleTable"),
                "BorrowingTable" => TableKind::Other("BorrowingTable"),
                "CognateTable" => TableKind::Other("CognateTable"),
                "MediaTable" => TableKind::Other("MediaTable"),
                "FormTable" => TableKind::Other("FormTable"),
                _ => TableKind::Unknown,
            }
        } else {
            TableKind::Unknown
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CldfTable {
    pub url: Option<String>,
    #[serde(rename = "dc:conformsTo", default)]
    pub conforms_to: Option<String>,
}

impl CldfTable {
    pub fn kind(&self) -> TableKind {
        match self.conforms_to.as_deref() {
            Some(t) => TableKind::from_dc_type(t),
            None => TableKind::Unknown,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CldfMetadata {
    #[serde(rename = "@context", default)]
    pub context: serde_json::Value,
    #[serde(rename = "dc:identifier", default)]
    pub dc_identifier: Option<String>,
    #[serde(rename = "dc:title", default)]
    pub dc_title: Option<String>,
    #[serde(default)]
    pub tables: Vec<CldfTable>,
}

impl CldfMetadata {
    pub fn dataset_iri(&self) -> Option<String> {
        self.dc_identifier
            .clone()
            .or_else(|| self.dc_title.as_ref().map(|t| format!("cldf:{t}")))
    }
}

pub fn load(dir: impl AsRef<Path>) -> Result<CldfMetadata, ImportError> {
    let dir = dir.as_ref();
    // Prefer `*-metadata.json` at the root. CLDF v1.0 dataset
    // conventions: <Name>-metadata.json (e.g. wals-metadata.json,
    // StructureDataset-metadata.json).
    let mut chosen: Option<std::path::PathBuf> = None;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        let name = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if name.ends_with("-metadata.json") {
            chosen = Some(p);
            break;
        }
    }
    let path = chosen.ok_or_else(|| {
        ImportError::Metadata(format!(
            "no `*-metadata.json` found in {}",
            dir.display()
        ))
    })?;
    let body = fs::read_to_string(&path)?;
    let meta: CldfMetadata = serde_json::from_str(&body)
        .map_err(|e| ImportError::Metadata(format!("parsing {}: {e}", path.display())))?;
    Ok(meta)
}
