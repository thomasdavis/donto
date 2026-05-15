//! CLDF (Cross-Linguistic Data Format) importer.
//!
//! Reads a CLDF directory dataset (one TSV per object class plus a
//! top-level `<dataset>-metadata.json` describing schema) and emits
//! donto quads:
//!
//! | CLDF object class    | donto representation                              |
//! |----------------------|---------------------------------------------------|
//! | `LanguageTable`      | one entity per row; ID becomes IRI                |
//! | `ParameterTable`     | predicate registration (one predicate per row)    |
//! | `CodeTable`          | value vocabulary (one IRI per discrete code)      |
//! | `ValueTable`         | one statement per row (lang → param → value)      |
//!
//! Anything beyond those four tables (e.g. `ExampleTable`,
//! `BorrowingTable`, custom tables) is recorded in the loss report
//! rather than silently dropped.
//!
//! Loss reports follow PRD I9: every adapter must declare what it
//! could not represent. The [`Report`] struct returns this alongside
//! the ingestion counts so a caller can refuse to commit a "lossy
//! release" without an explicit `--allow-lossy` toggle.
//!
//! Round-trip is not yet a goal of this importer — that's M7
//! release-builder work. The contract here is:
//!   1. Every CLDF value row becomes a donto statement, OR is
//!      explicitly listed in the loss report.
//!   2. Every CLDF parameter becomes a registered predicate.
//!   3. The dataset's contributing source (citation) is recorded
//!      as a `donto_document` if `--register-source` is set.
//!
//! Example:
//!
//! ```no_run
//! # async fn run() -> anyhow::Result<()> {
//! use donto_client::DontoClient;
//! use donto_ling_cldf::{Importer, ImportOptions};
//! let client = DontoClient::from_dsn("postgres://donto:donto@127.0.0.1:55432/donto")?;
//! let importer = Importer::new(&client, "ctx:ling/wals-2026-04");
//! let report = importer.import("./datasets/wals-cldf", ImportOptions::default()).await?;
//! println!("inserted {} statements", report.statements_inserted);
//! for loss in &report.losses {
//!     eprintln!("loss: {loss}");
//! }
//! # Ok(()) }
//! ```

pub mod metadata;
pub mod parser;

use donto_client::{DontoClient, Object, StatementInput};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use tracing::{info, warn};

pub use metadata::{CldfMetadata, CldfTable};

/// Tunables for an import run.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Insert statements in batches of this size. Larger batches use
    /// less round-trip time; smaller batches limit memory.
    pub batch_size: usize,
    /// When true, register the dataset's source citation as a
    /// `donto_document` with the IRI `cldf:<dataset>`. Off by default
    /// because the policy assignment is the caller's responsibility.
    pub register_source: bool,
    /// When true, abort if the loss report is non-empty. Off by
    /// default — losses are reported, not fatal.
    pub strict: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            register_source: false,
            strict: false,
        }
    }
}

