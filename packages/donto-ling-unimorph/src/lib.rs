//! UniMorph importer.
//!
//! UniMorph paradigm files are 3-column TSVs: `lemma TAB inflected
//! TAB tags`. Tags are semicolon-delimited cross-linguistic
//! morphosyntactic feature labels (e.g. `V;PRS;3;SG` for "verb,
//! present, third person, singular").
//!
//! One file per language; the language IRI is supplied by the
//! caller (UniMorph filenames historically use ISO 639-3 codes).
//!
//! Mapping:
//!
//! | UniMorph element  | donto                                             |
//! |-------------------|---------------------------------------------------|
//! | `lemma`           | `?lemma rdf:type unimorph:Lexeme`                 |
//! | `inflected`       | `?form rdf:type unimorph:WordForm`                |
//! |                   | `?form unimorph:lemma ?lemma`                     |
//! |                   | `?form unimorph:surface "<text>"`                 |
//! | `tag`             | `?form unimorph:hasTag unimorph:tag/<TAG>`        |
//!
//! The IRI shape: `unimorph:<lang>/lex/<lemma>` for lexemes,
//! `unimorph:<lang>/form/<lemma>/<i>` for forms (i is the row
//! index because two inflected rows for the same lemma+tags pair
//! are allowed in real corpora).
//!
//! Tags that look malformed (empty, whitespace-only) are recorded
//! as loss. Multi-word lemmas (whitespace-containing) are
//! supported — the whole string becomes the local part, with
//! spaces percent-encoded as `_` so the IRI stays valid.

pub mod parser;

use donto_client::{DontoClient, Object, StatementInput};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub batch_size: usize,
    pub strict: bool,
    /// ISO 639-3 (or any) language code prefixed into emitted IRIs.
    pub language: String,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            strict: false,
            language: "und".into(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Report {
    pub source_path: String,
    pub language: String,
    pub lexemes_seen: u64,
    pub forms_seen: u64,
    pub statements_inserted: u64,
    pub elapsed_ms: u64,
    pub losses: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed UniMorph line {line}: {msg}")]
    Parse { line: usize, msg: String },
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
            language: opts.language.clone(),
            ..Report::default()
        };
        self.client
            .ensure_context(&self.default_context, "custom", "permissive", None)
            .await?;

        let body = std::fs::read_to_string(path)?;
        let rows = parser::parse(&body, &mut report)?;
        let lang = &opts.language;

        let mut stmts: Vec<StatementInput> = Vec::new();
        let mut seen_lexemes = std::collections::HashSet::<String>::new();
        for (idx, row) in rows.iter().enumerate() {
            let lemma_iri = format!("unimorph:{lang}/lex/{}", iri_safe(&row.lemma));
            if seen_lexemes.insert(row.lemma.clone()) {
                stmts.push(
                    StatementInput::new(
                        lemma_iri.clone(),
                        "rdf:type",
                        Object::iri("unimorph:Lexeme"),
                    )
                    .with_context(&self.default_context),
                );
                stmts.push(
                    StatementInput::new(
                        lemma_iri.clone(),
                        "unimorph:citationForm",
                        Object::Literal(donto_client::Literal::string(&row.lemma)),
                    )
                    .with_context(&self.default_context),
                );
                report.lexemes_seen += 1;
            }
            let form_iri = format!(
                "unimorph:{lang}/form/{}/{i}",
                iri_safe(&row.lemma),
                i = idx
            );
            stmts.push(
                StatementInput::new(
                    form_iri.clone(),
                    "rdf:type",
                    Object::iri("unimorph:WordForm"),
                )
                .with_context(&self.default_context),
            );
            stmts.push(
                StatementInput::new(
                    form_iri.clone(),
                    "unimorph:lemma",
                    Object::iri(lemma_iri.clone()),
                )
                .with_context(&self.default_context),
            );
            stmts.push(
                StatementInput::new(
                    form_iri.clone(),
                    "unimorph:surface",
                    Object::Literal(donto_client::Literal::string(&row.inflected)),
                )
                .with_context(&self.default_context),
            );
            for tag in &row.tags {
                stmts.push(
                    StatementInput::new(
                        form_iri.clone(),
                        "unimorph:hasTag",
                        Object::iri(format!("unimorph:tag/{tag}")),
                    )
                    .with_context(&self.default_context),
                );
            }
            report.forms_seen += 1;
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
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
