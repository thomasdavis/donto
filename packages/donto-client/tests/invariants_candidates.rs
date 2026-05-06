//! Evidence substrate: candidate contexts and promotion.

use donto_client::{Object, Polarity, StatementInput};

mod common;
use common::{cleanup_prefix, connect, tag};

#[tokio::test]
async fn promote_candidate() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("cand-promo");
    cleanup_prefix(&client, &prefix).await;

    let cand_ctx = format!("{prefix}/candidates");
    let target_ctx = format!("{prefix}/promoted");
    client
        .ensure_context(&cand_ctx, "candidate", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&target_ctx, "source", "permissive", None)
        .await
        .unwrap();

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&cand_ctx),
        )
        .await
        .unwrap();

    let promoted_id: uuid::Uuid = c
        .query_one(
            "select donto_promote_candidate($1, $2)",
            &[&stmt_id, &target_ctx],
        )
        .await
        .unwrap()
        .get(0);

    assert_ne!(stmt_id, promoted_id, "promoted statement gets a new ID");

    // Original is retracted
    let original_open: bool = c
        .query_one(
            "select upper(tx_time) is null from donto_statement where statement_id = $1",
            &[&stmt_id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        !original_open,
        "candidate must be retracted after promotion"
    );

    // Promoted exists in target context
    let promoted_ctx: String = c.query_one(
        "select context from donto_statement where statement_id = $1 and upper(tx_time) is null",
        &[&promoted_id],
    ).await.unwrap().get(0);
    assert_eq!(promoted_ctx, target_ctx);

    // Lineage tracked
    let has_lineage: bool = c.query_one(
        "select exists(select 1 from donto_stmt_lineage where statement_id = $1 and source_stmt = $2)",
        &[&promoted_id, &stmt_id],
    ).await.unwrap().get(0);
    assert!(
        has_lineage,
        "promoted statement must track lineage to candidate"
    );
}

#[tokio::test]
async fn promote_non_candidate_fails() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("cand-fail");
    cleanup_prefix(&client, &prefix).await;

    let source_ctx = format!("{prefix}/source");
    client
        .ensure_context(&source_ctx, "source", "permissive", None)
        .await
        .unwrap();

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&source_ctx),
        )
        .await
        .unwrap();

    let err = c
        .execute(
            "select donto_promote_candidate($1, 'anywhere')",
            &[&stmt_id],
        )
        .await
        .err()
        .expect("promoting from non-candidate context must error");
    assert!(format!("{err:?}").contains("not in a candidate context"));
}

#[tokio::test]
async fn bulk_promote_above_threshold() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("cand-bulk");
    cleanup_prefix(&client, &prefix).await;

    let cand_ctx = format!("{prefix}/candidates");
    let target_ctx = format!("{prefix}/promoted");
    client
        .ensure_context(&cand_ctx, "candidate", "permissive", None)
        .await
        .unwrap();

    // Create 3 candidates with different confidences
    for (i, conf) in [0.3, 0.6, 0.9].iter().enumerate() {
        let stmt_id = client
            .assert(
                &StatementInput::new(
                    format!("{prefix}/s{i}"),
                    "ex:p",
                    Object::iri(format!("ex:o{i}")),
                )
                .with_context(&cand_ctx),
            )
            .await
            .unwrap();
        c.execute(
            "select donto_set_confidence($1, $2::double precision, 'extraction')",
            &[&stmt_id, conf],
        )
        .await
        .unwrap();
    }

    let promoted: i64 = c
        .query_one(
            "select donto_promote_candidates_above($1, $2, $3::double precision)",
            &[&cand_ctx, &target_ctx, &0.5f64],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        promoted, 2,
        "should promote 2 candidates (confidence >= 0.5)"
    );

    // Verify remaining candidate
    let remaining: i64 = c
        .query_one(
            "select count(*) from donto_statement where context = $1 and upper(tx_time) is null",
            &[&cand_ctx],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(remaining, 1, "one candidate below threshold should remain");

    // Verify promoted
    let in_target: i64 = c
        .query_one(
            "select count(*) from donto_statement where context = $1 and upper(tx_time) is null",
            &[&target_ctx],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(in_target, 2);
}

#[tokio::test]
async fn promoted_preserves_content() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("cand-pres");
    cleanup_prefix(&client, &prefix).await;

    let cand_ctx = format!("{prefix}/candidates");
    let target_ctx = format!("{prefix}/target");
    client
        .ensure_context(&cand_ctx, "candidate", "permissive", None)
        .await
        .unwrap();

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:claims", Object::iri("ex:thing"))
                .with_context(&cand_ctx)
                .with_polarity(Polarity::Negated)
                .with_maturity(1),
        )
        .await
        .unwrap();

    let promoted_id: uuid::Uuid = c
        .query_one(
            "select donto_promote_candidate($1, $2)",
            &[&stmt_id, &target_ctx],
        )
        .await
        .unwrap()
        .get(0);

    let row = c.query_one(
        "select subject, predicate, object_iri, donto_polarity(flags) as pol, donto_maturity(flags) as mat \
         from donto_statement where statement_id = $1",
        &[&promoted_id],
    ).await.unwrap();
    assert_eq!(row.get::<_, String>("subject"), format!("{prefix}/s"));
    assert_eq!(row.get::<_, String>("predicate"), "ex:claims");
    assert_eq!(
        row.get::<_, Option<String>>("object_iri").as_deref(),
        Some("ex:thing")
    );
    assert_eq!(row.get::<_, String>("pol"), "negated");
    assert_eq!(row.get::<_, i32>("mat"), 1);
}
