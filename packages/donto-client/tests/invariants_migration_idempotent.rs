//! Migration idempotency (PRD §3 principle 5, §1 schema hygiene).
//!
//! Every migration is `if not exists` / `create or replace` shaped; the
//! ledger in `donto_migration` records (name, sha256). Running migrate()
//! twice must produce exactly one ledger row per migration, unchanged
//! hashes, and no duplicate side effects.

mod common;

use donto_client::DontoClient;

#[tokio::test]
async fn migrate_twice_leaves_ledger_stable() {
    let c = pg_or_skip!(common::connect().await);

    // common::connect() already migrated once as part of process setup.
    // Capture the ledger state, then re-migrate, then capture again.
    let conn = c.pool().get().await.unwrap();

    let before: Vec<(String, Vec<u8>)> = conn
        .query(
            "select name, sha256 from donto_migration order by name",
            &[],
        )
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, Vec<u8>>(1)))
        .collect();
    assert!(
        before.len() >= 12,
        "expected at least 12 migrations recorded, got {}",
        before.len()
    );

    // Drop the connection so migrate() can take its own handle without
    // deadlocking on a single-connection pool.
    drop(conn);

    // Run migrate() again — should be a no-op.
    c.migrate().await.unwrap();

    let conn = c.pool().get().await.unwrap();
    let after: Vec<(String, Vec<u8>)> = conn
        .query(
            "select name, sha256 from donto_migration order by name",
            &[],
        )
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, Vec<u8>>(1)))
        .collect();
    assert_eq!(
        before, after,
        "ledger must be byte-identical after re-migrate"
    );
}

#[tokio::test]
async fn sql_functions_are_stable_across_re_migrate() {
    // The set of donto_* SQL functions is a contract. Running migrate() twice
    // must not orphan old definitions or double-up parametric overloads.
    let c = pg_or_skip!(common::connect().await);

    let query = "select proname, pg_get_function_identity_arguments(oid) \
                 from pg_proc \
                 where proname like 'donto\\_%' \
                 order by proname, pg_get_function_identity_arguments(oid)";

    let conn = c.pool().get().await.unwrap();
    let before: Vec<(String, String)> = conn
        .query(query, &[])
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
        .collect();
    drop(conn);

    c.migrate().await.unwrap();

    let conn = c.pool().get().await.unwrap();
    let after: Vec<(String, String)> = conn
        .query(query, &[])
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
        .collect();
    assert_eq!(
        before, after,
        "donto_* function surface must be identical after re-migrate",
    );
}

#[tokio::test]
async fn every_embedded_migration_is_ledger_recorded() {
    // The Rust-side constant MIGRATIONS must agree with the ledger — if
    // someone adds a migration file but forgets to wire it into
    // migrations.rs::MIGRATIONS it'll silently skip at runtime; this test
    // makes the mismatch visible.
    let c = pg_or_skip!(common::connect().await);

    let conn = c.pool().get().await.unwrap();
    let ledger_names: std::collections::HashSet<String> = conn
        .query("select name from donto_migration", &[])
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.get::<_, String>(0))
        .collect();

    for (name, _) in donto_client::migrations::MIGRATIONS {
        assert!(
            ledger_names.contains(*name),
            "migration {name} missing from donto_migration ledger"
        );
    }
}

#[tokio::test]
async fn migration_hashes_match_embedded_sources() {
    // Every `on conflict do update` path in apply_migrations rewrites the
    // ledger hash to match the shipped source. After a fresh migrate(), the
    // ledger sha256 for a given migration must equal sha256(source) for
    // *that same name* — except the 0001..=0003 backfill rows which ship
    // with a sentinel 0x00 hash.
    use sha2::{Digest, Sha256};

    let c = pg_or_skip!(common::connect().await);

    // Re-run so the update path definitely ran.
    c.migrate().await.unwrap();

    let conn = c.pool().get().await.unwrap();
    let ledger: std::collections::HashMap<String, Vec<u8>> = conn
        .query("select name, sha256 from donto_migration", &[])
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, Vec<u8>>(1)))
        .collect();

    for (name, sql) in donto_client::migrations::MIGRATIONS {
        let mut h = Sha256::new();
        h.update(sql.as_bytes());
        let expected = h.finalize().to_vec();
        let got = ledger.get(*name).cloned().unwrap_or_default();

        // Backfill sentinel is acceptable only for the first three
        // migrations, and only on a never-upgraded install.
        let backfill_sentinel = got == vec![0u8];
        if backfill_sentinel {
            assert!(
                matches!(*name, "0001_core" | "0002_flags" | "0003_functions"),
                "sentinel hash permitted only for initial migrations, not {name}"
            );
        } else {
            assert_eq!(
                got, expected,
                "ledger hash for {name} must match shipped SQL"
            );
        }
    }

    let _ = c; // silence unused warning on skip-path
    let _: Option<DontoClient> = None;
}
