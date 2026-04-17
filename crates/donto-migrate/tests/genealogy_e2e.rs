//! End-to-end test for the genealogy SQLite migrator (PRD §24).
//!
//! Builds a tiny `research.db` from `sql/fixtures/genealogy_seed.sql`,
//! migrates it into a live donto database, and asserts every mapping rule
//! the PRD promises.
//!
//! Test self-skips if Postgres is unreachable.

use donto_client::{ContextScope, DontoClient, Object, Polarity};
use rusqlite::Connection;
use std::path::PathBuf;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

fn fixture_seed_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop(); // crates/donto-migrate/ → repo root
    p.push("sql/fixtures/genealogy_seed.sql");
    p
}

fn build_research_db(target: &std::path::Path) -> rusqlite::Result<()> {
    let _ = std::fs::remove_file(target);
    let conn = Connection::open(target)?;
    let sql = std::fs::read_to_string(fixture_seed_path()).expect("read seed sql");
    conn.execute_batch(&sql)?;
    Ok(())
}

async fn boot() -> Option<(DontoClient, String, PathBuf)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let root = format!("ctx:test:gen/{id}");
    let dbpath = std::env::temp_dir().join(format!("donto_gen_{id}.sqlite"));
    build_research_db(&dbpath).ok()?;
    Some((c, root, dbpath))
}

// We can't import donto_migrate::genealogy directly (it lives under
// `src/main.rs`'s mod tree). Spawn the binary instead, but capture the
// output. For a unit test, we'll instead duplicate the call via the
// public migration API: call genealogy::migrate from a sibling module.
//
// Simplest path: use std::process::Command to invoke the donto-migrate
// binary, then assert the resulting database state.

fn run_migrator(dsn: &str, sqlite: &std::path::Path, root: &str) -> std::process::Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_donto-migrate"));
    cmd.args([
        "--dsn",
        dsn,
        "genealogy",
        sqlite.to_str().unwrap(),
        "--root",
        root,
    ]);
    cmd.output().expect("spawn donto-migrate")
}

