//! v1000 / I3: append-only event log (migration 0090).

mod common;
use common::{connect, tag};

#[tokio::test]
async fn emit_records_a_row() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let _ = tag("evlog-emit");

    let id: i64 = c
        .query_one(
            "select donto_emit_event('alignment', 'aln_test_1', 'created', \
                'tester', '{\"foo\": \"bar\"}'::jsonb, null, null)",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(id > 0);

    let row = c
        .query_one(
            "select target_kind, target_id, event_type, actor, payload \
             from donto_event_log where event_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let (kind, tid, ev, act, payload): (String, String, String, String, serde_json::Value) =
        (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4));
    assert_eq!(kind, "alignment");
    assert_eq!(tid, "aln_test_1");
    assert_eq!(ev, "created");
    assert_eq!(act, "tester");
    assert_eq!(payload["foo"], "bar");
}

#[tokio::test]
async fn invalid_target_kind_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .query_one(
            "select donto_emit_event('not_a_kind', 't1', 'created', 'a', '{}'::jsonb, null, null)",
            &[],
        )
        .await;
    assert!(res.is_err(), "invalid target_kind must be rejected by CHECK");
}

#[tokio::test]
async fn invalid_event_type_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .query_one(
            "select donto_emit_event('alignment', 't1', 'metamorphosed', 'a', '{}'::jsonb, null, null)",
            &[],
        )
        .await;
    assert!(res.is_err(), "invalid event_type must be rejected by CHECK");
}

#[tokio::test]
async fn history_returns_chain_descending() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    // Use a distinct target id so we don't conflict with parallel tests.
    let target = format!("aln_history_{}", uuid::Uuid::new_v4().simple());

    for ev in &["created", "updated", "approved"] {
        c.execute(
            "select donto_emit_event('alignment', $1, $2, 'tester', '{}'::jsonb, null, null)",
            &[&target, ev],
        )
        .await
        .unwrap();
    }

    let rows = c
        .query(
            "select event_type from donto_event_history('alignment', $1, 10)",
            &[&target],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);

    let kinds: Vec<String> = rows.iter().map(|r| r.get::<_, String>(0)).collect();
    assert_eq!(kinds[0], "approved", "newest first");
    assert_eq!(kinds[1], "updated");
    assert_eq!(kinds[2], "created");
}

#[tokio::test]
async fn prior_event_chain_links() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let target = format!("aln_chain_{}", uuid::Uuid::new_v4().simple());

    let first: i64 = c
        .query_one(
            "select donto_emit_event('alignment', $1, 'created', 'a', '{}'::jsonb, null, null)",
            &[&target],
        )
        .await
        .unwrap()
        .get(0);
    let second: i64 = c
        .query_one(
            "select donto_emit_event('alignment', $1, 'updated', 'a', '{}'::jsonb, $2, null)",
            &[&target, &first],
        )
        .await
        .unwrap()
        .get(0);

    let prior: Option<i64> = c
        .query_one(
            "select prior_event_id from donto_event_log where event_id = $1",
            &[&second],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(prior, Some(first));
}

#[tokio::test]
async fn request_id_indexed_lookup() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let req = format!("req-{}", uuid::Uuid::new_v4().simple());

    c.execute(
        "select donto_emit_event('alignment', 'aln-rl', 'created', 'a', '{}'::jsonb, null, $1)",
        &[&req],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_emit_event('policy', 'pol-rl', 'created', 'a', '{}'::jsonb, null, $1)",
        &[&req],
    )
    .await
    .unwrap();

    let n: i64 = c
        .query_one(
            "select count(*) from donto_event_log where request_id = $1",
            &[&req],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2, "both events grouped by request_id");
}
