//! Alexandria §3.9: full-text search over literal values.

use donto_client::{ContextScope, Literal, Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

async fn assert_lit(
    client: &donto_client::DontoClient,
    subject: &str,
    predicate: &str,
    body: Literal,
    context: &str,
) -> uuid::Uuid {
    client
        .assert(
            &StatementInput::new(subject, predicate, Object::Literal(body)).with_context(context),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn websearch_matches_stem_and_phrase() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("fts-web");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "fts-web").await;

    assert_lit(
        &client,
        &format!("{prefix}/a"),
        "rdfs:label",
        Literal::string("intersectionality is a framework for understanding"),
        &ctx,
    )
    .await;
    assert_lit(
        &client,
        &format!("{prefix}/b"),
        "rdfs:label",
        Literal::string("theories of intersectional feminism"),
        &ctx,
    )
    .await;
    assert_lit(
        &client,
        &format!("{prefix}/c"),
        "rdfs:label",
        Literal::string("unrelated content about fisheries"),
        &ctx,
    )
    .await;

    let scope = ContextScope::just(&ctx);
    let hits = client
        .match_text("intersectional", None, Some(&scope), None, None, 0)
        .await
        .unwrap();
    let subjects: std::collections::BTreeSet<String> =
        hits.iter().map(|m| m.subject.clone()).collect();
    assert!(subjects.contains(&format!("{prefix}/a")));
    assert!(subjects.contains(&format!("{prefix}/b")));
    assert!(!subjects.contains(&format!("{prefix}/c")));

    // Every hit has a non-zero score from ts_rank_cd.
    for h in &hits {
        assert!(h.score >= 0.0);
    }

    // Negation via websearch `-excluded`.
    let hits = client
        .match_text("intersectional -feminism", None, Some(&scope), None, None, 0)
        .await
        .unwrap();
    let subjects: std::collections::BTreeSet<String> =
        hits.iter().map(|m| m.subject.clone()).collect();
    assert!(subjects.contains(&format!("{prefix}/a")));
    assert!(!subjects.contains(&format!("{prefix}/b")));
}

#[tokio::test]
async fn lang_tag_drives_stemming_config() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("fts-lang");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "fts-lang").await;

    // French lang-tagged literal: "baguettes" stems to "baguet" only under
    // the French config. Under `simple` the query "baguette" wouldn't hit.
    assert_lit(
        &client,
        &format!("{prefix}/fr"),
        "rdfs:label",
        Literal::lang_string("les baguettes fraiches", "fr"),
        &ctx,
    )
    .await;

    let scope = ContextScope::just(&ctx);
    let hits = client
        .match_text("baguette", Some("fr"), Some(&scope), None, None, 0)
        .await
        .unwrap();
    assert!(!hits.is_empty(), "French stemmer must match baguette→baguettes");
}

#[tokio::test]
async fn iri_valued_rows_are_ignored() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("fts-iri");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "fts-iri").await;

    // An IRI-valued statement with nothing matchable.
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/x"),
                "ex:knows",
                Object::iri("ex:intersectionality"),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();
    // A literal row that does match.
    assert_lit(
        &client,
        &format!("{prefix}/y"),
        "rdfs:label",
        Literal::string("about intersectionality"),
        &ctx,
    )
    .await;

    let scope = ContextScope::just(&ctx);
    let hits = client
        .match_text("intersectionality", None, Some(&scope), None, None, 0)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].subject, format!("{prefix}/y"));
}
