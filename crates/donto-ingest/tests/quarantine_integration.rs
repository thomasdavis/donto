//! Quarantine routing (PRD §19).
//!
//! Rows that a curated context would reject are diverted to a per-source
//! quarantine context. Operators promote or discard from there. These
//! tests prove:
//!   * `quarantine::route` lands statements in the quarantine context,
//!     not the originally-requested context.
//!   * Re-routing the same batch is idempotent.
//!   * The quarantine context is registered as kind="quarantine" in
//!     `donto_context`, so operators can find it by kind.

use donto_client::{DontoClient, Object, StatementInput};
use donto_ingest::quarantine;

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
async fn route_sends_statements_to_quarantine_context() {
    let c = pg_or_skip!(connect().await);

    // Use a unique source name so our quarantine context is isolated.
    let source = format!("nq://replay/{}", uuid::Uuid::new_v4().simple());
    let q_iri = quarantine::quarantine_iri(&source);

    let stmts = vec![
        StatementInput::new("ex:qa", "ex:p", Object::iri("ex:qb")),
        StatementInput::new("ex:qa", "ex:p", Object::iri("ex:qc")),
    ];
    let n = quarantine::route(&c, &source, stmts).await.unwrap();
    assert_eq!(n, 2);

    // Every row must land in the quarantine context, not donto:anonymous
    // (which was the StatementInput default).
    let conn = c.pool().get().await.unwrap();
    let in_quar: i64 = conn
        .query_one(
            "select count(*) from donto_statement where context = $1",
            &[&q_iri],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(in_quar, 2);

    let in_anon: i64 = conn
        .query_one(
            "select count(*) from donto_statement \
             where context = 'donto:anonymous' and subject = 'ex:qa'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        in_anon, 0,
        "quarantine must override the source StatementInput context"
    );
}

#[tokio::test]
async fn quarantine_context_registered_as_quarantine_kind() {
    let c = pg_or_skip!(connect().await);

    let source = format!("kind-probe/{}", uuid::Uuid::new_v4().simple());
    let q_iri = quarantine::quarantine_iri(&source);
    quarantine::route(
        &c,
        &source,
        vec![StatementInput::new("ex:k", "ex:p", Object::iri("ex:v"))],
    )
    .await
    .unwrap();

    let conn = c.pool().get().await.unwrap();
    let (kind, mode): (String, String) = {
        let row = conn
            .query_one(
                "select kind, mode from donto_context where iri = $1",
                &[&q_iri],
            )
            .await
            .unwrap();
        (row.get(0), row.get(1))
    };
    assert_eq!(kind, "quarantine");
    assert_eq!(mode, "permissive");
}

#[tokio::test]
async fn routing_same_batch_twice_is_idempotent() {
    let c = pg_or_skip!(connect().await);
    let source = format!("replay/{}", uuid::Uuid::new_v4().simple());
    let q_iri = quarantine::quarantine_iri(&source);

    let stmts = vec![
        StatementInput::new("ex:r1", "ex:p", Object::iri("ex:r2")),
        StatementInput::new("ex:r3", "ex:p", Object::iri("ex:r4")),
    ];
    quarantine::route(&c, &source, stmts.clone()).await.unwrap();
    quarantine::route(&c, &source, stmts).await.unwrap();

    let conn = c.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_statement \
             where context = $1 and upper(tx_time) is null",
            &[&q_iri],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2, "re-routing identical content must not double-insert");
}

#[tokio::test]
async fn quarantine_iri_sanitizes_unsafe_chars() {
    // Any character outside [A-Za-z0-9_-] must be coerced to `_`.
    let q = quarantine::quarantine_iri("s3://bucket/key?version=2");
    assert!(q.starts_with("ctx:quarantine/"));
    let tail = q.strip_prefix("ctx:quarantine/").unwrap();
    for c in tail.chars() {
        assert!(
            c.is_ascii_alphanumeric() || c == '-' || c == '_',
            "unsanitized char {c:?} in {q}"
        );
    }
}
