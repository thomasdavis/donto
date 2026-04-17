//! Unit tests for every ingest format. No database required — these run
//! on every developer machine regardless of whether Postgres is up.
//!
//! Each test feeds a small, meaningful fragment through the parser and
//! asserts the derived [`StatementInput`] list matches expectations. This
//! is the first line of defense against parser drift.

use donto_client::{Object, Polarity};
use donto_ingest::{csv, jsonl, jsonld, nquads, property_graph, rdfxml, turtle};
use std::io::Write;

const CTX: &str = "ctx:test/unit";

fn write_temp(contents: &str, ext: &str) -> tempfile::NamedTempFile {
    let mut t = tempfile::Builder::new()
        .suffix(&format!(".{ext}"))
        .tempfile()
        .unwrap();
    t.write_all(contents.as_bytes()).unwrap();
    t
}

// ---------------------------------------------------------------------------
// N-Quads
// ---------------------------------------------------------------------------

#[test]
fn nquads_named_graph_maps_to_context() {
    let src = r#"<http://ex/a> <http://ex/p> <http://ex/b> <http://ex/g1> .
<http://ex/a> <http://ex/p> "literal" <http://ex/g2> .
"#;
    let f = write_temp(src, "nq");
    let out = nquads::parse_path(f.path(), CTX).unwrap();
    assert_eq!(out.len(), 2);
    // Graph IRI overrides the default context.
    assert_eq!(out[0].context, "http://ex/g1");
    assert_eq!(out[1].context, "http://ex/g2");
    assert_eq!(out[0].object, Object::iri("http://ex/b"));
    match &out[1].object {
        Object::Literal(l) => {
            assert_eq!(l.v, serde_json::Value::String("literal".into()));
            // rio emits the fully-qualified xsd:string IRI; either form is
            // acceptable but we pin what we actually get.
            assert!(
                l.dt.ends_with("#string") || l.dt == "xsd:string",
                "unexpected datatype {}",
                l.dt
            );
        }
        _ => panic!("expected literal"),
    }
}

#[test]
fn nquads_no_graph_falls_back_to_default_context() {
    let src = r#"<http://ex/a> <http://ex/p> <http://ex/b> .
"#;
    let f = write_temp(src, "nq");
    let out = nquads::parse_path(f.path(), CTX).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].context, CTX);
}

#[test]
fn nquads_blank_node_subject_preserves_prefix() {
    let src = r#"_:alice <http://ex/name> "Alice" .
"#;
    let f = write_temp(src, "nq");
    let out = nquads::parse_path(f.path(), CTX).unwrap();
    assert_eq!(out.len(), 1);
    assert!(
        out[0].subject.starts_with("_:"),
        "blank-node prefix must survive"
    );
}

#[test]
fn nquads_lang_tagged_literal_roundtrips_lang() {
    let src = r#"<http://ex/a> <http://ex/label> "hello"@en-GB .
"#;
    let f = write_temp(src, "nq");
    let out = nquads::parse_path(f.path(), CTX).unwrap();
    match &out[0].object {
        Object::Literal(l) => {
            // Rio normalises lang tags to lowercase — lock in that contract.
            assert_eq!(
                l.lang.as_deref().map(str::to_ascii_lowercase),
                Some("en-gb".into())
            );
            assert_eq!(l.dt, "rdf:langString");
        }
        _ => panic!("expected literal"),
    }
}

#[test]
fn nquads_malformed_returns_error_not_partial() {
    let src = "<this is not n-quads>\n";
    let f = write_temp(src, "nq");
    let r = nquads::parse_path(f.path(), CTX);
    assert!(r.is_err(), "malformed nquads must surface as Err");
}

// ---------------------------------------------------------------------------
// Turtle + TriG
// ---------------------------------------------------------------------------

#[test]
fn turtle_file_lands_in_default_context() {
    let src = r#"@prefix ex: <http://ex/> .
ex:alice ex:knows ex:bob ;
         ex:name "Alice" .
"#;
    let f = write_temp(src, "ttl");
    let out = turtle::parse_turtle_path(f.path(), CTX).unwrap();
    assert_eq!(out.len(), 2);
    for s in &out {
        assert_eq!(s.context, CTX, "turtle has no graph; must use default");
    }
}

