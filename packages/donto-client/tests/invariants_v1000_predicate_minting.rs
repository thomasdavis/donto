//! v1000 / §6.9 predicate minting (migration 0110).

use serde_json::json;

mod common;
use common::{connect, tag};

async fn mint(
    c: &deadpool_postgres::Object,
    iri: &str,
    label: &str,
    definition: &str,
    examples: serde_json::Value,
    nearest: serde_json::Value,
) -> Result<String, tokio_postgres::Error> {
    c.query_one(
        "select donto_mint_predicate_candidate($1, $2, $3, 'Person', 'Place', \
            'genealogy', $4, $5, 'donto-native', 'tester', null, null)",
        &[&iri, &label, &definition, &examples, &nearest],
    )
    .await
    .map(|r| r.get(0))
}

#[tokio::test]
async fn mint_requires_label() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mint-label");
    let res = mint(
        &c,
        &format!("ex:{prefix}/p"),
        "",
        "definition",
        json!([{"subject": "ex:a", "object": "ex:b"}]),
        json!([]),
    )
    .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn mint_requires_definition() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mint-def");
    let res = mint(
        &c,
        &format!("ex:{prefix}/p"),
        "label",
        "",
        json!([{"subject": "ex:a", "object": "ex:b"}]),
        json!([]),
    )
    .await;
    assert!(res.is_err(), "empty definition rejected");
}

#[tokio::test]
async fn mint_requires_at_least_one_example() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mint-ex");
    let res = mint(
        &c,
        &format!("ex:{prefix}/p"),
        "label",
        "good definition",
        json!([]),
        json!([]),
    )
    .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn mint_succeeds_and_records_status_candidate() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mint-ok");
    let iri = format!("ex:{prefix}/bornIn");
    let returned = mint(
        &c,
        &iri,
        "Born in",
        "Subject was born in object place.",
        json!([{"subject": "ex:mary", "object": "ex:cornwall"}]),
        json!([{"predicate_id": "ex:wasBornIn", "similarity": 0.93}]),
    )
    .await
    .unwrap();
    assert_eq!(returned, iri);

    let status: String = c
        .query_one(
            "select minting_status from donto_predicate_descriptor where iri = $1",
            &[&iri],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(status, "candidate");

    let approved: bool = c
        .query_one("select donto_predicate_is_approved($1)", &[&iri])
        .await
        .unwrap()
        .get(0);
    assert!(!approved, "candidate is not approved");
}

#[tokio::test]
async fn approve_promotes_candidate() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mint-approve");
    let iri = format!("ex:{prefix}/p");
    mint(
        &c,
        &iri,
        "lbl",
        "def",
        json!([{"subject": "a", "object": "b"}]),
        json!([]),
    )
    .await
    .unwrap();

    let promoted: bool = c
        .query_one("select donto_approve_predicate($1, 'reviewer')", &[&iri])
        .await
        .unwrap()
        .get(0);
    assert!(promoted);

    let again: bool = c
        .query_one("select donto_approve_predicate($1, 'reviewer')", &[&iri])
        .await
        .unwrap()
        .get(0);
    assert!(!again, "second approve is no-op");

    assert!(c
        .query_one("select donto_predicate_is_approved($1)", &[&iri])
        .await
        .unwrap()
        .get::<_, bool>(0));
}

#[tokio::test]
async fn deprecate_records_reason_and_event() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mint-depr");
    let iri = format!("ex:{prefix}/p");
    mint(
        &c,
        &iri,
        "lbl",
        "def",
        json!([{"subject": "a", "object": "b"}]),
        json!([]),
    )
    .await
    .unwrap();

    let depr: bool = c
        .query_one(
            "select donto_deprecate_predicate($1, 'reviewer', 'duplicate of ex:wasBornIn')",
            &[&iri],
        )
        .await
        .unwrap()
        .get(0);
    assert!(depr);

    let n_events: i64 = c
        .query_one(
            "select count(*) from donto_event_log where target_kind = 'predicate_descriptor' \
             and target_id = $1",
            &[&iri],
        )
        .await
        .unwrap()
        .get(0);
    assert!(n_events >= 2, "create + deprecate events");
}
