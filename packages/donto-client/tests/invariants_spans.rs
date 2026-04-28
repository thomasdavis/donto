//! Evidence substrate: standoff spans over document revisions.
//!
//!   * char_offset spans enforce start <= end
//!   * spans_overlapping returns correct overlap set
//!   * surface_text is stored and queryable

mod common;
use common::{connect, tag};
use uuid::Uuid;

async fn setup_doc_and_rev(client: &donto_client::DontoClient, name: &str) -> (Uuid, Uuid) {
    let iri = format!("test:doc/{}", tag(name));
    let doc_id = client
        .ensure_document(&iri, "text/plain", None, None, None)
        .await
        .unwrap();
    let rev_id = client
        .add_revision(
            doc_id,
            Some("The quick brown fox jumps over the lazy dog."),
            None,
            None,
        )
        .await
        .unwrap();
    (doc_id, rev_id)
}

#[tokio::test]
async fn char_span_stores_offsets_and_surface() {
    let client = pg_or_skip!(connect().await);
    let (_, rev_id) = setup_doc_and_rev(&client, "span-basic").await;

    let span_id = client.create_char_span(rev_id, 4, 9, Some("quick")).await.unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let row = c
        .query_one(
            "select span_type, start_offset, end_offset, surface_text \
             from donto_span where span_id = $1",
            &[&span_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("span_type"), "char_offset");
    assert_eq!(row.get::<_, i32>("start_offset"), 4);
    assert_eq!(row.get::<_, i32>("end_offset"), 9);
    assert_eq!(row.get::<_, Option<String>>("surface_text").as_deref(), Some("quick"));
}

#[tokio::test]
async fn char_span_rejects_inverted_offsets() {
    let client = pg_or_skip!(connect().await);
    let (_, rev_id) = setup_doc_and_rev(&client, "span-invert").await;

    let err = client
        .create_char_span(rev_id, 10, 5, None)
        .await
        .err()
        .expect("inverted offsets must error");
    assert!(format!("{err:?}").contains("start"));
}

#[tokio::test]
async fn overlapping_spans_query() {
    let client = pg_or_skip!(connect().await);
    let (_, rev_id) = setup_doc_and_rev(&client, "span-overlap").await;

    // "The quick brown fox jumps over the lazy dog."
    //  0   4     10    16  20    26   31   35  39  43
    let s1 = client.create_char_span(rev_id, 0, 9, Some("The quick")).await.unwrap();
    let s2 = client.create_char_span(rev_id, 4, 15, Some("quick brown")).await.unwrap();
    let _s3 = client.create_char_span(rev_id, 20, 25, Some("jumps")).await.unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let rows = c
        .query(
            "select span_id from donto_spans_overlapping($1, $2, $3)",
            &[&rev_id, &3i32, &10i32],
        )
        .await
        .unwrap();
    let ids: Vec<Uuid> = rows.iter().map(|r| r.get(0)).collect();
    assert!(ids.contains(&s1), "s1 overlaps [3,10)");
    assert!(ids.contains(&s2), "s2 overlaps [3,10)");
    assert_eq!(ids.len(), 2, "s3 does not overlap [3,10)");
}

#[tokio::test]
async fn zero_length_span_allowed() {
    let client = pg_or_skip!(connect().await);
    let (_, rev_id) = setup_doc_and_rev(&client, "span-zero").await;

    let span_id = client.create_char_span(rev_id, 5, 5, None).await.unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let row = c
        .query_one(
            "select start_offset, end_offset from donto_span where span_id = $1",
            &[&span_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, i32>("start_offset"), 5);
    assert_eq!(row.get::<_, i32>("end_offset"), 5);
}