#[test]
fn trig_named_graphs_become_contexts() {
    let src = r#"@prefix ex: <http://ex/> .
ex:g1 { ex:alice ex:knows ex:bob . }
ex:g2 { ex:alice ex:name "Alice" . }
"#;
    let f = write_temp(src, "trig");
    let out = turtle::parse_trig_path(f.path(), CTX).unwrap();
    assert_eq!(out.len(), 2);
    let ctxs: std::collections::HashSet<_> = out.iter().map(|s| s.context.clone()).collect();
    assert!(ctxs.contains("http://ex/g1"));
    assert!(ctxs.contains("http://ex/g2"));
}

// ---------------------------------------------------------------------------
// RDF/XML
// ---------------------------------------------------------------------------

#[test]
fn rdfxml_basic_description_parses() {
    let src = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:ex="http://ex/">
  <rdf:Description rdf:about="http://ex/a">
    <ex:knows rdf:resource="http://ex/b"/>
  </rdf:Description>
</rdf:RDF>
"#;
    let f = write_temp(src, "rdf");
    let out = rdfxml::parse_path(f.path(), CTX).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].subject, "http://ex/a");
    assert_eq!(out[0].object, Object::iri("http://ex/b"));
}

// ---------------------------------------------------------------------------
// JSON-LD
// ---------------------------------------------------------------------------

#[test]
fn jsonld_prefix_context_expands_ids() {
    let src = r#"{
  "@context": {"ex": "http://ex/"},
  "@id": "ex:alice",
  "ex:name": "Alice",
  "ex:knows": {"@id": "ex:bob"}
}"#;
    let f = write_temp(src, "jsonld");
    let out = jsonld::parse_path(f.path(), CTX).unwrap();
    let subjects: std::collections::HashSet<_> = out.iter().map(|s| s.subject.clone()).collect();
    assert!(
        subjects.contains("http://ex/alice"),
        "prefix ex: must be expanded to http://ex/; got {subjects:?}",
    );
}

#[test]
fn jsonld_graph_at_top_level_produces_multiple_subjects() {
    let src = r#"{
  "@context": {"ex": "http://ex/"},
  "@graph": [
    {"@id": "ex:a", "ex:p": "one"},
    {"@id": "ex:b", "ex:p": "two"}
  ]
}"#;
    let f = write_temp(src, "jsonld");
    let out = jsonld::parse_path(f.path(), CTX).unwrap();
    let subjects: std::collections::HashSet<_> = out.iter().map(|s| s.subject.clone()).collect();
    assert!(subjects.contains("http://ex/a"));
    assert!(subjects.contains("http://ex/b"));
}

// ---------------------------------------------------------------------------
// JSONL streaming
// ---------------------------------------------------------------------------

#[test]
fn jsonl_iri_and_literal_forms_both_parse() {
    let src = r#"{"s":"ex:a","p":"ex:knows","o":{"iri":"ex:b"},"c":"ctx:src"}
{"s":"ex:a","p":"ex:name","o":{"v":"Alice","dt":"xsd:string","lang":null},"c":"ctx:src"}
"#;
    let f = write_temp(src, "jsonl");
    let out = jsonl::parse_path(f.path(), CTX).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].context, "ctx:src");
    assert_eq!(out[0].object, Object::iri("ex:b"));
    match &out[1].object {
        Object::Literal(l) => assert_eq!(l.v, serde_json::Value::String("Alice".into())),
        _ => panic!("expected literal"),
    }
}

#[test]
fn jsonl_missing_context_falls_back_to_default() {
    let src = r#"{"s":"ex:a","p":"ex:p","o":{"iri":"ex:b"}}
"#;
    let f = write_temp(src, "jsonl");
    let out = jsonl::parse_path(f.path(), CTX).unwrap();
    assert_eq!(out[0].context, CTX);
}

