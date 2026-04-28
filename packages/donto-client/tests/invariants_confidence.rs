//! Evidence substrate: statement-level confidence.

use donto_client::{Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

#[tokio::test]
async fn set_and_get_confidence() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("conf-basic");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "conf-basic").await;

    let stmt_id = client.assert(
        &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o")).with_context(&ctx),
    ).await.unwrap();

    c.execute("select donto_set_confidence($1, $2::double precision, $3)",
        &[&stmt_id, &0.85f64, &"extraction"]).await.unwrap();

    let conf: Option<f64> = c.query_one(
        "select donto_get_confidence($1)", &[&stmt_id],
    ).await.unwrap().get(0);
    assert!((conf.unwrap() - 0.85).abs() < 1e-9);
}

#[tokio::test]
async fn confidence_upsert() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("conf-ups");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "conf-ups").await;

    let stmt_id = client.assert(
        &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o")).with_context(&ctx),
    ).await.unwrap();

    c.execute("select donto_set_confidence($1, 0.5, 'extraction')", &[&stmt_id]).await.unwrap();
    c.execute("select donto_set_confidence($1, 0.9, 'human')", &[&stmt_id]).await.unwrap();

    let source: String = c.query_one(
        "select confidence_source from donto_stmt_confidence where statement_id = $1", &[&stmt_id],
    ).await.unwrap().get(0);
    assert_eq!(source, "human", "upsert must update source");

    let conf: f64 = c.query_one(
        "select confidence from donto_stmt_confidence where statement_id = $1", &[&stmt_id],
    ).await.unwrap().get(0);
    assert!((conf - 0.9).abs() < 1e-9, "upsert must update confidence");
}

#[tokio::test]
async fn confidence_range_enforced() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("conf-range");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "conf-range").await;

    let stmt_id = client.assert(
        &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o")).with_context(&ctx),
    ).await.unwrap();

    let err = c.execute("select donto_set_confidence($1, 1.5, 'extraction')", &[&stmt_id])
        .await.err().expect("confidence > 1.0 must error");
    assert!(format!("{err:?}").contains("confidence"));

    let err = c.execute("select donto_set_confidence($1, -0.1, 'extraction')", &[&stmt_id])
        .await.err().expect("confidence < 0.0 must error");
    assert!(format!("{err:?}").contains("confidence"));
}

#[tokio::test]
async fn low_confidence_query() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("conf-low");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "conf-low").await;

    for (i, conf) in [0.3, 0.5, 0.9].iter().enumerate() {
        let stmt_id = client.assert(
            &StatementInput::new(format!("{prefix}/s{i}"), "ex:p", Object::iri(format!("ex:o{i}")))
                .with_context(&ctx),
        ).await.unwrap();
        c.execute("select donto_set_confidence($1, $2::double precision, 'extraction')",
            &[&stmt_id, conf]).await.unwrap();
    }

    let low: i64 = c.query_one(
        "select count(*) from donto_low_confidence_statements($1, 0.5)",
        &[&ctx],
    ).await.unwrap().get(0);
    assert_eq!(low, 1, "only confidence < 0.5 should appear");
}

#[tokio::test]
async fn unset_confidence_is_null() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("conf-null");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "conf-null").await;

    let stmt_id = client.assert(
        &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o")).with_context(&ctx),
    ).await.unwrap();

    let conf: Option<f64> = c.query_one(
        "select donto_get_confidence($1)", &[&stmt_id],
    ).await.unwrap().get(0);
    assert!(conf.is_none(), "unset confidence must return null");
}
