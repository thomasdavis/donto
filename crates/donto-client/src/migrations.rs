//! Embeds the Phase 0 SQL migrations and applies them in order.
//!
//! The migrations are intentionally idempotent (`create table if not exists`,
//! `create or replace function`) so re-running is safe. A real migration
//! tracker is out of Phase 0 scope; we add it in Phase 1 alongside the
//! extension packaging.

use crate::Result;
use deadpool_postgres::Pool;

/// Embedded migration source. Order matters.
pub const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_core",      include_str!("../../../sql/migrations/0001_core.sql")),
    ("0002_flags",     include_str!("../../../sql/migrations/0002_flags.sql")),
    ("0003_functions", include_str!("../../../sql/migrations/0003_functions.sql")),
];

/// Apply all migrations using a pooled connection. Each migration is run
/// inside a transaction.
pub async fn apply_migrations(pool: &Pool) -> Result<()> {
    let mut client = pool.get().await?;
    for (name, sql) in MIGRATIONS {
        tracing::info!(name = %name, "applying migration");
        let tx = client.transaction().await?;
        tx.batch_execute(sql).await?;
        tx.commit().await?;
    }
    Ok(())
}
