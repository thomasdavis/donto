//! Shared test harness. Each test gets a freshly-named schema search_path so
//! tests can run in parallel against the same database without collisions.
//!
//! Requires a running Postgres reachable via $DONTO_TEST_DSN (or the default
//! `postgres://donto:donto@127.0.0.1:55432/donto`). Tests that cannot connect
//! are skipped via `pg_or_skip!`.

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
        Err(e) => { eprintln!("test: cannot build client: {e}"); return None; }
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
    ).await.ok();
    c.execute(
        "delete from donto_context where iri like $1",
        &[&format!("{prefix}%")],
    ).await.ok();
}

#[macro_export]
macro_rules! pg_or_skip {
    ($client:expr) => {
        match $client {
            Some(c) => c,
            None => { eprintln!("skipping: postgres not available"); return; }
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
    client.ensure_context(&ctx, "custom", "permissive", None).await.expect("ensure_context");
    ctx
}

/// Helper: build a unique CURATED context for a single-context test.
pub async fn curated_ctx(client: &DontoClient, name: &str) -> String {
    let prefix = tag(name);
    let ctx = format!("{prefix}/curated");
    client.ensure_context(&ctx, "custom", "curated", None).await.expect("ensure_context");
    ctx
}
