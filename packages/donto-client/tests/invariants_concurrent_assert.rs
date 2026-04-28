//! Concurrent-assert invariants (PRD §3 principle 1 — paraconsistency does
//! not require serialization, §12 — assert is idempotent on content).
//!
//! These tests pound the assert path from many tokio tasks at once and
//! verify two things:
//!   * same (subject, predicate, object, context) content yields the
//!     same statement_id no matter how many racing callers try to insert,
//!   * distinct content inserted concurrently never corrupts count or
//!     leaves phantom rows behind.

mod common;

use donto_client::{ContextScope, Object, Polarity, StatementInput};
use std::collections::HashSet;

#[tokio::test]
async fn same_content_many_writers_returns_same_id() {
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "conc_same").await;

    let input = StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx);

    let mut joins = Vec::with_capacity(16);
    for _ in 0..16 {
        let c2 = c.clone();
        let input2 = input.clone();
        joins.push(tokio::spawn(
            async move { c2.assert(&input2).await.unwrap() },
        ));
    }
    let mut ids = HashSet::new();
    for j in joins {
        ids.insert(j.await.unwrap());
    }
    assert_eq!(
        ids.len(),
        1,
        "16 concurrent asserts of identical content must collapse to one id; got {ids:?}"
    );

    // Exactly one open row in the DB.
    let conn = c.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_statement \
             where subject='ex:a' and predicate='ex:p' and context=$1 and upper(tx_time) is null",
            &[&ctx],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1);
}

#[tokio::test]
async fn distinct_content_many_writers_all_land() {
    // 32 distinct (s, p, o) triples inserted in parallel must produce 32
    // distinct ids and 32 rows — no dropped inserts, no collisions.
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "conc_distinct").await;

    let mut joins = Vec::with_capacity(32);
    for i in 0..32 {
        let c2 = c.clone();
        let ctx2 = ctx.clone();
        joins.push(tokio::spawn(async move {
            c2.assert(
                &StatementInput::new(format!("ex:s{i}"), "ex:p", Object::iri(format!("ex:o{i}")))
                    .with_context(&ctx2),
            )
            .await
            .unwrap()
        }));
    }
    let mut ids = HashSet::new();
    for j in joins {
        ids.insert(j.await.unwrap());
    }
    assert_eq!(
        ids.len(),
        32,
        "each distinct assert must produce a unique id"
    );

    let rows = c
        .match_pattern(
            None,
            Some("ex:p"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 32);
}

#[tokio::test]
async fn concurrent_batch_and_single_dont_duplicate() {
    // Interleave a batch-of-N and N single asserts of the same content;
    // the final row count must equal N (idempotent on content), not 2·N.
    let c = pg_or_skip!(common::connect().await);
    let ctx = common::ctx(&c, "conc_mixed").await;

    const N: usize = 10;
    let triples: Vec<StatementInput> = (0..N)
        .map(|i| {
            StatementInput::new(format!("ex:m{i}"), "ex:p", Object::iri(format!("ex:o{i}")))
                .with_context(&ctx)
        })
        .collect();

    let batch_fut = {
        let c2 = c.clone();
        let triples2 = triples.clone();
        tokio::spawn(async move { c2.assert_batch(&triples2).await.unwrap() })
    };

    let mut singles = Vec::with_capacity(N);
    for t in triples {
        let c2 = c.clone();
        singles.push(tokio::spawn(async move { c2.assert(&t).await.unwrap() }));
    }

    let _ = batch_fut.await.unwrap();
    for j in singles {
        let _ = j.await.unwrap();
    }

    let rows = c
        .match_pattern(
            None,
            Some("ex:p"),
            None,
            Some(&ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        N,
        "batch + N singles of the same content must converge to N rows"
    );
}
