//! Alexandria §3.6: parallel-literal alignment (SameMeaning).

use donto_client::{Literal, Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

async fn assert_label(
    client: &donto_client::DontoClient,
    subject: &str,
    body: Literal,
    ctx: &str,
) -> uuid::Uuid {
    client
        .assert(
            &StatementInput::new(subject, "rdfs:label", Object::Literal(body)).with_context(ctx),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn translations_form_a_cluster() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("sm-fr-en");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "sm-fr-en").await;

    let en = assert_label(
        &client,
        &format!("{prefix}/claim/en"),
        Literal::lang_string("the cat is on the mat", "en"),
        &ctx,
    )
    .await;
    let fr = assert_label(
        &client,
        &format!("{prefix}/claim/fr"),
        Literal::lang_string("le chat est sur le tapis", "fr"),
        &ctx,
    )
    .await;
    let de = assert_label(
        &client,
        &format!("{prefix}/claim/de"),
        Literal::lang_string("die Katze liegt auf der Matte", "de"),
        &ctx,
    )
    .await;

    // Align en↔fr and fr↔de. en↔de should fall out transitively.
    client.align_meaning(en, fr, &ctx, None).await.unwrap();
    client.align_meaning(fr, de, &ctx, None).await.unwrap();

    let from_en = client
        .meaning_cluster(en, Some(&donto_client::ContextScope::just(&ctx)))
        .await
        .unwrap();
    let set: std::collections::BTreeSet<uuid::Uuid> = from_en.into_iter().collect();
    assert!(set.contains(&en));
    assert!(set.contains(&fr));
    assert!(set.contains(&de));

    // Starting from de we reach en too.
    let from_de = client
        .meaning_cluster(de, Some(&donto_client::ContextScope::just(&ctx)))
        .await
        .unwrap();
    let set: std::collections::BTreeSet<uuid::Uuid> = from_de.into_iter().collect();
    assert!(set.contains(&en));
}

#[tokio::test]
async fn self_alignment_is_rejected() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("sm-self");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "sm-self").await;

    let id = assert_label(
        &client,
        &format!("{prefix}/x"),
        Literal::string("hi"),
        &ctx,
    )
    .await;
    let err = client
        .align_meaning(id, id, &ctx, None)
        .await
        .err()
        .expect("self alignment must error");
    assert!(format!("{err:?}").contains("cannot align with itself"));
}

#[tokio::test]
async fn alignment_is_bidirectional() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("sm-bi");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "sm-bi").await;

    let a = assert_label(
        &client,
        &format!("{prefix}/a"),
        Literal::string("a"),
        &ctx,
    )
    .await;
    let b = assert_label(
        &client,
        &format!("{prefix}/b"),
        Literal::string("b"),
        &ctx,
    )
    .await;
    client.align_meaning(a, b, &ctx, None).await.unwrap();

    // Both directions materialized.
    let pool = client.pool().get().await.unwrap();
    let count: i64 = pool
        .query_one(
            "select count(*) from donto_statement \
             where predicate = 'donto:SameMeaning' and context = $1 and upper(tx_time) is null",
            &[&ctx],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 2);
}
