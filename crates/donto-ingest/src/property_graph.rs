//! Property-graph JSON ingester. Accepts a Neo4j-style export shape:
//!
//! ```json
//! { "nodes":[ {"id":"alice","labels":["Person"],"props":{"name":"Alice"}} ],
//!   "edges":[ {"id":"e1","from":"alice","to":"bob","type":"KNOWS","props":{"since":2010}} ] }
//! ```
//!
//! Mapping per PRD §24:
//!   * each node label → `<id> rdf:type ex:<Label>`
//!   * each node prop  → `<id> ex:<key> <literal>`
//!   * each edge       → reified event-node: ex:<id> ex:from <from>; ex:<id> ex:to <to>; ex:<id> rdf:type ex:<Type>; properties on the event.

use anyhow::Result;
use donto_client::{Literal, Object, Polarity, StatementInput};
use serde::Deserialize;
use serde_json::Value as J;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct Doc {
    #[serde(default)]
    nodes: Vec<Node>,
    #[serde(default)]
    edges: Vec<Edge>,
}

#[derive(Debug, Deserialize)]
struct Node {
    id: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    props: serde_json::Map<String, J>,
}

#[derive(Debug, Deserialize)]
struct Edge {
    id: String,
    from: String,
    to: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    props: serde_json::Map<String, J>,
}

pub fn parse_path(path: &Path, default_context: &str, prefix: &str) -> Result<Vec<StatementInput>> {
    let f = std::fs::File::open(path)?;
    let doc: Doc = serde_json::from_reader(f)?;
    Ok(parse_doc(&doc, default_context, prefix))
}

pub fn parse_doc(doc: &Doc, default_context: &str, prefix: &str) -> Vec<StatementInput> {
    let mut out = Vec::new();
    for n in &doc.nodes {
        let id = format!("{prefix}{}", n.id);
        for l in &n.labels {
            push(&mut out, &id, "rdf:type", Object::iri(format!("{prefix}{l}")), default_context);
        }
        for (k, v) in &n.props {
            push_value(&mut out, &id, &format!("{prefix}{k}"), v, default_context);
        }
    }
    for e in &doc.edges {
        let id = format!("{prefix}edge/{}", e.id);
        push(&mut out, &id, "rdf:type",            Object::iri(format!("{prefix}{}", e.ty)), default_context);
        push(&mut out, &id, &format!("{prefix}from"), Object::iri(format!("{prefix}{}", e.from)), default_context);
        push(&mut out, &id, &format!("{prefix}to"),   Object::iri(format!("{prefix}{}", e.to)), default_context);
        for (k, v) in &e.props {
            push_value(&mut out, &id, &format!("{prefix}{k}"), v, default_context);
        }
    }
    out
}

fn push(out: &mut Vec<StatementInput>, s: &str, p: &str, o: Object, ctx: &str) {
    out.push(StatementInput::new(s, p, o).with_context(ctx).with_polarity(Polarity::Asserted));
}

fn push_value(out: &mut Vec<StatementInput>, s: &str, p: &str, v: &J, ctx: &str) {
    match v {
        J::String(s2) => push(out, s, p, Object::lit(Literal::string(s2)), ctx),
        J::Number(n)  => {
            let lit = if let Some(i) = n.as_i64() { Literal::integer(i) }
                      else { Literal { v: J::Number(n.clone()), dt: "xsd:decimal".into(), lang: None } };
            push(out, s, p, Object::lit(lit), ctx);
        }
        J::Bool(b)    => push(out, s, p, Object::lit(Literal { v: J::Bool(*b), dt: "xsd:boolean".into(), lang: None }), ctx),
        J::Null       => {}
        J::Array(arr) => for x in arr { push_value(out, s, p, x, ctx); },
        J::Object(_)  => push(out, s, p, Object::lit(Literal { v: v.clone(), dt: "donto:json".into(), lang: None }), ctx),
    }
}
