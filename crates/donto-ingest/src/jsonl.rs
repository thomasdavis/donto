//! JSONL streaming. Each line is one of:
//!
//! - `{"s":"...","p":"...","o":{"iri":"..."},"c":"...","pol":"asserted"}`
//! - `{"s":"...","p":"...","o":{"v":..., "dt":"xsd:string", "lang":null},"c":"..."}`
//!
//! Designed for LLM extractor pipelines: writers append; this reader streams.

use anyhow::{anyhow, Result};
use donto_client::{Literal, Object, Polarity, StatementInput};
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Deserialize)]
struct Row {
    s: String,
    p: String,
    o: ObjForm,
    #[serde(default)]
    c: Option<String>,
    #[serde(default)]
    pol: Option<String>,
    #[serde(default)]
    maturity: Option<u8>,
    #[serde(default)]
    valid_lo: Option<chrono::NaiveDate>,
    #[serde(default)]
    valid_hi: Option<chrono::NaiveDate>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ObjForm {
    Iri { iri: String },
    Lit(Literal),
}

pub fn parse_path(path: &Path, default_context: &str) -> Result<Vec<StatementInput>> {
    let f = std::fs::File::open(path)?;
    parse_reader(BufReader::new(f), default_context)
}

pub fn parse_reader<R: BufRead>(reader: R, default_context: &str) -> Result<Vec<StatementInput>> {
    let mut out = Vec::new();
    for (lineno, line) in reader.lines().enumerate() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let row: Row =
            serde_json::from_str(line).map_err(|e| anyhow!("jsonl line {}: {e}", lineno + 1))?;
        let object = match row.o {
            ObjForm::Iri { iri } => Object::iri(iri),
            ObjForm::Lit(l) => Object::lit(l),
        };
        let pol = row
            .pol
            .as_deref()
            .and_then(Polarity::parse)
            .unwrap_or(Polarity::Asserted);
        out.push(
            StatementInput::new(row.s, row.p, object)
                .with_context(row.c.unwrap_or_else(|| default_context.into()))
                .with_polarity(pol)
                .with_maturity(row.maturity.unwrap_or(0))
                .with_valid(row.valid_lo, row.valid_hi),
        );
    }
    Ok(out)
}
