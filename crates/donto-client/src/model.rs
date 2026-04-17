use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Polarity per PRD §6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Polarity {
    Asserted,
    Negated,
    Absent,
    Unknown,
}

impl Polarity {
    pub fn as_str(self) -> &'static str {
        match self {
            Polarity::Asserted => "asserted",
            Polarity::Negated => "negated",
            Polarity::Absent => "absent",
            Polarity::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "asserted" => Polarity::Asserted,
            "negated" => Polarity::Negated,
            "absent" => Polarity::Absent,
            "unknown" => Polarity::Unknown,
            _ => return None,
        })
    }
}

/// A literal object: value + datatype IRI + optional language tag.
/// Encoded as JSON `{"v": ..., "dt": "...", "lang": null|"en"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Literal {
    /// Stored value. JSON-typed for flexibility (string, number, bool, etc).
    pub v: serde_json::Value,
    /// Datatype IRI (e.g. `xsd:string`, `xsd:integer`). Required.
    pub dt: String,
    /// Optional language tag (BCP-47).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

impl Literal {
    pub fn string(s: impl Into<String>) -> Self {
        Self {
            v: serde_json::Value::String(s.into()),
            dt: "xsd:string".into(),
            lang: None,
        }
    }
    pub fn lang_string(s: impl Into<String>, lang: impl Into<String>) -> Self {
        Self {
            v: serde_json::Value::String(s.into()),
            dt: "rdf:langString".into(),
            lang: Some(lang.into()),
        }
    }
    pub fn integer(n: i64) -> Self {
        Self {
            v: serde_json::Value::Number(n.into()),
            dt: "xsd:integer".into(),
            lang: None,
        }
    }
    pub fn date(d: NaiveDate) -> Self {
        Self {
            v: serde_json::Value::String(d.format("%Y-%m-%d").to_string()),
            dt: "xsd:date".into(),
            lang: None,
        }
    }
}

/// Either an IRI object or a literal object. Exactly one is set per statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Object {
    Iri(String),
    Literal(Literal),
}

impl Object {
    pub fn iri(s: impl Into<String>) -> Self {
        Object::Iri(s.into())
    }
    pub fn lit(l: Literal) -> Self {
        Object::Literal(l)
    }
}

/// Input for [`crate::DontoClient::assert`]. Carries every field a caller
/// might want to set; defaults are applied server-side.
#[derive(Debug, Clone)]
pub struct StatementInput {
    pub subject: String,
    pub predicate: String,
    pub object: Object,
    pub context: String,
    pub polarity: Polarity,
    pub maturity: u8,
    pub valid_lo: Option<NaiveDate>,
    pub valid_hi: Option<NaiveDate>,
}

impl StatementInput {
    pub fn new(subject: impl Into<String>, predicate: impl Into<String>, object: Object) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object,
            context: "donto:anonymous".into(),
            polarity: Polarity::Asserted,
            maturity: 0,
            valid_lo: None,
            valid_hi: None,
        }
    }

    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context = ctx.into();
        self
    }
    pub fn with_polarity(mut self, p: Polarity) -> Self {
        self.polarity = p;
        self
    }
    pub fn with_maturity(mut self, m: u8) -> Self {
        self.maturity = m;
        self
    }
    pub fn with_valid(mut self, lo: Option<NaiveDate>, hi: Option<NaiveDate>) -> Self {
        self.valid_lo = lo;
        self.valid_hi = hi;
        self
    }
}

/// A row returned by [`crate::DontoClient::match_pattern`].
#[derive(Debug, Clone)]
pub struct Statement {
    pub statement_id: Uuid,
    pub subject: String,
    pub predicate: String,
    pub object: Object,
    pub context: String,
    pub polarity: Polarity,
    pub maturity: u8,
    pub valid_lo: Option<NaiveDate>,
    pub valid_hi: Option<NaiveDate>,
    pub tx_lo: DateTime<Utc>,
    pub tx_hi: Option<DateTime<Utc>>,
}
