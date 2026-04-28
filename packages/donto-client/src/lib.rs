//! donto-client — typed wrapper around the Phase 0 SQL surface.
//!
//! Per PRD §12 Surface A, donto exposes a SQL-function API. This crate is the
//! Rust binding for it. Higher surfaces (DontoQL, SPARQL) are Phase 4.
//!
//! The client does not embed schema migrations — call [`apply_migrations`]
//! at startup or run `cargo run -p donto-cli -- migrate`.

#![warn(missing_debug_implementations, rust_2018_idioms)]

pub mod client;
pub mod error;
pub mod migrations;
pub mod model;
pub mod scope;

pub use client::DontoClient;
pub use error::{Error, Result};
pub use migrations::apply_migrations;
pub use model::{
    Agent, ArgumentRelation, Document, DocumentRevision, EvidenceLink, ExtractionRun, Literal,
    Object, ObligationStatus, Polarity, ProofObligation, Reaction, ReactionKind, ShapeVerdict,
    Span, Statement, StatementInput, TextMatch, TimeBucket,
};
pub use scope::ContextScope;
