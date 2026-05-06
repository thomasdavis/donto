//! Evidence substrate: temporal expressions.

use chrono::NaiveDate;

mod common;
use common::{connect, tag};

#[tokio::test]
async fn temporal_expression_lifecycle() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("temp-life");

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
        .add_revision(doc_id, Some("In 2023, the model was released."), None, None)
        .await
        .unwrap();
    let span_id = client
        .create_char_span(rev_id, 3, 7, Some("2023"))
        .await
        .unwrap();

    let expr_id: uuid::Uuid = c
        .query_one(
            "select donto_add_temporal_expression($1, $2, $3::date, $4::date, $5)",
            &[
                &span_id,
                &"2023",
                &NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                &NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                &"year",
            ],
        )
        .await
        .unwrap()
        .get(0);

    let row = c.query_one(
        "select raw_text, resolved_from, resolved_to, resolution from donto_temporal_expression where expression_id = $1",
        &[&expr_id],
    ).await.unwrap();
    assert_eq!(row.get::<_, String>("raw_text"), "2023");
    assert_eq!(
        row.get::<_, NaiveDate>("resolved_from"),
        NaiveDate::from_ymd_opt(2023, 1, 1).unwrap()
    );
    assert_eq!(
        row.get::<_, NaiveDate>("resolved_to"),
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
    );
    assert_eq!(row.get::<_, String>("resolution"), "year");
}

#[tokio::test]
async fn temporal_range_query() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("temp-range");

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
        .add_revision(doc_id, Some("1850 and 2023"), None, None)
        .await
        .unwrap();

    let s1 = client
        .create_char_span(rev_id, 0, 4, Some("1850"))
        .await
        .unwrap();
    let s2 = client
        .create_char_span(rev_id, 9, 13, Some("2023"))
        .await
        .unwrap();

    c.execute(
        "select donto_add_temporal_expression($1, '1850', '1850-01-01'::date, '1851-01-01'::date, 'year')",
        &[&s1],
    ).await.unwrap();
    c.execute(
        "select donto_add_temporal_expression($1, '2023', '2023-01-01'::date, '2024-01-01'::date, 'year')",
        &[&s2],
    ).await.unwrap();

    // Query for 1840-1860 should find "1850" but not "2023"
    let rows = c.query(
        "select * from donto_temporal_expressions_in_range('1840-01-01'::date, '1860-01-01'::date)",
        &[],
    ).await.unwrap();
    let raw_texts: Vec<String> = rows.iter().map(|r| r.get("raw_text")).collect();
    assert!(raw_texts.contains(&"1850".to_string()));
    assert!(!raw_texts.contains(&"2023".to_string()));
}

#[tokio::test]
async fn approximate_temporal_expression() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("temp-approx");

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
        .add_revision(doc_id, Some("circa 1850"), None, None)
        .await
        .unwrap();
    let span_id = client
        .create_char_span(rev_id, 0, 10, Some("circa 1850"))
        .await
        .unwrap();

    c.execute(
        "select donto_add_temporal_expression($1, 'circa 1850', '1845-01-01'::date, '1855-01-01'::date, 'approximate', null, 0.7::double precision)",
        &[&span_id],
    ).await.unwrap();

    let row = c
        .query_one(
            "select resolution, confidence from donto_temporal_expression where span_id = $1",
            &[&span_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("resolution"), "approximate");
    let conf: f64 = row.get("confidence");
    assert!((conf - 0.7).abs() < 1e-9);
}
