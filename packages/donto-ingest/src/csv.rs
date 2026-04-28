//! CSV with column mapping. The mapping describes:
//!   * which column is the subject IRI (or how to construct it from columns),
//!   * for each remaining column, the predicate IRI and a literal datatype.

use anyhow::{anyhow, Result};
use donto_client::{Literal, Object, Polarity, StatementInput};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CsvMapping {
    pub default_context: String,
    pub subject: SubjectSource,
    pub columns: Vec<ColumnMap>,
    #[serde(default)]
    pub skip_blank: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind")]
pub enum SubjectSource {
    Column {
        column: String,
        prefix: Option<String>,
    },
    Template {
        template: String,
    }, // e.g. "ex:user/{id}"
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColumnMap {
    pub column: String,
    pub predicate: String,
    #[serde(default)]
    pub datatype: Option<String>,
    #[serde(default)]
    pub iri: bool, // when true, treat the cell value as an IRI rather than a literal
    #[serde(default)]
    pub iri_prefix: Option<String>,
}

pub fn parse_path(path: &Path, mapping: &CsvMapping) -> Result<Vec<StatementInput>> {
    let mut rdr = csv::Reader::from_path(path)?;
    let headers = rdr.headers()?.clone();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let r = rec?;
        let mut row: std::collections::HashMap<&str, &str> = headers.iter().zip(r.iter()).collect();
        let subject = match &mapping.subject {
            SubjectSource::Column { column, prefix } => {
                let v = row
                    .get(column.as_str())
                    .ok_or_else(|| anyhow!("subject column `{column}` missing"))?;
                if mapping.skip_blank && v.is_empty() {
                    continue;
                }
                match prefix {
                    Some(p) => format!("{p}{v}"),
                    None => v.to_string(),
                }
            }
            SubjectSource::Template { template } => render_template(template, &row),
        };
        for col in &mapping.columns {
            let val = row.remove(col.column.as_str()).unwrap_or("");
            if mapping.skip_blank && val.is_empty() {
                continue;
            }
            let object = if col.iri {
                Object::iri(match &col.iri_prefix {
                    Some(p) => format!("{p}{val}"),
                    None => val.to_string(),
                })
            } else {
                let dt = col.datatype.clone().unwrap_or_else(|| "xsd:string".into());
                let v = match dt.as_str() {
                    "xsd:integer" => val
                        .parse::<i64>()
                        .map(|n| serde_json::json!(n))
                        .unwrap_or(serde_json::Value::String(val.into())),
                    "xsd:boolean" => match val.to_ascii_lowercase().as_str() {
                        "true" | "1" | "yes" => serde_json::Value::Bool(true),
                        "false" | "0" | "no" => serde_json::Value::Bool(false),
                        _ => serde_json::Value::String(val.into()),
                    },
                    _ => serde_json::Value::String(val.into()),
                };
                Object::lit(Literal { v, dt, lang: None })
            };
            out.push(
                StatementInput::new(&subject, &col.predicate, object)
                    .with_context(&mapping.default_context)
                    .with_polarity(Polarity::Asserted),
            );
        }
    }
    Ok(out)
}

fn render_template(t: &str, row: &std::collections::HashMap<&str, &str>) -> String {
    let mut out = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            for c in chars.by_ref() {
                if c == '}' {
                    break;
                } else {
                    name.push(c);
                }
            }
            out.push_str(row.get(name.as_str()).copied().unwrap_or(""));
        } else {
            out.push(c);
        }
    }
    out
}
