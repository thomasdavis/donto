//! Integration tests for the paraconsistency density analyzer (C2).
//!
//! Tests seed conflicting statements (same subject+predicate, opposing
//! polarities) and assert that the top-K views return non-empty results
//! after the analyzer runs.

mod common;

use chrono::{Duration, Utc};
use donto_analytics::analyzer_paraconsistency::{run as run_analyzer, ParaconsistencyConfig};
use donto_client::{Object, Polarity, StatementInput};

#[tokio::test]
async fn analyzer_populates_density_table_from_conflicts() {
    let client = pg_or_skip!(common::connect().await);
    let tag = common::tag("para_density");

    // Create two source contexts with opposing claims.
    let ctx_a = format!("{tag}/src_a");
    let ctx_b = format!("{tag}/src_b");
    client
        .ensure_context(&ctx_a, "source", "permissive", None)
        .await
        .expect("ctx_a");
    client
        .ensure_context(&ctx_b, "source", "permissive", None)
        .await
        .expect("ctx_b");

    // Seed 5 conflicted (subject, predicate) pairs.
    for i in 0..5 {
        let subject = format!("{tag}/entity:{i}");
        let predicate = format!("{tag}/prop:{i}");

        // Source A asserts.
        client
            .assert(
                &StatementInput::new(&subject, &predicate, Object::iri("ex:true"))
                    .with_context(&ctx_a)
                    .with_polarity(Polarity::Asserted),
            )
            .await
            .expect("assert A");

        // Source B negates.
        client
            .assert(
                &StatementInput::new(&subject, &predicate, Object::iri("ex:true"))
                    .with_context(&ctx_b)
                    .with_polarity(Polarity::Negated),
            )
            .await
            .expect("assert B");
    }

    // Run the analyzer over a window covering now. The +5s buffer protects
    // against clock skew between the host (Utc::now) and Postgres (now()):
    // freshly-inserted statements have lower(tx_time) ≈ Postgres now, which
    // can be slightly ahead of the host clock under Docker/Hyper-V.
    let window_end = Utc::now() + Duration::seconds(5);
    let window_start = window_end - Duration::hours(1);
    let cfg = ParaconsistencyConfig {
        window_start,
        window_end,
    };

    let report = run_analyzer(&client, &cfg).await.expect("analyzer run");

    assert!(
        report.pairs_upserted > 0,
        "expected upserted rows, got {}",
        report.pairs_upserted
    );

    // Verify the top-K views return non-empty results.
    let c = client.pool().get().await.expect("pool");

    let predicate_rows = c
        .query(
            "select predicate, total_score, windows
             from donto_v_top_contested_predicates
             where predicate like $1",
            &[&format!("{tag}%")],
        )
        .await
        .expect("top contested predicates query");

    assert!(
        !predicate_rows.is_empty(),
        "donto_v_top_contested_predicates should be non-empty for our test predicates"
    );

    let subject_rows = c
        .query(
            "select subject, peak_score, windows
             from donto_v_top_contested_subjects
             where subject like $1",
            &[&format!("{tag}%")],
        )
        .await
        .expect("top contested subjects query");

    assert!(
        !subject_rows.is_empty(),
        "donto_v_top_contested_subjects should be non-empty for our test subjects"
    );

    // Verify conflict_score is in [0, 1].
    let density_rows = c
        .query(
            "select conflict_score, distinct_polarities
             from donto_paraconsistency_density
             where subject like $1",
            &[&format!("{tag}%")],
        )
        .await
        .expect("density table query");

    for row in &density_rows {
        let score: f64 = row.get("conflict_score");
        let dp: i32 = row.get("distinct_polarities");
        assert!(
            (0.0..=1.0).contains(&score),
            "conflict_score={score} out of [0,1]"
        );
        assert!(dp >= 2, "distinct_polarities={dp} should be >= 2");
    }

    // Cleanup.
    c.execute(
        "delete from donto_paraconsistency_density where subject like $1",
        &[&format!("{tag}%")],
    )
    .await
    .ok();
    c.execute(
        "delete from donto_statement where context like $1",
        &[&format!("{tag}%")],
    )
    .await
    .ok();
    c.execute(
        "delete from donto_context where iri like $1",
        &[&format!("{tag}%")],
    )
    .await
    .ok();
}

#[tokio::test]
async fn analyzer_upsert_is_idempotent() {
    let client = pg_or_skip!(common::connect().await);
    let tag = common::tag("para_idem");

    let ctx_a = format!("{tag}/src_a");
    let ctx_b = format!("{tag}/src_b");
    client
        .ensure_context(&ctx_a, "source", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&ctx_b, "source", "permissive", None)
        .await
        .unwrap();

    let subject = format!("{tag}/entity:0");
    let predicate = format!("{tag}/prop:0");

    client
        .assert(
            &StatementInput::new(&subject, &predicate, Object::iri("ex:v"))
                .with_context(&ctx_a)
                .with_polarity(Polarity::Asserted),
        )
        .await
        .unwrap();
    client
        .assert(
            &StatementInput::new(&subject, &predicate, Object::iri("ex:v"))
                .with_context(&ctx_b)
                .with_polarity(Polarity::Negated),
        )
        .await
        .unwrap();

    // +5s buffer guards against host/Postgres clock skew (see other test).
    let window_end = Utc::now() + Duration::seconds(5);
    let window_start = window_end - Duration::hours(1);
    let cfg = ParaconsistencyConfig {
        window_start,
        window_end,
    };

    // Run twice — should not double-insert.
    run_analyzer(&client, &cfg).await.expect("first run");
    run_analyzer(&client, &cfg)
        .await
        .expect("second run (idempotent)");

    let c = client.pool().get().await.unwrap();
    let count: i64 = c
        .query_one(
            "select count(*) from donto_paraconsistency_density
             where subject = $1 and predicate = $2 and window_start = $3",
            &[&subject, &predicate, &window_start],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 1, "upsert must not duplicate rows");

    c.execute(
        "delete from donto_paraconsistency_density where subject like $1",
        &[&format!("{tag}%")],
    )
    .await
    .ok();
    c.execute(
        "delete from donto_statement where context like $1",
        &[&format!("{tag}%")],
    )
    .await
    .ok();
    c.execute(
        "delete from donto_context where iri like $1",
        &[&format!("{tag}%")],
    )
    .await
    .ok();
}
