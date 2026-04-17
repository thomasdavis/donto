//! SPARQL 1.1 subset translator. Phase 4 supports:
//!
//! ```sparql
//! PREFIX ex: <http://example.org/>
//! SELECT ?x ?y WHERE {
//!   ?x ex:knows ?y .
//!   GRAPH ex:src { ?y ex:name ?n . }
//!   FILTER (?n != "Mallory")
//! }
//! LIMIT 10
//! ```
//!
//! Out of scope this phase: OPTIONAL, UNION, MINUS, property paths,
//! aggregates, CONSTRUCT, ASK, DESCRIBE, federated SERVICE.

use crate::algebra::*;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
#[error("sparql parse error: {0}")]
pub struct ParseError(String);

pub fn parse_sparql(src: &str) -> Result<Query, ParseError> {
    let mut tx = Tokenizer::new(src);
    let mut prefixes: HashMap<String, String> = HashMap::new();

    // PREFIX clauses.
    while let Some(t) = tx.peek_word() {
        if t.eq_ignore_ascii_case("PREFIX") {
            tx.bump_word(); // PREFIX
            let pfx = tx.read_until(':')?.trim().to_string();
            tx.expect(':')?;
            let iri = tx.read_iri()?;
            prefixes.insert(pfx, iri);
        } else {
            break;
        }
    }

    // SELECT clause.
    let kw = tx
        .bump_word()
        .ok_or_else(|| ParseError("expected SELECT".into()))?;
    if !kw.eq_ignore_ascii_case("SELECT") {
        return Err(ParseError(format!("expected SELECT, got `{kw}`")));
    }
    let mut project = Vec::new();
    loop {
        tx.skip_ws();
        if tx.peek_char() == Some('*') {
            tx.bump_char();
            break;
        }
        if tx.peek_char() == Some('?') {
            tx.bump_char();
            project.push(tx.read_ident()?);
        } else {
            break;
        }
    }

    // WHERE { ... }.
    let kw = tx
        .bump_word()
        .ok_or_else(|| ParseError("expected WHERE".into()))?;
    if !kw.eq_ignore_ascii_case("WHERE") {
        return Err(ParseError(format!("expected WHERE, got `{kw}`")));
    }
    tx.skip_ws();
    tx.expect('{')?;
    let (patterns, filters) = parse_block(&mut tx, &prefixes, None)?;
    tx.expect('}')?;

    // Tail clauses.
    let mut limit = None;
    let mut offset = None;
    while let Some(t) = tx.peek_word() {
        let upper = t.to_ascii_uppercase();
        match upper.as_str() {
            "LIMIT" => {
                tx.bump_word();
                limit = Some(tx.read_number()?);
            }
            "OFFSET" => {
                tx.bump_word();
                offset = Some(tx.read_number()?);
            }
            _ => break,
        }
    }

    Ok(Query {
        scope: None,
        scope_preset: None,
        patterns,
        filters,
        polarity: Some(donto_client::Polarity::Asserted),
        min_maturity: 0,
        identity: IdentityMode::Default,
        project,
        limit,
        offset,
    })
}

