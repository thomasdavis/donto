//! Alexandria §3.1: bitemporal canonical aliases.
//!
//!   * alias→canonical mapping can vary by valid_time
//!   * resolution at an as-of date picks the matching interval
//!   * with no matching interval we fall back to the timeless canonical_of
//!   * with neither, we return the alias itself (open-world)
//!   * canonical must not be an alias-chain target

use chrono::NaiveDate;

mod common;
use common::{cleanup_prefix, connect, tag};

async fn resolve(
    client: &donto_client::DontoClient,
    alias: &str,
    as_of: Option<NaiveDate>,
) -> String {
    let pool = client.pool().get().await.unwrap();
    let row = pool
        .query_one(
            "select donto_canonical_predicate_at($1, $2)",
            &[&alias, &as_of],
        )
        .await
        .unwrap();
    row.get(0)
}

async fn register_alias_at(
    client: &donto_client::DontoClient,
    alias: &str,
    canonical: &str,
    lo: Option<NaiveDate>,
    hi: Option<NaiveDate>,
) {
    let pool = client.pool().get().await.unwrap();
    pool.execute(
        "select donto_register_alias_at($1, $2, $3, $4, null)",
        &[&alias, &canonical, &lo, &hi],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn lit_meant_different_things_at_different_times() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("btc-lit");
    cleanup_prefix(&client, &prefix).await;

    let alias = format!("{prefix}/lit");
    let bright = format!("{prefix}/bright");
    let excellent = format!("{prefix}/excellent");

    register_alias_at(
        &client,
        &alias,
        &bright,
        None,
        Some(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
    )
    .await;
    register_alias_at(
        &client,
        &alias,
        &excellent,
        Some(NaiveDate::from_ymd_opt(2015, 1, 1).unwrap()),
        None,
    )
    .await;

    assert_eq!(
        resolve(&client, &alias, NaiveDate::from_ymd_opt(1950, 6, 1)).await,
        bright
    );
    assert_eq!(
        resolve(&client, &alias, NaiveDate::from_ymd_opt(2020, 6, 1)).await,
        excellent
    );
    // Gap: no interval contains 2010 — falls through to pass-through.
    assert_eq!(
        resolve(&client, &alias, NaiveDate::from_ymd_opt(2010, 6, 1)).await,
        alias
    );
}

#[tokio::test]
async fn bitemporal_takes_priority_over_timeless() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("btc-prio");
    cleanup_prefix(&client, &prefix).await;

    let alias = format!("{prefix}/foo");
    let timeless = format!("{prefix}/timeless");
    let decade_canonical = format!("{prefix}/decade");

    // Register timeless alias_of via the existing predicate registry.
    // Canonical must exist first (FK on donto_predicate.canonical_of).
    let pool = client.pool().get().await.unwrap();
    pool.execute("select donto_implicit_register($1)", &[&timeless])
        .await
        .unwrap();
    pool.execute(
        "select donto_register_predicate($1, null, null, $2, null, null, null, null)",
        &[&alias, &timeless],
    )
    .await
    .unwrap();

    // Also register a bitemporal alias for 2000..2010.
    register_alias_at(
        &client,
        &alias,
        &decade_canonical,
        Some(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
        Some(NaiveDate::from_ymd_opt(2010, 1, 1).unwrap()),
    )
    .await;

    // Inside the interval → bitemporal wins.
    assert_eq!(
        resolve(&client, &alias, NaiveDate::from_ymd_opt(2005, 1, 1)).await,
        decade_canonical
    );
    // Outside → falls through to timeless.
    assert_eq!(
        resolve(&client, &alias, NaiveDate::from_ymd_opt(2030, 1, 1)).await,
        timeless
    );
    // No as-of → also timeless (bitemporal path requires a date).
    assert_eq!(resolve(&client, &alias, None).await, timeless);
}

#[tokio::test]
async fn canonical_must_not_itself_be_an_alias() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("btc-chain");
    cleanup_prefix(&client, &prefix).await;

    let a = format!("{prefix}/a");
    let b = format!("{prefix}/b");
    let c = format!("{prefix}/c");

    // Make b an alias of c (timeless). Canonical c must exist first.
    let pool = client.pool().get().await.unwrap();
    pool.execute("select donto_implicit_register($1)", &[&c])
        .await
        .unwrap();
    pool.execute(
        "select donto_register_predicate($1, null, null, $2, null, null, null, null)",
        &[&b, &c],
    )
    .await
    .unwrap();

    // Registering a → b should fail because b is an alias.
    let err = pool
        .execute(
            "select donto_register_alias_at($1, $2, null, null, null)",
            &[&a, &b],
        )
        .await
        .err()
        .expect("chain must fail");
    assert!(format!("{err:?}").contains("itself an alias"));
}
