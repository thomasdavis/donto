//! JSON-LD subset.
//!
//! Phase 8 supports:
//!   * top-level @context with simple prefix bindings (`{"ex": "..."}`)
//!   * @id, @type
//!   * scalar property values (string → xsd:string, number → xsd:integer/xsd:decimal,
//!     bool → xsd:boolean, object with @id → IRI, object with @value → typed literal)
//!   * arrays of the above
//!   * @graph at top level (each entry becomes a subject in the named graph)
//!
//! Out of scope this phase: remote @context fetching, framing, expanded IRIs
//! beyond simple prefix substitution, list/set semantics, language maps.

use anyhow::{anyhow, Result};
use donto_client::{Literal, Object, Polarity, StatementInput};
use serde_json::Value as J;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Default)]
struct Ctx {
    prefixes: HashMap<String, String>,
    base: Option<String>,
}

impl Ctx {
    fn expand(&self, s: &str) -> String {
        if let Some((pfx, local)) = s.split_once(':') {
            if let Some(base) = self.prefixes.get(pfx) {
                return format!("{base}{local}");
            }
        }
        s.to_string()
    }
}

pub fn parse_path(path: &Path, default_context: &str) -> Result<Vec<StatementInput>> {
    let f = std::fs::File::open(path)?;
    let v: J = serde_json::from_reader(f)?;
    let mut out = Vec::new();
    parse_value(&v, default_context, &Ctx::default(), &mut out)?;
    Ok(out)
}

fn parse_value(v: &J, default_ctx: &str, parent_ctx: &Ctx, out: &mut Vec<StatementInput>) -> Result<()> {
    match v {
        J::Object(map) => {
            let mut ctx = Ctx { prefixes: parent_ctx.prefixes.clone(), base: parent_ctx.base.clone() };
            if let Some(c) = map.get("@context") { absorb_context(c, &mut ctx); }
            if let Some(g) = map.get("@graph") {
                if let J::Array(arr) = g {
                    for entry in arr { parse_subject(entry, default_ctx, &ctx, out)?; }
                } else {
                    parse_subject(g, default_ctx, &ctx, out)?;
                }
            } else {
                parse_subject(v, default_ctx, &ctx, out)?;
            }
        }
        J::Array(arr) => for entry in arr { parse_value(entry, default_ctx, parent_ctx, out)?; },
        _ => return Err(anyhow!("jsonld: top must be object or array")),
    }
    Ok(())
}

fn absorb_context(c: &J, ctx: &mut Ctx) {
    if let J::Object(m) = c {
        for (k, v) in m {
            if k == "@base" { if let Some(s) = v.as_str() { ctx.base = Some(s.into()); } continue; }
            if let Some(s) = v.as_str() { ctx.prefixes.insert(k.clone(), s.into()); }
        }
    }
}

fn parse_subject(v: &J, default_ctx: &str, ctx: &Ctx, out: &mut Vec<StatementInput>) -> Result<()> {
    let map = v.as_object().ok_or_else(|| anyhow!("jsonld: subject must be object"))?;
    let subject = map.get("@id").and_then(|x| x.as_str())
        .map(|s| ctx.expand(s)).unwrap_or_else(|| format!("_:b{}", uuid::Uuid::new_v4().simple()));
    let resolved_subject = ctx.base.as_deref().map(|b| if subject.starts_with("_:") { subject.clone() } else { format!("{b}{subject}") }).unwrap_or(subject);

    if let Some(t) = map.get("@type") {
        match t {
            J::String(s) => emit(out, &resolved_subject,
                "rdf:type", Object::iri(ctx.expand(s)), default_ctx),
            J::Array(arr) => for tt in arr {
                if let Some(s) = tt.as_str() {
                    emit(out, &resolved_subject, "rdf:type", Object::iri(ctx.expand(s)), default_ctx);
                }
            },
            _ => {}
        }
    }

    for (k, val) in map {
        if k.starts_with('@') { continue; }
        let predicate = ctx.expand(k);
        emit_value(out, &resolved_subject, &predicate, val, default_ctx, ctx)?;
    }
    Ok(())
}

fn emit_value(
    out: &mut Vec<StatementInput>,
    subject: &str,
    predicate: &str,
    v: &J,
    default_ctx: &str,
    ctx: &Ctx,
) -> Result<()> {
    match v {
        J::String(s)  => emit(out, subject, predicate, Object::lit(Literal::string(s)), default_ctx),
        J::Number(n)  => {
            let lit = if let Some(i) = n.as_i64() { Literal::integer(i) }
                      else { Literal { v: J::Number(n.clone()), dt: "xsd:decimal".into(), lang: None } };
            emit(out, subject, predicate, Object::lit(lit), default_ctx);
        }
        J::Bool(b)    => emit(out, subject, predicate, Object::lit(Literal {
            v: J::Bool(*b), dt: "xsd:boolean".into(), lang: None }), default_ctx),
        J::Object(m)  => {
            if let Some(id) = m.get("@id").and_then(|x| x.as_str()) {
                emit(out, subject, predicate, Object::iri(ctx.expand(id)), default_ctx);
            } else if let Some(val) = m.get("@value") {
                let dt = m.get("@type").and_then(|x| x.as_str()).map(|s| ctx.expand(s)).unwrap_or("xsd:string".into());
                let lang = m.get("@language").and_then(|x| x.as_str()).map(String::from);
                emit(out, subject, predicate, Object::lit(Literal { v: val.clone(), dt, lang }), default_ctx);
            } else {
                // nested subject: recurse to extract its triples and link via blank node id.
                let bid = format!("_:b{}", uuid::Uuid::new_v4().simple());
                let mut sub = m.clone();
                sub.entry("@id".to_string()).or_insert_with(|| J::String(bid.clone()));
                let nested = J::Object(sub);
                parse_subject(&nested, default_ctx, ctx, out)?;
                emit(out, subject, predicate, Object::iri(bid), default_ctx);
            }
        }
        J::Array(arr) => for x in arr { emit_value(out, subject, predicate, x, default_ctx, ctx)?; },
        J::Null => {}
    }
    Ok(())
}

fn emit(out: &mut Vec<StatementInput>, s: &str, p: &str, o: Object, ctx: &str) {
    out.push(StatementInput::new(s, p, o).with_context(ctx).with_polarity(Polarity::Asserted));
}
