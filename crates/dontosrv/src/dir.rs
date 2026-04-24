//! DIR: Donto Intermediate Representation (PRD §13).
//!
//! Phase 4 ships a JSON-encoded DIR for ergonomics. The binary protobuf form
//! is Phase 5 (when the Lean encoder lands and we need wire stability).
//! The directives below cover what Phase 4-7 actually emit and accept.

use axum::{response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const DIR_VERSION: &str = "0.1.0-json";
pub const DIR_VERSION_MAJOR: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Directive {
    DeclarePredicate {
        iri: String,
        label: Option<String>,
        canonical_of: Option<String>,
    },
    DeclareContext {
        iri: String,
        kind: String,
        parent: Option<String>,
        mode: String,
    },
    DeclareShape {
        iri: String,
        focus: String,
        body: serde_json::Value,
        severity: String,
    },
    DeclareRule {
        iri: String,
        pattern: String,
        output_ctx: String,
        body: serde_json::Value,
    },
    AssertBatch {
        context: String,
        statements: Vec<DirStatement>,
    },
    Retract {
        statement_id: uuid::Uuid,
    },
    Correct {
        statement_id: uuid::Uuid,
        new: DirStatement,
    },
    ValidateRequest {
        shape_iri: String,
        scope: serde_json::Value,
    },
    ValidateResponse {
        shape_iri: String,
        focus_count: u64,
        violations: Vec<DirViolation>,
        certificate: Option<serde_json::Value>,
    },
    DeriveRequest {
        rule_iri: String,
        scope: serde_json::Value,
        into: String,
    },
    DeriveResponse {
        rule_iri: String,
        into: String,
        emitted: u64,
        certificate: Option<serde_json::Value>,
    },
    Certificate {
        kind: String,
        subject_stmt: uuid::Uuid,
        body: serde_json::Value,
    },
    IngestDocument {
        iri: String,
        media_type: String,
        label: Option<String>,
        source_url: Option<String>,
        language: Option<String>,
    },
    IngestRevision {
        document_iri: String,
        body: Option<String>,
        parser_version: Option<String>,
    },
    CreateSpan {
        revision_id: uuid::Uuid,
        span_type: String,
        start_offset: Option<i32>,
        end_offset: Option<i32>,
        surface_text: Option<String>,
    },
    CreateAnnotation {
        span_id: uuid::Uuid,
        space_iri: String,
        feature: String,
        value: Option<String>,
        confidence: Option<f64>,
    },
    StartExtraction {
        model_id: Option<String>,
        source_revision_id: Option<uuid::Uuid>,
        context: Option<String>,
    },
    CompleteExtraction {
        run_id: uuid::Uuid,
        status: String,
    },
    LinkEvidence {
        statement_id: uuid::Uuid,
        link_type: String,
        target: serde_json::Value,
    },
    RegisterAgent {
        iri: String,
        agent_type: String,
        label: Option<String>,
        model_id: Option<String>,
    },
    AssertArgument {
        source: uuid::Uuid,
        target: uuid::Uuid,
        relation: String,
        context: String,
        strength: Option<f64>,
    },
    EmitObligation {
        statement_id: uuid::Uuid,
        obligation_type: String,
        context: String,
        priority: Option<i16>,
    },
    ResolveObligation {
        obligation_id: uuid::Uuid,
        status: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirStatement {
    pub subject: String,
    pub predicate: String,
    pub object_iri: Option<String>,
    pub object_lit: Option<serde_json::Value>,
    pub polarity: Option<String>,
    pub maturity: Option<u8>,
    pub valid_lo: Option<chrono::NaiveDate>,
    pub valid_hi: Option<chrono::NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirViolation {
    pub focus: String,
    pub reason: String,
    pub evidence: Vec<uuid::Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct DirEnvelope {
    pub version: String,
    pub directives: Vec<Directive>,
}

#[derive(Debug, Serialize)]
pub struct DirReply {
    pub version: String,
    pub directives: Vec<Directive>,
}

pub async fn handle(Json(env): Json<DirEnvelope>) -> impl IntoResponse {
    if !env.version.starts_with("0.") {
        return Json(json!({
            "error": "dir version mismatch",
            "expected_major": DIR_VERSION_MAJOR,
            "got": env.version,
        }))
        .into_response();
    }
    // Phase 4: echo + recognize. Real handlers in Phase 5+.
    let recognized: Vec<&'static str> = env
        .directives
        .iter()
        .map(|d| match d {
            Directive::DeclarePredicate { .. } => "declare_predicate",
            Directive::DeclareContext { .. } => "declare_context",
            Directive::DeclareShape { .. } => "declare_shape",
            Directive::DeclareRule { .. } => "declare_rule",
            Directive::AssertBatch { .. } => "assert_batch",
            Directive::Retract { .. } => "retract",
            Directive::Correct { .. } => "correct",
            Directive::ValidateRequest { .. } => "validate_request",
            Directive::ValidateResponse { .. } => "validate_response",
            Directive::DeriveRequest { .. } => "derive_request",
            Directive::DeriveResponse { .. } => "derive_response",
            Directive::Certificate { .. } => "certificate",
            Directive::IngestDocument { .. } => "ingest_document",
            Directive::IngestRevision { .. } => "ingest_revision",
            Directive::CreateSpan { .. } => "create_span",
            Directive::CreateAnnotation { .. } => "create_annotation",
            Directive::StartExtraction { .. } => "start_extraction",
            Directive::CompleteExtraction { .. } => "complete_extraction",
            Directive::LinkEvidence { .. } => "link_evidence",
            Directive::RegisterAgent { .. } => "register_agent",
            Directive::AssertArgument { .. } => "assert_argument",
            Directive::EmitObligation { .. } => "emit_obligation",
            Directive::ResolveObligation { .. } => "resolve_obligation",
        })
        .collect();
    Json(json!({
        "version": DIR_VERSION,
        "recognized": recognized,
        "ack": env.directives.len(),
    }))
    .into_response()
}
