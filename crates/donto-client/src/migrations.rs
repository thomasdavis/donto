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

    // Detect first-time install: the ledger table only exists after 0004 has
    // run at least once. On first install we run every migration in order;
    // on subsequent runs we consult the ledger for *every* migration so that
    // later overrides (e.g. 0006_predicate redefining donto_assert) are not
    // clobbered by re-running an earlier migration.
    let ledger_exists: bool = client
        .query_one(
            "select to_regclass('public.donto_migration') is not null",
            &[],
        )
        .await?
        .get(0);

    for (name, sql) in MIGRATIONS.iter() {
        let hash = sha256_of(sql);

        if ledger_exists {
            let already = client.query_opt(
                "select 1 from donto_migration where name = $1 and sha256 = $2",
                &[&name, &hash],
            ).await?;
            if already.is_some() {
                tracing::debug!(name, "skipping migration (already applied)");
                continue;
            }
        }

        tracing::info!(name, "applying migration");
        let tx = client.transaction().await?;
        tx.batch_execute(sql).await?;

        // After 0004 has run, the ledger table exists and every subsequent
        // migration (and 0004 itself) is recorded. Migrations 0001..=0003 on
        // first install are recorded by the seed inside 0004 itself.
        let ledger_should_exist = ledger_exists || MIGRATIONS.iter()
            .position(|(n, _)| *n == *name)
            .map_or(false, |i| i >= 3); // 0004 is index 3
        if ledger_should_exist {
            tx.execute(
                "insert into donto_migration (name, sha256) values ($1, $2)
                 on conflict (name) do update set sha256 = excluded.sha256, applied_at = now()",
                &[&name, &hash],
            ).await?;
        }

        tx.commit().await?;
    }
    Ok(())
}
