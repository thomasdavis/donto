//! Rule derivation invariants (PRD §17).
//!
//! "Pure functions of inputs. Same inputs, same outputs."
//! "Re-runs with identical fingerprint skip execution; the previous
//!  derivation report is returned."

use axum::body::Body;
use axum::http::Request;
use donto_client::{DontoClient, Object, StatementInput};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::util::ServiceExt;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(Arc<dontosrv::AppState>, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let prefix = format!("test:rule:{}", uuid::Uuid::new_v4().simple());
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .ok()?;
    Some((
        Arc::new(dontosrv::AppState {
            client: c,
            lean: None,
        }),
        ctx,
    ))
}

async fn derive(state: Arc<dontosrv::AppState>, rule: &str, ctx: &str, into: &str) -> Value {
    let app = dontosrv::router(state);
    let req = Request::builder()
        .method("POST")
        .uri("/rules/derive")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "rule_iri": rule, "scope": {"include":[ctx]}, "into": into,
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1_048_576)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn transitive_closure_derives_full_set() {
    let Some((state, ctx)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;
    // a -> b -> c -> d
    for (s, o) in [("ex:a", "ex:b"), ("ex:b", "ex:c"), ("ex:c", "ex:d")] {
        c.assert(&StatementInput::new(s, "ex:parent", Object::iri(o)).with_context(&ctx))
            .await
            .unwrap();
    }
    let into = format!("ctx:der:{}", uuid::Uuid::new_v4().simple());
    let v = derive(state.clone(), "builtin:transitive/ex:parent", &ctx, &into).await;

    // Closure of a chain a→b→c→d under our recursive CTE has 6 pairs:
    // (a,b),(b,c),(c,d) [base] + (a,c),(b,d),(a,d) [derived].
    let emitted = v.get("emitted").and_then(|x| x.as_u64()).unwrap();
    assert_eq!(emitted, 6, "got {v}");

    // Output context contains exactly those 6 statements.
    let conn = c.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_statement where context = $1 and predicate = 'ex:parent+'",
            &[&into.as_str()],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 6);
}

#[tokio::test]
async fn rerun_with_identical_inputs_returns_cached() {
    let Some((state, ctx)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;
    c.assert(&StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    let into = format!("ctx:der:{}", uuid::Uuid::new_v4().simple());

    let v1 = derive(state.clone(), "builtin:transitive/ex:p", &ctx, &into).await;
    let v2 = derive(state.clone(), "builtin:transitive/ex:p", &ctx, &into).await;
    assert_eq!(v1.get("source").and_then(|x| x.as_str()), Some("builtin"));
    assert_eq!(
        v2.get("source").and_then(|x| x.as_str()),
        Some("cached"),
        "second identical derive must short-circuit: got {v2}"
    );
}

#[tokio::test]
async fn lineage_pointers_recorded_for_derived_statements() {
    let Some((state, ctx)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;
    let id_ab = c
        .assert(&StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    let id_bc = c
        .assert(&StatementInput::new("ex:b", "ex:p", Object::iri("ex:c")).with_context(&ctx))
        .await
        .unwrap();
    let into = format!("ctx:der:{}", uuid::Uuid::new_v4().simple());
    derive(state.clone(), "builtin:transitive/ex:p", &ctx, &into).await;

    // Find the derived (a,c) statement.
    let conn = c.pool().get().await.unwrap();
    let row = conn
        .query_one(
            "select statement_id from donto_statement
         where context = $1 and subject = 'ex:a' and object_iri = 'ex:c'",
            &[&into.as_str()],
        )
        .await
        .unwrap();
    let id_ac: uuid::Uuid = row.get(0);

    // Lineage links to BOTH base statements.
    let lineage: Vec<uuid::Uuid> = conn.query(
        "select source_stmt from donto_stmt_lineage where statement_id = $1 order by source_stmt",
        &[&id_ac],
    ).await.unwrap().into_iter().map(|r| r.get::<_, uuid::Uuid>(0)).collect();
    assert!(
        lineage.contains(&id_ab) && lineage.contains(&id_bc),
        "derived statement must reference both inputs: lineage={lineage:?} ab={id_ab} bc={id_bc}"
    );
}

#[tokio::test]
async fn derived_statement_carries_higher_maturity() {
    // PRD §2 level 3: rule-derived. dontosrv assigns maturity=3 on emit.
    let Some((state, ctx)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let c = &state.client;
    c.assert(&StatementInput::new("ex:a", "ex:p", Object::iri("ex:b")).with_context(&ctx))
        .await
        .unwrap();
    let into = format!("ctx:der:{}", uuid::Uuid::new_v4().simple());
    derive(state.clone(), "builtin:transitive/ex:p", &ctx, &into).await;

    let conn = c.pool().get().await.unwrap();
    let row = conn
        .query_one(
            "select donto_maturity(flags) from donto_statement where context = $1 limit 1",
            &[&into.as_str()],
        )
        .await
        .unwrap();
    let mat: i32 = row.get(0);
    assert_eq!(
        mat, 3,
        "derived statements must carry maturity = 3 (Level 3 of the ladder)"
    );
}

#[tokio::test]
async fn unknown_rule_iri_errors_without_writing() {
    let Some((state, ctx)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let into = format!("ctx:der:{}", uuid::Uuid::new_v4().simple());
    let v = derive(state.clone(), "builtin:nosuch/ex:p", &ctx, &into).await;
    assert!(v.get("error").is_some(), "unknown rule must error: {v}");

    let conn = state.client.pool().get().await.unwrap();
    let n: i64 = conn
        .query_one(
            "select count(*) from donto_statement where context = $1",
            &[&into.as_str()],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        n, 0,
        "errored derive must not write into the output context"
    );
}