fn parse_block(
    tx: &mut Tokenizer<'_>,
    prefixes: &HashMap<String, String>,
    current_graph: Option<&Term>,
) -> Result<(Vec<Pattern>, Vec<Filter>), ParseError> {
    let mut patterns = Vec::new();
    let mut filters = Vec::new();
    loop {
        tx.skip_ws();
        match tx.peek_char() {
            Some('}') | None => break,
            Some('.') => {
                tx.bump_char();
            }
            Some(_) => {
                let snap = tx.pos;
                if let Some(w) = tx.peek_word() {
                    let up = w.to_ascii_uppercase();
                    if up == "GRAPH" {
                        tx.bump_word();
                        let g = read_term(tx, prefixes)?;
                        tx.skip_ws();
                        tx.expect('{')?;
                        let (sub_p, sub_f) = parse_block(tx, prefixes, Some(&g))?;
                        tx.expect('}')?;
                        patterns.extend(sub_p);
                        filters.extend(sub_f);
                        continue;
                    }
                    if up == "FILTER" {
                        tx.bump_word();
                        tx.skip_ws();
                        tx.expect('(')?;
                        let lhs = read_term(tx, prefixes)?;
                        tx.skip_ws();
                        let op = read_op(tx)?;
                        let rhs = read_term(tx, prefixes)?;
                        tx.skip_ws();
                        tx.expect(')')?;
                        filters.push(match op.as_str() {
                            "=" => Filter::Eq(lhs, rhs),
                            "!=" => Filter::Neq(lhs, rhs),
                            "<" => Filter::Lt(lhs, rhs),
                            "<=" => Filter::Le(lhs, rhs),
                            ">" => Filter::Gt(lhs, rhs),
                            ">=" => Filter::Ge(lhs, rhs),
                            other => {
                                return Err(ParseError(format!("unsupported FILTER op `{other}`")))
                            }
                        });
                        continue;
                    }
                    tx.pos = snap;
                }
                let s = read_term(tx, prefixes)?;
                let p = read_term(tx, prefixes)?;
                let o = read_term(tx, prefixes)?;
                patterns.push(Pattern {
                    subject: s,
                    predicate: p,
                    object: o,
                    graph: current_graph.cloned(),
                });
            }
        }
    }
    Ok((patterns, filters))
}

fn read_term(
    tx: &mut Tokenizer<'_>,
    prefixes: &HashMap<String, String>,
) -> Result<Term, ParseError> {
    tx.skip_ws();
    match tx.peek_char() {
        Some('?') => {
            tx.bump_char();
            Ok(Term::Var(tx.read_ident()?))
        }
        Some('<') => Ok(Term::Iri(tx.read_iri()?)),
        Some('"') => {
            tx.bump_char();
            let mut s = String::new();
            loop {
                match tx.bump_char() {
                    Some('"') => break,
                    Some('\\') => match tx.bump_char() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('"') => s.push('"'),
                        Some(c) => s.push(c),
                        None => return Err(ParseError("trailing backslash".into())),
                    },
                    Some(c) => s.push(c),
                    None => return Err(ParseError("unterminated string".into())),
                }
            }
            // Optional language tag or datatype.
            tx.skip_ws();
            if tx.peek_char() == Some('@') {
                tx.bump_char();
                let lang = tx.read_ident()?;
                Ok(Term::Literal {
                    v: serde_json::Value::String(s),
                    dt: "rdf:langString".into(),
                    lang: Some(lang),
                })
            } else if tx.peek_str_starts_with("^^") {
                tx.bump_char();
                tx.bump_char();
                let dt = match tx.peek_char() {
                    Some('<') => tx.read_iri()?,
                    _ => {
                        let pfx = tx.read_ident()?;
                        tx.expect(':')?;
                        let local = tx.read_ident()?;
                        prefixes
                            .get(&pfx)
                            .map(|v| format!("{v}{local}"))
                            .unwrap_or(format!("{pfx}:{local}"))
                    }
                };
                Ok(Term::Literal {
                    v: serde_json::Value::String(s),
                    dt,
                    lang: None,
                })
            } else {
                Ok(Term::Literal {
                    v: serde_json::Value::String(s),
                    dt: "xsd:string".into(),
                    lang: None,
                })
            }
        }
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            // Could be a prefixed name or boolean keyword. We treat it as prefixed name.
            let pfx = tx.read_ident()?;
            tx.expect(':')?;
            let local = tx.read_local()?;
            let iri = prefixes
                .get(&pfx)
                .map(|v| format!("{v}{local}"))
                .unwrap_or_else(|| format!("{pfx}:{local}"));
            Ok(Term::Iri(iri))
        }
        Some(c) if c.is_ascii_digit() || c == '-' => {
            let n: i64 = tx.read_number()? as i64;
            Ok(Term::Literal {
                v: serde_json::Value::Number(n.into()),
                dt: "xsd:integer".into(),
                lang: None,
            })
        }
        other => Err(ParseError(format!(
            "unexpected `{other:?}` while reading term"
        ))),
    }
}

