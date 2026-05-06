//! Evidence substrate: cross-system entity aliases.

mod common;
use common::connect;
use common::tag;

#[tokio::test]
async fn register_and_resolve_alias() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("ea-basic");

    let alias = format!("{prefix}/doi:10.1234");
    let canonical = format!("{prefix}/arxiv:1234");

    c.execute(
        "select donto_register_entity_alias($1, $2, $3, $4::double precision)",
        &[&alias, &canonical, &"doi", &1.0f64],
    )
    .await
    .unwrap();

    let resolved: String = c
        .query_one("select donto_resolve_entity($1)", &[&alias])
        .await
        .unwrap()
        .get(0);
    assert_eq!(resolved, canonical);
}

#[tokio::test]
async fn unregistered_resolves_to_self() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let iri = format!("test:unregistered/{}", tag("ea-self"));
    let resolved: String = c
        .query_one("select donto_resolve_entity($1)", &[&iri])
        .await
        .unwrap()
        .get(0);
    assert_eq!(resolved, iri, "unregistered entity must resolve to itself");
}

#[tokio::test]
async fn no_self_alias() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let iri = format!("test:entity/{}", tag("ea-no-self"));
    let err = c
        .execute("select donto_register_entity_alias($1, $1)", &[&iri])
        .await
        .err()
        .expect("self-alias must error");
    assert!(format!("{err:?}").contains("alias_distinct") || format!("{err:?}").contains("check"));
}

#[tokio::test]
async fn idempotent_registration() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("ea-idem");

    let alias = format!("{prefix}/alias");
    let canonical = format!("{prefix}/canonical");

    c.execute(
        "select donto_register_entity_alias($1, $2, 'system-a', 0.8)",
        &[&alias, &canonical],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_register_entity_alias($1, $2, 'system-a', 0.95)",
        &[&alias, &canonical],
    )
    .await
    .unwrap();

    let conf: f64 = c
        .query_one(
            "select confidence from donto_entity_alias where alias_iri = $1 and canonical_iri = $2",
            &[&alias, &canonical],
        )
        .await
        .unwrap()
        .get(0);
    assert!((conf - 0.95).abs() < 1e-9, "should keep highest confidence");
}

#[tokio::test]
async fn query_aliases_both_directions() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("ea-both");

    let canonical = format!("{prefix}/canonical");
    let alias1 = format!("{prefix}/alias1");
    let alias2 = format!("{prefix}/alias2");

    c.execute(
        "select donto_register_entity_alias($1, $2, 'sys1')",
        &[&alias1, &canonical],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_register_entity_alias($1, $2, 'sys2')",
        &[&alias2, &canonical],
    )
    .await
    .unwrap();

    // Query from canonical → finds both aliases
    let rows = c
        .query("select * from donto_entity_aliases($1)", &[&canonical])
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);

    // Query from alias1 → finds the pair
    let rows = c
        .query("select * from donto_entity_aliases($1)", &[&alias1])
        .await
        .unwrap();
    assert!(rows.len() >= 1);
}

#[tokio::test]
async fn highest_confidence_wins_resolution() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("ea-conf");

    let alias = format!("{prefix}/ambiguous");
    let c1 = format!("{prefix}/candidate1");
    let c2 = format!("{prefix}/candidate2");

    c.execute(
        "select donto_register_entity_alias($1, $2, 'sys', 0.7)",
        &[&alias, &c1],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_register_entity_alias($1, $2, 'sys', 0.95)",
        &[&alias, &c2],
    )
    .await
    .unwrap();

    let resolved: String = c
        .query_one("select donto_resolve_entity($1)", &[&alias])
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        resolved, c2,
        "should resolve to highest confidence canonical"
    );
}
