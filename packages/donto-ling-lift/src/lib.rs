//! LIFT (Lexicon Interchange Format) importer.
//!
//! LIFT is an XML format used by SIL FieldWorks and other lexicon
//! editing tools. A LIFT document is a `<lift>` root with `<entry>`
//! children, each of which carries:
//!
//!   - `id` attribute (the lexeme's stable identifier)
//!   - one `<lexical-unit>` with `<form>` per writing system
//!   - zero or more `<sense>` elements, each with `<gloss>` and
//!     `<definition>` per writing system
//!   - optional `<grammatical-info value="..."/>` per sense
//!
//! Mapping:
//!
//! | LIFT element            | donto                                                  |
//! |-------------------------|--------------------------------------------------------|
//! | `<entry id="x">`        | `lift:entry/x rdf:type lift:Entry`                     |
//! | `<lexical-unit><form>`  | `lift:entry/x lift:form/<lang> "<text>"` (per WS)      |
//! | `<sense id="y">`        | `lift:entry/x lift:hasSense lift:sense/y`              |
//! |                         | `lift:sense/y rdf:type lift:Sense`                     |
//! | `<gloss lang="en">`     | `lift:sense/y lift:gloss/<lang> "<text>"`              |
//! | `<definition>`          | `lift:sense/y lift:definition/<lang> "<text>"`         |
//! | `<grammatical-info>`    | `lift:sense/y lift:grammaticalCategory <value>`        |
//!
//! Everything else (relations, traits, custom fields, examples,
//! pronunciation, citation, …) is recorded in the loss report.

pub mod parser;

use donto_client::{DontoClient, Object, StatementInput};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub batch_size: usize,
    pub strict: bool,
}
impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            strict: false,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Report {
    pub source_path: String,
    pub entries_seen: u64,
    pub senses_seen: u64,
    pub statements_inserted: u64,
    pub elapsed_ms: u64,
    pub losses: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed LIFT XML: {0}")]
    Parse(String),
    #[error("strict mode: loss report is non-empty ({0} item(s))")]
    StrictLosses(usize),
    #[error("client error: {0}")]
    Client(#[from] donto_client::Error),
}

pub struct Importer<'a> {
    client: &'a DontoClient,
    default_context: String,
}

impl<'a> Importer<'a> {
    pub fn new(client: &'a DontoClient, default_context: impl Into<String>) -> Self {
        Self {
            client,
            default_context: default_context.into(),
        }
    }

    pub async fn import(
        &self,
        path: impl AsRef<Path>,
        opts: ImportOptions,
    ) -> Result<Report, ImportError> {
        let path = path.as_ref();
        let started = std::time::Instant::now();
        let mut report = Report {
            source_path: path.display().to_string(),
            ..Report::default()
        };

        self.client
            .ensure_context(&self.default_context, "custom", "permissive", None)
            .await?;

        let body = std::fs::read_to_string(path)?;
        let entries = parser::parse(&body, &mut report)?;
        report.entries_seen = entries.len() as u64;

        let mut stmts: Vec<StatementInput> = Vec::new();
        for entry in &entries {
            let entry_iri = format!("lift:entry/{}", iri_safe(&entry.id));
            stmts.push(
                StatementInput::new(entry_iri.clone(), "rdf:type", Object::iri("lift:Entry"))
                    .with_context(&self.default_context),
            );
            for (lang, form) in &entry.lexical_unit {
                stmts.push(
                    StatementInput::new(
                        entry_iri.clone(),
                        format!("lift:form/{lang}"),
                        Object::Literal(donto_client::Literal::string(form)),
                    )
                    .with_context(&self.default_context),
                );
            }
            for sense in &entry.senses {
                report.senses_seen += 1;
                let sense_iri = format!("lift:sense/{}", iri_safe(&sense.id));
                stmts.push(
                    StatementInput::new(
                        entry_iri.clone(),
                        "lift:hasSense",
                        Object::iri(sense_iri.clone()),
                    )
                    .with_context(&self.default_context),
                );
                stmts.push(
                    StatementInput::new(sense_iri.clone(), "rdf:type", Object::iri("lift:Sense"))
                        .with_context(&self.default_context),
                );
                for (lang, text) in &sense.glosses {
                    stmts.push(
                        StatementInput::new(
                            sense_iri.clone(),
                            format!("lift:gloss/{lang}"),
                            Object::Literal(donto_client::Literal::string(text)),
                        )
                        .with_context(&self.default_context),
                    );
                }
                for (lang, text) in &sense.definitions {
                    stmts.push(
                        StatementInput::new(
                            sense_iri.clone(),
                            format!("lift:definition/{lang}"),
                            Object::Literal(donto_client::Literal::string(text)),
                        )
                        .with_context(&self.default_context),
                    );
                }
                if let Some(g) = &sense.grammatical_info {
                    stmts.push(
                        StatementInput::new(
                            sense_iri.clone(),
                            "lift:grammaticalCategory",
                            Object::Literal(donto_client::Literal::string(g)),
                        )
                        .with_context(&self.default_context),
                    );
                }
            }
        }

        let mut inserted: u64 = 0;
        for chunk in stmts.chunks(opts.batch_size) {
            self.client.assert_batch(chunk).await?;
            inserted += chunk.len() as u64;
        }
        report.statements_inserted = inserted;
        report.elapsed_ms = started.elapsed().as_millis() as u64;

        if opts.strict && !report.losses.is_empty() {
            return Err(ImportError::StrictLosses(report.losses.len()));
        }
        Ok(report)
    }
}

fn iri_safe(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
