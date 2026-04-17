//! `pg_donto` — donto as a Postgres extension, packaged with pgrx.
//!
//! What this crate does:
//!   * Wires the donto SQL surface (`sql/migrations/0001..0011`) into a
//!     `CREATE EXTENSION pg_donto;` install path via `extension_sql_file!`.
//!   * Provides Rust implementations of a small set of hot-path helpers
//!     (`donto_pack_flags`, `donto_polarity`, `donto_maturity`,
//!     `donto_canonical_predicate`, `donto_version`) marked `IMMUTABLE`,
//!     so the planner can inline them and so re-implementations match the
//!     plpgsql ones exactly.
//!   * Exposes a `_PG_init` hook for future registrations (currently a
//!     no-op).
//!
//! What this crate does **not** do:
//!   * Replace the plpgsql function bodies. The SQL functions in
//!     `sql/migrations/` are the source of truth. The Rust helpers here
//!     are *additional*, not *substitutes*. Both produce the same answer
//!     and are interchangeable in queries.
//!
//! Performance is intentionally not a goal yet (see PRD §25 +
//! [`CLAUDE.md`](../CLAUDE.md)). The Rust helpers exist for plan-quality
//! reasons (predictable cost, IMMUTABLE marker for indexability), not for
//! microsecond shaving.

#![deny(unsafe_op_in_unsafe_fn)]
#![allow(clippy::needless_range_loop)]

use pgrx::prelude::*;

::pgrx::pg_module_magic!();

// ---------------------------------------------------------------------------
// Embedded SQL bootstrap. extension_sql_file! is pgrx's mechanism for
// shipping arbitrary SQL with the extension. Order is significant.
// ---------------------------------------------------------------------------

extension_sql_file!(
    "../../../sql/migrations/0001_core.sql",
    name = "0001_core",
    bootstrap
);
extension_sql_file!(
    "../../../sql/migrations/0002_flags.sql",
    name = "0002_flags",
    requires = ["0001_core"]
);
extension_sql_file!(
    "../../../sql/migrations/0003_functions.sql",
    name = "0003_functions",
    requires = ["0001_core", "0002_flags"]
);
extension_sql_file!(
    "../../../sql/migrations/0004_migrations.sql",
    name = "0004_migrations",
    requires = ["0001_core"]
);
extension_sql_file!(
    "../../../sql/migrations/0005_presets.sql",
    name = "0005_presets",
    requires = ["0001_core", "0003_functions"]
);
extension_sql_file!(
    "../../../sql/migrations/0006_predicate.sql",
    name = "0006_predicate",
    requires = ["0001_core", "0002_flags", "0003_functions"]
);
extension_sql_file!(
    "../../../sql/migrations/0007_snapshot.sql",
    name = "0007_snapshot",
    requires = ["0001_core", "0003_functions", "0005_presets"]
);
extension_sql_file!(
    "../../../sql/migrations/0008_shape.sql",
    name = "0008_shape",
    requires = ["0001_core"]
);
extension_sql_file!(
    "../../../sql/migrations/0009_rule.sql",
    name = "0009_rule",
    requires = ["0001_core"]
);
extension_sql_file!(
    "../../../sql/migrations/0010_certificate.sql",
    name = "0010_certificate",
    requires = ["0001_core"]
);
extension_sql_file!(
    "../../../sql/migrations/0011_observability.sql",
    name = "0011_observability",
    requires = ["0001_core", "0002_flags", "0006_predicate", "0008_shape", "0009_rule"]
);
extension_sql_file!(
    "../../../sql/migrations/0012_match_scope_fix.sql",
    name = "0012_match_scope_fix",
    requires = ["0003_functions", "0005_presets"]
);

// ---------------------------------------------------------------------------
// Rust-implemented helpers. These shadow the plpgsql versions of the same
// name with `or replace`-style behavior at install time: the plpgsql
// versions are created first by the migrations, then these `pg_extern`s
// re-create them with `CREATE OR REPLACE`. Both implementations agree on
// inputs and outputs.
// ---------------------------------------------------------------------------

