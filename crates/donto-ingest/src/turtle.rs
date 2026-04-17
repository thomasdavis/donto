//! Turtle and TriG.
//!
//! Turtle has no graph component → the entire file lands in `default_context`.
//! TriG has graph blocks → the graph IRI becomes the context per quad.

use anyhow::{anyhow, Result};
use donto_client::{Object, Polarity, StatementInput};
use rio_api::model::{GraphName, Quad, Subject, Term, Triple};
use rio_api::parser::{QuadsParser, TriplesParser};
use rio_turtle::{TriGParser, TurtleParser};
use std::io::BufRead;
use std::path::Path;

pub fn parse_turtle_path(path: &Path, default_context: &str) -> Result<Vec<StatementInput>> {
    let f = std::fs::File::open(path)?;
    parse_turtle_reader(std::io::BufReader::new(f), path.to_string_lossy().as_ref(), default_context)
}

pub fn parse_turtle_reader<R: BufRead>(reader: R, base_iri: &str, default_context: &str) -> Result<Vec<StatementInput>> {
    let _ = base_iri;       // base resolution not used in Phase 8 ingest
    let mut p = TurtleParser::new(reader, None);
    let mut out = Vec::new();
    let mut err: Option<String> = None;
    p.parse_all(&mut |t: Triple<'_>| -> Result<(), rio_turtle::TurtleError> {
        match triple_to_input(t, default_context) {
            Ok(s) => out.push(s),
            Err(e) => err = Some(e.to_string()),
        }
        Ok(())
    }).map_err(|e| anyhow!("turtle: {e}"))?;
    if let Some(e) = err { return Err(anyhow!(e)); }
    Ok(out)
}

pub fn parse_trig_path(path: &Path, default_context: &str) -> Result<Vec<StatementInput>> {
    let f = std::fs::File::open(path)?;
    parse_trig_reader(std::io::BufReader::new(f), path.to_string_lossy().as_ref(), default_context)
}

pub fn parse_trig_reader<R: BufRead>(reader: R, base_iri: &str, default_context: &str) -> Result<Vec<StatementInput>> {
    let _ = base_iri;
    let mut p = TriGParser::new(reader, None);
    let mut out = Vec::new();
    let mut err: Option<String> = None;
    p.parse_all(&mut |q: Quad<'_>| -> Result<(), rio_turtle::TurtleError> {
        match quad_to_input(q, default_context) {
            Ok(s) => out.push(s),
            Err(e) => err = Some(e.to_string()),
        }
        Ok(())
    }).map_err(|e| anyhow!("trig: {e}"))?;
    if let Some(e) = err { return Err(anyhow!(e)); }
    Ok(out)
}

fn triple_to_input(t: Triple<'_>, ctx: &str) -> Result<StatementInput> {
    let subject = match t.subject {
        Subject::NamedNode(n) => n.iri.to_string(),
        Subject::BlankNode(b) => format!("_:{}", b.id),
        Subject::Triple(_) => return Err(anyhow!("RDF-star unsupported")),
    };
    let predicate = t.predicate.iri.to_string();
    let object = match t.object {
        Term::NamedNode(n) => Object::Iri(n.iri.to_string()),
        Term::BlankNode(b) => Object::Iri(format!("_:{}", b.id)),
        Term::Literal(l)   => Object::Literal(crate::nquads::literal_from_rio(&l)),
        Term::Triple(_)    => return Err(anyhow!("RDF-star unsupported")),
    };
    Ok(StatementInput::new(subject, predicate, object)
        .with_context(ctx).with_polarity(Polarity::Asserted))
}

fn quad_to_input(q: Quad<'_>, default_ctx: &str) -> Result<StatementInput> {
    let subject = match q.subject {
        Subject::NamedNode(n) => n.iri.to_string(),
        Subject::BlankNode(b) => format!("_:{}", b.id),
        Subject::Triple(_) => return Err(anyhow!("RDF-star unsupported")),
    };
    let predicate = q.predicate.iri.to_string();
    let object = match q.object {
        Term::NamedNode(n) => Object::Iri(n.iri.to_string()),
        Term::BlankNode(b) => Object::Iri(format!("_:{}", b.id)),
        Term::Literal(l)   => Object::Literal(crate::nquads::literal_from_rio(&l)),
        Term::Triple(_)    => return Err(anyhow!("RDF-star unsupported")),
    };
    let ctx = match q.graph_name {
        None                          => default_ctx.to_string(),
        Some(GraphName::NamedNode(n)) => n.iri.to_string(),
        Some(GraphName::BlankNode(b)) => format!("_:{}", b.id),
    };
    Ok(StatementInput::new(subject, predicate, object)
        .with_context(ctx).with_polarity(Polarity::Asserted))
}
