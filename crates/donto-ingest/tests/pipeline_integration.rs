//! End-to-end ingest through the `Pipeline` + live Postgres. Skipped
//! cleanly when Postgres is unreachable.
//!
//! These tests close the loop parser → pipeline → client.assert_batch →
//! client.match_pattern, proving that statements ingested from a source
//! can be queried back identically.

use donto_client::{ContextScope, DontoClient, Object, Polarity};
use donto_ingest::{jsonl, nquads, property_graph, Pipeline};
use std::io::Write;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn connect() -> Option<DontoClient> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    Some(c)
}

fn tag(prefix: &str) -> String {
    format!("{prefix}:{}", uuid::Uuid::new_v4().simple())
}

fn write_temp(contents: &str, ext: &str) -> tempfile::NamedTempFile {
    let mut t = tempfile::Builder::new()
        .suffix(&format!(".{ext}"))
        .tempfile()
        .unwrap();
    t.write_all(contents.as_bytes()).unwrap();
    t
}

macro_rules! pg_or_skip {
    ($e:expr) => {
        match $e {
            Some(v) => v,
            None => {
                eprintln!("skipping: postgres not available");
                return;
            }
        }
    };
}

#[tokio::test]
async fn nquads_round_trip_via_pipeline() {
    let c = pg_or_skip!(connect().await);
    let prefix = tag("test:ingest:nq");
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    let src = format!(
        "<{prefix}/alice> <{prefix}/knows> <{prefix}/bob> <{ctx}> .\n\
         <{prefix}/alice> <{prefix}/name> \"Alice\" <{ctx}> .\n\
         <{prefix}/bob>   <{prefix}/name> \"Bob\"   <{ctx}> .\n",
    );
    let f = write_temp(&src, "nq");
    let stmts = nquads::parse_path(f.path(), &ctx).unwrap();
    assert_eq!(stmts.len(), 3);

    let report = Pipeline::new(&c, &ctx)
        .batch_size(128)
        .run("memory://nq", "nq", stmts)
        .await
        .unwrap();
    assert_eq!(report.statements_in, 3);
    assert_eq!(report.statements_inserted, 3);

    // Query back: Alice knows exactly one thing, Bob.
    let rows = c
        .match_pattern(
            Some(&format!("{prefix}/alice")),
            Some(&format!("{prefix}/knows")),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].object, Object::iri(format!("{prefix}/bob")));
}

#[tokio::test]
async fn re_ingest_same_source_is_idempotent() {
    // A replay of the same N-Quads file must not double-count rows.
    // content-hashed rows collapse on the SQL side, so the second
    // pipeline run reports inserted=0 for content that already exists.
    let c = pg_or_skip!(connect().await);
    let prefix = tag("test:ingest:idem");
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    let src = format!(
        "<{prefix}/a> <{prefix}/p> <{prefix}/b> <{ctx}> .\n\
         <{prefix}/a> <{prefix}/q> \"v\" <{ctx}> .\n",
    );
    let f = write_temp(&src, "nq");
    let stmts1 = nquads::parse_path(f.path(), &ctx).unwrap();
    let stmts2 = nquads::parse_path(f.path(), &ctx).unwrap();

    let first = Pipeline::new(&c, &ctx)
        .run("replay://a", "nq", stmts1)
        .await
        .unwrap();
    let second = Pipeline::new(&c, &ctx)
        .run("replay://a", "nq", stmts2)
        .await
        .unwrap();
    assert_eq!(first.statements_in, 2);
    assert_eq!(second.statements_in, 2);

    // Exactly two open rows in the DB — not four.
    let conn = c.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_statement \
             where context=$1 and upper(tx_time) is null",
            &[&ctx],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2, "re-ingest must collapse on content hash, got {n}");
}

#[tokio::test]
async fn jsonl_ingest_with_explicit_polarity_and_maturity() {
    let c = pg_or_skip!(connect().await);
    let prefix = tag("test:ingest:jsonl");
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    let src = format!(
        r#"{{"s":"{prefix}/a","p":"{prefix}/p","o":{{"iri":"{prefix}/b"}},"c":"{ctx}","pol":"asserted","maturity":2}}
{{"s":"{prefix}/a","p":"{prefix}/p","o":{{"iri":"{prefix}/c"}},"c":"{ctx}","pol":"negated","maturity":2}}
"#,
    );
    let f = write_temp(&src, "jsonl");
    let stmts = jsonl::parse_path(f.path(), &ctx).unwrap();
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[0].polarity, Polarity::Asserted);
    assert_eq!(stmts[0].maturity, 2);
    assert_eq!(stmts[1].polarity, Polarity::Negated);

    Pipeline::new(&c, &ctx)
        .run("memory://jsonl", "jsonl", stmts)
        .await
        .unwrap();

    // Mature-enough asserted: 1. Mature-enough negated: 1. Below maturity 3: 0.
    let scope = ContextScope::just(&ctx);
    let asserted = c
        .match_pattern(
            Some(&format!("{prefix}/a")),
            None,
            None,
            Some(&scope),
            Some(Polarity::Asserted),
            2,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(asserted.len(), 1);

    let negated = c
        .match_pattern(
            Some(&format!("{prefix}/a")),
            None,
            None,
            Some(&scope),
            Some(Polarity::Negated),
            2,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(negated.len(), 1);

    let too_mature = c
        .match_pattern(
            Some(&format!("{prefix}/a")),
            None,
            None,
            Some(&scope),
            None,
            3,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(
        too_mature.is_empty(),
        "min_maturity 3 must exclude maturity-2 rows"
    );
}

#[tokio::test]
async fn property_graph_reified_edge_is_retrievable() {
    let c = pg_or_skip!(connect().await);
    let prefix = tag("test:ingest:pg");
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .unwrap();

    let src = r#"{
      "nodes":[{"id":"alice","labels":["Person"],"props":{"name":"Alice"}}],
      "edges":[{"id":"e1","from":"alice","to":"bob","type":"KNOWS","props":{"since":2020}}]
    }"#;
    let f = write_temp(src, "json");
    let ingest_prefix = format!("{prefix}/");
    let mut stmts = property_graph::parse_path(f.path(), &ctx, &ingest_prefix).unwrap();
    // Re-context onto the test context, so cleanup is scoped.
    for s in &mut stmts {
        s.context = ctx.clone();
    }

    Pipeline::new(&c, &ctx)
        .run("memory://pg", "property_graph", stmts)
        .await
        .unwrap();

    // The reified edge should be queryable by its synthetic event IRI.
    let edge_subj = format!("{ingest_prefix}edge/e1");
    let rows = c
        .match_pattern(
            Some(&edge_subj),
            None,
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(
        rows.len() >= 3,
        "reified edge must be retrievable; got {} rows",
        rows.len()
    );
}
