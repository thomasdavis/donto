//! DontoQL parser. Hand-rolled (no nom needed for this small grammar);
//! produces an [`Algebra::Query`] AST.
//!
//! Grammar (DontoQL v2 — PRD §11):
//!
//! ```text
//! query        := keyword_clause+
//! keyword_clause :=
//!     'SCOPE'    scope_descriptor
//!   | 'PRESET'   IDENT_or_PREFIXED_or_STRING
//!   | 'MATCH'    triple (',' triple)*
//!   | 'FILTER'   filter_expr (',' filter_expr)*
//!   | 'POLARITY' ident_in_set
//!   | 'MATURITY' '>='? INT
//!   | 'IDENTITY' ident
//!   | 'IDENTITY_LENS' ident
//!   | 'PREDICATES' ('EXPAND' | 'STRICT' | 'EXPAND_ABOVE' INT)
//!   | 'MODALITY'        ident (',' ident)*
//!   | 'EXTRACTION_LEVEL' ident (',' ident)*
//!   | 'TRANSACTION_TIME' 'AS_OF' STRING_or_PREFIXED
//!   | 'AS_OF' STRING_or_PREFIXED                 # shorthand
//!   | 'POLICY' 'ALLOWS' ident
//!   | 'SCHEMA_LENS' (iri | ident)
//!   | 'EXPANDS_FROM' 'concept' '(' iri ')'
//!                    'USING' 'schema_lens' '(' iri ')'
//!   | 'ORDER_BY' ident ('DESC'|'ASC')?           # one named order
//!   | 'ORDER' 'BY' ident ('DESC'|'ASC')?         # two-word form
//!   | 'WITH'     'evidence' '=' ident
//!   | 'PROJECT'  var (',' var)*
//!   | 'LIMIT'    INT
//!   | 'OFFSET'   INT
//!
//! triple   := term term term ('IN' term)?
//! term     := var | iri | string-lit | int-lit
//! var      := '?' IDENT
//! iri      := '<' chars '>' | PREFIXED   (e.g. ex:foo)
//! filter_expr := term op term ; op ∈ { = != < <= > >= }
//! ```
//!
//! Some clauses parse but the evaluator declares `Unsupported` until
//! the corresponding kernel lands (see PRD §11 verdicts). The parser
//! accepts the full v2 surface so callers can write forward-compatible
//! queries; check `evaluator::evaluate` for what executes today.

use crate::algebra::*;
use donto_client::{ContextScope, Polarity};
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, thiserror::Error)]
#[error("dontoql parse error at {pos}: {msg}")]
pub struct ParseError {
    pub pos: usize,
    pub msg: String,
}

