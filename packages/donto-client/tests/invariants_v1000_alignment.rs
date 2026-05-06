//! v1000 / I7: alignment relations v2 (migration 0092).

mod common;
use common::{connect, tag};

async fn register(client: &donto_client::DontoClient, src: &str, tgt: &str, rel: &str)
    -> Result<uuid::Uuid, donto_client::Error>
{
    let c = client.pool().get().await.unwrap();
    let row = c
        .query_one(
            "select donto_register_alignment($1, $2, $3, $4)",
            &[&src, &tgt, &rel, &1.0_f64],
        )
        .await;
    match row {
        Ok(r) => Ok(r.get(0)),
        Err(e) => Err(donto_client::Error::Postgres(e)),
    }
}

#[tokio::test]
async fn v1000_relations_accepted() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("aln-v2-kinds");

    for (i, rel) in [
        "broad_match",
        "has_value_mapping",
        "derived_from",
        "local_specialization",
        "narrow_match",
        "exact_match",
        "inverse_of",
        "decomposes_to",
        "incompatible_with",
    ]
    .iter()
    .enumerate()
    {
        let src = format!("ex:{prefix}/p{i}");
        let tgt = format!("ex:{prefix}/q{i}");
        register(&client, &src, &tgt, rel)
            .await
            .unwrap_or_else(|e| panic!("relation {rel}: {e}"));
    }
}

#[tokio::test]
async fn v0_relations_still_accepted() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("aln-v0-kinds");

    for (i, rel) in [
        "exact_equivalent",
        "inverse_equivalent",
        "sub_property_of",
        "close_match",
        "decomposition",
        "not_equivalent",
    ]
    .iter()
    .enumerate()
    {
        let src = format!("ex:{prefix}/p{i}");
        let tgt = format!("ex:{prefix}/q{i}");
        register(&client, &src, &tgt, rel)
            .await
            .unwrap_or_else(|e| panic!("relation {rel}: {e}"));
    }
}

#[tokio::test]
async fn invalid_relation_rejected() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("aln-bad");
    let res = register(
        &client,
        &format!("ex:{prefix}/p"),
        &format!("ex:{prefix}/q"),
        "definitely-not-a-relation",
    )
    .await;
    assert!(res.is_err(), "unknown relation must be rejected");
}

#[tokio::test]
async fn safety_flags_default_correctly() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("aln-safety");

    let id = register(
        &client,
        &format!("ex:{prefix}/p"),
        &format!("ex:{prefix}/q"),
        "close_match",
    )
    .await
    .unwrap();

    let row = c
        .query_one(
            "select safe_for_query_expansion, safe_for_export, safe_for_logical_inference \
             from donto_predicate_alignment where alignment_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let (q, e, l): (bool, bool, bool) = (row.get(0), row.get(1), row.get(2));
    assert!(q, "safe_for_query_expansion defaults true");
    assert!(!e, "safe_for_export defaults false (caller opts in)");
    assert!(!l, "safe_for_logical_inference defaults false (caller opts in)");
}

#[tokio::test]
async fn review_status_default_candidate() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("aln-review");

    let id = register(
        &client,
        &format!("ex:{prefix}/p"),
        &format!("ex:{prefix}/q"),
        "close_match",
    )
    .await
    .unwrap();

    let status: String = c
        .query_one(
            "select review_status from donto_predicate_alignment where alignment_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(status, "candidate");
}

#[tokio::test]
async fn value_mapping_round_trip() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("aln-vmap");

    let id = register(
        &client,
        &format!("ex:{prefix}/wals"),
        &format!("ex:{prefix}/grambank"),
        "has_value_mapping",
    )
    .await
    .unwrap();

    c.execute(
        "select donto_register_value_mapping($1, '1', 'present', 1.0, null)",
        &[&id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_register_value_mapping($1, '0', 'absent', 1.0, null)",
        &[&id],
    )
    .await
    .unwrap();

    let n: i64 = c
        .query_one(
            "select count(*) from donto_alignment_value_mapping where alignment_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2);

    // Idempotent on conflict
    c.execute(
        "select donto_register_value_mapping($1, '1', 'present', 0.95, 'updated')",
        &[&id],
    )
    .await
    .unwrap();

    let conf: f64 = c
        .query_one(
            "select confidence from donto_alignment_value_mapping \
             where alignment_id = $1 and left_value = '1' and right_value = 'present'",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert!((conf - 0.95).abs() < 1e-9);
}

#[tokio::test]
async fn relation_view_has_eleven_rows() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let n: i64 = c
        .query_one(
            "select count(*) from donto_v_alignment_relation_v1000",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 11);
}

#[tokio::test]
async fn scope_can_be_set_to_existing_context() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("aln-scope");

    let id = register(
        &client,
        &format!("ex:{prefix}/p"),
        &format!("ex:{prefix}/q"),
        "close_match",
    )
    .await
    .unwrap();

    // Ensure a context exists then set scope.
    let ctx_iri = format!("ctx:{prefix}/scope");
    c.execute("select donto_ensure_context($1)", &[&ctx_iri])
        .await
        .unwrap();
    c.execute(
        "update donto_predicate_alignment set scope = $1 where alignment_id = $2",
        &[&ctx_iri, &id],
    )
    .await
    .unwrap();

    let scope: Option<String> = c
        .query_one(
            "select scope from donto_predicate_alignment where alignment_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(scope.as_deref(), Some(ctx_iri.as_str()));
}

#[tokio::test]
async fn scope_must_be_real_context() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("aln-scope-fk");

    let id = register(
        &client,
        &format!("ex:{prefix}/p"),
        &format!("ex:{prefix}/q"),
        "close_match",
    )
    .await
    .unwrap();

    let res = c
        .execute(
            "update donto_predicate_alignment set scope = 'ctx:does-not-exist' \
             where alignment_id = $1",
            &[&id],
        )
        .await;
    assert!(res.is_err(), "FK violation: scope must reference real context");
}
