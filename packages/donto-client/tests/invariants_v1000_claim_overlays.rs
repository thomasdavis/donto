//! v1000 claim-model overlays: modality (0099), extraction_level (0100),
//! confidence multivalue (0101), maturity E-naming (0102), multi-context
//! (0103), claim_kind (0104), polarity v2 (0098).

use donto_client::{Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

async fn make_stmt(client: &donto_client::DontoClient, prefix: &str, ctx: &str) -> uuid::Uuid {
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                Object::iri(format!("{prefix}/o")),
            )
            .with_context(ctx),
        )
        .await
        .unwrap()
}

// -------------------- modality (0099) -------------------- //

#[tokio::test]
async fn modality_set_and_get() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mod-set");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "mod-set").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;

    c.execute(
        "select donto_set_modality($1, 'descriptive', 'tester')",
        &[&s],
    )
    .await
    .unwrap();

    let m: String = c
        .query_one("select donto_get_modality($1)", &[&s])
        .await
        .unwrap()
        .get(0);
    assert_eq!(m, "descriptive");
}

#[tokio::test]
async fn modality_invalid_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mod-bad");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "mod-bad").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;

    let res = c
        .execute("select donto_set_modality($1, 'mythical')", &[&s])
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn modality_overlay_upserts() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mod-ups");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "mod-ups").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;

    c.execute("select donto_set_modality($1, 'inferred')", &[&s])
        .await
        .unwrap();
    c.execute("select donto_set_modality($1, 'reconstructed')", &[&s])
        .await
        .unwrap();
    let m: String = c
        .query_one("select donto_get_modality($1)", &[&s])
        .await
        .unwrap()
        .get(0);
    assert_eq!(m, "reconstructed");
}

#[tokio::test]
async fn modality_view_lists_all_kinds() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let n: i64 = c
        .query_one("select count(*) from donto_v_modality_v1000", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 15);
}

// -------------------- extraction level (0100) -------------------- //

#[tokio::test]
async fn extraction_level_set_and_max_promotion() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("xl-set");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "xl-set").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;

    c.execute("select donto_set_extraction_level($1, 'quoted')", &[&s])
        .await
        .unwrap();
    let lvl: String = c
        .query_one("select donto_get_extraction_level($1)", &[&s])
        .await
        .unwrap()
        .get(0);
    assert_eq!(lvl, "quoted");

    let max_q: i32 = c
        .query_one("select donto_max_auto_maturity('quoted')", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(max_q, 2, "quoted may auto-reach E2");

    let max_m: i32 = c
        .query_one("select donto_max_auto_maturity('model_hypothesis')", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(max_m, 1, "model_hypothesis caps at E1");
}

#[tokio::test]
async fn extraction_level_invalid_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("xl-bad");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "xl-bad").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;

    let res = c
        .execute(
            "select donto_set_extraction_level($1, 'something_else')",
            &[&s],
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn extraction_level_all_levels_supported() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("xl-all");
    cleanup_prefix(&client, &prefix).await;

    for lvl in &[
        "quoted",
        "table_read",
        "example_observed",
        "source_generalization",
        "cross_source_inference",
        "model_hypothesis",
        "human_hypothesis",
        "manual_entry",
        "registry_import",
        "adapter_import",
    ] {
        let ctx_iri = ctx(&client, &format!("xl-all-{lvl}")).await;
        let s = make_stmt(&client, &format!("{prefix}-{lvl}"), &ctx_iri).await;
        c.execute("select donto_set_extraction_level($1, $2)", &[&s, lvl])
            .await
            .unwrap_or_else(|e| panic!("level {lvl}: {e}"));
    }
}

// -------------------- confidence multivalue (0101) -------------------- //

#[tokio::test]
async fn confidence_multivalue_lens_resolution() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cm-lens");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "cm-lens").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;

    c.execute("select donto_set_confidence($1, 0.6)", &[&s])
        .await
        .unwrap();
    c.execute("select donto_set_calibrated_confidence($1, 0.8)", &[&s])
        .await
        .unwrap();
    c.execute("select donto_set_human_confidence($1, 0.95)", &[&s])
        .await
        .unwrap();
    c.execute("select donto_set_source_reliability($1, 0.7)", &[&s])
        .await
        .unwrap();

    let m: f64 = c
        .query_one("select donto_confidence_lens($1, 'machine')", &[&s])
        .await
        .unwrap()
        .get(0);
    let cal: f64 = c
        .query_one("select donto_confidence_lens($1, 'calibrated')", &[&s])
        .await
        .unwrap()
        .get(0);
    let h: f64 = c
        .query_one("select donto_confidence_lens($1, 'human')", &[&s])
        .await
        .unwrap()
        .get(0);
    let sw: f64 = c
        .query_one("select donto_confidence_lens($1, 'source_weighted')", &[&s])
        .await
        .unwrap()
        .get(0);
    let multi: f64 = c
        .query_one("select donto_confidence_lens($1, 'multi')", &[&s])
        .await
        .unwrap()
        .get(0);

    // Note: each setter overwrites the primary `confidence` column too,
    // so we just check that the per-lens fields hold their direct values
    // and that 'multi' falls within the bounds of contributing values.
    assert!((cal - 0.8).abs() < 1e-6);
    assert!((h - 0.95).abs() < 1e-6);
    assert!((sw - 0.7).abs() < 1e-6);
    assert!((0.0..=1.0).contains(&m));
    assert!((0.6..=0.95).contains(&multi));
}

#[tokio::test]
async fn confidence_range_check() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("cm-range");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "cm-range").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;

    let res = c
        .execute("select donto_set_calibrated_confidence($1, 1.5)", &[&s])
        .await;
    assert!(res.is_err(), "out-of-range calibrated_confidence rejected");
}

