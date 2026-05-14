//! Internal algebra. Both DontoQL and SPARQL surfaces compile to this.
//!
//! The algebra is intentionally small for Phase 4. It will grow with
//! property paths, OPTIONAL, etc. in later phases.

use donto_client::{ContextScope, Polarity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Term {
    Var(String),
    Iri(String),
    Literal {
        v: serde_json::Value,
        dt: String,
        lang: Option<String>,
    },
}

impl Term {
    pub fn var(name: impl Into<String>) -> Self {
        Term::Var(name.into())
    }
    pub fn iri(s: impl Into<String>) -> Self {
        Term::Iri(s.into())
    }
    pub fn is_var(&self) -> bool {
        matches!(self, Term::Var(_))
    }
    pub fn as_var(&self) -> Option<&str> {
        if let Term::Var(n) = self {
            Some(n)
        } else {
            None
        }
    }
}

/// A single triple/quad pattern. The graph slot is optional; when present it
/// over-rides the query's scope for this pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pattern {
    pub subject: Term,
    pub predicate: Term,
    pub object: Term,
    pub graph: Option<Term>,
}

/// Identity expansion mode (PRD §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdentityMode {
    Default,
    ExpandClusters,
    ExpandSameAsTransitive,
    Strict,
}

/// Predicate-alignment expansion mode (Predicate Alignment Layer).
///
/// Controls how a query treats the predicate slot. `Expand` is the default and
/// rides the predicate closure (migration 0055 makes `donto_match` expand by
/// default). `Strict` pins to the exact predicate IRI; `ExpandAbove(pct)`
/// expands only via alignments whose confidence ≥ pct/100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PredicateExpansion {
    Expand,
    Strict,
    ExpandAbove(u8),
}

impl Default for PredicateExpansion {
    fn default() -> Self {
        PredicateExpansion::Expand
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Filter {
    Eq(Term, Term),
    Neq(Term, Term),
    Bound(String), // BOUND(?x)
    Lt(Term, Term),
    Le(Term, Term),
    Gt(Term, Term),
    Ge(Term, Term),
}

/// Ordering applied after filters, before LIMIT/OFFSET. The only named
/// ordering today is `CONTRADICTION_PRESSURE` (PRD §11 delta), computed
/// from `donto_contradiction_frontier`. Default ordering is unspecified;
/// donto deliberately exposes no implicit ORDER (PRD §I-No-hidden-ordering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderBy {
    None,
    ContradictionPressureDesc,
    ContradictionPressureAsc,
}

impl Default for OrderBy {
    fn default() -> Self {
        OrderBy::None
    }
}

/// `WITH evidence = redacted_if_required` post-clause. Today the
/// evaluator does not attach evidence to result rows; parsing this
/// clause records intent for the future result-shape extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceShape {
    None,
    RedactedIfRequired,
    Full,
}

impl Default for EvidenceShape {
    fn default() -> Self {
        EvidenceShape::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub scope: Option<ContextScope>,
    pub scope_preset: Option<String>,
    pub patterns: Vec<Pattern>,
    pub filters: Vec<Filter>,
    pub polarity: Option<Polarity>,
    pub min_maturity: u8,
    pub identity: IdentityMode,
    pub project: Vec<String>, // empty = all bound vars
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub predicate_expansion: PredicateExpansion,
    /// Bitemporal time-travel target (tx_time). Set by the
    /// `TRANSACTION_TIME AS_OF` clause or `PRESET as_of:<ts>`.
    /// None = current state (open tx_time).
    #[serde(default)]
    pub as_of_tx: Option<chrono::DateTime<chrono::Utc>>,
    /// Sparse-overlay filter on `donto_stmt_modality`. None = any modality.
    #[serde(default)]
    pub modality: Option<Vec<String>>,
    /// Sparse-overlay filter on `donto_stmt_extraction_level`.
    /// None = any level.
    #[serde(default)]
    pub extraction_level: Option<Vec<String>>,
    /// Policy gate (PRD §11): require the statement's source-policy
    /// to allow the named action (e.g. `read_metadata`,
    /// `publish_release`). None = no policy gate.
    #[serde(default)]
    pub policy_allows: Option<String>,
    /// Schema-lens directive (PRD §11 §M3 delta). Recorded only.
    #[serde(default)]
    pub schema_lens: Option<String>,
    /// `EXPANDS_FROM concept(...) USING schema_lens(...)` directive
    /// (PRD §11.2). Parsed shape only; evaluator pending.
    #[serde(default)]
    pub expands_from: Option<ExpandsFrom>,
    #[serde(default)]
    pub order_by: OrderBy,
    #[serde(default)]
    pub evidence_shape: EvidenceShape,
}

/// `EXPANDS_FROM concept(<iri>) USING schema_lens(<iri>)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpandsFrom {
    pub concept: String,
    pub schema_lens: String,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            scope: None,
            scope_preset: None,
            patterns: vec![],
            filters: vec![],
            polarity: Some(Polarity::Asserted),
            min_maturity: 0,
            identity: IdentityMode::Default,
            project: vec![],
            limit: None,
            offset: None,
            predicate_expansion: PredicateExpansion::Expand,
            as_of_tx: None,
            modality: None,
            extraction_level: None,
            policy_allows: None,
            schema_lens: None,
            expands_from: None,
            order_by: OrderBy::None,
            evidence_shape: EvidenceShape::None,
        }
    }
}