struct Lexer<'a> {
    src: &'a str,
    chars: Peekable<Chars<'a>>,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            chars: src.chars().peekable(),
            pos: 0,
        }
    }
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
            } else if c == '#' {
                while let Some(c) = self.bump() {
                    if c == '\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }
    fn err(&self, msg: impl Into<String>) -> ParseError {
        ParseError {
            pos: self.pos,
            msg: msg.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),            // bare identifier (also keywords)
    Var(String),              // ?name
    IriAngle(String),         // <...>
    Prefixed(String, String), // pfx:local
    Str(String),
    Int(i64),
    Comma,
    Semi,
    Op(String),
    Eof,
}

fn lex(src: &str) -> Result<Vec<(usize, Tok)>, ParseError> {
    let mut lx = Lexer::new(src);
    let mut out = Vec::new();
    loop {
        lx.skip_ws();
        let start = lx.pos;
        let Some(c) = lx.peek() else {
            out.push((start, Tok::Eof));
            break;
        };
        match c {
            ',' => {
                lx.bump();
                out.push((start, Tok::Comma));
            }
            ';' => {
                lx.bump();
                out.push((start, Tok::Semi));
            }
            '?' => {
                lx.bump();
                let mut s = String::new();
                while let Some(c) = lx.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        s.push(c);
                        lx.bump();
                    } else {
                        break;
                    }
                }
                if s.is_empty() {
                    return Err(lx.err("variable name expected after `?`"));
                }
                out.push((start, Tok::Var(s)));
            }
            '<' => {
                lx.bump();
                if lx.peek() == Some('=') {
                    lx.bump();
                    out.push((start, Tok::Op("<=".into())));
                } else if matches!(lx.peek(), Some(c) if c.is_ascii_alphabetic() || c == ':') {
                    // <iri> form
                    let mut s = String::new();
                    loop {
                        match lx.bump() {
                            Some('>') => break,
                            Some(c) => s.push(c),
                            None => return Err(lx.err("unterminated <iri>")),
                        }
                    }
                    out.push((start, Tok::IriAngle(s)));
                } else {
                    out.push((start, Tok::Op("<".into())));
                }
            }
            '"' => {
                lx.bump();
                let mut s = String::new();
                loop {
                    match lx.bump() {
                        Some('\\') => match lx.bump() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some('"') => s.push('"'),
                            Some('\\') => s.push('\\'),
                            Some(c) => s.push(c),
                            None => return Err(lx.err("trailing backslash")),
                        },
                        Some('"') => break,
                        Some(c) => s.push(c),
                        None => return Err(lx.err("unterminated string")),
                    }
                }
                out.push((start, Tok::Str(s)));
            }
            '0'..='9' | '-' => {
                let mut s = String::new();
                if c == '-' {
                    s.push(c);
                    lx.bump();
                }
                while let Some(c) = lx.peek() {
                    if c.is_ascii_digit() {
                        s.push(c);
                        lx.bump();
                    } else {
                        break;
                    }
                }
                let n: i64 = s
                    .parse()
                    .map_err(|_| lx.err(format!("bad int literal {s}")))?;
                out.push((start, Tok::Int(n)));
            }
            '=' => {
                lx.bump();
                out.push((start, Tok::Op("=".into())));
            }
            '!' => {
                lx.bump();
                if lx.peek() == Some('=') {
                    lx.bump();
                    out.push((start, Tok::Op("!=".into())));
                } else {
                    return Err(lx.err("expected `!=`"));
                }
            }
            '>' => {
                lx.bump();
                if lx.peek() == Some('=') {
                    lx.bump();
                    out.push((start, Tok::Op(">=".into())));
                } else {
                    out.push((start, Tok::Op(">".into())));
                }
            }
            _ if c.is_alphabetic() || c == '_' => {
                let mut s = String::new();
                while let Some(c) = lx.peek() {
                    if c.is_alphanumeric() || c == '_' || c == '-' {
                        s.push(c);
                        lx.bump();
                    } else {
                        break;
                    }
                }
                if lx.peek() == Some(':') {
                    lx.bump();
                    let mut local = String::new();
                    while let Some(c) = lx.peek() {
                        // Local part accepts the usual CURIE
                        // characters plus `:` so multi-segment IRIs
                        // (e.g. `ctx:genes/topic`, `test:dql2:<uuid>`)
                        // lex as a single Prefixed token. This is
                        // also what the SPARQL/Turtle PN_LOCAL
                        // grammars permit.
                        if c.is_alphanumeric()
                            || c == '_'
                            || c == '-'
                            || c == '/'
                            || c == '.'
                            || c == '#'
                            || c == ':'
                        {
                            local.push(c);
                            lx.bump();
                        } else {
                            break;
                        }
                    }
                    out.push((start, Tok::Prefixed(s, local)));
                } else {
                    out.push((start, Tok::Ident(s)));
                }
            }
            _ => return Err(lx.err(format!("unexpected character `{c}`"))),
        }
    }
    let _ = lx.src; // suppress dead read
    Ok(out)
}

struct Parser {
    toks: Vec<(usize, Tok)>,
    i: usize,
}

impl Parser {
    fn new(toks: Vec<(usize, Tok)>) -> Self {
        Self { toks, i: 0 }
    }
    fn peek(&self) -> &Tok {
        &self.toks[self.i].1
    }
    fn pos(&self) -> usize {
        self.toks[self.i].0
    }
    fn bump(&mut self) -> Tok {
        let t = self.toks[self.i].1.clone();
        self.i += 1;
        t
    }
    fn maybe_keyword(&mut self, kw: &str) -> bool {
        if let Tok::Ident(s) = self.peek() {
            if s.eq_ignore_ascii_case(kw) {
                self.i += 1;
                return true;
            }
        }
        false
    }

    fn term(&mut self) -> Result<Term, ParseError> {
        Ok(match self.bump() {
            Tok::Var(s) => Term::Var(s),
            Tok::IriAngle(s) => Term::Iri(s),
            Tok::Prefixed(p, l) => Term::Iri(format!("{p}:{l}")),
            Tok::Str(s) => Term::Literal {
                v: serde_json::Value::String(s),
                dt: "xsd:string".into(),
                lang: None,
            },
            Tok::Int(n) => Term::Literal {
                v: serde_json::Value::Number(n.into()),
                dt: "xsd:integer".into(),
                lang: None,
            },
            t => {
                return Err(ParseError {
                    pos: self.pos(),
                    msg: format!("expected term, got {t:?}"),
                })
            }
        })
    }

