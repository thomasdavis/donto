//!  / §6.7 entity extension (migration 0108): kind, external_ids,
//! identity_status, multilingual labels.

mod common;
use common::{connect, tag};

async fn ensure_symbol(c: &deadpool_postgres::Object, iri: &str) -> i64 {
    c.query_one(
        "select donto_ensure_symbol($1, null, null, null, null, null)",
        &[&iri],
    )
    .await
    .unwrap()
    .get(0)
}

#[tokio::test]
async fn entity_kind_default_null_then_settable() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ent-kind");
    let iri = format!("ent:{prefix}/x");
    let id = ensure_symbol(&c, &iri).await;

    let kind: Option<String> = c
        .query_one(
            "select entity_kind from donto_entity_symbol where symbol_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(kind.is_none());

    c.execute(
        "update donto_entity_symbol set entity_kind = 'lexeme' where symbol_id = $1",
        &[&id],
    )
    .await
    .unwrap();

    let kind2: Option<String> = c
        .query_one(
            "select entity_kind from donto_entity_symbol where symbol_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(kind2.as_deref(), Some("lexeme"));
}

#[tokio::test]
async fn entity_kind_check_constraint() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ent-kind-bad");
    let iri = format!("ent:{prefix}/x");
    let id = ensure_symbol(&c, &iri).await;

    let res = c
        .execute(
            "update donto_entity_symbol set entity_kind = 'lavafield' where symbol_id = $1",
            &[&id],
        )
        .await;
    assert!(res.is_err(), "invalid entity_kind rejected");
}

#[tokio::test]
async fn external_ids_round_trip() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ent-extids");
    let iri = format!("ent:{prefix}/lang/test");
    let id = ensure_symbol(&c, &iri).await;

    // Use unique-per-test external IDs to avoid collisions with other tests
    // that may register the same registry+id under a different symbol.
    let glot = format!("test-{}", uuid::Uuid::new_v4().simple());
    let iso = format!("test-{}", uuid::Uuid::new_v4().simple());

    c.execute(
        "select donto_add_external_id($1, 'glottolog', $2, 1.0)",
        &[&id, &glot],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_external_id($1, 'iso639-3', $2, 1.0)",
        &[&id, &iso],
    )
    .await
    .unwrap();

    let ext_id_count: i32 = c
        .query_one(
            "select jsonb_array_length(external_ids) from donto_entity_symbol \
             where symbol_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(ext_id_count, 2);

    let resolved: Option<i64> = c
        .query_one(
            "select donto_symbol_by_external_id('glottolog', $1)",
            &[&glot],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(resolved, Some(id));
}

#[tokio::test]
async fn identity_status_check_constraint() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ent-istat");
    let id = ensure_symbol(&c, &format!("ent:{prefix}/x")).await;
    let res = c
        .execute(
            "update donto_entity_symbol set identity_status = 'corrupted' where symbol_id = $1",
            &[&id],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn add_entity_label_with_language() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ent-label");
    let id = ensure_symbol(&c, &format!("ent:{prefix}/lang/y")).await;

    c.execute(
        "select donto_add_entity_label($1, 'Yalanji', 'en', 'Latn', 'preferred')",
        &[&id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_entity_label($1, 'Kuku Yalanji', 'en', 'Latn', 'alternate')",
        &[&id],
    )
    .await
    .unwrap();

    let n: i64 = c
        .query_one(
            "select count(*) from donto_entity_label where symbol_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2);
}

#[tokio::test]
async fn add_entity_label_idempotent_on_unique_tuple() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ent-label-idem");
    let id = ensure_symbol(&c, &format!("ent:{prefix}/lang/z")).await;

    c.execute(
        "select donto_add_entity_label($1, 'Yalanji', 'en', 'Latn', 'preferred')",
        &[&id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_entity_label($1, 'Yalanji', 'en', 'Latn', 'alternate')",
        &[&id],
    )
    .await
    .unwrap();

    let row = c
        .query_one(
            "select count(*), max(label_status) from donto_entity_label where symbol_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let (n, status): (i64, String) = (row.get(0), row.get(1));
    assert_eq!(n, 1, "tuple is unique; second call updates status");
    // Idempotent UPSERT: the most recent label_status wins.
    assert_eq!(status, "alternate", "second call overwrote status");
}

#[tokio::test]
async fn label_status_check_constraint() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ent-label-bad");
    let id = ensure_symbol(&c, &format!("ent:{prefix}/lang/q")).await;
    let res = c
        .execute(
            "select donto_add_entity_label($1, 'X', null, null, 'mythical')",
            &[&id],
        )
        .await;
    assert!(res.is_err());
}
