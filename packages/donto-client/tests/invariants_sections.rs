//! Evidence substrate: document sections, tables, and table cells.

mod common;
use common::{connect, tag};

#[tokio::test]
async fn section_hierarchy() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("sec-hier");

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
        .add_revision(doc_id, Some("Introduction\nMethods\nResults"), None, None)
        .await
        .unwrap();

    let s1: uuid::Uuid = c
        .query_one(
            "select donto_add_section($1, $2, $3::smallint)",
            &[&rev_id, &"Introduction", &1i16],
        )
        .await
        .unwrap()
        .get(0);
    let s2: uuid::Uuid = c
        .query_one(
            "select donto_add_section($1, $2, $3::smallint)",
            &[&rev_id, &"Methods", &1i16],
        )
        .await
        .unwrap()
        .get(0);
    let s2a: uuid::Uuid = c
        .query_one(
            "select donto_add_section($1, $2, $3::smallint, $4)",
            &[&rev_id, &"Data Collection", &2i16, &s2],
        )
        .await
        .unwrap()
        .get(0);

    assert_ne!(s1, s2);
    assert_ne!(s2, s2a);

    let parent: Option<uuid::Uuid> = c
        .query_one(
            "select parent_section_id from donto_document_section where section_id = $1",
            &[&s2a],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(parent, Some(s2));
}

#[tokio::test]
async fn section_no_self_parent() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("sec-self");

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

    let sec_id: uuid::Uuid = c
        .query_one("select donto_add_section($1, $2)", &[&rev_id, &"Test"])
        .await
        .unwrap()
        .get(0);

    let err = c
        .execute(
            "update donto_document_section set parent_section_id = $1 where section_id = $1",
            &[&sec_id],
        )
        .await
        .err()
        .expect("self-parent must error");
    assert!(format!("{err:?}").contains("no_self_parent"));
}

#[tokio::test]
async fn table_and_cells() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("tbl-cell");

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
        .add_revision(doc_id, Some("Model | MMLU\nMistral | 60.1%"), None, None)
        .await
        .unwrap();

    let tbl_id: uuid::Uuid = c
        .query_one(
            "select donto_add_table($1, $2, $3, $4::int, $5::int)",
            &[&rev_id, &"Table 2", &"Benchmark results", &3i32, &4i32],
        )
        .await
        .unwrap()
        .get(0);

    // Add header row
    c.execute(
        "select donto_add_table_cell($1, 0, 0, $2, null, null, true)",
        &[&tbl_id, &"Model"],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_table_cell($1, 0, 1, $2, null, null, true)",
        &[&tbl_id, &"MMLU"],
    )
    .await
    .unwrap();

    // Add data row
    let cell_id: uuid::Uuid = c
        .query_one(
            "select donto_add_table_cell($1, 1, 0, $2, $3, $4, false, null)",
            &[&tbl_id, &"Mistral 7B", &"Mistral 7B", &"Model"],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "select donto_add_table_cell($1, 1, 1, $2, $3, $4, false, $5::double precision)",
        &[&tbl_id, &"60.1%", &"Mistral 7B", &"MMLU", &60.1f64],
    )
    .await
    .unwrap();

    // Query cells
    let rows = c
        .query("select * from donto_table_cells($1)", &[&tbl_id])
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);

    let data_cell = rows
        .iter()
        .find(|r| r.get::<_, i32>("row_idx") == 1 && r.get::<_, i32>("col_idx") == 1)
        .unwrap();
    assert_eq!(
        data_cell.get::<_, Option<String>>("value").as_deref(),
        Some("60.1%")
    );
    assert!((data_cell.get::<_, Option<f64>>("value_numeric").unwrap() - 60.1).abs() < 0.01);
    assert_eq!(
        data_cell.get::<_, Option<String>>("col_header").as_deref(),
        Some("MMLU")
    );
    assert_eq!(
        data_cell.get::<_, Option<String>>("row_header").as_deref(),
        Some("Mistral 7B")
    );

    assert!(cell_id != uuid::Uuid::nil());
}

#[tokio::test]
async fn table_cell_upsert() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("tbl-ups");

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
        .add_revision(doc_id, Some("data"), None, None)
        .await
        .unwrap();
    let tbl_id: uuid::Uuid = c
        .query_one("select donto_add_table($1, $2)", &[&rev_id, &"T1"])
        .await
        .unwrap()
        .get(0);

    c.execute("select donto_add_table_cell($1, 0, 0, 'old')", &[&tbl_id])
        .await
        .unwrap();
    c.execute("select donto_add_table_cell($1, 0, 0, 'new')", &[&tbl_id])
        .await
        .unwrap();

    let val: String = c.query_one(
        "select value from donto_table_cell where table_id = $1 and row_idx = 0 and col_idx = 0",
        &[&tbl_id],
    ).await.unwrap().get(0);
    assert_eq!(val, "new", "upsert must update the value");

    let count: i64 = c
        .query_one(
            "select count(*) from donto_table_cell where table_id = $1",
            &[&tbl_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 1, "upsert must not duplicate");
}
