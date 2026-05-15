//! Universal Dependencies (CoNLL-U) importer.
//!
//! Reads a CoNLL-U file or a directory of `.conllu` files and emits
//! donto quads. Each sentence is recorded as a small graph rooted at
//! a sentence IRI; each token becomes a `ud:Token` entity with
//! attributes mirroring the ten CoNLL-U columns:
//!
//! | column  | predicate              | object kind                          |
//! |---------|------------------------|--------------------------------------|
//! | FORM    | `ud:form`              | xsd:string literal                   |
//! | LEMMA   | `ud:lemma`             | xsd:string literal                   |
//! | UPOS    | `ud:upos`              | IRI (`upos:NOUN`, `upos:VERB`, …)    |
//! | XPOS    | `ud:xpos`              | xsd:string literal                   |
//! | FEATS   | `ud:feat`              | per-pair `ud:feat:<Name>` IRI → lit  |
//! | HEAD    | `ud:head`              | IRI (sibling token in same sentence) |
//! | DEPREL  | `ud:deprel`            | IRI (`udrel:nsubj`, `udrel:obj`, …)  |
//! | DEPS    | `ud:enhanced_dep`      | recorded as loss for v1              |
//! | MISC    | `ud:misc_<key>`        | per-key literal; complex misc → loss |
//!
//! `# sent_id = …` and `# text = …` sentence-level metadata become
//! attributes on the sentence IRI. Multi-word tokens (`1-2 …`) and
//! empty nodes (`5.1 …`) are recognised and recorded as loss — they
//! need richer modeling that's not part of this v1.
//!
//! Loss reporting follows PRD I9; the shape mirrors `donto-ling-cldf`
//! so a caller that handles one handles both.
//!
//! Example:
//!
//! ```no_run
//! # async fn run() -> anyhow::Result<()> {
//! use donto_client::DontoClient;
//! use donto_ling_ud::{Importer, ImportOptions};
//! let client = DontoClient::from_dsn("postgres://donto:donto@127.0.0.1:55432/donto")?;
//! let importer = Importer::new(&client, "ctx:ling/ud/en-ewt");
//! let report = importer.import("./ud/en_ewt-ud-dev.conllu", ImportOptions::default()).await?;
//! println!("inserted {} statements ({} sentences)",
//!     report.statements_inserted, report.sentences_seen);
//! # Ok(()) }
//! ```

pub mod parser;

