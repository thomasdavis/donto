//!  / I10: dataset releases (migration 0094).

mod common;
use common::{connect, tag};

async fn make_release(
    client: &donto_client::DontoClient,
    name: &str,
) -> Result<uuid::Uuid, donto_client::Error> {
    let c = client.pool().get().await.unwrap();
    c.query_one(
        "insert into donto_dataset_release (release_name, release_version, query_spec) \
         values ($1, $2, '{\"q\": \"select 1\"}'::jsonb) returning release_id",
        &[&name, &"0.1.0"],
    )
    .await
    .map(|r| r.get(0))
    .map_err(donto_client::Error::Postgres)
}

#[tokio::test]
async fn release_round_trip() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let name = tag("rel-rt");

    let id = make_release(&client, &name).await.unwrap();
    let row = c
        .query_one(
            "select release_name, release_version, reproducibility_status, visibility \
             from donto_dataset_release where release_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let (n, v, r, vis): (String, String, String, String) =
        (row.get(0), row.get(1), row.get(2), row.get(3));
    assert_eq!(n, name);
    assert_eq!(v, "0.1.0");
    assert_eq!(r, "reproducible");
    assert_eq!(vis, "private");
}

#[tokio::test]
async fn release_name_version_unique() {
    let client = pg_or_skip!(connect().await);
    let name = tag("rel-uniq");
    make_release(&client, &name).await.unwrap();
    let res = make_release(&client, &name).await;
    assert!(res.is_err(), "duplicate (name, version) must be rejected");
}

#[tokio::test]
async fn release_artifact_unique_per_format() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let name = tag("rel-art");
    let release_id = make_release(&client, &name).await.unwrap();

    c.execute(
        "insert into donto_release_artifact (release_id, format, storage_uri) values ($1, 'cldf', 's3://bkt/a.zip')",
        &[&release_id],
    )
    .await
    .unwrap();

    let res = c
        .execute(
            "insert into donto_release_artifact (release_id, format, storage_uri) values ($1, 'cldf', 's3://bkt/a.zip')",
            &[&release_id],
        )
        .await;
    assert!(res.is_err(), "(release_id, format) is unique");
}

#[tokio::test]
async fn release_seal_emits_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let name = tag("rel-seal");
    let release_id = make_release(&client, &name).await.unwrap();

    let sealed: bool = c
        .query_one("select donto_seal_release($1, 'tester')", &[&release_id])
        .await
        .unwrap()
        .get(0);
    assert!(sealed);

    let again: bool = c
        .query_one("select donto_seal_release($1, 'tester')", &[&release_id])
        .await
        .unwrap()
        .get(0);
    assert!(!again, "second seal returns false (already sealed)");

    let n: i64 = c
        .query_one(
            "select count(*) from donto_event_log \
             where target_kind = 'release' and target_id = $1::text",
            &[&release_id.to_string()],
        )
        .await
        .unwrap()
        .get(0);
    assert!(n >= 1, "seal emitted event");
}

#[tokio::test]
async fn release_invalid_status_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let res = c
        .execute(
            "insert into donto_dataset_release (release_name, query_spec, reproducibility_status) \
             values ('bad', '{}'::jsonb, 'totally-stable')",
            &[],
        )
        .await;
    assert!(res.is_err(), "invalid reproducibility_status rejected");
}

#[tokio::test]
async fn release_summary_view() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let name = tag("rel-summary");
    let release_id = make_release(&client, &name).await.unwrap();
    c.execute(
        "insert into donto_release_artifact (release_id, format, storage_uri) values ($1, 'jsonl', 's3://x/y.jsonl')",
        &[&release_id],
    )
    .await
    .unwrap();
    c.execute(
        "insert into donto_release_artifact (release_id, format, storage_uri) values ($1, 'cldf', 's3://x/y.cldf')",
        &[&release_id],
    )
    .await
    .unwrap();

    let row = c
        .query_one(
            "select release_name, artifact_count from donto_release_summary($1)",
            &[&release_id],
        )
        .await
        .unwrap();
    let (n, count): (String, i64) = (row.get(0), row.get(1));
    assert_eq!(n, name);
    assert_eq!(count, 2);
}
