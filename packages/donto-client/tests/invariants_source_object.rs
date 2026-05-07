//!  / §6.1–§6.3: source object + source version + anchor kind
//! registry (migrations 0095, 0096, 0097).

mod common;
use common::{connect, tag};

#[tokio::test]
async fn register_source_requires_policy() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("src-policy-required");

    let res = c
        .query_one(
            "select donto_register_source($1, 'pdf', null)",
            &[&format!("src:{prefix}/no-policy")],
        )
        .await;
    assert!(res.is_err(), "policy_id is required for register_source");
}

#[tokio::test]
async fn register_source_with_policy_succeeds() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("src-with-policy");

    let id: uuid::Uuid = c
        .query_one(
            "select donto_register_source($1, 'pdf', 'policy:default/public', \
                'application/pdf', 'a label', 'https://example.com/x.pdf')",
            &[&format!("src:{prefix}/with-policy")],
        )
        .await
        .unwrap()
        .get(0);
    assert_ne!(id, uuid::Uuid::nil());
}

#[tokio::test]
async fn source_status_check_constraint() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("src-status-check");
    c.execute(
        "select donto_register_source($1, 'pdf', 'policy:default/public')",
        &[&format!("src:{prefix}/x")],
    )
    .await
    .unwrap();
    let res = c
        .execute(
            "update donto_document set status = 'fictional' where iri = $1",
            &[&format!("src:{prefix}/x")],
        )
        .await;
    assert!(res.is_err(), "invalid status rejected");
}

#[tokio::test]
async fn source_kind_check_constraint() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("src-kind-check");
    let res = c
        .execute(
            "select donto_register_source($1, 'unicorn', 'policy:default/public')",
            &[&format!("src:{prefix}/uni")],
        )
        .await;
    assert!(res.is_err(), "invalid source_kind rejected by CHECK");
}

#[tokio::test]
async fn source_version_kind_default_and_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ver-kind");

    let doc_id: uuid::Uuid = c
        .query_one(
            "select donto_register_source($1, 'pdf', 'policy:default/public')",
            &[&format!("src:{prefix}/d")],
        )
        .await
        .unwrap()
        .get(0);

    let rev_id: uuid::Uuid = c
        .query_one(
            "select donto_add_revision_typed($1, 'ocr', 'hello world')",
            &[&doc_id],
        )
        .await
        .unwrap()
        .get(0);

    let kind: String = c
        .query_one(
            "select version_kind from donto_document_revision where revision_id = $1",
            &[&rev_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(kind, "ocr");

    let res = c
        .execute(
            "update donto_document_revision set version_kind = 'mythical' where revision_id = $1",
            &[&rev_id],
        )
        .await;
    assert!(res.is_err(), "version_kind CHECK rejects unknown");
}

#[tokio::test]
async fn revision_lineage_walk() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ver-lineage");

    let doc_id: uuid::Uuid = c
        .query_one(
            "select donto_register_source($1, 'pdf', 'policy:default/public')",
            &[&format!("src:{prefix}/d")],
        )
        .await
        .unwrap()
        .get(0);

    let raw: uuid::Uuid = c
        .query_one(
            "select donto_add_revision_typed($1, 'raw', 'raw bytes')",
            &[&doc_id],
        )
        .await
        .unwrap()
        .get(0);
    let ocr: uuid::Uuid = c
        .query_one(
            "select donto_add_revision_typed($1, 'ocr', 'ocr text', null, null, '{}'::jsonb, $2)",
            &[&doc_id, &vec![raw]],
        )
        .await
        .unwrap()
        .get(0);
    let norm: uuid::Uuid = c
        .query_one(
            "select donto_add_revision_typed($1, 'normalized', 'normalised text', null, null, '{}'::jsonb, $2)",
            &[&doc_id, &vec![ocr]],
        )
        .await
        .unwrap()
        .get(0);

    let rows = c
        .query(
            "select revision_id, depth from donto_revision_lineage($1) order by depth",
            &[&norm],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 3, "lineage should be 3 deep");
    let depths: Vec<i32> = rows.iter().map(|r| r.get::<_, i32>(1)).collect();
    assert_eq!(depths, vec![0, 1, 2]);
}

#[tokio::test]
async fn anchor_kinds_seeded() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let n: i64 = c
        .query_one(
            "select count(*) from donto_anchor_kind where is_active = true",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(n >= 13, "expected 13  anchor kinds, got {n}");
}

#[tokio::test]
async fn anchor_validate_each_kind_passes_minimal_locator() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    // Valid minimal locators per kind.
    let cases: &[(&str, &str)] = &[
        ("whole_source", "{}"),
        ("char_span", "{\"start\": 0, \"end\": 5}"),
        (
            "page_box",
            "{\"page\": 1, \"x\": 0.1, \"y\": 0.1, \"w\": 0.1, \"h\": 0.1}",
        ),
        ("image_box", "{\"x\": 0, \"y\": 0, \"w\": 100, \"h\": 100}"),
        ("media_time", "{\"start_ms\": 0, \"end_ms\": 1000}"),
        ("table_cell", "{\"row_id\": \"r1\", \"column\": \"c1\"}"),
        ("csv_row", "{\"row_index\": 1, \"columns\": [\"a\",\"b\"]}"),
        ("json_pointer", "{\"pointer\": \"/x/0\"}"),
        ("xml_xpath", "{\"xpath\": \"//a\"}"),
        ("html_css", "{\"selector\": \"div.x\"}"),
        (
            "token_range",
            "{\"sentence_id\": \"s1\", \"start\": 0, \"end\": 3}",
        ),
        ("annotation_id", "{\"annotation_id\": \"a1\"}"),
        (
            "archive_field",
            "{\"record_id\": \"r1\", \"field_name\": \"title\"}",
        ),
    ];
    for (kind, locator) in cases {
        let v: serde_json::Value = serde_json::from_str(locator).unwrap();
        let valid: bool = c
            .query_one("select donto_validate_anchor_locator($1, $2)", &[kind, &v])
            .await
            .unwrap()
            .get(0);
        assert!(valid, "kind {kind} minimal locator should validate");
    }
}

#[tokio::test]
async fn anchor_assert_locator_raises() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .execute(
            "select donto_assert_anchor_locator('char_span', '{\"start\": 0}'::jsonb)",
            &[],
        )
        .await;
    assert!(res.is_err(), "missing required key must raise");
}