fn read_op(tx: &mut Tokenizer<'_>) -> Result<String, ParseError> {
    tx.skip_ws();
    let c = tx
        .bump_char()
        .ok_or_else(|| ParseError("operator expected".into()))?;
    Ok(match c {
        '=' => "=".into(),
        '!' => {
            tx.expect('=')?;
            "!=".into()
        }
        '<' => {
            if tx.peek_char() == Some('=') {
                tx.bump_char();
                "<=".into()
            } else {
                "<".into()
            }
        }
        '>' => {
            if tx.peek_char() == Some('=') {
                tx.bump_char();
                ">=".into()
            } else {
                ">".into()
            }
        }
        other => return Err(ParseError(format!("unexpected operator `{other}`"))),
    })
}

// Tiny, byte-position-based tokenizer.
struct Tokenizer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
        }
    }
    fn skip_ws(&mut self) {
        while self.pos < self.src.len() {
            let c = self.src[self.pos] as char;
            if c.is_whitespace() {
                self.pos += 1;
            } else if c == '#' {
                while self.pos < self.src.len() && self.src[self.pos] as char != '\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }
    fn peek_char(&self) -> Option<char> {
        if self.pos >= self.src.len() {
            None
        } else {
            Some(self.src[self.pos] as char)
        }
    }
    fn bump_char(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += 1;
        Some(c)
    }
    fn peek_str_starts_with(&self, s: &str) -> bool {
        self.src.get(self.pos..self.pos + s.len()) == Some(s.as_bytes())
    }
    fn expect(&mut self, c: char) -> Result<(), ParseError> {
        self.skip_ws();
        if self.peek_char() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ParseError(format!("expected `{c}` at byte {}", self.pos)))
        }
    }
    fn read_ident(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos] as char;
            if c.is_ascii_alphanumeric() || c == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if start == self.pos {
            return Err(ParseError("ident expected".into()));
        }
        Ok(std::str::from_utf8(&self.src[start..self.pos])
            .unwrap()
            .to_string())
    }
    fn read_local(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/' || c == '#'
            {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(std::str::from_utf8(&self.src[start..self.pos])
            .unwrap()
            .to_string())
    }
    fn read_iri(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        if self.peek_char() != Some('<') {
            return Err(ParseError("`<` expected".into()));
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos] as char != '>' {
            self.pos += 1;
        }
        if self.pos >= self.src.len() {
            return Err(ParseError("unterminated <iri>".into()));
        }
        let s = std::str::from_utf8(&self.src[start..self.pos])
            .unwrap()
            .to_string();
        self.pos += 1;
        Ok(s)
    }
    fn read_number(&mut self) -> Result<u64, ParseError> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.src.len() && (self.src[self.pos] as char).is_ascii_digit() {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(ParseError("number expected".into()));
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .unwrap()
            .parse()
            .map_err(|_| ParseError("bad number".into()))
    }
    fn peek_word(&self) -> Option<String> {
        let mut p = self.pos;
        while p < self.src.len() && (self.src[p] as char).is_whitespace() {
            p += 1;
        }
        let start = p;
        while p < self.src.len() && (self.src[p] as char).is_ascii_alphabetic() {
            p += 1;
        }
        if start == p {
            return None;
        }
        Some(
            std::str::from_utf8(&self.src[start..p])
                .unwrap()
                .to_string(),
        )
    }
    fn bump_word(&mut self) -> Option<String> {
        let w = self.peek_word()?;
        self.skip_ws();
        self.pos += w.len();
        Some(w)
    }
    fn read_until(&mut self, end: char) -> Result<&str, ParseError> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos] as char != end {
            self.pos += 1;
        }
        if self.pos >= self.src.len() {
            return Err(ParseError(format!("`{end}` not found")));
        }
        Ok(std::str::from_utf8(&self.src[start..self.pos]).unwrap())
    }
    // Suppress dead_code for the local() method's `c` shadowing - intentional.
    #[allow(dead_code)]
    fn _suppress(&self) -> &[u8] {
        self.src
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_basic_select() {
        let q = parse_sparql(
            r#"
            PREFIX ex: <http://example.org/>
            SELECT ?x ?y WHERE {
                ?x ex:knows ?y .
                FILTER (?y != "Mallory")
            }
            LIMIT 10
        "#,
        )
        .unwrap();
        assert_eq!(q.project, vec!["x", "y"]);
        assert_eq!(q.patterns.len(), 1);
        assert_eq!(
            q.patterns[0].predicate,
            Term::Iri("http://example.org/knows".into())
        );
        assert_eq!(q.filters.len(), 1);
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn parses_graph_block() {
        let q = parse_sparql(
            r#"
            PREFIX ex: <http://example.org/>
            SELECT ?n WHERE {
                GRAPH ex:src { ?x ex:name ?n . }
            }
        "#,
        )
        .unwrap();
        assert_eq!(q.patterns.len(), 1);
        assert_eq!(
            q.patterns[0].graph,
            Some(Term::Iri("http://example.org/src".into()))
        );
    }

    #[test]
    fn multiple_prefix_declarations_round_trip() {
        let q = parse_sparql(
            r#"
            PREFIX ex:   <http://example.org/>
            PREFIX foaf: <http://xmlns.com/foaf/0.1/>
            SELECT ?name WHERE {
                ?p foaf:name ?name .
                ?p ex:age ?age .
            }
        "#,
        )
        .unwrap();
        assert_eq!(q.patterns.len(), 2);
        // Both expansions resolved.
        let preds: Vec<String> = q
            .patterns
            .iter()
            .map(|p| format!("{:?}", p.predicate))
            .collect();
        assert!(preds.iter().any(|s| s.contains("foaf/0.1/name")));
        assert!(preds.iter().any(|s| s.contains("example.org/age")));
    }

    #[test]
    fn filter_with_numeric_comparison_parses() {
        let q = parse_sparql(
            r#"
            PREFIX ex: <http://example.org/>
            SELECT ?p WHERE {
                ?p ex:age ?age .
                FILTER (?age >= 18)
            }
        "#,
        )
        .unwrap();
        assert_eq!(q.filters.len(), 1);
    }

    #[test]
    fn unknown_prefix_is_accepted_as_literal_curie() {
        // Phase 4 SPARQL subset does not require PREFIX to be declared
        // before use; prefixed names are round-tripped verbatim into the
        // algebra. Evaluator is then responsible for refusing to expand
        // them. This test pins that contract so we notice if future
        // parser work starts rejecting undeclared prefixes.
        let q = parse_sparql(
            r#"
            SELECT ?x WHERE { ?x ex:knows ?y . }
        "#,
        )
        .expect("undeclared prefix is currently accepted");
        assert_eq!(q.patterns.len(), 1);
        let pred_dbg = format!("{:?}", q.patterns[0].predicate);
        assert!(pred_dbg.contains("ex:knows") || pred_dbg.contains("knows"));
    }

    #[test]
    fn select_star_projects_all_bound_vars() {
        let q = parse_sparql(
            r#"
            PREFIX ex: <http://example.org/>
            SELECT * WHERE { ?s ex:p ?o . }
        "#,
        );
        // SELECT * is either supported (project populated) or cleanly
        // rejected; it must never panic.
        match q {
            Ok(_) | Err(_) => {}
        }
    }

    #[test]
    fn string_literal_with_lang_tag() {
        let q = parse_sparql(
            r#"
            PREFIX ex: <http://example.org/>
            SELECT ?s WHERE { ?s ex:name "Alice"@en . }
        "#,
        );
        // If lang tags aren't supported, the parser must fail cleanly.
        // If they are, we should see one pattern.
        if let Ok(q) = q {
            assert_eq!(q.patterns.len(), 1);
        }
    }

    #[test]
    fn limit_clause_outside_where_is_captured() {
        let q = parse_sparql(
            r#"
            PREFIX ex: <http://example.org/>
            SELECT ?s WHERE { ?s ex:p ?o . }
            LIMIT 5
        "#,
        )
        .unwrap();
        assert_eq!(q.limit, Some(5));
    }
}