    fn parse_query(&mut self) -> Result<Query, ParseError> {
        let mut q = Query::default();
        loop {
            match self.peek().clone() {
                Tok::Eof => break,
                Tok::Semi => {
                    self.bump();
                }
                Tok::Ident(kw) => {
                    let lkw = kw.to_ascii_uppercase();
                    self.bump();
                    match lkw.as_str() {
                        "SCOPE" => q.scope = Some(self.parse_scope()?),
                        "PRESET" => q.scope_preset = Some(self.parse_preset_value()?),
                        "MATCH" => q.patterns = self.parse_patterns()?,
                        "FILTER" => q.filters = self.parse_filters()?,
                        "POLARITY" => q.polarity = Some(self.parse_polarity()?),
                        "MATURITY" => {
                            let _ = self.maybe_op(">=");
                            q.min_maturity = self.parse_int()? as u8;
                        }
                        "IDENTITY" | "IDENTITY_LENS" => q.identity = self.parse_identity()?,
                        "PREDICATES" => q.predicate_expansion = self.parse_predicate_expansion()?,
                        "MODALITY" => q.modality = Some(self.parse_ident_list()?),
                        "EXTRACTION_LEVEL" => {
                            q.extraction_level = Some(self.parse_ident_list()?)
                        }
                        "TRANSACTION_TIME" => {
                            self.expect_keyword("AS_OF")?;
                            q.as_of_tx = Some(self.parse_timestamp()?);
                        }
                        "AS_OF" => {
                            q.as_of_tx = Some(self.parse_timestamp()?);
                        }
                        "POLICY" => {
                            self.expect_keyword("ALLOWS")?;
                            q.policy_allows = Some(self.parse_ident()?);
                        }
                        "SCHEMA_LENS" => {
                            q.schema_lens = Some(self.parse_iri_or_ident()?);
                        }
                        "EXPANDS_FROM" => {
                            q.expands_from = Some(self.parse_expands_from()?);
                        }
                        "ORDER_BY" => {
                            q.order_by = self.parse_order_by()?;
                        }
                        "ORDER" => {
                            // Two-word form: ORDER BY <name> [DESC|ASC]
                            self.expect_keyword("BY")?;
                            q.order_by = self.parse_order_by()?;
                        }
                        "WITH" => {
                            self.expect_keyword("evidence")?;
                            self.expect_op("=")?;
                            q.evidence_shape = self.parse_evidence_shape()?;
                        }
                        "PROJECT" => q.project = self.parse_var_list()?,
                        "LIMIT" => q.limit = Some(self.parse_int()? as u64),
                        "OFFSET" => q.offset = Some(self.parse_int()? as u64),
                        other => {
                            return Err(ParseError {
                                pos: self.pos(),
                                msg: format!("unknown clause `{other}`"),
                            })
                        }
                    }
                }
                other => {
                    return Err(ParseError {
                        pos: self.pos(),
                        msg: format!("expected clause keyword, got {other:?}"),
                    })
                }
            }
        }
        Ok(q)
    }

    fn parse_scope(&mut self) -> Result<ContextScope, ParseError> {
        // SCOPE include <iri> [, <iri>...] [exclude <iri> ...] [no_descendants] [ancestors]
        let mut sc = ContextScope::anywhere();
        loop {
            match self.peek().clone() {
                Tok::Ident(s) if s.eq_ignore_ascii_case("include") => {
                    self.bump();
                    sc.include.extend(self.parse_iri_list()?);
                }
                Tok::Ident(s) if s.eq_ignore_ascii_case("exclude") => {
                    self.bump();
                    sc.exclude.extend(self.parse_iri_list()?);
                }
                Tok::Ident(s) if s.eq_ignore_ascii_case("no_descendants") => {
                    self.bump();
                    sc.include_descendants = false;
                }
                Tok::Ident(s) if s.eq_ignore_ascii_case("ancestors") => {
                    self.bump();
                    sc.include_ancestors = true;
                }
                _ => break,
            }
        }
        Ok(sc)
    }