// -------------------- maturity E (0102) -------------------- //

#[tokio::test]
async fn maturity_label_round_trip() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    let cases: &[(&str, i32)] = &[
        ("E0", 0),
        ("E1", 1),
        ("E2", 2),
        ("E3", 3),
        ("E4", 5),
        ("E5", 4),
    ];
    for (lbl, stored) in cases {
        let from: i32 = c
            .query_one("select donto_maturity_from_label($1)", &[lbl])
            .await
            .unwrap()
            .get(0);
        assert_eq!(from, *stored, "label {lbl} → stored {stored}");

        let back: String = c
            .query_one("select donto_maturity_label($1)", &[stored])
            .await
            .unwrap()
            .get(0);
        assert_eq!(back, *lbl, "stored {stored} → label {lbl}");
    }
}

#[tokio::test]
async fn maturity_legacy_l_names_map() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let cases: &[(&str, i32)] = &[("L0", 0), ("L1", 1), ("L2", 2), ("L3", 3), ("L4", 4)];
    for (l, stored) in cases {
        let from: i32 = c
            .query_one("select donto_maturity_from_label($1)", &[l])
            .await
            .unwrap()
            .get(0);
        assert_eq!(from, *stored, "legacy {l}");
    }
}

#[tokio::test]
async fn maturity_ladder_view_has_six_rows() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let n: i64 = c
        .query_one("select count(*) from donto_v_maturity_ladder_v1000", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 6);
}

#[tokio::test]
async fn e_level_helper_extracts_label_from_flags() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    // pack maturity = 3 (E3 reviewed), polarity = asserted (0)
    let lbl: String = c
        .query_one("select donto_e_level(donto_pack_flags('asserted', 3))", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(lbl, "E3");
}

// -------------------- multi-context (0103) -------------------- //

#[tokio::test]
async fn multi_context_add_and_remove() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mc-add");
    cleanup_prefix(&client, &prefix).await;
    let ctx1 = ctx(&client, "mc-primary").await;
    let s = make_stmt(&client, &prefix, &ctx1).await;

    let ctx2 = format!("ctx:{prefix}/secondary");

    c.execute(
        "select donto_add_statement_context($1, $2, 'secondary', 'tester')",
        &[&s, &ctx2],
    )
    .await
    .unwrap();

    let n: i64 = c
        .query_one(
            "select count(*) from donto_statement_context where statement_id = $1",
            &[&s],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1);

    let removed: bool = c
        .query_one(
            "select donto_remove_statement_context($1, $2, 'secondary')",
            &[&s, &ctx2],
        )
        .await
        .unwrap()
        .get(0);
    assert!(removed);
}

#[tokio::test]
async fn multi_context_view_unions_primary_and_secondary() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mc-view");
    cleanup_prefix(&client, &prefix).await;
    let ctx_pri = ctx(&client, "mc-pri").await;
    let s = make_stmt(&client, &prefix, &ctx_pri).await;

    let ctx_sec = format!("ctx:{prefix}/sec");
    c.execute(
        "select donto_add_statement_context($1, $2, 'hypothesis_lens')",
        &[&s, &ctx_sec],
    )
    .await
    .unwrap();

    let rows = c
        .query(
            "select context, role from donto_v_statement_contexts \
             where statement_id = $1",
            &[&s],
        )
        .await
        .unwrap();
    let mut roles: Vec<String> = rows.iter().map(|r| r.get::<_, String>(1)).collect();
    roles.sort();
    assert_eq!(roles, vec!["hypothesis_lens", "primary"]);
}

#[tokio::test]
async fn multi_context_role_check_constraint() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("mc-bad");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "mc-bad").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;
    let ctx2 = format!("ctx:{prefix}/x");
    c.execute("select donto_ensure_context($1)", &[&ctx2])
        .await
        .unwrap();
    let res = c
        .execute(
            "insert into donto_statement_context (statement_id, context, role) \
             values ($1, $2, 'mythical-role')",
            &[&s, &ctx2],
        )
        .await;
    assert!(res.is_err(), "invalid role rejected");
}

// -------------------- claim_kind (0104) -------------------- //

#[tokio::test]
async fn claim_kind_default_atomic() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ck-default");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "ck-default").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;

    let k: String = c
        .query_one("select donto_get_claim_kind($1)", &[&s])
        .await
        .unwrap()
        .get(0);
    assert_eq!(k, "atomic");
}

#[tokio::test]
async fn claim_kind_set_overlay() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ck-set");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "ck-set").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;

    c.execute("select donto_set_claim_kind($1, 'frame_summary')", &[&s])
        .await
        .unwrap();
    let k: String = c
        .query_one("select donto_get_claim_kind($1)", &[&s])
        .await
        .unwrap()
        .get(0);
    assert_eq!(k, "frame_summary");
}

#[tokio::test]
async fn claim_kind_invalid_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ck-bad");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "ck-bad").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;
    let res = c
        .execute("select donto_set_claim_kind($1, 'mythical-kind')", &[&s])
        .await;
    assert!(res.is_err());
}

// -------------------- polarity v2 (0098) -------------------- //

#[tokio::test]
async fn polarity_view_returns_stored_when_no_conflict() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("pv2-no-conflict");
    cleanup_prefix(&client, &prefix).await;
    let ctx_iri = ctx(&client, "pv2-no-conflict").await;
    let s = make_stmt(&client, &prefix, &ctx_iri).await;

    let p: String = c
        .query_one(
            "select effective_polarity from donto_v_statement_polarity_v1000 \
             where statement_id = $1",
            &[&s],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(p, "asserted");
}

#[tokio::test]
async fn polarity_view_lists_five_kinds() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let n: i64 = c
        .query_one("select count(*) from donto_v_polarity_v1000", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 5);
}
