//! Alexandria §3.7: environment/bias overlay.
//!
//!   * set/get/delete round-trip
//!   * advisory — queries that ignore the overlay see every statement
//!   * contexts_with_env narrows by exact-match pairs
//!   * match-all pairs = no filter

use donto_client::{Object, StatementInput};
use serde_json::json;

mod common;
use common::{cleanup_prefix, connect, tag};

#[tokio::test]
async fn set_get_delete_round_trip() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("env-rt");
    cleanup_prefix(&client, &prefix).await;

    let ctx = format!("{prefix}/ctx");
    client
        .ensure_context(&ctx, "user", "permissive", None)
        .await
        .unwrap();

    client
        .context_env_set(&ctx, "location", &json!("San Francisco"), None)
        .await
        .unwrap();
    let v = client.context_env_get(&ctx, "location").await.unwrap();
    assert_eq!(v, Some(json!("San Francisco")));

    // Overwrite.
    client
        .context_env_set(&ctx, "location", &json!("NYC"), None)
        .await
        .unwrap();
    let v = client.context_env_get(&ctx, "location").await.unwrap();
    assert_eq!(v, Some(json!("NYC")));

    // Missing key.
    let v = client.context_env_get(&ctx, "dialect").await.unwrap();
    assert_eq!(v, None);

    // Delete.
    let pool = client.pool().get().await.unwrap();
    pool.execute(
        "select donto_context_env_delete($1, $2)",
        &[&ctx, &"location"],
    )
    .await
    .unwrap();
    let v = client.context_env_get(&ctx, "location").await.unwrap();
    assert_eq!(v, None);
}

#[tokio::test]
async fn overlay_is_advisory_only() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("env-adv");
    cleanup_prefix(&client, &prefix).await;

    let alice_ctx = format!("{prefix}/alice");
    client
        .ensure_context(&alice_ctx, "user", "permissive", None)
        .await
        .unwrap();
    client
        .context_env_set(&alice_ctx, "climate_band", &json!("arctic"), None)
        .await
        .unwrap();

    // Alice, in an arctic band, says "10°C is warm".
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/claim"),
                "ex:isWarm",
                Object::iri("ex:tenC"),
            )
            .with_context(&alice_ctx),
        )
        .await
        .unwrap();

    // A query that IGNORES the overlay (plain match_pattern under full scope)
    // sees the statement — bias is not a pre-filter.
    let scope = donto_client::ContextScope::just(&alice_ctx);
    let hits = client
        .match_pattern(
            Some(&format!("{prefix}/claim")),
            None,
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
}

#[tokio::test]
async fn contexts_with_env_narrows_by_pair() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("env-narrow");
    cleanup_prefix(&client, &prefix).await;

    let a = format!("{prefix}/a");
    let b = format!("{prefix}/b");
    let c = format!("{prefix}/c");
    for k in [&a, &b, &c] {
        client
            .ensure_context(k, "user", "permissive", None)
            .await
            .unwrap();
    }
    client
        .context_env_set(&a, "location", &json!("SF"), None)
        .await
        .unwrap();
    client
        .context_env_set(&a, "dialect", &json!("en-US-west"), None)
        .await
        .unwrap();
    client
        .context_env_set(&b, "location", &json!("SF"), None)
        .await
        .unwrap();
    client
        .context_env_set(&c, "location", &json!("NYC"), None)
        .await
        .unwrap();

    // location=SF -> {a, b}
    let hits = client
        .contexts_with_env(&json!({"location": "SF"}))
        .await
        .unwrap();
    let s: std::collections::BTreeSet<String> = hits.into_iter().collect();
    assert!(s.contains(&a));
    assert!(s.contains(&b));
    assert!(!s.contains(&c));

    // location=SF AND dialect=en-US-west -> {a}
    let hits = client
        .contexts_with_env(&json!({"location": "SF", "dialect": "en-US-west"}))
        .await
        .unwrap();
    let s: std::collections::BTreeSet<String> = hits.into_iter().collect();
    assert!(s.contains(&a));
    assert!(!s.contains(&b));

    // Empty required => match everything.
    let hits = client.contexts_with_env(&json!({})).await.unwrap();
    let s: std::collections::BTreeSet<String> = hits.into_iter().collect();
    assert!(s.contains(&a) && s.contains(&b) && s.contains(&c));
}
