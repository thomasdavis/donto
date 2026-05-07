//! Evidence substrate: extraction run lifecycle.
//!
//!   * start creates a run in 'running' status
//!   * complete transitions to terminal status and sets completed_at
//!   * runs link to source revisions and contexts
//!   * annotation run_id FK is enforced

mod common;
use common::{connect, ctx, tag};

#[tokio::test]
async fn extraction_lifecycle() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ext-life").await;

    let iri = format!("test:doc/{}", tag("ext-life"));
    let doc_id = client
        .ensure_document(&iri, "text/plain", None, None, None)
        .await
        .unwrap();
    let rev_id = client
        .add_revision(doc_id, Some("Source text for extraction"), None, None)
        .await
        .unwrap();

    let run_id = client
        .start_extraction(
            Some("claude-sonnet-4-6"),
            Some("20250514"),
            Some(rev_id),
            Some(&ctx),
        )
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();

    // Verify initial state.
    let row = c
        .query_one(
            "select status, model_id, source_revision_id, completed_at \
             from donto_extraction_run where run_id = $1",
            &[&run_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("status"), "running");
    assert_eq!(
        row.get::<_, Option<String>>("model_id").as_deref(),
        Some("claude-sonnet-4-6")
    );
    assert_eq!(
        row.get::<_, Option<uuid::Uuid>>("source_revision_id"),
        Some(rev_id)
    );
    assert!(row
        .get::<_, Option<chrono::DateTime<chrono::Utc>>>("completed_at")
        .is_none());

    // Complete it.
    client
        .complete_extraction(run_id, "completed", Some(42), Some(100))
        .await
        .unwrap();

    let row = c
        .query_one(
            "select status, statements_emitted, annotations_emitted, completed_at \
             from donto_extraction_run where run_id = $1",
            &[&run_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("status"), "completed");
    assert_eq!(row.get::<_, i64>("statements_emitted"), 42);
    assert_eq!(row.get::<_, i64>("annotations_emitted"), 100);
    assert!(row
        .get::<_, Option<chrono::DateTime<chrono::Utc>>>("completed_at")
        .is_some());
}

#[tokio::test]
async fn extraction_run_without_source() {
    let client = pg_or_skip!(connect().await);

    let run_id = client
        .start_extraction(Some("gpt-4"), None, None, None)
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let row = c
        .query_one(
            "select source_revision_id, context from donto_extraction_run where run_id = $1",
            &[&run_id],
        )
        .await
        .unwrap();
    assert!(row
        .get::<_, Option<uuid::Uuid>>("source_revision_id")
        .is_none());
    assert!(row.get::<_, Option<String>>("context").is_none());
}

#[tokio::test]
async fn annotation_run_fk_enforced() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let iri = format!("test:doc/{}", tag("ann-runfk"));
    let doc_id = client
        .ensure_document(&iri, "text/plain", None, None, None)
        .await
        .unwrap();
    let rev_id = client
        .add_revision(doc_id, Some("text"), None, None)
        .await
        .unwrap();
    let span_id = client.create_char_span(rev_id, 0, 4, None).await.unwrap();

    let space_iri = format!("test:space/{}", tag("ann-runfk"));
    let space_id: uuid::Uuid = c
        .query_one("select donto_ensure_annotation_space($1)", &[&space_iri])
        .await
        .unwrap()
        .get(0);

    // A valid run_id works.
    let run_id = client
        .start_extraction(Some("test-model"), None, Some(rev_id), None)
        .await
        .unwrap();
    c.execute(
        "select donto_annotate_span($1, $2, $3, $4, $5, $6, $7)",
        &[
            &span_id,
            &space_id,
            &"test",
            &"val",
            &Option::<serde_json::Value>::None,
            &Option::<f64>::None,
            &run_id,
        ],
    )
    .await
    .unwrap();

    // A fake run_id fails.
    let fake_run = uuid::Uuid::new_v4();
    let err = c
        .execute(
            "select donto_annotate_span($1, $2, $3, $4, $5, $6, $7)",
            &[
                &span_id,
                &space_id,
                &"test2",
                &"val2",
                &Option::<serde_json::Value>::None,
                &Option::<f64>::None,
                &fake_run,
            ],
        )
        .await
        .err()
        .expect("fake run_id must violate FK");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("annotation_run_fk") || msg.contains("foreign key"),
        "expected FK violation, got: {msg}"
    );
}

#[tokio::test]
async fn failed_extraction_records_status() {
    let client = pg_or_skip!(connect().await);

    let run_id = client
        .start_extraction(Some("bad-model"), None, None, None)
        .await
        .unwrap();

    client
        .complete_extraction(run_id, "failed", Some(0), Some(0))
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let status: String = c
        .query_one(
            "select status from donto_extraction_run where run_id = $1",
            &[&run_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(status, "failed");
}
