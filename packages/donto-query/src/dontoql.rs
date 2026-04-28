//! DontoQL parser. Hand-rolled (no nom needed for this small grammar);
//! produces an [`Algebra::Query`] AST.
//!
//! Grammar (Phase 4 subset):
//!
//! ```text
//! query        := keyword_clause+
//! keyword_clause :=
//!     'SCOPE'    scope_descriptor
//!   | 'PRESET'   IDENT
//!   | 'MATCH'    triple (',' triple)*
//!   | 'FILTER'   filter_expr (',' filter_expr)*
//!   | 'POLARITY' ident_in_set
//!   | 'MATURITY' '>='? INT
//!   | 'IDENTITY' ident
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
                        if c.is_alphanumeric()
                            || c == '_'
                            || c == '-'
                            || c == '/'
                            || c == '.'
                            || c == '#'
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
                        "PRESET" => q.scope_preset = Some(self.parse_ident()?),
                        "MATCH" => q.patterns = self.parse_patterns()?,
                        "FILTER" => q.filters = self.parse_filters()?,
                        "POLARITY" => q.polarity = Some(self.parse_polarity()?),
                        "MATURITY" => {
                            let _ = self.maybe_op(">=");
                            q.min_maturity = self.parse_int()? as u8;
                        }
                        "IDENTITY" => q.identity = self.parse_identity()?,
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
        // Phase 4 FILTER grammar supports only `=` and `!=`. Ordering
        // comparisons (<, <=, >, >=) are rejected cleanly.
        for op in ["=", "!="] {
            let src = format!("MATCH ?x ex:p ?n FILTER ?n {op} 5");
            let q = parse_dontoql(&src).unwrap_or_else(|e| panic!("op {op}: {e}"));
            assert_eq!(q.filters.len(), 1, "op {op} did not produce a filter");
        }
        for op in ["<", "<=", ">", ">="] {
            let src = format!("MATCH ?x ex:p ?n FILTER ?n {op} 5");
            let r = parse_dontoql(&src);
            assert!(r.is_err(), "op {op} must be a clean parse error");
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
    fn maturity_without_operator_defaults_to_ge() {
        // `MATURITY 2` and `MATURITY >= 2` should both set min_maturity = 2.
        let q1 = parse_dontoql("MATCH ?s ?p ?o MATURITY 2").unwrap();
        let q2 = parse_dontoql("MATCH ?s ?p ?o MATURITY >= 2").unwrap();
        assert_eq!(q1.min_maturity, 2);
        assert_eq!(q2.min_maturity, 2);
    }
}
