//! Evidence substrate: documents and revisions.
//!
//!   * ensure_document is idempotent
//!   * register_document upserts metadata
//!   * revisions auto-increment and deduplicate by content_hash
//!   * latest_revision returns the highest revision_number

mod common;
use common::{connect, tag};

#[tokio::test]
async fn ensure_document_is_idempotent() {
    let client = pg_or_skip!(connect().await);
    let iri = format!("test:doc/{}", tag("doc-idem"));

    let id1 = client
        .ensure_document(&iri, "text/plain", Some("Test Doc"), None, None)
        .await
        .unwrap();
    let id2 = client
        .ensure_document(&iri, "text/plain", Some("Test Doc"), None, None)
        .await
        .unwrap();
    assert_eq!(id1, id2, "ensure_document must be idempotent");
}

#[tokio::test]
async fn register_document_upserts_metadata() {
    let client = pg_or_skip!(connect().await);
    let iri = format!("test:doc/{}", tag("doc-upsert"));
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    c.execute(
        "select donto_register_document($1, $2, $3, $4, $5, $6)",
        &[
            &iri,
            &"text/html",
            &"Original",
            &"https://example.com/a",
            &Option::<String>::None,
            &serde_json::json!({"version": 1}),
        ],
    )
    .await
    .unwrap();

    c.execute(
        "select donto_register_document($1, $2, $3, $4, $5, $6)",
        &[
            &iri,
            &"text/html",
            &Option::<String>::None,
            &Option::<String>::None,
            &"en",
            &serde_json::json!({"version": 2}),
        ],
    )
    .await
    .unwrap();

    let row = c
        .query_one(
            "select label, source_url, language, metadata from donto_document where iri = $1",
            &[&iri],
        )
        .await
        .unwrap();
    let label: String = row.get("label");
    let source: String = row.get("source_url");
    let lang: String = row.get("language");
    let meta: serde_json::Value = row.get("metadata");

    assert_eq!(label, "Original", "label preserved from first insert");
    assert_eq!(source, "https://example.com/a", "source_url preserved");
    assert_eq!(lang, "en", "language set on second call");
    assert_eq!(meta["version"], 2, "metadata merged");
}

#[tokio::test]
async fn revision_auto_increments() {
    let client = pg_or_skip!(connect().await);
    let iri = format!("test:doc/{}", tag("rev-incr"));

    let doc_id = client
        .ensure_document(&iri, "text/plain", None, None, None)
        .await
        .unwrap();

    let r1 = client
        .add_revision(doc_id, Some("First content"), None, None)
        .await
        .unwrap();
    let r2 = client
        .add_revision(doc_id, Some("Second content"), None, None)
        .await
        .unwrap();

    assert_ne!(r1, r2, "different content → different revision");

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let nums: Vec<i32> = c
        .query(
            "select revision_number from donto_document_revision \
             where document_id = $1 order by revision_number",
            &[&doc_id],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(nums, vec![1, 2]);
}

#[tokio::test]
async fn revision_deduplicates_by_hash() {
    let client = pg_or_skip!(connect().await);
    let iri = format!("test:doc/{}", tag("rev-dedup"));

    let doc_id = client
        .ensure_document(&iri, "text/plain", None, None, None)
        .await
        .unwrap();

    let r1 = client
        .add_revision(doc_id, Some("Same content"), None, None)
        .await
        .unwrap();
    let r2 = client
        .add_revision(doc_id, Some("Same content"), None, None)
        .await
        .unwrap();
    assert_eq!(r1, r2, "identical content returns same revision_id");
}

#[tokio::test]
async fn latest_revision_returns_highest() {
    let client = pg_or_skip!(connect().await);
    let iri = format!("test:doc/{}", tag("rev-latest"));

    let doc_id = client
        .ensure_document(&iri, "text/plain", None, None, None)
        .await
        .unwrap();

    client
        .add_revision(doc_id, Some("v1"), None, None)
        .await
        .unwrap();
    let r2 = client
        .add_revision(doc_id, Some("v2"), None, None)
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let latest: uuid::Uuid = c
        .query_one("select donto_latest_revision($1)", &[&doc_id])
        .await
        .unwrap()
        .get(0);
    assert_eq!(latest, r2);
}
