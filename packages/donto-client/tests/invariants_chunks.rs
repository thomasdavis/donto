//! Evidence substrate: extraction chunks.

mod common;
use common::{connect, ctx, tag};

#[tokio::test]
async fn chunk_lifecycle() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let ctx = ctx(&client, "chunk-life").await;

    let doc_id = client
        .ensure_document(
            &format!("test:doc/{}", tag("chunk")),
            "text/plain",
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let rev_id = client
        .add_revision(
            doc_id,
            Some("A long document with many chunks of text."),
            None,
            None,
        )
        .await
        .unwrap();
    let run_id = client
        .start_extraction(Some("test-model"), None, Some(rev_id), Some(&ctx))
        .await
        .unwrap();

    // Add 3 chunks
    for i in 0..3 {
        let chunk_id: uuid::Uuid = c
            .query_one(
                "select donto_add_extraction_chunk($1, $2, $3, $4::int, $5::int, $6::int)",
                &[
                    &run_id,
                    &rev_id,
                    &i,
                    &(i * 100i32),
                    &((i + 1) * 100i32),
                    &500i32,
                ],
            )
            .await
            .unwrap()
            .get(0);
        assert!(chunk_id != uuid::Uuid::nil());
    }

    // Query chunks
    let rows = c
        .query("select * from donto_extraction_chunks($1)", &[&run_id])
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].get::<_, i32>("chunk_index"), 0);
    assert_eq!(rows[1].get::<_, i32>("chunk_index"), 1);
    assert_eq!(rows[2].get::<_, i32>("chunk_index"), 2);
    assert_eq!(rows[0].get::<_, Option<i32>>("start_offset"), Some(0));
    assert_eq!(rows[0].get::<_, Option<i32>>("end_offset"), Some(100));
}

#[tokio::test]
async fn chunk_upsert() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let ctx = ctx(&client, "chunk-ups").await;

    let doc_id = client
        .ensure_document(
            &format!("test:doc/{}", tag("chunk-ups")),
            "text/plain",
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let rev_id = client
        .add_revision(doc_id, Some("text"), None, None)
        .await
        .unwrap();
    let run_id = client
        .start_extraction(Some("m"), None, Some(rev_id), Some(&ctx))
        .await
        .unwrap();

    c.execute(
        "select donto_add_extraction_chunk($1, $2, 0)",
        &[&run_id, &rev_id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_extraction_chunk($1, $2, 0, null, null, null, null, 500)",
        &[&run_id, &rev_id],
    )
    .await
    .unwrap();

    let count: i64 = c
        .query_one(
            "select count(*) from donto_extraction_chunk where run_id = $1",
            &[&run_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 1, "upsert must not duplicate");

    let latency: Option<i32> = c
        .query_one(
            "select latency_ms from donto_extraction_chunk where run_id = $1 and chunk_index = 0",
            &[&run_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(latency, Some(500));
}

#[tokio::test]
async fn chunk_unique_per_run() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("chunk-uniq");
    let ctx = ctx(&client, "chunk-uniq").await;

    let doc_id = client
        .ensure_document(
            &format!("test:doc/{prefix}"),
            "text/plain",
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let rev_id = client
        .add_revision(doc_id, Some("text"), None, None)
        .await
        .unwrap();
    let run1 = client
        .start_extraction(Some("m"), None, Some(rev_id), Some(&ctx))
        .await
        .unwrap();
    let run2 = client
        .start_extraction(Some("m"), None, Some(rev_id), Some(&ctx))
        .await
        .unwrap();

    // Same chunk_index in different runs is OK
    c.execute(
        "select donto_add_extraction_chunk($1, $2, 0)",
        &[&run1, &rev_id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_extraction_chunk($1, $2, 0)",
        &[&run2, &rev_id],
    )
    .await
    .unwrap();

    let total: i64 = c
        .query_one(
            "select count(*) from donto_extraction_chunk where revision_id = $1",
            &[&rev_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(total, 2);
}
