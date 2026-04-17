//! Minimal N-Quads ingester. Streams via `rio_turtle::NQuadsParser`,
//! batches statements, and posts them to the donto client.

use anyhow::{anyhow, Result};
use donto_client::{DontoClient, Literal, Object, Polarity, StatementInput};
use rio_api::model::{GraphName, Quad, Subject, Term};
use rio_api::parser::QuadsParser;
use rio_turtle::NQuadsParser;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub async fn ingest_file(
    client: &DontoClient,
    path: &Path,
    default_context: &str,
    batch_size: usize,
) -> Result<usize> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut parser = NQuadsParser::new(reader);

    let mut buf: Vec<StatementInput> = Vec::with_capacity(batch_size);
    let mut total = 0usize;

    // rio's parser callback is sync. We collect into batches and flush
    // outside the callback.
    let mut error: Option<String> = None;
    parser.parse_all(&mut |q: Quad<'_>| -> Result<(), rio_turtle::TurtleError> {
        match quad_to_input(q, default_context) {
            Ok(s)  => buf.push(s),
            Err(e) => { error = Some(e.to_string()); },
        }
        Ok(())
    }).map_err(|e| anyhow!("nquads parse error: {e}"))?;

    if let Some(e) = error { return Err(anyhow!("nquads conversion error: {e}")); }

    for chunk in buf.chunks(batch_size) {
        let n = client.assert_batch(chunk).await?;
        total += n;
    }
    Ok(total)
}

fn quad_to_input(q: Quad<'_>, default_context: &str) -> Result<StatementInput> {
    let subject = match q.subject {
        Subject::NamedNode(n) => n.iri.to_string(),
        Subject::BlankNode(b) => format!("_:{}", b.id),
        Subject::Triple(_) => return Err(anyhow!("RDF-star not supported in Phase 0")),
    };
    let predicate = q.predicate.iri.to_string();

    let object = match q.object {
        Term::NamedNode(n) => Object::Iri(n.iri.to_string()),
        Term::BlankNode(b) => Object::Iri(format!("_:{}", b.id)),
        Term::Literal(lit) => Object::Literal(literal_from_rio(&lit)),
        Term::Triple(_) => return Err(anyhow!("RDF-star not supported in Phase 0")),
    };

    let context = match q.graph_name {
        None => default_context.to_string(),
        Some(GraphName::NamedNode(n)) => n.iri.to_string(),
        Some(GraphName::BlankNode(b)) => format!("_:{}", b.id),
    };

    Ok(StatementInput::new(subject, predicate, object)
        .with_context(context)
        .with_polarity(Polarity::Asserted)
        .with_maturity(0))
}

fn literal_from_rio(lit: &rio_api::model::Literal<'_>) -> Literal {
    use rio_api::model::Literal as R;
    match lit {
        R::Simple { value } => Literal::string(*value),
        R::LanguageTaggedString { value, language } =>
            Literal::lang_string(*value, *language),
        R::Typed { value, datatype } => Literal {
            v: serde_json::Value::String((*value).into()),
            dt: datatype.iri.to_string(),
            lang: None,
        },
    }
}