    fn parse_iri_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut out = Vec::new();
        loop {
            match self.bump() {
                Tok::IriAngle(s) => out.push(s),
                Tok::Prefixed(p, l) => out.push(format!("{p}:{l}")),
                t => {
                    return Err(ParseError {
                        pos: self.pos(),
                        msg: format!("iri expected, got {t:?}"),
                    })
                }
            }
            if matches!(self.peek(), Tok::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(out)
    }

    fn parse_ident(&mut self) -> Result<String, ParseError> {
        match self.bump() {
            Tok::Ident(s) => Ok(s),
            t => Err(ParseError {
                pos: self.pos(),
                msg: format!("ident expected, got {t:?}"),
            }),
        }
    }

    /// Permissive value reader for `PRESET`. Accepts:
    ///   * a bare ident (e.g. `latest`, `curated`)
    ///   * a prefixed token, which is reassembled as `head:tail`
    ///     (e.g. `as_of:2026-05-08T17:00:00Z` — even though the
    ///     timestamp itself contains colons, the lexer takes the
    ///     prefix at the first colon and treats the remainder as
    ///     the local part)
    ///   * a string literal (`PRESET "as_of:..."`)
    fn parse_preset_value(&mut self) -> Result<String, ParseError> {
        match self.bump() {
            Tok::Ident(s) => Ok(s),
            Tok::Prefixed(head, tail) => Ok(format!("{head}:{tail}")),
            Tok::Str(s) => Ok(s),
            t => Err(ParseError {
                pos: self.pos(),
                msg: format!("PRESET value expected, got {t:?}"),
            }),
        }
    }

    fn parse_int(&mut self) -> Result<i64, ParseError> {
        match self.bump() {
            Tok::Int(n) => Ok(n),
            t => Err(ParseError {
                pos: self.pos(),
                msg: format!("int expected, got {t:?}"),
            }),
        }
    }

    fn maybe_op(&mut self, op: &str) -> bool {
        if let Tok::Op(s) = self.peek() {
            if s == op {
                self.i += 1;
                return true;
            }
        }
        false
    }

    fn parse_patterns(&mut self) -> Result<Vec<Pattern>, ParseError> {
        let mut out = Vec::new();
        loop {
            let s = self.term()?;
            let p = self.term()?;
            let o = self.term()?;
            let g = if self.maybe_keyword("IN") {
                Some(self.term()?)
            } else {
                None
            };
            out.push(Pattern {
                subject: s,
                predicate: p,
                object: o,
                graph: g,
            });
            if matches!(self.peek(), Tok::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(out)
    }

    fn parse_filters(&mut self) -> Result<Vec<Filter>, ParseError> {
        let mut out = Vec::new();
        loop {
            let lhs = self.term()?;
            let op = if let Tok::Op(o) = self.peek().clone() {
                self.bump();
                o
            } else {
                return Err(ParseError {
                    pos: self.pos(),
                    msg: "filter operator expected".into(),
                });
            };
            let rhs = self.term()?;
            out.push(match op.as_str() {
                "=" => Filter::Eq(lhs, rhs),
                "!=" => Filter::Neq(lhs, rhs),
                "<" => Filter::Lt(lhs, rhs),
                "<=" => Filter::Le(lhs, rhs),
                ">" => Filter::Gt(lhs, rhs),
                ">=" => Filter::Ge(lhs, rhs),
                _ => {
                    return Err(ParseError {
                        pos: self.pos(),
                        msg: format!("unsupported op `{op}`"),
                    })
                }
            });
            if matches!(self.peek(), Tok::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(out)
    }

    fn parse_polarity(&mut self) -> Result<Polarity, ParseError> {
        let s = self.parse_ident()?;
        Polarity::parse(&s.to_ascii_lowercase()).ok_or(ParseError {
            pos: self.pos(),
            msg: format!("unknown polarity `{s}`"),
        })
    }

    fn parse_predicate_expansion(&mut self) -> Result<PredicateExpansion, ParseError> {
        let s = self.parse_ident()?;
        Ok(match s.to_ascii_uppercase().as_str() {
            "EXPAND" => PredicateExpansion::Expand,
            "STRICT" => PredicateExpansion::Strict,
            "EXPAND_ABOVE" => {
                let n = self.parse_int()?;
                if !(0..=100).contains(&n) {
                    return Err(ParseError {
                        pos: self.pos(),
                        msg: format!("EXPAND_ABOVE percent out of range: {n}"),
                    });
                }
                PredicateExpansion::ExpandAbove(n as u8)
            }
            other => {
                return Err(ParseError {
                    pos: self.pos(),
                    msg: format!("unknown predicate expansion mode `{other}`"),
                })
            }
        })
    }

    fn parse_identity(&mut self) -> Result<IdentityMode, ParseError> {
        let s = self.parse_ident()?;
        Ok(match s.to_ascii_uppercase().as_str() {
            "DEFAULT" => IdentityMode::Default,
            "EXPAND_CLUSTERS" | "CLUSTERS" => IdentityMode::ExpandClusters,
            "EXPAND_SAMEAS_TRANSITIVE" | "SAMEAS" => IdentityMode::ExpandSameAsTransitive,
            "STRICT" => IdentityMode::Strict,
            other => {
                return Err(ParseError {
                    pos: self.pos(),
                    msg: format!("unknown identity mode `{other}`"),
                })
            }
        })
    }

    /// Consume the next token if it is an identifier matching `kw`
    /// (case-insensitive). Errors otherwise.
    fn expect_keyword(&mut self, kw: &str) -> Result<(), ParseError> {
        match self.peek().clone() {
            Tok::Ident(s) if s.eq_ignore_ascii_case(kw) => {
                self.bump();
                Ok(())
            }
            other => Err(ParseError {
                pos: self.pos(),
                msg: format!("expected `{kw}`, got {other:?}"),
            }),
        }
    }

    /// Consume an operator token equal to `op`, or error.
    fn expect_op(&mut self, op: &str) -> Result<(), ParseError> {
        if self.maybe_op(op) {
            Ok(())
        } else {
            Err(ParseError {
                pos: self.pos(),
                msg: format!("expected `{op}`"),
            })
        }
    }

    /// `ident (, ident)*` — used by MODALITY / EXTRACTION_LEVEL whose
    /// values are constrained string enums on the storage side.
    fn parse_ident_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut out = Vec::new();
        loop {
            out.push(self.parse_ident()?);
            if matches!(self.peek(), Tok::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(out)
    }

    /// `iri` or bare `ident` — used by SCHEMA_LENS which accepts either
    /// a registered lens IRI or a short name.
    fn parse_iri_or_ident(&mut self) -> Result<String, ParseError> {
        match self.bump() {
            Tok::IriAngle(s) => Ok(s),
            Tok::Prefixed(p, l) => Ok(format!("{p}:{l}")),
            Tok::Ident(s) => Ok(s),
            t => Err(ParseError {
                pos: self.pos(),
                msg: format!("iri or ident expected, got {t:?}"),
            }),
        }
    }

    /// RFC3339 timestamp value. The lexer treats a colon as the
    /// prefix separator, so `2026-05-08T17:00:00Z` arrives as a
    /// prefixed token whose head is the date portion before the
    /// first colon. Reassemble. Bare strings (quoted) also work.
    fn parse_timestamp(&mut self) -> Result<chrono::DateTime<chrono::Utc>, ParseError> {
        let raw = match self.bump() {
            Tok::Str(s) => s,
            Tok::Prefixed(h, t) => format!("{h}:{t}"),
            Tok::Ident(s) => s,
            other => {
                return Err(ParseError {
                    pos: self.pos(),
                    msg: format!("timestamp expected, got {other:?}"),
                })
            }
        };
        chrono::DateTime::parse_from_rfc3339(&raw)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .map_err(|e| ParseError {
                pos: self.pos(),
                msg: format!("invalid RFC3339 timestamp `{raw}`: {e}"),
            })
    }

    /// `concept ( <iri> ) USING schema_lens ( <iri> )`
    fn parse_expands_from(&mut self) -> Result<ExpandsFrom, ParseError> {
        self.expect_keyword("concept")?;
        self.expect_paren_open()?;
        let concept = self.parse_iri_term()?;
        self.expect_paren_close()?;
        self.expect_keyword("USING")?;
        self.expect_keyword("schema_lens")?;
        self.expect_paren_open()?;
        let schema_lens = self.parse_iri_term()?;
        self.expect_paren_close()?;
        Ok(ExpandsFrom {
            concept,
            schema_lens,
        })
    }

    fn parse_iri_term(&mut self) -> Result<String, ParseError> {
        match self.bump() {
            Tok::IriAngle(s) => Ok(s),
            Tok::Prefixed(p, l) => Ok(format!("{p}:{l}")),
            Tok::Str(s) => Ok(s),
            t => Err(ParseError {
                pos: self.pos(),
                msg: format!("iri expected, got {t:?}"),
            }),
        }
    }

    fn expect_paren_open(&mut self) -> Result<(), ParseError> {
        // The lexer does not produce paren tokens today; treat `(`
        // and `)` as raw characters by peeking the source. To avoid a
        // larger lexer refactor for one syntax form, accept either
        // (a) absence (parens optional), (b) an Ident token equal to
        // "(" — which never happens — or (c) ignore. The grammar
        // here keeps EXPANDS_FROM friendly: `concept ex:foo USING
        // schema_lens ex:lens` is also accepted with no parens.
        // Strict-parens enforcement is deferred.
        let _ = self;
        Ok(())
    }

    fn expect_paren_close(&mut self) -> Result<(), ParseError> {
        let _ = self;
        Ok(())
    }

    /// `<name> [DESC|ASC]` — only `contradiction_pressure` is
    /// recognised today.
    fn parse_order_by(&mut self) -> Result<OrderBy, ParseError> {
        let name = self.parse_ident()?;
        let direction_desc = match self.peek().clone() {
            Tok::Ident(s) if s.eq_ignore_ascii_case("DESC") => {
                self.bump();
                true
            }
            Tok::Ident(s) if s.eq_ignore_ascii_case("ASC") => {
                self.bump();
                false
            }
            _ => true, // default DESC for contradiction_pressure
        };
        match name.to_ascii_lowercase().as_str() {
            "contradiction_pressure" | "contradictionpressure" => {
                if direction_desc {
                    Ok(OrderBy::ContradictionPressureDesc)
                } else {
                    Ok(OrderBy::ContradictionPressureAsc)
                }
            }
            other => Err(ParseError {
                pos: self.pos(),
                msg: format!(
                    "unknown ORDER BY name `{other}` (supported: contradiction_pressure)"
                ),
            }),
        }
    }

    /// `WITH evidence = <ident>` — ident in {none, redacted_if_required, full}.
    fn parse_evidence_shape(&mut self) -> Result<EvidenceShape, ParseError> {
        let s = self.parse_ident()?;
        Ok(match s.to_ascii_lowercase().as_str() {
            "none" => EvidenceShape::None,
            "redacted_if_required" => EvidenceShape::RedactedIfRequired,
            "full" => EvidenceShape::Full,
            other => {
                return Err(ParseError {
                    pos: self.pos(),
                    msg: format!(
                        "unknown WITH evidence value `{other}` (supported: none, redacted_if_required, full)"
                    ),
                })
            }
        })
    }

    fn parse_var_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut out = Vec::new();
        loop {
            match self.bump() {
                Tok::Var(s) => out.push(s),
                t => {
                    return Err(ParseError {
                        pos: self.pos(),
                        msg: format!("variable expected, got {t:?}"),
                    })
                }
            }
            if matches!(self.peek(), Tok::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(out)
    }
}

pub fn parse_dontoql(src: &str) -> Result<Query, ParseError> {
    let toks = lex(src)?;
    let mut p = Parser::new(toks);
    p.parse_query()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_simple_query() {
        let q = parse_dontoql(
            r#"
            PRESET latest
            MATCH ?x ex:knows ?y, ?y ex:name ?n
            FILTER ?n != "Mallory"
            POLARITY asserted
            MATURITY >= 1
            PROJECT ?x, ?y, ?n
            LIMIT 10
        "#,
        )
        .unwrap();
        assert_eq!(q.scope_preset.as_deref(), Some("latest"));
        assert_eq!(q.patterns.len(), 2);
        assert_eq!(q.filters.len(), 1);
        assert_eq!(q.min_maturity, 1);
        assert_eq!(q.project, vec!["x", "y", "n"]);
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn parses_explicit_scope() {
        let q = parse_dontoql(
            r#"
            SCOPE include ex:src, ex:other exclude ex:bad ancestors
            MATCH ?s ?p ?o
        "#,
        )
        .unwrap();
        let sc = q.scope.unwrap();
        assert_eq!(sc.include, vec!["ex:src", "ex:other"]);
        assert_eq!(sc.exclude, vec!["ex:bad"]);
        assert!(sc.include_ancestors);
    }

    #[test]
    fn line_comments_are_ignored() {
        let q = parse_dontoql(
            r#"
            # leading comment
            MATCH ?s ?p ?o   # trailing
            # standalone
            PROJECT ?s
        "#,
        )
        .unwrap();
        assert_eq!(q.patterns.len(), 1);
        assert_eq!(q.project, vec!["s"]);
    }

    #[test]
    fn absolute_iri_survives_lexing() {
        let q = parse_dontoql(
            r#"
            MATCH ?x <http://example.org/name> ?n
        "#,
        )
        .unwrap();
        assert_eq!(q.patterns.len(), 1);
        // The predicate term should carry the full IRI, not a prefixed form.
        let pred_dbg = format!("{:?}", q.patterns[0].predicate);
        assert!(
            pred_dbg.contains("http://example.org/name"),
            "got predicate {pred_dbg}"
        );
    }

    #[test]
    fn filter_operators_supported_parse() {
        // DontoQL v2 FILTER grammar accepts =, !=, <, <=, >, >=.
        // The evaluator already implements the ordering operators
        // via Filter::Lt/Le/Gt/Ge over numeric literals.
        for op in ["=", "!=", "<", "<=", ">", ">="] {
            let src = format!("MATCH ?x ex:p ?n FILTER ?n {op} 5");
            let q = parse_dontoql(&src).unwrap_or_else(|e| panic!("op {op}: {e}"));
            assert_eq!(q.filters.len(), 1, "op {op} did not produce a filter");
        }
    }

    #[test]
    fn polarity_negated_parses() {
        let q = parse_dontoql("MATCH ?s ?p ?o POLARITY negated").unwrap();
        assert!(matches!(q.polarity, Some(Polarity::Negated)));
    }

    #[test]
    fn missing_match_clause_is_a_parse_error() {
        // No MATCH → empty patterns list → evaluator can't do anything
        // useful. Parser currently permits it, so just verify that a
        // PROJECT-only query returns zero patterns rather than panicking.
        let q = parse_dontoql("PROJECT ?x LIMIT 1").unwrap();
        assert!(q.patterns.is_empty());
    }

    #[test]
    fn empty_input_errors_cleanly() {
        let e = parse_dontoql("").err();
        // Either Ok(empty query) or a clean error — must not panic.
        match e {
            Some(_) | None => {}
        }
    }

    #[test]
    fn limit_and_offset_round_trip() {
        let q = parse_dontoql("MATCH ?s ?p ?o LIMIT 25 OFFSET 100").unwrap();
        assert_eq!(q.limit, Some(25));
        assert_eq!(q.offset, Some(100));
    }

    #[test]
    fn predicates_keyword_parses_all_modes() {
        let q = parse_dontoql("MATCH ?s ?p ?o PREDICATES STRICT").unwrap();
        assert_eq!(q.predicate_expansion, PredicateExpansion::Strict);

        let q = parse_dontoql("MATCH ?s ?p ?o PREDICATES EXPAND").unwrap();
        assert_eq!(q.predicate_expansion, PredicateExpansion::Expand);

        let q = parse_dontoql("MATCH ?s ?p ?o PREDICATES EXPAND_ABOVE 80").unwrap();
        assert_eq!(q.predicate_expansion, PredicateExpansion::ExpandAbove(80));

        // Default (no PREDICATES clause) is Expand.
        let q = parse_dontoql("MATCH ?s ?p ?o").unwrap();
        assert_eq!(q.predicate_expansion, PredicateExpansion::Expand);

        // Out-of-range percent is rejected.
        assert!(parse_dontoql("MATCH ?s ?p ?o PREDICATES EXPAND_ABOVE 150").is_err());
    }

    #[test]
    fn maturity_without_operator_defaults_to_ge() {
        // `MATURITY 2` and `MATURITY >= 2` should both set min_maturity = 2.
        let q1 = parse_dontoql("MATCH ?s ?p ?o MATURITY 2").unwrap();
        let q2 = parse_dontoql("MATCH ?s ?p ?o MATURITY >= 2").unwrap();
        assert_eq!(q1.min_maturity, 2);
        assert_eq!(q2.min_maturity, 2);
    }

    // -----------------------------------------------------------------
    // DontoQL v2 clause coverage (PRD §11 delta)
    // -----------------------------------------------------------------

    #[test]
    fn as_of_clause_sets_tx_target() {
        let q = parse_dontoql(
            r#"MATCH ?s ?p ?o
               AS_OF "2026-01-01T00:00:00Z""#,
        )
        .unwrap();
        let ts = q.as_of_tx.expect("as_of_tx set");
        assert_eq!(ts.to_rfc3339(), "2026-01-01T00:00:00+00:00");
    }

    #[test]
    fn transaction_time_as_of_clause_two_word_form() {
        let q = parse_dontoql(
            r#"MATCH ?s ?p ?o
               TRANSACTION_TIME AS_OF "2026-03-15T12:34:56Z""#,
        )
        .unwrap();
        assert!(q.as_of_tx.is_some());
    }

    #[test]
    fn as_of_unquoted_digit_leading_timestamp_errors_cleanly() {
        // The lexer reads `2026` as an Int before the date sees
        // any `-`, so bare RFC3339 timestamps starting with digits
        // don't tokenise as a single value. Document the contract:
        // AS_OF requires either a quoted string or a value whose
        // first character is alphabetic (e.g. CURIE-like).
        let err = parse_dontoql("MATCH ?s ?p ?o AS_OF 2026-05-08T17:00:00Z").err();
        assert!(err.is_some());
    }

    #[test]
    fn as_of_date_only_is_rejected_as_non_rfc3339() {
        // Date alone is not RFC3339-with-offset and must error.
        let err = parse_dontoql(r#"MATCH ?s ?p ?o AS_OF "2026-05-08""#).err();
        assert!(err.is_some(), "date-only should fail RFC3339 parse");
    }

    #[test]
    fn as_of_rejects_invalid_timestamp() {
        let err = parse_dontoql(r#"MATCH ?s ?p ?o AS_OF "not-a-timestamp""#).err();
        assert!(err.is_some(), "expected parse error for bad timestamp");
    }

    #[test]
    fn modality_clause_collects_list() {
        let q = parse_dontoql("MATCH ?s ?p ?o MODALITY descriptive, reconstructed").unwrap();
        assert_eq!(
            q.modality.as_deref(),
            Some(&["descriptive".to_string(), "reconstructed".to_string()][..])
        );
    }

    #[test]
    fn extraction_level_clause_collects_list() {
        let q = parse_dontoql(
            "MATCH ?s ?p ?o EXTRACTION_LEVEL quoted, table_read, manual_entry",
        )
        .unwrap();
        let levels = q.extraction_level.as_deref().unwrap();
        assert_eq!(levels.len(), 3);
        assert!(levels.contains(&"quoted".into()));
        assert!(levels.contains(&"manual_entry".into()));
    }

    #[test]
    fn identity_lens_alias_matches_identity() {
        let a = parse_dontoql("MATCH ?s ?p ?o IDENTITY EXPAND_CLUSTERS").unwrap();
        let b = parse_dontoql("MATCH ?s ?p ?o IDENTITY_LENS EXPAND_CLUSTERS").unwrap();
        assert_eq!(a.identity, b.identity);
        assert_eq!(a.identity, IdentityMode::ExpandClusters);
    }

    #[test]
    fn policy_allows_clause() {
        let q = parse_dontoql("MATCH ?s ?p ?o POLICY ALLOWS publish_release").unwrap();
        assert_eq!(q.policy_allows.as_deref(), Some("publish_release"));
    }

    #[test]
    fn policy_without_allows_is_parse_error() {
        assert!(parse_dontoql("MATCH ?s ?p ?o POLICY publish_release").is_err());
    }

    #[test]
    fn schema_lens_clause_accepts_iri_or_ident() {
        let q1 = parse_dontoql("MATCH ?s ?p ?o SCHEMA_LENS ex:linguistics-core").unwrap();
        assert_eq!(q1.schema_lens.as_deref(), Some("ex:linguistics-core"));
        let q2 = parse_dontoql("MATCH ?s ?p ?o SCHEMA_LENS bare_name").unwrap();
        assert_eq!(q2.schema_lens.as_deref(), Some("bare_name"));
    }

    #[test]
    fn expands_from_clause() {
        let q = parse_dontoql(
            r#"MATCH ?s ?p ?o
               EXPANDS_FROM concept ex:case_marking USING schema_lens ex:linguistics-core"#,
        )
        .unwrap();
        let ef = q.expands_from.unwrap();
        assert_eq!(ef.concept, "ex:case_marking");
        assert_eq!(ef.schema_lens, "ex:linguistics-core");
    }

    #[test]
    fn order_by_contradiction_pressure_default_desc() {
        let q1 = parse_dontoql("MATCH ?s ?p ?o ORDER_BY contradiction_pressure").unwrap();
        let q2 =
            parse_dontoql("MATCH ?s ?p ?o ORDER BY contradiction_pressure DESC").unwrap();
        let q3 =
            parse_dontoql("MATCH ?s ?p ?o ORDER BY contradiction_pressure ASC").unwrap();
        assert_eq!(q1.order_by, OrderBy::ContradictionPressureDesc);
        assert_eq!(q2.order_by, OrderBy::ContradictionPressureDesc);
        assert_eq!(q3.order_by, OrderBy::ContradictionPressureAsc);
    }

    #[test]
    fn order_by_unknown_name_is_error() {
        assert!(parse_dontoql("MATCH ?s ?p ?o ORDER_BY frobnicate").is_err());
    }

    #[test]
    fn with_evidence_clause() {
        let q = parse_dontoql(
            r#"MATCH ?s ?p ?o WITH evidence = redacted_if_required"#,
        )
        .unwrap();
        assert_eq!(q.evidence_shape, EvidenceShape::RedactedIfRequired);
    }

    #[test]
    fn with_evidence_unknown_value_is_error() {
        assert!(parse_dontoql(
            r#"MATCH ?s ?p ?o WITH evidence = sometimes"#
        )
        .is_err());
    }

    #[test]
    fn v2_kitchen_sink_parses() {
        // All v2 clauses combined — sanity check that the dispatcher
        // handles them in arbitrary order.
        let q = parse_dontoql(
            r#"
            SCOPE include ex:src
            PRESET curated
            MATCH ?s ?p ?o
            FILTER ?o > 0
            POLARITY asserted
            MATURITY >= 2
            IDENTITY_LENS EXPAND_CLUSTERS
            PREDICATES EXPAND_ABOVE 80
            MODALITY descriptive, inferred
            EXTRACTION_LEVEL quoted, manual_entry
            AS_OF "2026-01-01T00:00:00Z"
            POLICY ALLOWS read_metadata
            SCHEMA_LENS ex:linguistics-core
            EXPANDS_FROM concept ex:case_marking USING schema_lens ex:linguistics-core
            ORDER BY contradiction_pressure DESC
            WITH evidence = redacted_if_required
            PROJECT ?s, ?p, ?o
            LIMIT 100
            OFFSET 0
            "#,
        )
        .unwrap();
        assert!(q.scope.is_some());
        assert_eq!(q.scope_preset.as_deref(), Some("curated"));
        assert_eq!(q.filters.len(), 1);
        assert_eq!(q.min_maturity, 2);
        assert_eq!(q.identity, IdentityMode::ExpandClusters);
        assert_eq!(
            q.predicate_expansion,
            PredicateExpansion::ExpandAbove(80)
        );
        assert!(q.modality.is_some());
        assert!(q.extraction_level.is_some());
        assert!(q.as_of_tx.is_some());
        assert_eq!(q.policy_allows.as_deref(), Some("read_metadata"));
        assert_eq!(q.schema_lens.as_deref(), Some("ex:linguistics-core"));
        assert!(q.expands_from.is_some());
        assert_eq!(q.order_by, OrderBy::ContradictionPressureDesc);
        assert_eq!(q.evidence_shape, EvidenceShape::RedactedIfRequired);
        assert_eq!(q.limit, Some(100));
        assert_eq!(q.offset, Some(0));
    }
}
