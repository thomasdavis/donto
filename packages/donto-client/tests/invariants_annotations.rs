//! Evidence substrate: annotation spaces, annotations, and edges.
//!
//!   * annotation spaces are idempotent
//!   * annotations attach feature-value pairs to spans
//!   * edges connect annotations (dependency arcs, coref links)
//!   * annotations_for_span filters by space and feature

mod common;
use common::{connect, tag};
use uuid::Uuid;

async fn setup_span(client: &donto_client::DontoClient, name: &str) -> Uuid {
    let iri = format!("test:doc/{}", tag(name));
    let doc_id = client
        .ensure_document(&iri, "text/plain", None, None, None)
        .await
        .unwrap();
    let rev_id = client
        .add_revision(doc_id, Some("The cat sat on the mat"), None, None)
        .await
        .unwrap();
    client
        .create_char_span(rev_id, 0, 3, Some("The"))
        .await
        .unwrap()
}

#[tokio::test]
async fn annotation_space_is_idempotent() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let iri = format!("test:space/{}", tag("sp-idem"));
    let id1: Uuid = c
        .query_one(
            "select donto_ensure_annotation_space($1, $2, $3, $4)",
            &[&iri, &"UD POS", &"universaldependencies.org", &"2.0"],
        )
        .await
        .unwrap()
        .get(0);
    let id2: Uuid = c
        .query_one(
            "select donto_ensure_annotation_space($1, $2, $3, $4)",
            &[&iri, &"UD POS", &"universaldependencies.org", &"2.0"],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(id1, id2);
}

#[tokio::test]
async fn annotate_span_stores_feature_value() {
    let client = pg_or_skip!(connect().await);
    let span_id = setup_span(&client, "ann-basic").await;
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let space_iri = format!("test:space/{}", tag("ann-basic"));
    let space_id: Uuid = c
        .query_one("select donto_ensure_annotation_space($1)", &[&space_iri])
        .await
        .unwrap()
        .get(0);

    let ann_id: Uuid = c
        .query_one(
            "select donto_annotate_span($1, $2, $3, $4, $5, $6, $7)",
            &[
                &span_id,
                &space_id,
                &"upos",
                &"DET",
                &Option::<serde_json::Value>::None,
                &0.95f64,
                &Option::<Uuid>::None,
            ],
        )
        .await
        .unwrap()
        .get(0);

    let row = c
        .query_one(
            "select feature, value, confidence from donto_annotation where annotation_id = $1",
            &[&ann_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("feature"), "upos");
    assert_eq!(
        row.get::<_, Option<String>>("value").as_deref(),
        Some("DET")
    );
    let conf: f64 = row.get("confidence");
    assert!((conf - 0.95).abs() < 1e-9);
}

#[tokio::test]
async fn annotations_for_span_filters_by_space_and_feature() {
    let client = pg_or_skip!(connect().await);
    let span_id = setup_span(&client, "ann-filter").await;
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let space_a_iri = format!("test:space/{}/a", tag("ann-filt"));
    let space_b_iri = format!("test:space/{}/b", tag("ann-filt"));
    let space_a: Uuid = c
        .query_one("select donto_ensure_annotation_space($1)", &[&space_a_iri])
        .await
        .unwrap()
        .get(0);
    let space_b: Uuid = c
        .query_one("select donto_ensure_annotation_space($1)", &[&space_b_iri])
        .await
        .unwrap()
        .get(0);

    // Two annotations in space_a, one in space_b.
    for (sp, feat, val) in [
        (space_a, "upos", "DET"),
        (space_a, "deprel", "det"),
        (space_b, "ner", "O"),
    ] {
        c.execute(
            "select donto_annotate_span($1, $2, $3, $4)",
            &[&span_id, &sp, &feat, &val],
        )
        .await
        .unwrap();
    }

    // All annotations for the span.
    let all: i64 = c
        .query_one(
            "select count(*) from donto_annotations_for_span($1)",
            &[&span_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(all, 3);

    // Filtered by space_a.
    let in_a: i64 = c
        .query_one(
            "select count(*) from donto_annotations_for_span($1, $2)",
            &[&span_id, &space_a],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(in_a, 2);

    // Filtered by space_a + feature "upos".
    let upos: i64 = c
        .query_one(
            "select count(*) from donto_annotations_for_span($1, $2, $3)",
            &[&span_id, &space_a, &"upos"],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(upos, 1);
}

#[tokio::test]
async fn annotation_edge_links_two_annotations() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let iri = format!("test:doc/{}", tag("ann-edge"));
    let doc_id = client
        .ensure_document(&iri, "text/plain", None, None, None)
        .await
        .unwrap();
    let rev_id = client
        .add_revision(doc_id, Some("The cat sat"), None, None)
        .await
        .unwrap();

    let span_the = client
        .create_char_span(rev_id, 0, 3, Some("The"))
        .await
        .unwrap();
    let span_cat = client
        .create_char_span(rev_id, 4, 7, Some("cat"))
        .await
        .unwrap();

    let space_iri = format!("test:space/{}", tag("ann-edge"));
    let space_id: Uuid = c
        .query_one("select donto_ensure_annotation_space($1)", &[&space_iri])
        .await
        .unwrap()
        .get(0);

    let ann_the: Uuid = c
        .query_one(
            "select donto_annotate_span($1, $2, $3, $4)",
            &[&span_the, &space_id, &"upos", &"DET"],
        )
        .await
        .unwrap()
        .get(0);
    let ann_cat: Uuid = c
        .query_one(
            "select donto_annotate_span($1, $2, $3, $4)",
            &[&span_cat, &space_id, &"upos", &"NOUN"],
        )
        .await
        .unwrap()
        .get(0);

    let edge_id: Uuid = c
        .query_one(
            "select donto_link_annotations($1, $2, $3, $4)",
            &[&ann_the, &ann_cat, &space_id, &"det"],
        )
        .await
        .unwrap()
        .get(0);

    // Query outgoing edges from ann_the.
    let rows = c
        .query("select * from donto_edges_from($1)", &[&ann_the])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, Uuid>("edge_id"), edge_id);
    assert_eq!(rows[0].get::<_, Uuid>("target_annotation_id"), ann_cat);
    assert_eq!(rows[0].get::<_, String>("relation"), "det");

    // Query incoming edges to ann_cat.
    let rows = c
        .query("select * from donto_edges_to($1)", &[&ann_cat])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, Uuid>("source_annotation_id"), ann_the);
}

#[tokio::test]
async fn annotation_edge_rejects_self_link() {
    let client = pg_or_skip!(connect().await);
    let span_id = setup_span(&client, "ann-self").await;
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let space_iri = format!("test:space/{}", tag("ann-self"));
    let space_id: Uuid = c
        .query_one("select donto_ensure_annotation_space($1)", &[&space_iri])
        .await
        .unwrap()
        .get(0);
    let ann: Uuid = c
        .query_one(
            "select donto_annotate_span($1, $2, $3, $4)",
            &[&span_id, &space_id, &"upos", &"NOUN"],
        )
        .await
        .unwrap()
        .get(0);

    let err = c
        .execute(
            "select donto_link_annotations($1, $2, $3, $4)",
            &[&ann, &ann, &space_id, &"self"],
        )
        .await
        .err()
        .expect("self-link must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("source and target must differ") || msg.contains("no_self"),
        "expected self-link rejection, got: {msg}"
    );
}
