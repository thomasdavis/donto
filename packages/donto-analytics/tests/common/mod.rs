//! Shared test helpers for donto-analytics integration tests.
//!
//! Mirrors the pattern in packages/donto-client/tests/common/mod.rs.

#![allow(dead_code)]

use donto_client::DontoClient;
use std::sync::OnceLock;

pub fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

static MIGRATED: OnceLock<tokio::sync::Mutex<bool>> = OnceLock::new();

pub async fn connect() -> Option<DontoClient> {
    let dsn = dsn();
    let client = match DontoClient::from_dsn(&dsn) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("test: cannot build client: {e}");
            return None;
        }
    };
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

pub fn tag(name: &str) -> String {
    format!("test:analytics:{name}:{}", uuid::Uuid::new_v4().simple())
}

/// Build and ensure a permissive context.
pub async fn ctx(client: &DontoClient, name: &str) -> String {
    let prefix = tag(name);
    let ctx = format!("{prefix}/ctx");
    client
        .ensure_context(&ctx, "custom", "permissive", None)
        .await
        .expect("ensure_context");
    ctx
}

#[macro_export]
macro_rules! pg_or_skip {
    ($expr:expr) => {
        match $expr {
            Some(c) => c,
            None => {
                eprintln!("skipping: postgres not available");
                return;
            }
        }
    };
}
