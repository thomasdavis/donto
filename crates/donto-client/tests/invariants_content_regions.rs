//! Evidence substrate: content regions (images, charts, code blocks, etc.)

mod common;
use common::{connect, tag};

#[tokio::test]
async fn content_region_lifecycle() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("cr-life");

    let doc_id = client.ensure_document(&format!("test:doc/{prefix}"), "text/plain", None, None, None).await.unwrap();
    let rev_id = client.add_revision(doc_id, Some("text with figure"), None, None).await.unwrap();

    let region_id: uuid::Uuid = c.query_one(
        "select donto_add_content_region($1, 'image', $2, $3, $4)",
        &[&rev_id, &"Figure 1", &"Performance comparison chart", &"Bar chart showing MMLU scores"],
    ).await.unwrap().get(0);

    let row = c.query_one(
        "select region_type, label, caption, alt_text from donto_content_region where region_id = $1",
        &[&region_id],
    ).await.unwrap();
    assert_eq!(row.get::<_, String>("region_type"), "image");
    assert_eq!(row.get::<_, Option<String>>("label").as_deref(), Some("Figure 1"));
    assert_eq!(row.get::<_, Option<String>>("caption").as_deref(), Some("Performance comparison chart"));
    assert_eq!(row.get::<_, Option<String>>("alt_text").as_deref(), Some("Bar chart showing MMLU scores"));
}

#[tokio::test]
async fn content_region_types_validated() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("cr-type");

    let doc_id = client.ensure_document(&format!("test:doc/{prefix}"), "text/plain", None, None, None).await.unwrap();
    let rev_id = client.add_revision(doc_id, Some("text"), None, None).await.unwrap();

    // Valid types work
    for t in ["image", "chart", "diagram", "code_block", "formula", "screenshot"] {
        c.execute(
            "insert into donto_content_region (revision_id, region_type) values ($1, $2)",
            &[&rev_id, &t],
        ).await.unwrap();
    }

    // Invalid type fails
    let err = c.execute(
        "insert into donto_content_region (revision_id, region_type) values ($1, 'invalid')",
        &[&rev_id],
    ).await.err().expect("invalid region_type must error");
    assert!(format!("{err:?}").contains("region_type"));
}

#[tokio::test]
async fn content_region_with_section() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("cr-sec");

    let doc_id = client.ensure_document(&format!("test:doc/{prefix}"), "text/plain", None, None, None).await.unwrap();
    let rev_id = client.add_revision(doc_id, Some("Results section with figure"), None, None).await.unwrap();

    let sec_id: uuid::Uuid = c.query_one(
        "select donto_add_section($1, 'Results')", &[&rev_id],
    ).await.unwrap().get(0);

    let region_id: uuid::Uuid = c.query_one(
        "select donto_add_content_region($1, 'chart', 'Figure 4', null, null, null, $2)",
        &[&rev_id, &sec_id],
    ).await.unwrap().get(0);

    let stored_sec: Option<uuid::Uuid> = c.query_one(
        "select section_id from donto_content_region where region_id = $1",
        &[&region_id],
    ).await.unwrap().get(0);
    assert_eq!(stored_sec, Some(sec_id));
}