#[test]
fn jsonl_bad_line_reported_as_error() {
    let src = "{this is not json}\n";
    let f = write_temp(src, "jsonl");
    let r = jsonl::parse_path(f.path(), CTX);
    assert!(r.is_err(), "malformed JSONL must surface as Err");
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

#[test]
fn csv_mapping_emits_one_statement_per_non_subject_column() {
    // Mapping:
    //   subject = id column, prefix = ex:user/
    //   columns: name (xsd:string), age (xsd:integer), homepage (iri)
    let mapping = csv::CsvMapping {
        default_context: CTX.into(),
        subject: csv::SubjectSource::Column {
            column: "id".into(),
            prefix: Some("ex:user/".into()),
        },
        columns: vec![
            csv::ColumnMap {
                column: "name".into(),
                predicate: "ex:name".into(),
                datatype: Some("xsd:string".into()),
                iri: false,
                iri_prefix: None,
            },
            csv::ColumnMap {
                column: "age".into(),
                predicate: "ex:age".into(),
                datatype: Some("xsd:integer".into()),
                iri: false,
                iri_prefix: None,
            },
            csv::ColumnMap {
                column: "homepage".into(),
                predicate: "ex:homepage".into(),
                datatype: None,
                iri: true,
                iri_prefix: None,
            },
        ],
        skip_blank: false,
    };

    let src = "id,name,age,homepage\n\
               42,Alice,30,http://alice.example\n\
               43,Bob,25,http://bob.example\n";
    let f = write_temp(src, "csv");
    let out = csv::parse_path(f.path(), &mapping).unwrap();
    // Two rows × three data columns = six statements.
    assert_eq!(out.len(), 6);
    let subjects: std::collections::HashSet<_> = out.iter().map(|s| s.subject.clone()).collect();
    assert!(subjects.contains("ex:user/42"));
    assert!(subjects.contains("ex:user/43"));
    // Every row carries the default context.
    assert!(out.iter().all(|s| s.context == CTX));
    // The homepage column must produce an IRI object, not a literal.
    let hp: &_ = out
        .iter()
        .find(|s| s.predicate == "ex:homepage")
        .expect("homepage row");
    match &hp.object {
        Object::Iri(_) => {}
        _ => panic!("iri=true column must produce Object::Iri"),
    }
}

#[test]
fn csv_skip_blank_drops_empty_cells() {
    let mapping = csv::CsvMapping {
        default_context: CTX.into(),
        subject: csv::SubjectSource::Column {
            column: "id".into(),
            prefix: Some("ex:u/".into()),
        },
        columns: vec![csv::ColumnMap {
            column: "name".into(),
            predicate: "ex:name".into(),
            datatype: Some("xsd:string".into()),
            iri: false,
            iri_prefix: None,
        }],
        skip_blank: true,
    };

    let src = "id,name\n1,Alice\n2,\n3,Bob\n";
    let f = write_temp(src, "csv");
    let out = csv::parse_path(f.path(), &mapping).unwrap();
    assert_eq!(out.len(), 2, "blank cell must be skipped");
    assert!(out.iter().all(|s| s.polarity == Polarity::Asserted));
}

// ---------------------------------------------------------------------------
// Property-graph JSON
// ---------------------------------------------------------------------------

#[test]
fn property_graph_nodes_labels_and_edges_are_reified() {
    let src = r#"{
  "nodes": [
    {"id":"alice","labels":["Person"],"props":{"name":"Alice"}},
    {"id":"bob","labels":["Person"],"props":{"name":"Bob"}}
  ],
  "edges": [
    {"id":"e1","from":"alice","to":"bob","type":"KNOWS","props":{"since":2010}}
  ]
}"#;
    let f = write_temp(src, "json");
    let out = property_graph::parse_path(f.path(), CTX, "ex:").unwrap();

    // For each node: at least one rdf:type and at least one prop statement.
    assert!(out
        .iter()
        .any(|s| s.subject == "ex:alice" && s.predicate.contains("type")));
    assert!(out
        .iter()
        .any(|s| s.subject == "ex:alice" && s.predicate == "ex:name"));

    // The edge is reified as its own event-node with the `edge/` prefix.
    let edge_subj = "ex:edge/e1";
    let edge_rows: Vec<_> = out.iter().filter(|s| s.subject == edge_subj).collect();
    assert!(
        edge_rows.len() >= 3,
        "edge should reify to ≥3 statements under {edge_subj}, got {}",
        edge_rows.len()
    );
    assert!(edge_rows.iter().any(|s| s.predicate == "ex:from"));
    assert!(edge_rows.iter().any(|s| s.predicate == "ex:to"));
    // The edge prop also lands on the reified event node.
    assert!(edge_rows.iter().any(|s| s.predicate == "ex:since"));
}
