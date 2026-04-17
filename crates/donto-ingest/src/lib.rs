//! donto-ingest — ingestion pipelines (PRD §19).
//!
//! Phase 8 ships:
//!   * N-Quads (also reachable from donto-cli)
//!   * Turtle and TriG
//!   * RDF/XML
//!   * JSON-LD (subset: top-level @context, no remote-context fetching)
//!   * CSV with column mapping
//!   * JSONL streaming (one statement per line)
//!   * Property-graph JSON (Apache AGE / Neo4j export shape)
//!   * Quarantine path for shape-violating content
//!
//! All formats route through [`Pipeline`] which:
//!   1. converts source → [`donto_client::StatementInput`] iterator,
//!   2. batches and posts via [`donto_client::DontoClient::assert_batch`],
//!   3. emits a per-source [`IngestReport`].

#![warn(missing_debug_implementations, rust_2018_idioms)]

pub mod csv;
pub mod jsonl;
pub mod jsonld;
pub mod nquads;
pub mod pipeline;
pub mod property_graph;
pub mod quarantine;
pub mod rdfxml;
pub mod turtle;

pub use pipeline::{IngestReport, Pipeline};
