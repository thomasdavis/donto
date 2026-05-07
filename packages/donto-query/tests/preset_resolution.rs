//! PRESET resolution in the evaluator.
//!
//! Six presets translate `PRESET <name>` into concrete query
//! adjustments: `latest` | `raw` | `curated` | `under_hypothesis` |
//! `as_of:<RFC3339>` | `anywhere`. Tests verify each preset takes
//! effect, and that an unknown preset returns a structured error.

use chrono::{Duration, Utc};
use donto_client::{DontoClient, Object, StatementInput};
use donto_query::{evaluate, parse_dontoql, Term};

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let prefix = format!("test:preset:{}", uuid::Uuid::new_v4().simple());
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .ok()?;
    Some((c, ctx, prefix))
}

async fn assert_at_maturity(
    client: &DontoClient,
    subj: &str,
    pred: &str,
    obj_iri: &str,
    ctx: &str,
    maturity: u8,
) -> uuid::Uuid {
    client
        .assert(
            &StatementInput::new(subj, pred, Object::iri(obj_iri))
                .with_context(ctx)
                .with_maturity(maturity),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn preset_latest_is_default_no_op() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    assert_at_maturity(
        &client,
        &format!("{prefix}/s"),
        "ex:p",
        &format!("{prefix}/o"),
        &ctx_iri,
        0,
    )
    .await;

    let q = parse_dontoql(&format!(
        "PRESET latest\nSCOPE include <{ctx_iri}>\nMATCH ?s ex:p ?o\nPROJECT ?s, ?o"
    ))
    .unwrap();
    let rows = evaluate(&client, &q).await.unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn preset_curated_filters_to_e2_and_above() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };

    assert_at_maturity(
        &client,
        &format!("{prefix}/raw"),
        "ex:p",
        &format!("{prefix}/o-raw"),
        &ctx_iri,
        0,
    )
    .await;
    assert_at_maturity(
        &client,
        &format!("{prefix}/curated"),
        "ex:p",
        &format!("{prefix}/o-curated"),
        &ctx_iri,
        2,
    )
    .await;

    let q = parse_dontoql(&format!(
        "PRESET curated\nSCOPE include <{ctx_iri}>\nMATCH ?s ex:p ?o\nPROJECT ?s, ?o"
    ))
    .unwrap();
    let rows = evaluate(&client, &q).await.unwrap();
    assert_eq!(rows.len(), 1, "only E2+ rows pass curated");
}

#[tokio::test]
async fn preset_curated_does_not_lower_existing_maturity_floor() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    assert_at_maturity(
        &client,
        &format!("{prefix}/e3"),
        "ex:p",
        &format!("{prefix}/o-e3"),
        &ctx_iri,
        3,
    )
    .await;
    assert_at_maturity(
        &client,
        &format!("{prefix}/e2"),
        "ex:p",
        &format!("{prefix}/o-e2"),
        &ctx_iri,
        2,
    )
    .await;
    let q = parse_dontoql(&format!(
        "PRESET curated\nSCOPE include <{ctx_iri}>\nMATCH ?s ex:p ?o\nMATURITY >= 3\nPROJECT ?s, ?o"
    ))
    .unwrap();
    let rows = evaluate(&client, &q).await.unwrap();
    assert_eq!(rows.len(), 1, "MATURITY 3 wins over PRESET curated's 2");
}

