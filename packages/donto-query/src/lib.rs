//! donto-query: parsers + internal algebra for DontoQL and SPARQL 1.1 subset.
//!
//! Per PRD §12 surfaces B and C. Both surfaces compile to the [`algebra`]
//! types and are evaluated by the same engine.
//!
//! Phase 4 scope:
//!   * DontoQL: SCOPE, MATCH, FILTER, POLARITY, MATURITY, PROJECT, LIMIT,
//!     OFFSET, EXPAND CLUSTERS / STRICT IDENTITY (modifiers; expansion
//!     ships in Phase 6 with rules).
//!   * SPARQL: SELECT with basic graph pattern + GRAPH + FILTER (=, !=).
//!     Property paths, OPTIONAL, UNION, MINUS, aggregates: follow-on.

#![warn(missing_debug_implementations, rust_2018_idioms)]

pub mod algebra;
pub mod dontoql;
pub mod evaluator;
pub mod sparql;

pub use algebra::*;
pub use dontoql::parse_dontoql;
pub use evaluator::{evaluate, Bindings, EvalRow};
pub use sparql::parse_sparql;
