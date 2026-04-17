use serde::{Deserialize, Serialize};
use serde_json::json;

/// Context scope per PRD §7. Phase 0 supports include/exclude and inheritance
/// flags; the maturity floor is applied as a separate parameter at query time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextScope {
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default = "default_true")]
    pub include_descendants: bool,
    #[serde(default)]
    pub include_ancestors: bool,
}

fn default_true() -> bool { true }

impl ContextScope {
    /// Single-context inclusion with descendants.
    pub fn just(iri: impl Into<String>) -> Self {
        Self {
            include: vec![iri.into()],
            exclude: vec![],
            include_descendants: true,
            include_ancestors: false,
        }
    }

    /// Empty scope = visible everywhere (the resolver short-circuits).
    pub fn anywhere() -> Self {
        Self {
            include: vec![],
            exclude: vec![],
            include_descendants: true,
            include_ancestors: false,
        }
    }

    pub fn excluding(mut self, iri: impl Into<String>) -> Self {
        self.exclude.push(iri.into());
        self
    }

    pub fn with_ancestors(mut self) -> Self { self.include_ancestors = true; self }
    pub fn without_descendants(mut self) -> Self { self.include_descendants = false; self }

    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "include": self.include,
            "exclude": self.exclude,
            "include_descendants": self.include_descendants,
            "include_ancestors": self.include_ancestors,
        })
    }
}

impl Default for ContextScope {
    fn default() -> Self { Self::anywhere() }
}
