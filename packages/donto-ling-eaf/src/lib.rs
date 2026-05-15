//! EAF (ELAN Annotation Format) importer.
//!
//! EAF is XML produced by ELAN, the multimodal-corpus annotation
//! tool. A typical EAF document carries:
//!
//!   - `<HEADER>` with `<MEDIA_DESCRIPTOR>` rows (audio / video paths)
//!   - `<TIME_ORDER>` with `<TIME_SLOT TIME_SLOT_ID="ts1" TIME_VALUE="0"/>`
//!   - one or more `<TIER TIER_ID="..." LINGUISTIC_TYPE_REF="..." PARTICIPANT="..."/>`
//!     containing `<ANNOTATION>` children. Two annotation shapes:
//!       * `<ALIGNABLE_ANNOTATION>` — has explicit TIME_SLOT_REF1/2
//!       * `<REF_ANNOTATION>` — references a parent annotation
//!
//! Mapping (v1):
//!
//! | EAF element                                | donto                                            |
//! |--------------------------------------------|--------------------------------------------------|
//! | document root                              | `eaf:doc/<basename>` typed `eaf:Document`        |
//! | media descriptor                           | `eaf:doc/<id> eaf:media <url>` (literal)         |
//! | tier                                       | `eaf:tier/<TIER_ID> rdf:type eaf:Tier` +         |
//! |                                            | `eaf:doc/<id> eaf:hasTier eaf:tier/<TIER_ID>`    |
//! | tier participant                           | `eaf:tier/<TIER_ID> eaf:participant "<name>"`    |
//! | alignable annotation                       | `eaf:ann/<ann_id> rdf:type eaf:AlignableAnnotation` |
//! |                                            | `eaf:tier/<...> eaf:hasAnnotation eaf:ann/<id>`  |
//! |                                            | `eaf:ann/<...> eaf:startMs <int>`                |
//! |                                            | `eaf:ann/<...> eaf:endMs <int>`                  |
//! |                                            | `eaf:ann/<...> eaf:value "<text>"`               |
//! | ref annotation                             | `eaf:ann/<id> rdf:type eaf:RefAnnotation`        |
//! |                                            | `eaf:ann/<id> eaf:refersTo eaf:ann/<parent>`     |
//!
//! Loss: linguistic-type definitions (tier templates), controlled
//! vocabularies, language descriptors, and the `LICENSES` block are
//! recorded but not represented.

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
    pub doc_iri: String,
    pub tiers_seen: u64,
    pub annotations_seen: u64,
    pub statements_inserted: u64,
    pub elapsed_ms: u64,
    pub losses: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed EAF XML: {0}")]
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
        let basename = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "doc".into());
        let doc_iri = format!("eaf:doc/{}", iri_safe(&basename));
        let mut report = Report {
            source_path: path.display().to_string(),
            doc_iri: doc_iri.clone(),
            ..Report::default()
        };

        self.client
            .ensure_context(&self.default_context, "custom", "permissive", None)
            .await?;

        let body = std::fs::read_to_string(path)?;
        let parsed = parser::parse(&body, &mut report)?;

        let mut stmts: Vec<StatementInput> = Vec::new();
        stmts.push(
            StatementInput::new(doc_iri.clone(), "rdf:type", Object::iri("eaf:Document"))
                .with_context(&self.default_context),
        );
        for url in &parsed.media_urls {
            stmts.push(
                StatementInput::new(
                    doc_iri.clone(),
                    "eaf:media",
                    Object::Literal(donto_client::Literal::string(url)),
                )
                .with_context(&self.default_context),
            );
        }

        for tier in &parsed.tiers {
            report.tiers_seen += 1;
            let tier_iri = format!("eaf:tier/{}", iri_safe(&tier.id));
            stmts.push(
                StatementInput::new(tier_iri.clone(), "rdf:type", Object::iri("eaf:Tier"))
                    .with_context(&self.default_context),
            );
            stmts.push(
                StatementInput::new(
                    doc_iri.clone(),
                    "eaf:hasTier",
                    Object::iri(tier_iri.clone()),
                )
                .with_context(&self.default_context),
            );
            if let Some(p) = &tier.participant {
                stmts.push(
                    StatementInput::new(
                        tier_iri.clone(),
                        "eaf:participant",
                        Object::Literal(donto_client::Literal::string(p)),
                    )
                    .with_context(&self.default_context),
                );
            }
            for ann in &tier.annotations {
                report.annotations_seen += 1;
                let ann_iri = format!("eaf:ann/{}", iri_safe(&ann.id));
                let type_iri = match ann.kind {
                    parser::AnnotationKind::Alignable => "eaf:AlignableAnnotation",
                    parser::AnnotationKind::Ref => "eaf:RefAnnotation",
                };
                stmts.push(
                    StatementInput::new(ann_iri.clone(), "rdf:type", Object::iri(type_iri))
                        .with_context(&self.default_context),
                );
                stmts.push(
                    StatementInput::new(
                        tier_iri.clone(),
                        "eaf:hasAnnotation",
                        Object::iri(ann_iri.clone()),
                    )
                    .with_context(&self.default_context),
                );
                if let Some(v) = &ann.value {
                    stmts.push(
                        StatementInput::new(
                            ann_iri.clone(),
                            "eaf:value",
                            Object::Literal(donto_client::Literal::string(v)),
                        )
                        .with_context(&self.default_context),
                    );
                }
                if let Some(start) = ann.start_ms {
                    stmts.push(
                        StatementInput::new(
                            ann_iri.clone(),
                            "eaf:startMs",
                            Object::Literal(donto_client::Literal::integer(start)),
                        )
                        .with_context(&self.default_context),
                    );
                }
                if let Some(end) = ann.end_ms {
                    stmts.push(
                        StatementInput::new(
                            ann_iri.clone(),
                            "eaf:endMs",
                            Object::Literal(donto_client::Literal::integer(end)),
                        )
                        .with_context(&self.default_context),
                    );
                }
                if let Some(parent_ann) = &ann.ref_to {
                    stmts.push(
                        StatementInput::new(
                            ann_iri.clone(),
                            "eaf:refersTo",
                            Object::iri(format!("eaf:ann/{}", iri_safe(parent_ann))),
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
