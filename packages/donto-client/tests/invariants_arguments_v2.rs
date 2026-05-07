//!  / I4: argument relations v2 (migration 0091).

use donto_client::{Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

async fn make_two_statements(
    client: &donto_client::DontoClient,
    prefix: &str,
    ctx: &str,
) -> (uuid::Uuid, uuid::Uuid) {
    let s1 = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s1"),
                "ex:p",
                Object::iri(format!("{prefix}/o1")),
            )
            .with_context(ctx),
        )
        .await
        .unwrap();
    let s2 = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s2"),
                "ex:p",
                Object::iri(format!("{prefix}/o2")),
            )
            .with_context(ctx),
        )
        .await
        .unwrap();
    (s1, s2)
}

#[tokio::test]
async fn extended_kinds_accepted() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("arg-v2-kinds");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-v2-kinds").await;
    let (s1, s2) = make_two_statements(&client, &prefix, &ctx).await;

    for kind in &[
        "alternative_analysis_of",
        "same_evidence_different_analysis",
        "same_claim_different_schema",
        "explains",
    ] {
        c.execute(
            "select donto_assert_argument($1, $2, $3, $4)",
            &[&s1, &s2, kind, &ctx],
        )
        .await
        .unwrap_or_else(|e| panic!("kind {kind} should be accepted: {e}"));
    }
}

#[tokio::test]
async fn v0_kinds_still_accepted() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("arg-v2-v0");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-v2-v0").await;
    let (s1, s2) = make_two_statements(&client, &prefix, &ctx).await;

    for kind in &["supports", "rebuts", "undercuts", "qualifies"] {
        c.execute(
            "select donto_assert_argument($1, $2, $3, $4)",
            &[&s1, &s2, kind, &ctx],
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn invalid_kind_rejected() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("arg-v2-invalid");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-v2-invalid").await;
    let (s1, s2) = make_two_statements(&client, &prefix, &ctx).await;

    let res = c
        .execute(
            "select donto_assert_argument($1, $2, $3, $4)",
            &[&s1, &s2, &"invents-a-new-kind", &ctx],
        )
        .await;
    assert!(res.is_err(), "non-vocabulary kind must be rejected");
}

#[tokio::test]
async fn review_state_default_and_constraint() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("arg-v2-review");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-v2-review").await;
    let (s1, s2) = make_two_statements(&client, &prefix, &ctx).await;

    let arg_id: uuid::Uuid = c
        .query_one(
            "select donto_assert_argument($1, $2, 'supports', $3)",
            &[&s1, &s2, &ctx],
        )
        .await
        .unwrap()
        .get(0);

    let state: String = c
        .query_one(
            "select review_state from donto_argument where argument_id = $1",
            &[&arg_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(state, "unreviewed", "default review_state");

    // valid update
    c.execute(
        "update donto_argument set review_state = 'accepted' where argument_id = $1",
        &[&arg_id],
    )
    .await
    .unwrap();

    // invalid update should fail
    let res = c
        .execute(
            "update donto_argument set review_state = 'mythical' where argument_id = $1",
            &[&arg_id],
        )
        .await;
    assert!(
        res.is_err(),
        "review_state CHECK must reject unknown values"
    );
}

#[tokio::test]
async fn evidence_anchor_ids_default_empty_array() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("arg-v2-anchors");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "arg-v2-anchors").await;
    let (s1, s2) = make_two_statements(&client, &prefix, &ctx).await;

    let arg_id: uuid::Uuid = c
        .query_one(
            "select donto_assert_argument($1, $2, 'supports', $3)",
            &[&s1, &s2, &ctx],
        )
        .await
        .unwrap()
        .get(0);

    let len: i32 = c
        .query_one(
            "select coalesce(array_length(evidence_anchor_ids, 1), 0) \
             from donto_argument where argument_id = $1",
            &[&arg_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(len, 0);
}

#[tokio::test]
async fn relation_view_has_all_kinds() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let n: i64 = c
        .query_one("select count(*) from donto_v_argument_relation", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 9, " argument relation reference view has 9 rows");
}