use donto_client::{DontoClient, Object, StatementInput};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// Tunables for an import run.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub batch_size: usize,
    /// When true, abort if the loss report is non-empty.
    pub strict: bool,
    /// Sentence IRI prefix. Default: `ud:sent/`. Each sentence's
    /// `sent_id` (or auto-generated index) becomes the local part.
    pub sentence_prefix: String,
    /// Token IRI prefix. Default: `ud:tok/`. Concatenated with
    /// `<sent_id>/<token_id>`.
    pub token_prefix: String,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            strict: false,
            sentence_prefix: "ud:sent/".into(),
            token_prefix: "ud:tok/".into(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Report {
    pub source_path: String,
    pub sentences_seen: u64,
    pub tokens_seen: u64,
    pub statements_inserted: u64,
    pub elapsed_ms: u64,
    pub losses: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed CoNLL-U at line {line}: {msg}")]
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
            ..Report::default()
        };

        self.client
            .ensure_context(&self.default_context, "custom", "permissive", None)
            .await?;

        let body = std::fs::read_to_string(path)?;
        let sentences = parser::parse(&body, &mut report)?;
        report.sentences_seen = sentences.len() as u64;

        let mut stmts: Vec<StatementInput> = Vec::new();
        for (idx, sent) in sentences.iter().enumerate() {
            let sent_local = sent
                .sent_id
                .clone()
                .unwrap_or_else(|| format!("autogen-{idx}"));
            let sent_iri = format!("{}{sent_local}", opts.sentence_prefix);

            // Sentence-level: rdf:type ud:Sentence + text/sent_id
            stmts.push(
                StatementInput::new(sent_iri.clone(), "rdf:type", Object::iri("ud:Sentence"))
                    .with_context(&self.default_context),
            );
            if let Some(text) = &sent.text {
                stmts.push(
                    StatementInput::new(
                        sent_iri.clone(),
                        "ud:text",
                        Object::Literal(donto_client::Literal::string(text)),
                    )
                    .with_context(&self.default_context),
                );
            }

            // Tokens.
            for tok in &sent.tokens {
                report.tokens_seen += 1;
                let tok_iri = format!("{}{}/{}", opts.token_prefix, sent_local, tok.id);
                // Sentence ↔ token membership.
                stmts.push(
                    StatementInput::new(tok_iri.clone(), "rdf:type", Object::iri("ud:Token"))
                        .with_context(&self.default_context),
                );
                stmts.push(
                    StatementInput::new(
                        sent_iri.clone(),
                        "ud:hasToken",
                        Object::iri(tok_iri.clone()),
                    )
                    .with_context(&self.default_context),
                );
                if let Some(s) = &tok.form {
                    stmts.push(
                        StatementInput::new(
                            tok_iri.clone(),
                            "ud:form",
                            Object::Literal(donto_client::Literal::string(s)),
                        )
                        .with_context(&self.default_context),
                    );
                }
                if let Some(s) = &tok.lemma {
                    stmts.push(
                        StatementInput::new(
                            tok_iri.clone(),
                            "ud:lemma",
                            Object::Literal(donto_client::Literal::string(s)),
                        )
                        .with_context(&self.default_context),
                    );
                }
                if let Some(s) = &tok.upos {
                    stmts.push(
                        StatementInput::new(
                            tok_iri.clone(),
                            "ud:upos",
                            Object::iri(format!("upos:{s}")),
                        )
                        .with_context(&self.default_context),
                    );
                }
                if let Some(s) = &tok.xpos {
                    stmts.push(
                        StatementInput::new(
                            tok_iri.clone(),
                            "ud:xpos",
                            Object::Literal(donto_client::Literal::string(s)),
                        )
                        .with_context(&self.default_context),
                    );
                }
                for (k, v) in &tok.feats {
                    stmts.push(
                        StatementInput::new(
                            tok_iri.clone(),
                            format!("ud:feat:{k}"),
                            Object::Literal(donto_client::Literal::string(v)),
                        )
                        .with_context(&self.default_context),
                    );
                }
                if let Some(head) = &tok.head {
                    // Head 0 is the syntactic root marker; emit as a
                    // distinct claim (?tok ud:root true) rather than
                    // linking to a non-existent token.
                    if head == "0" {
                        stmts.push(
                            StatementInput::new(
                                tok_iri.clone(),
                                "ud:isRoot",
                                Object::Literal(donto_client::Literal::string("true")),
                            )
                            .with_context(&self.default_context),
                        );
                    } else {
                        let head_iri =
                            format!("{}{}/{}", opts.token_prefix, sent_local, head);
                        stmts.push(
                            StatementInput::new(
                                tok_iri.clone(),
                                "ud:head",
                                Object::iri(head_iri),
                            )
                            .with_context(&self.default_context),
                        );
                    }
                }
                if let Some(d) = &tok.deprel {
                    stmts.push(
                        StatementInput::new(
                            tok_iri.clone(),
                            "ud:deprel",
                            Object::iri(format!("udrel:{d}")),
                        )
                        .with_context(&self.default_context),
                    );
                }
                if tok.deps_raw.as_deref().is_some_and(|d| d != "_") {
                    // v1: record enhanced deps as a loss line per sentence.
                    report.losses.push(format!(
                        "sentence {sent_local} token {tok_id}: enhanced DEPS not represented",
                        tok_id = tok.id
                    ));
                }
                if let Some(misc) = &tok.misc_raw {
                    if !misc.is_empty() && misc != "_" {
                        // Split into key=value pairs when possible.
                        for kv in misc.split('|') {
                            if let Some((k, v)) = kv.split_once('=') {
                                stmts.push(
                                    StatementInput::new(
                                        tok_iri.clone(),
                                        format!("ud:misc_{k}"),
                                        Object::Literal(donto_client::Literal::string(v)),
                                    )
                                    .with_context(&self.default_context),
                                );
                            } else {
                                report.losses.push(format!(
                                    "sentence {sent_local} token {tok_id}: \
                                     un-keyed MISC entry `{kv}`",
                                    tok_id = tok.id
                                ));
                            }
                        }
                    }
                }
            }

            // Multi-word tokens and empty nodes — recorded but not
            // expanded.
            for mwt in &sent.mwt_ranges {
                report.losses.push(format!(
                    "sentence {sent_local}: multi-word token range `{mwt}` recorded but not expanded"
                ));
            }
            for en in &sent.empty_nodes {
                report.losses.push(format!(
                    "sentence {sent_local}: empty node `{en}` recorded but not expanded"
                ));
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
