//! Evidence substrate: argumentation layer.
//!
//!   * arguments are bitemporal (tx_time lifecycle)
//!   * at most one open argument per (source, target, relation, context)
//!   * self-arguments rejected
//!   * contradiction frontier computes attack/support pressure
//!   * retraction closes tx_time without deleting

use donto_client::{Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

async fn two_stmts(
    client: &donto_client::DontoClient,
    prefix: &str,
    ctx: &str,
) -> (uuid::Uuid, uuid::Uuid) {
    let a = client
        .assert(
            &StatementInput::new(format!("{prefix}/a"), "ex:claims", Object::iri("ex:X"))
                .with_context(ctx),
        )
        .await
        .unwrap();
    let b = client
        .assert(
            &StatementInput::new(format!("{prefix}/b"), "ex:claims", Object::iri("ex:Y"))
                .with_context(ctx),
        )
        .await
        .unwrap();
    (a, b)
}

#[tokio::test]
async fn supports_and_rebuts() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("arg-sr");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-sr").await;
    let (a, b) = two_stmts(&client, &prefix, &ctx).await;

    let sup_id = client
        .assert_argument(a, b, "supports", &ctx, Some(0.8), None, None)
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let rows = c
        .query("select * from donto_arguments_for($1)", &[&b])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, uuid::Uuid>("argument_id"), sup_id);
    assert_eq!(rows[0].get::<_, String>("relation"), "supports");
    let strength: f64 = rows[0].get("strength");
    assert!((strength - 0.8).abs() < 1e-9);
}

#[tokio::test]
async fn self_argument_rejected() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("arg-self");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-self").await;

    let a = client
        .assert(
            &StatementInput::new(format!("{prefix}/a"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let err = client
        .assert_argument(a, a, "supports", &ctx, None, None, None)
        .await
        .err()
        .expect("self-argument must error");
    assert!(format!("{err:?}").contains("source and target must differ"));
}

#[tokio::test]
async fn argument_replacement_closes_prior() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("arg-repl");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-repl").await;
    let (a, b) = two_stmts(&client, &prefix, &ctx).await;

    let id1 = client
        .assert_argument(a, b, "supports", &ctx, Some(0.5), None, None)
        .await
        .unwrap();
    let id2 = client
        .assert_argument(a, b, "supports", &ctx, Some(0.9), None, None)
        .await
        .unwrap();
    assert_ne!(id1, id2);

    // Only the latest is open.
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let open_count: i64 = c
        .query_one(
            "select count(*) from donto_argument \
             where source_statement_id = $1 and target_statement_id = $2 \
               and relation = 'supports' and upper(tx_time) is null",
            &[&a, &b],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(open_count, 1);

    // History preserved: both rows exist.
    let total: i64 = c
        .query_one(
            "select count(*) from donto_argument \
             where source_statement_id = $1 and target_statement_id = $2 \
               and relation = 'supports'",
            &[&a, &b],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(total, 2);
}

#[tokio::test]
async fn retract_argument() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("arg-retract");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-retract").await;
    let (a, b) = two_stmts(&client, &prefix, &ctx).await;

    let arg_id = client
        .assert_argument(a, b, "rebuts", &ctx, None, None, None)
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let retracted: bool = c
        .query_one("select donto_retract_argument($1)", &[&arg_id])
        .await
        .unwrap()
        .get(0);
    assert!(retracted);

    // No open arguments for b anymore.
    let rows = c
        .query("select * from donto_arguments_for($1)", &[&b])
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);
}

#[tokio::test]
async fn contradiction_frontier() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("arg-front");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-front").await;

    // Claim X, two attackers, one supporter.
    let target = client
        .assert(
            &StatementInput::new(format!("{prefix}/target"), "ex:claims", Object::iri("ex:X"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    for i in 0..2 {
        let attacker = client
            .assert(
                &StatementInput::new(
                    format!("{prefix}/attacker{i}"),
                    "ex:claims",
                    Object::iri(format!("ex:not-X-{i}")),
                )
                .with_context(&ctx),
            )
            .await
            .unwrap();
        client
            .assert_argument(attacker, target, "rebuts", &ctx, None, None, None)
            .await
            .unwrap();
    }

    let supporter = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/supporter"),
                "ex:claims",
                Object::iri("ex:also-X"),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();
    client
        .assert_argument(supporter, target, "supports", &ctx, None, None, None)
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let rows = c
        .query("select * from donto_contradiction_frontier($1)", &[&ctx])
        .await
        .unwrap();

    let target_row = rows
        .iter()
        .find(|r| r.get::<_, uuid::Uuid>("statement_id") == target);
    assert!(
        target_row.is_some(),
        "target must appear in contradiction frontier"
    );
    let row = target_row.unwrap();
    assert_eq!(row.get::<_, i64>("attack_count"), 2);
    assert_eq!(row.get::<_, i64>("support_count"), 1);
    assert_eq!(row.get::<_, i64>("net_pressure"), -1);
}

#[tokio::test]
async fn strength_range_enforced() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("arg-range");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-range").await;
    let (a, b) = two_stmts(&client, &prefix, &ctx).await;

    let err = client
        .assert_argument(a, b, "supports", &ctx, Some(1.5), None, None)
        .await
        .err()
        .expect("strength > 1.0 must error");
    assert!(format!("{err:?}").contains("strength_range"));
}