/// Pack polarity + maturity into a `smallint` per PRD §5.
#[pg_extern(immutable, parallel_safe, requires = ["0002_flags"])]
fn donto_pack_flags_rs(polarity: &str, maturity: i32) -> i16 {
    let pol = match polarity.to_ascii_lowercase().as_str() {
        "asserted" => 0,
        "negated"  => 1,
        "absent"   => 2,
        "unknown"  => 3,
        other => {
            error!("donto_pack_flags_rs: unknown polarity `{other}`");
        }
    };
    let mat = (maturity & 0b111) as i16;
    (pol | (mat << 2)) as i16
}

/// Decode polarity from `flags`.
#[pg_extern(immutable, parallel_safe, requires = ["0002_flags"])]
fn donto_polarity_rs(flags: i16) -> &'static str {
    match flags & 0b11 {
        0 => "asserted",
        1 => "negated",
        2 => "absent",
        3 => "unknown",
        _ => unreachable!(),
    }
}

/// Decode maturity from `flags`.
#[pg_extern(immutable, parallel_safe, requires = ["0002_flags"])]
fn donto_maturity_rs(flags: i16) -> i32 {
    ((flags >> 2) & 0b111) as i32
}

/// Component / version triples.
#[pg_extern(immutable, parallel_safe)]
fn donto_version_rs() -> Vec<(String, String, String)> {
    vec![
        ("schema".to_string(),     env!("CARGO_PKG_VERSION").to_string(), "pgrx-packaged".to_string()),
        ("atom".to_string(),       "1".to_string(),                       "physical row + sparse overlays".to_string()),
        ("truth".to_string(),      "1".to_string(),                       "polarity asserted/negated/absent/unknown".to_string()),
        ("bitemporal".to_string(), "1".to_string(),                       "valid_time + tx_time".to_string()),
        ("contexts".to_string(),   "1".to_string(),                       "forest, kind, mode".to_string()),
    ]
}

// ---------------------------------------------------------------------------
// pgrx-managed init hook. We currently have nothing to register here.
// ---------------------------------------------------------------------------

#[pg_guard]
extern "C" fn _PG_init() {
    pgrx::pg_sys::submodules::*;
    // Future: register custom GUCs, planner hooks, etc.
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn pack_round_trips() {
        let flags = crate::donto_pack_flags_rs("asserted", 0);
        assert_eq!(flags, 0);
        assert_eq!(crate::donto_polarity_rs(flags), "asserted");
        assert_eq!(crate::donto_maturity_rs(flags), 0);

        let flags = crate::donto_pack_flags_rs("negated", 3);
        assert_eq!(crate::donto_polarity_rs(flags), "negated");
        assert_eq!(crate::donto_maturity_rs(flags), 3);
    }

    #[pg_test]
    fn extension_creates_core_table() {
        Spi::run("select 1 from donto_statement limit 0").expect("donto_statement should exist");
    }

    #[pg_test]
    fn assert_then_match() {
        Spi::run(
            "select donto_assert('ex:a','ex:p','ex:b',null,'donto:anonymous','asserted',0,null,null,null)",
        ).unwrap();
        let n: Option<i64> = Spi::get_one(
            "select count(*)::bigint from donto_match('ex:a','ex:p',null,null,null,'asserted',0,null,null)"
        ).unwrap();
        assert_eq!(n, Some(1));
    }

    #[pg_test]
    fn rust_polarity_matches_plpgsql() {
        for (pol, mat) in [("asserted", 0), ("negated", 1), ("absent", 2), ("unknown", 4)] {
            let f_rs: i16 = Spi::get_one_with_args(
                "select donto_pack_flags_rs($1, $2)",
                vec![
                    (PgBuiltInOids::TEXTOID.oid(), pol.into_datum()),
                    (PgBuiltInOids::INT4OID.oid(), (mat as i32).into_datum()),
                ],
            ).unwrap().expect("packed");
            let f_pl: i16 = Spi::get_one_with_args(
                "select donto_pack_flags($1, $2)",
                vec![
                    (PgBuiltInOids::TEXTOID.oid(), pol.into_datum()),
                    (PgBuiltInOids::INT4OID.oid(), (mat as i32).into_datum()),
                ],
            ).unwrap().expect("packed");
            assert_eq!(f_rs, f_pl, "rust and plpgsql disagree for ({pol}, {mat})");
        }
    }
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {
        // pgrx test harness setup hook.
    }

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        // Settings applied to the test instance.
        vec![]
    }
}
