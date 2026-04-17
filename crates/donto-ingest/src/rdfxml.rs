//! RDF/XML via rio_xml.

use anyhow::{anyhow, Result};
use donto_client::{Object, Polarity, StatementInput};
use rio_api::model::{Subject, Term, Triple};
use rio_api::parser::TriplesParser;
use rio_xml::RdfXmlParser;
use std::io::BufRead;
use std::path::Path;

pub fn parse_path(path: &Path, default_context: &str) -> Result<Vec<StatementInput>> {
    let f = std::fs::File::open(path)?;
    parse_reader(std::io::BufReader::new(f), default_context)
}

pub fn parse_reader<R: BufRead>(reader: R, default_context: &str) -> Result<Vec<StatementInput>> {
    let mut p = RdfXmlParser::new(reader, None);
    let mut out = Vec::new();
    let mut err: Option<String> = None;
    p.parse_all(&mut |t: Triple<'_>| -> Result<(), rio_xml::RdfXmlError> {
        let s = match t.subject {
            Subject::NamedNode(n) => n.iri.to_string(),
            Subject::BlankNode(b) => format!("_:{}", b.id),
            Subject::Triple(_)    => { err = Some("RDF-star unsupported".into()); return Ok(()); }
        };
        let p = t.predicate.iri.to_string();
        let o = match t.object {
            Term::NamedNode(n) => Object::Iri(n.iri.to_string()),
            Term::BlankNode(b) => Object::Iri(format!("_:{}", b.id)),
            Term::Literal(l)   => Object::Literal(crate::nquads::literal_from_rio(&l)),
            Term::Triple(_)    => { err = Some("RDF-star unsupported".into()); return Ok(()); }
        };
        out.push(StatementInput::new(s, p, o)
            .with_context(default_context).with_polarity(Polarity::Asserted));
        Ok(())
    }).map_err(|e| anyhow!("rdf/xml: {e}"))?;
    if let Some(e) = err { return Err(anyhow!(e)); }
    Ok(out)
}
