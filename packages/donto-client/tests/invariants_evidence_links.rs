//! Evidence substrate: evidence links.
//!
//!   * links are additive (don't mutate the statement)
//!   * exactly-one-target constraint enforced
//!   * retraction closes tx_time
//!   * evidence_for returns current (open) links only

use donto_client::{Object, StatementInput};

mod common;
use common::{connect, ctx, tag};

#[tokio::test]
async fn link_span_to_statement() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ev-span").await;
    let prefix = tag("ev-span");

    // Create a statement.
    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    // Create a document + revision + span.
    let doc_iri = format!("test:doc/{prefix}");
    let doc_id = client
        .ensure_document(&doc_iri, "text/plain", None, None, None)
        .await
        .unwrap();
    let rev_id = client
        .add_revision(doc_id, Some("evidence text"), None, None)
        .await
        .unwrap();
    let span_id = client
        .create_char_span(rev_id, 0, 8, Some("evidence"))
        .await
        .unwrap();

    // Link them.
    let link_id = client
        .link_evidence_span(stmt_id, span_id, "extracted_from", Some(0.9), Some(&ctx))
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let rows = c
        .query("select * from donto_evidence_for($1)", &[&stmt_id])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, uuid::Uuid>("link_id"), link_id);
    assert_eq!(rows[0].get::<_, String>("link_type"), "extracted_from");
    assert_eq!(
        rows[0].get::<_, Option<uuid::Uuid>>("target_span_id"),
        Some(span_id)
    );
    let conf: f64 = rows[0].get("confidence");
    assert!((conf - 0.9).abs() < 1e-9);
}

#[tokio::test]
async fn link_run_to_statement() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ev-run").await;
    let prefix = tag("ev-run");

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let run_id = client
        .start_extraction(Some("claude"), None, None, Some(&ctx))
        .await
        .unwrap();

    let link_id = client
        .link_evidence_run(stmt_id, run_id, "produced_by", Some(&ctx))
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let rows = c
        .query("select * from donto_evidence_for($1)", &[&stmt_id])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, uuid::Uuid>("link_id"), link_id);
    assert_eq!(
        rows[0].get::<_, Option<uuid::Uuid>>("target_run_id"),
        Some(run_id)
    );
}

#[tokio::test]
async fn exactly_one_target_enforced() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let ctx = ctx(&client, "ev-multi").await;
    let prefix = tag("ev-multi");
    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    // Zero targets must fail.
    let err = c
        .execute(
            "insert into donto_evidence_link (statement_id, link_type) \
             values ($1, 'extracted_from')",
            &[&stmt_id],
        )
        .await
        .err()
        .expect("zero targets must violate check");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("has_target") || msg.contains("check constraint"),
        "expected target check violation, got: {msg}"
    );
}

#[tokio::test]
async fn retract_evidence_link() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ev-retract").await;
    let prefix = tag("ev-retract");

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let doc_iri = format!("test:doc/{prefix}");
    let doc_id = client
        .ensure_document(&doc_iri, "text/plain", None, None, None)
        .await
        .unwrap();
    let rev_id = client
        .add_revision(doc_id, Some("text"), None, None)
        .await
        .unwrap();
    let span_id = client.create_char_span(rev_id, 0, 4, None).await.unwrap();

    let link_id = client
        .link_evidence_span(stmt_id, span_id, "extracted_from", None, None)
        .await
        .unwrap();

    // Retract it.
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let retracted: bool = c
        .query_one("select donto_retract_evidence_link($1)", &[&link_id])
        .await
        .unwrap()
        .get(0);
    assert!(retracted);

    // evidence_for no longer returns it.
    let rows = c
        .query("select * from donto_evidence_for($1)", &[&stmt_id])
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        0,
        "retracted link must vanish from current view"
    );

    // But the row still exists (tx_time closed).
    let closed: bool = c
        .query_one(
            "select upper(tx_time) is not null from donto_evidence_link where link_id = $1",
            &[&link_id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        closed,
        "retracted link row must persist with closed tx_time"
    );
}

#[tokio::test]
async fn link_does_not_mutate_statement() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ev-noop").await;
    let prefix = tag("ev-noop");

    let stmt_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let hash_before: Vec<u8> = c
        .query_one(
            "select content_hash from donto_statement where statement_id = $1",
            &[&stmt_id],
        )
        .await
        .unwrap()
        .get(0);

    let doc_iri = format!("test:doc/{prefix}");
    let doc_id = client
        .ensure_document(&doc_iri, "text/plain", None, None, None)
        .await
        .unwrap();
    let rev_id = client
        .add_revision(doc_id, Some("text"), None, None)
        .await
        .unwrap();
    let span_id = client.create_char_span(rev_id, 0, 4, None).await.unwrap();
    client
        .link_evidence_span(stmt_id, span_id, "extracted_from", None, None)
        .await
        .unwrap();

    let hash_after: Vec<u8> = c
        .query_one(
            "select content_hash from donto_statement where statement_id = $1",
            &[&stmt_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        hash_before, hash_after,
        "evidence link must not mutate statement"
    );
}