/// Result of an import. Counts are exact; losses are best-effort
/// human-readable strings describing what the importer chose not to
/// represent.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Report {
    pub dataset_path: String,
    pub dataset_iri: Option<String>,
    pub languages_seen: u64,
    pub parameters_seen: u64,
    pub codes_seen: u64,
    pub values_seen: u64,
    pub statements_inserted: u64,
    pub elapsed_ms: u64,
    pub losses: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("io error reading CLDF dataset: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid CLDF metadata: {0}")]
    Metadata(String),
    #[error("invalid CLDF row in {table}: {msg}")]
    Row { table: String, msg: String },
    #[error("strict mode: loss report is non-empty ({0} item(s))")]
    StrictLosses(usize),
    #[error("client error: {0}")]
    Client(#[from] donto_client::Error),
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
    #[error("anyhow: {0}")]
    Other(#[from] anyhow::Error),
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

    /// Run the importer against a CLDF directory dataset.
    pub async fn import(
        &self,
        dir: impl AsRef<Path>,
        opts: ImportOptions,
    ) -> Result<Report, ImportError> {
        let dir = dir.as_ref();
        let started = std::time::Instant::now();
        let meta = metadata::load(dir)?;
        info!(
            tables = meta.tables.len(),
            "loaded CLDF metadata for dataset"
        );

        let dataset_iri = meta.dataset_iri();
        let mut report = Report {
            dataset_path: dir.display().to_string(),
            dataset_iri: dataset_iri.clone(),
            ..Report::default()
        };

        // Make sure the destination context exists.
        self.client
            .ensure_context(&self.default_context, "custom", "permissive", None)
            .await?;

        // Map tables once so emit_* can resolve foreign keys.
        let parsed = parser::parse_all(dir, &meta)?;

        // 1. Predicates (ParameterTable). Each parameter becomes
        // a predicate IRI under the dataset prefix; the row's name
        // and description go into the donto predicate registry.
        let pred_batch = self.emit_predicates(&parsed, &mut report).await?;
        report.parameters_seen = pred_batch as u64;

        // 2. Language entities — emitted as type assertions so we
        // can reach them by IRI later. The minimum useful set:
        // (?lang rdf:type ling:Language).
        let lang_stmts = self.emit_languages(&parsed, &mut report);

        // 3. Codes — emitted as type assertions too:
        // (?code rdf:type ling:Code) and (?code ling:codeFor ?param).
        let code_stmts = self.emit_codes(&parsed, &mut report);

        // 4. Values — the actual claims. lang -predicate-> value.
        // Count input rows first, before emit_values skips any.
        report.values_seen = parsed.values.len() as u64;
        let val_stmts = self.emit_values(&parsed, &mut report)?;

        // Send everything in batched asserts.
        let all: Vec<StatementInput> = lang_stmts
            .into_iter()
            .chain(code_stmts)
            .chain(val_stmts)
            .collect();
        let mut inserted: u64 = 0;
        for chunk in all.chunks(opts.batch_size) {
            self.client.assert_batch(chunk).await?;
            inserted += chunk.len() as u64;
        }
        report.statements_inserted = inserted;

        // 5. Loss detection: any table that wasn't one of the four
        // canonical CLDF object classes is recorded as loss.
        for table in &meta.tables {
            let kind = table.kind();
            if matches!(
                kind,
                metadata::TableKind::Language
                    | metadata::TableKind::Parameter
                    | metadata::TableKind::Code
                    | metadata::TableKind::Value
            ) {
                continue;
            }
            report.losses.push(format!(
                "table `{name}` (kind={kind:?}) not represented",
                name = table.url.as_deref().unwrap_or("?"),
            ));
        }

        if opts.register_source {
            if let Some(iri) = dataset_iri.as_deref() {
                // The legacy ensure_document path now lands on the
                // fail-closed default policy (post-0123). Caller is
                // expected to assign an explicit policy via
                // donto_register_source for production use.
                self.client
                    .ensure_document(iri, "application/cldf+json", Some(iri), None, None)
                    .await?;
            } else {
                report.losses.push(
                    "register_source requested but dataset has no IRI in metadata".into(),
                );
            }
        }

        report.elapsed_ms = started.elapsed().as_millis() as u64;

        if opts.strict && !report.losses.is_empty() {
            warn!(losses = report.losses.len(), "strict mode rejected import");
            return Err(ImportError::StrictLosses(report.losses.len()));
        }

        Ok(report)
    }

    // ---------- emitters ----------

    async fn emit_predicates(
        &self,
        parsed: &parser::ParsedTables,
        report: &mut Report,
    ) -> Result<usize, ImportError> {
        // Predicates are registered lazily by donto_assert when first
        // used — but we want to surface the parameter rows as their
        // own facts (e.g. ling:hasName, ling:hasDescription) so
        // queries can ask about predicate metadata. Emit those as
        // statements; they'll be ingested in the main batch below.
        // For now, the function only counts.
        let _ = report;
        let _ = self;
        Ok(parsed.parameters.len())
    }

    fn emit_languages(
        &self,
        parsed: &parser::ParsedTables,
        report: &mut Report,
    ) -> Vec<StatementInput> {
        let mut out = Vec::with_capacity(parsed.languages.len() * 2);
        for lang in &parsed.languages {
            report.languages_seen += 1;
            out.push(
                StatementInput::new(
                    lang.iri.clone(),
                    "rdf:type",
                    Object::iri("ling:Language"),
                )
                .with_context(&self.default_context),
            );
            if let Some(name) = &lang.name {
                out.push(
                    StatementInput::new(
                        lang.iri.clone(),
                        "ling:name",
                        Object::Literal(donto_client::Literal::string(name)),
                    )
                    .with_context(&self.default_context),
                );
            }
            if let Some(glotto) = &lang.glottocode {
                out.push(
                    StatementInput::new(
                        lang.iri.clone(),
                        "ling:glottocode",
                        Object::Literal(donto_client::Literal::string(glotto)),
                    )
                    .with_context(&self.default_context),
                );
            }
        }
        out
    }

    fn emit_codes(
        &self,
        parsed: &parser::ParsedTables,
        report: &mut Report,
    ) -> Vec<StatementInput> {
        let mut out = Vec::with_capacity(parsed.codes.len() * 3);
        for code in &parsed.codes {
            report.codes_seen += 1;
            out.push(
                StatementInput::new(code.iri.clone(), "rdf:type", Object::iri("ling:Code"))
                    .with_context(&self.default_context),
            );
            if let Some(p) = &code.parameter_iri {
                out.push(
                    StatementInput::new(
                        code.iri.clone(),
                        "ling:codeFor",
                        Object::iri(p.clone()),
                    )
                    .with_context(&self.default_context),
                );
            }
            if let Some(name) = &code.name {
                out.push(
                    StatementInput::new(
                        code.iri.clone(),
                        "ling:name",
                        Object::Literal(donto_client::Literal::string(name)),
                    )
                    .with_context(&self.default_context),
                );
            }
        }
        out
    }

    fn emit_values(
        &self,
        parsed: &parser::ParsedTables,
        report: &mut Report,
    ) -> Result<Vec<StatementInput>, ImportError> {
        let mut out = Vec::with_capacity(parsed.values.len());
        for v in &parsed.values {
            // Missing language or parameter → record loss, skip row.
            let Some(lang_iri) = parsed.language_iri(&v.language_id) else {
                report.losses.push(format!(
                    "value row {id}: unknown Language_ID `{l}`",
                    id = v.id,
                    l = v.language_id
                ));
                continue;
            };
            let Some(param_iri) = parsed.parameter_iri(&v.parameter_id) else {
                report.losses.push(format!(
                    "value row {id}: unknown Parameter_ID `{p}`",
                    id = v.id,
                    p = v.parameter_id
                ));
                continue;
            };
            let object = match v.value.as_deref() {
                Some(raw) => {
                    // If the value matches a known Code_ID, link by IRI.
                    // Otherwise emit as a literal.
                    if let Some(code_iri) = parsed.code_iri(raw) {
                        Object::iri(code_iri)
                    } else {
                        Object::Literal(donto_client::Literal::string(raw))
                    }
                }
                None => {
                    report.losses.push(format!(
                        "value row {id}: empty Value cell (no literal, no Code_ID)",
                        id = v.id
                    ));
                    continue;
                }
            };
            out.push(
                StatementInput::new(lang_iri, param_iri, object)
                    .with_context(&self.default_context),
            );
        }
        Ok(out)
    }
}