#[tokio::test]
async fn preset_raw_admits_e0() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    assert_at_maturity(
        &client,
        &format!("{prefix}/s"),
        "ex:p",
        &format!("{prefix}/o"),
        &ctx_iri,
        0,
    )
    .await;
    let q = parse_dontoql(&format!(
        "PRESET raw\nSCOPE include <{ctx_iri}>\nMATCH ?s ex:p ?o\nPROJECT ?s"
    ))
    .unwrap();
    let rows = evaluate(&client, &q).await.unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn preset_anywhere_drops_scope() {
    let Some((client, ctx_a, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let ctx_b = format!("{prefix}/ctx-b");
    client
        .ensure_context(&ctx_b, "custom", "permissive", None)
        .await
        .unwrap();

    let subj_a = format!("{prefix}/a");
    let subj_b = format!("{prefix}/b");
    // Single-colon predicate so the dontoql parser treats it as a
    // prefixed term. Multi-colon would need <…> form.
    let pred_local = uuid::Uuid::new_v4().simple().to_string();
    let pred = format!("ex:{pred_local}");
    assert_at_maturity(&client, &subj_a, &pred, &format!("{prefix}/oa"), &ctx_a, 0).await;
    assert_at_maturity(&client, &subj_b, &pred, &format!("{prefix}/ob"), &ctx_b, 0).await;

    let q = parse_dontoql(&format!("PRESET anywhere\nMATCH ?s {pred} ?o\nPROJECT ?s")).unwrap();
    let rows = evaluate(&client, &q).await.unwrap();
    let bound: Vec<String> = rows
        .iter()
        .filter_map(|r| match r.0.get("s") {
            Some(Term::Iri(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert!(bound.contains(&subj_a) && bound.contains(&subj_b));
}

#[tokio::test]
async fn preset_under_hypothesis_restricts_to_hypothesis_kind_contexts() {
    let Some((client, _ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let c = client.pool().get().await.unwrap();

    let hyp = format!("ctx:{prefix}/hyp");
    let src = format!("ctx:{prefix}/src");
    c.execute(
        "select donto_ensure_context($1, 'hypothesis', 'permissive', null)",
        &[&hyp],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_ensure_context($1, 'source', 'permissive', null)",
        &[&src],
    )
    .await
    .unwrap();

    let pred_local = uuid::Uuid::new_v4().simple().to_string();
    let pred = format!("ex:{pred_local}");
    assert_at_maturity(
        &client,
        &format!("{prefix}/h"),
        &pred,
        &format!("{prefix}/oh"),
        &hyp,
        0,
    )
    .await;
    assert_at_maturity(
        &client,
        &format!("{prefix}/s"),
        &pred,
        &format!("{prefix}/os"),
        &src,
        0,
    )
    .await;

    let q = parse_dontoql(&format!(
        "PRESET under_hypothesis\nMATCH ?x {pred} ?o\nPROJECT ?x, ?o"
    ))
    .unwrap();
    let rows = evaluate(&client, &q).await.unwrap();
    let xs: Vec<String> = rows
        .iter()
        .filter_map(|r| match r.0.get("x") {
            Some(Term::Iri(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert!(xs.iter().any(|x| x == &format!("{prefix}/h")));
    assert!(!xs.iter().any(|x| x == &format!("{prefix}/s")));
}

#[tokio::test]
async fn preset_as_of_with_rfc3339_timestamp() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };

    let id = assert_at_maturity(
        &client,
        &format!("{prefix}/s"),
        "ex:p",
        &format!("{prefix}/o"),
        &ctx_iri,
        0,
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let before_retract = Utc::now();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    client.retract(id).await.unwrap();

    // Timestamps contain multiple colons; use string-literal form
    // so the lexer doesn't try to parse them as prefix:local.
    let ts = before_retract.to_rfc3339();
    let q = parse_dontoql(&format!(
        "PRESET \"as_of:{ts}\"\nSCOPE include <{ctx_iri}>\nMATCH ?s ex:p ?o\nPROJECT ?s"
    ))
    .unwrap();
    let rows = evaluate(&client, &q).await.unwrap();
    assert_eq!(rows.len(), 1, "as_of in open window sees the row");

    let early = (Utc::now() - Duration::days(10000)).to_rfc3339();
    let q_early = parse_dontoql(&format!(
        "PRESET \"as_of:{early}\"\nSCOPE include <{ctx_iri}>\nMATCH ?s ex:p ?o\nPROJECT ?s"
    ))
    .unwrap();
    let rows_early = evaluate(&client, &q_early).await.unwrap();
    assert_eq!(rows_early.len(), 0);
}

#[tokio::test]
async fn preset_as_of_without_timestamp_errors() {
    let Some((client, ctx_iri, _prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let q = parse_dontoql(&format!(
        "PRESET as_of\nSCOPE include <{ctx_iri}>\nMATCH ?s ex:p ?o\nPROJECT ?s"
    ))
    .unwrap();
    let res = evaluate(&client, &q).await;
    assert!(res.is_err());
    let err = format!("{:?}", res.unwrap_err());
    assert!(err.contains("as_of"));
}

#[tokio::test]
async fn preset_as_of_with_garbage_timestamp_errors() {
    let Some((client, _ctx, _prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let q =
        parse_dontoql("PRESET \"as_of:not-a-timestamp\"\nMATCH ?s ex:p ?o\nPROJECT ?s").unwrap();
    let res = evaluate(&client, &q).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn unknown_preset_returns_structured_error() {
    let Some((client, _ctx, _prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let q = parse_dontoql("PRESET mythical\nMATCH ?s ex:p ?o\nPROJECT ?s").unwrap();
    let res = evaluate(&client, &q).await;
    assert!(res.is_err());
    let err = format!("{:?}", res.unwrap_err());
    assert!(err.contains("PRESET") && err.contains("mythical"));
}

#[tokio::test]
async fn no_preset_default_path_unchanged() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    assert_at_maturity(
        &client,
        &format!("{prefix}/s"),
        "ex:p",
        &format!("{prefix}/o"),
        &ctx_iri,
        0,
    )
    .await;
    let q = parse_dontoql(&format!(
        "SCOPE include <{ctx_iri}>\nMATCH ?s ex:p ?o\nPROJECT ?s"
    ))
    .unwrap();
    let rows = evaluate(&client, &q).await.unwrap();
    assert_eq!(rows.len(), 1);
}