#[tokio::test]
async fn migrator_round_trips_every_prd_section_24_rule() {
    let Some((c, root, dbpath)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let dsn = dsn();

    let out = run_migrator(&dsn, &dbpath, &root);
    assert!(
        out.status.success(),
        "migrator failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "parse migrator stdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });

    // 1. Two source contexts created.
    assert_eq!(report["source_contexts"], serde_json::json!(2));
    let conn = c.pool().get().await.unwrap();
    let n_src: i64 = conn
        .query_one(
            "select count(*) from donto_context where iri like $1 and kind = 'source'",
            &[&format!("{root}/source/%")],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n_src, 2);

    // 2. Three entity IRIs minted with rdfs:label.
    assert_eq!(report["entities"], serde_json::json!(3));
    let scope = scope_under(&root);
    let labels = c
        .match_pattern(
            None,
            Some("rdfs:label"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    let label_objs: Vec<String> = labels
        .iter()
        .filter_map(|s| match &s.object {
            Object::Literal(l) => l.v.as_str().map(String::from),
            _ => None,
        })
        .collect();
    assert!(label_objs.contains(&"Alice Brackenridge".to_string()));
    assert!(label_objs.contains(&"Alice Julian".to_string()));
    assert!(label_objs.contains(&"Bob Davis".to_string()));

    // 3. Claims become statements; high confidence → maturity 1.
    assert_eq!(report["claims"], serde_json::json!(3));
    let strong = c
        .match_pattern(
            Some(&format!("{root}/iri/alice_old")),
            Some("ex:birthYear"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            1,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        strong.len(),
        1,
        "strong-confidence claim must reach maturity 1"
    );
    assert_eq!(strong[0].context, format!("{root}/source/src/census1900"));

    let speculative = c
        .match_pattern(
            Some(&format!("{root}/iri/alice_young")),
            Some("ex:birthYear"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            1,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        speculative.len(),
        0,
        "speculative claim must NOT pass maturity ≥ 1 floor"
    );
    let any = c
        .match_pattern(
            Some(&format!("{root}/iri/alice_young")),
            Some("ex:birthYear"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        any.len(),
        1,
        "speculative claim still present at maturity 0"
    );

    // 4 + 5. Events become event-nodes; participants get role predicates.
    assert_eq!(report["events"], serde_json::json!(2));
    assert_eq!(report["participants"], serde_json::json!(3));
    let marriage = format!("{root}/event/ev_1900_marriage");
    let typ = c
        .match_pattern(
            Some(&marriage),
            Some("rdf:type"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(typ.iter().any(|s| s.object == Object::iri("ex:Marriage")));
    let spouses = c
        .match_pattern(
            Some(&marriage),
            Some("ex:role/spouse"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        spouses.len(),
        2,
        "marriage must have two spouse participants"
    );

    // 6. Relationships → donto:sameAs/possiblySame/differentFrom.
    assert_eq!(report["relationships"], serde_json::json!(3));
    let sa = c
        .match_pattern(
            None,
            Some("donto:sameAs"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(sa.len(), 1);
    let df = c
        .match_pattern(
            None,
            Some("donto:differentFrom"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(df.len(), 1);
    let ps = c
        .match_pattern(
            None,
            Some("donto:possiblySame"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(ps.len(), 1);

    // 7. Aliases come back with bitemporal valid_time bounded by year_start/end.
    assert_eq!(report["aliases"], serde_json::json!(2));
    let allie = c
        .match_pattern(
            Some(&format!("{root}/iri/alice_young")),
            Some("ex:knownAs"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            Some(chrono::NaiveDate::from_ymd_opt(1910, 6, 1).unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(
        allie.len(),
        1,
        "alias must be visible during its valid_time"
    );
    let allie_out = c
        .match_pattern(
            Some(&format!("{root}/iri/alice_young")),
            Some("ex:knownAs"),
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            Some(chrono::NaiveDate::from_ymd_opt(1925, 1, 1).unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(
        allie_out.len(),
        0,
        "alias must NOT match outside its valid_time"
    );

    // 8. Discrepancies → donto:Discrepancy nodes.
    assert_eq!(report["discrepancies"], serde_json::json!(2));
    let discs = c
        .match_pattern(
            None,
            Some("rdf:type"),
            Some("donto:Discrepancy"),
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(discs.len(), 2);

    // 9. Hypotheses become hypothesis-kind contexts AND donto:Hypothesis nodes.
    assert_eq!(report["hypotheses"], serde_json::json!(1));
    let hcount: i64 = conn.query_one(
        "select count(*) from donto_context where iri = 'ctx:hypo/hypo_alice_merge' and kind = 'hypothesis'",
        &[],
    ).await.unwrap().get(0);
    assert_eq!(hcount, 1);

    // 10. Ingestion log row count surfaced (counted, not migrated as
    //     statements, because audit lives separately).
    assert_eq!(report["ingestion_log"], serde_json::json!(3));

    // statements_emitted is the sum of asserts the migrator actually ran.
    let emitted = report["statements_emitted"].as_u64().unwrap();
    assert!(
        emitted > 15,
        "expected a non-trivial number of statements, got {emitted}"
    );

    // Cleanup the SQLite file.
    let _ = std::fs::remove_file(&dbpath);
}

#[tokio::test]
async fn migrator_is_idempotent_on_rerun() {
    let Some((c, root, dbpath)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let dsn = dsn();
    run_migrator(&dsn, &dbpath, &root);
    let scope = scope_under(&root);
    let n1 = c
        .match_pattern(
            None,
            None,
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap()
        .len();

    // Re-run on the same source — every assert should be idempotent.
    run_migrator(&dsn, &dbpath, &root);
    let n2 = c
        .match_pattern(
            None,
            None,
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap()
        .len();

    assert_eq!(
        n1, n2,
        "second migration must not produce new statements (idempotency)"
    );
    let _ = std::fs::remove_file(&dbpath);
}

#[tokio::test]
async fn migrator_dry_run_writes_nothing() {
    let Some((c, root, dbpath)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let dsn = dsn();
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_donto-migrate"));
    cmd.args([
        "--dsn",
        &dsn,
        "genealogy",
        dbpath.to_str().unwrap(),
        "--root",
        &root,
        "--dry-run",
    ]);
    let out = cmd.output().expect("spawn");
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["dry_run"], serde_json::json!(true));

    let n = c
        .match_pattern(
            None,
            None,
            None,
            Some(&scope_under(&root)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap()
        .len();
    assert_eq!(n, 0, "dry run must not write any statements");
    let _ = std::fs::remove_file(&dbpath);
}

fn scope_under(root: &str) -> ContextScope {
    let mut sc = ContextScope::just(root);
    sc.include_descendants = true;
    sc
}
