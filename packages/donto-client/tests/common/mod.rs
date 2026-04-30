//! Shared test harness. Each test gets a freshly-named schema search_path so
//! tests can run in parallel against the same database without collisions.
//!
//! Requires a running Postgres reachable via $DONTO_TEST_DSN (or the default
//! `postgres://donto:donto@127.0.0.1:55432/donto`). Tests that cannot connect
//! are skipped via `pg_or_skip!`.

// Each integration-test binary only pulls in the helpers it actually uses;
// clippy flags the rest as dead. Suppress at module level.
#![allow(dead_code)]

use deadpool_postgres::Pool;
use donto_client::DontoClient;
use std::sync::OnceLock;

pub fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

static MIGRATED: OnceLock<tokio::sync::Mutex<bool>> = OnceLock::new();

/// Connect to the test database and run migrations once per process.
pub async fn connect() -> Option<DontoClient> {
    let dsn = dsn();
    let client = match DontoClient::from_dsn(&dsn) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("test: cannot build client: {e}");
            return None;
        }
    };

    // Probe.
    if let Err(e) = client.pool().get().await {
        eprintln!("test: cannot reach postgres at {dsn}: {e}");
        return None;
    }

    let m = MIGRATED.get_or_init(|| tokio::sync::Mutex::new(false));
    let mut g = m.lock().await;
    if !*g {
        if let Err(e) = client.migrate().await {
            eprintln!("test: migrate failed: {e}");
            return None;
        }
        *g = true;
    }

    Some(client)
}

/// Truncate test data scoped to a context-IRI prefix. Each test uses a unique
/// prefix so they do not interfere.
pub async fn cleanup_prefix(client: &DontoClient, prefix: &str) {
    let pool: &Pool = client.pool();
    let c = pool.get().await.expect("test: cleanup connection");
    c.execute(
        "delete from donto_statement where context like $1",
        &[&format!("{prefix}%")],
    )
    .await
    .ok();
    c.execute(
        "delete from donto_context where iri like $1",
        &[&format!("{prefix}%")],
    )
    .await
    .ok();
}

#[macro_export]
macro_rules! pg_or_skip {
    ($client:expr) => {
        match $client {
            Some(c) => c,
            None => {
                eprintln!("skipping: postgres not available");
                return;
            }
        }
    };
}

/// Build a unique test tag — uses a UUID so tests run in parallel without
/// stomping on each other.
pub fn tag(name: &str) -> String {
    format!("test:{name}:{}", uuid::Uuid::new_v4().simple())
}

/// Helper: build a unique permissive context for a single-context test.
pub async fn ctx(client: &DontoClient, name: &str) -> String {
    let prefix = tag(name);
    let ctx = format!("{prefix}/ctx");
    client
        .ensure_context(&ctx, "custom", "permissive", None)
        .await
        .expect("ensure_context");
    ctx
}

/// Helper: build a unique CURATED context for a single-context test.
pub async fn curated_ctx(client: &DontoClient, name: &str) -> String {
    let prefix = tag(name);
    let ctx = format!("{prefix}/curated");
    client
        .ensure_context(&ctx, "custom", "curated", None)
        .await
        .expect("ensure_context");
    ctx
}

/// `donto_rebuild_predicate_closure` does a TRUNCATE-then-INSERT on a shared
/// table. With cargo running test functions in parallel, two concurrent
/// rebuilds can deadlock on the closure index. Retry on deadlock so the suite
/// stays stable.
pub async fn rebuild_closure_with_retry(client: &DontoClient) {
    for attempt in 0..6 {
        match client.rebuild_predicate_closure().await {
            Ok(_) => return,
            Err(e) => {
                let msg = format!("{e:?}");
                if msg.contains("deadlock") || msg.contains("40P01") {
                    let backoff = 50u64 << attempt;
                    tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
                    continue;
                }
                panic!("rebuild_predicate_closure failed: {e:?}");
            }
        }
    }
    panic!("rebuild_predicate_closure deadlocked after retries");
}
