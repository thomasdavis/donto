//! Embedded SQL migrations.
//!
//! Each migration is an idempotent SQL script. We hash the script and record
//! it in `donto_migration` after a successful apply. Re-running skips
//! migrations whose hash matches a recorded entry.

use crate::Result;
use deadpool_postgres::Pool;
use sha2::{Digest, Sha256};

/// Embedded migration source. Order matters.
pub const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_core",       include_str!("../../../sql/migrations/0001_core.sql")),
    ("0002_flags",      include_str!("../../../sql/migrations/0002_flags.sql")),
    ("0003_functions",  include_str!("../../../sql/migrations/0003_functions.sql")),
    ("0004_migrations", include_str!("../../../sql/migrations/0004_migrations.sql")),
    ("0005_presets",    include_str!("../../../sql/migrations/0005_presets.sql")),
    ("0006_predicate",  include_str!("../../../sql/migrations/0006_predicate.sql")),
    ("0007_snapshot",   include_str!("../../../sql/migrations/0007_snapshot.sql")),
    ("0008_shape",      include_str!("../../../sql/migrations/0008_shape.sql")),
    ("0009_rule",       include_str!("../../../sql/migrations/0009_rule.sql")),
    ("0010_certificate",include_str!("../../../sql/migrations/0010_certificate.sql")),
    ("0011_observability",include_str!("../../../sql/migrations/0011_observability.sql")),
];

fn sha256_of(s: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    h.finalize().to_vec()
}

pub async fn apply_migrations(pool: &Pool) -> Result<()> {
    let mut client = pool.get().await?;

    // The ledger table itself lives inside migration 0004. We bootstrap by
    // running migrations 1..=4 unconditionally (they are all `if not exists`
    // shaped), then consult the ledger for the rest.
    for (name, sql) in MIGRATIONS.iter().take(4) {
        tracing::info!(name, "applying bootstrap migration");
        let tx = client.transaction().await?;
        tx.batch_execute(sql).await?;
        tx.commit().await?;
    }

    for (name, sql) in MIGRATIONS.iter().skip(4) {
        let hash = sha256_of(sql);
        let already = client
            .query_opt(
                "select 1 from donto_migration where name = $1 and sha256 = $2",
                &[&name, &hash],
            )
            .await?;
        if already.is_some() {
            tracing::debug!(name, "skipping migration (already applied)");
            continue;
        }
        tracing::info!(name, "applying migration");
        let tx = client.transaction().await?;
        tx.batch_execute(sql).await?;
        tx.execute(
            "insert into donto_migration (name, sha256) values ($1, $2)
             on conflict (name) do update set sha256 = excluded.sha256, applied_at = now()",
            &[&name, &hash],
        ).await?;
        tx.commit().await?;
    }
    Ok(())
}
