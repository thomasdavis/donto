use anyhow::{anyhow, Result};
use donto_client::{Literal, Object, Polarity, StatementInput};
use rio_api::model::{GraphName, Quad, Subject, Term};
use rio_api::parser::QuadsParser;
use rio_turtle::NQuadsParser;
use std::io::BufRead;
use std::path::Path;

pub fn parse_path(path: &Path, default_context: &str) -> Result<Vec<StatementInput>> {
    let f = std::fs::File::open(path)?;
    parse_reader(std::io::BufReader::new(f), default_context)
}

pub fn parse_reader<R: BufRead>(reader: R, default_context: &str) -> Result<Vec<StatementInput>> {
    let mut out = Vec::new();
    let mut error: Option<String> = None;
    NQuadsParser::new(reader)
        .parse_all(&mut |q: Quad<'_>| -> Result<(), rio_turtle::TurtleError> {
            match quad_to_input(q, default_context) {
                Ok(s) => out.push(s),
                Err(e) => {
                    error = Some(e.to_string());
                }
            }
            Ok(())
        })
        .map_err(|e| anyhow!("nquads parse error: {e}"))?;
    if let Some(e) = error {
        return Err(anyhow!(e));
    }
    Ok(out)
}

pub(crate) fn quad_to_input(q: Quad<'_>, default_context: &str) -> Result<StatementInput> {
    let subject = match q.subject {
        Subject::NamedNode(n) => n.iri.to_string(),
        Subject::BlankNode(b) => format!("_:{}", b.id),
        Subject::Triple(_) => return Err(anyhow!("RDF-star not supported")),
    };
    let predicate = q.predicate.iri.to_string();
    let object = match q.object {
        Term::NamedNode(n) => Object::Iri(n.iri.to_string()),
        Term::BlankNode(b) => Object::Iri(format!("_:{}", b.id)),
        Term::Literal(lit) => Object::Literal(literal_from_rio(&lit)),
        Term::Triple(_) => return Err(anyhow!("RDF-star not supported")),
    };
    let context = match q.graph_name {
        None => default_context.to_string(),
        Some(GraphName::NamedNode(n)) => n.iri.to_string(),
        Some(GraphName::BlankNode(b)) => format!("_:{}", b.id),
    };
    Ok(StatementInput::new(subject, predicate, object)
        .with_context(context)
        .with_polarity(Polarity::Asserted))
}

pub(crate) fn literal_from_rio(lit: &rio_api::model::Literal<'_>) -> Literal {
    use rio_api::model::Literal as R;
    match lit {
        R::Simple { value } => Literal::string(*value),
        R::LanguageTaggedString { value, language } => Literal::lang_string(*value, *language),
        R::Typed { value, datatype } => Literal {
            v: serde_json::Value::String((*value).into()),
            dt: datatype.iri.to_string(),
            lang: None,
        },
    }
}
