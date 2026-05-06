//! Lexical normalizer + alignment suggestion invariants (migration 0056).
//!
//! `donto_normalize_predicate` strips the namespace, splits camelCase, and
//! lowercases. `donto_predicate_lexical_similarity` returns trigram
//! similarity over the normalized forms. `donto_suggest_alignments` proposes
//! candidates whose normalized labels score above a threshold against a
//! source predicate, skipping already-aligned pairs.

mod common;

use common::{connect, tag};

async fn register_predicate(client: &donto_client::DontoClient, iri: &str) {
    let c = client.pool().get().await.unwrap();
    c.execute(
        "select donto_register_predicate($1, null, null, null, null, null, null, null)",
        &[&iri],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn normalize_splits_camel_case() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    let r: String = c
        .query_one("select donto_normalize_predicate($1)", &[&"ex:bornIn"])
        .await
        .unwrap()
        .get(0);
    assert_eq!(r, "born in", "camelCase must split: bornIn → born in");
}

#[tokio::test]
async fn normalize_handles_auxiliary_prefix() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    // wasBornIn → "was born in" — note that the normalizer doesn't strip
    // auxiliaries, just splits + lowercases. Trigram similarity vs "born in"
    // is what carries the recall (verified in the next test).
    let r: String = c
        .query_one("select donto_normalize_predicate($1)", &[&"ex:wasBornIn"])
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        r, "was born in",
        "wasBornIn must normalize to 'was born in'"
    );
}

#[tokio::test]
async fn lexical_similarity_high_for_near_synonyms() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();

    let sim: f64 = c
        .query_one(
            "select donto_predicate_lexical_similarity($1, $2)",
            &[&"ex:bornIn", &"ex:wasBornIn"],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        sim >= 0.5,
        "bornIn vs wasBornIn must score similarly, got {sim}"
    );

    // Sanity floor: an unrelated pair scores lower.
    let sim_diff: f64 = c
        .query_one(
            "select donto_predicate_lexical_similarity($1, $2)",
            &[&"ex:bornIn", &"ex:occupation"],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        sim > sim_diff,
        "near-synonym similarity ({sim}) must exceed unrelated pair ({sim_diff})"
    );
}

#[tokio::test]
async fn suggest_alignments_returns_candidates() {
    let client = pg_or_skip!(connect().await);

    // donto_suggest_alignments is open-world: it ranks against every active
    // predicate. Other tests register many predicates whose normalized form
    // collides with common names like "bornIn", saturating the top-K results.
    // Use a token unique to this test so the candidate list is isolated.
    let token = format!("xq{}", uuid::Uuid::new_v4().simple());

    let source = format!("test:ln-suggest/{token}Source");
    let near = format!("test:ln-suggest/was{}Source", token); // shares "<token>Source"
    let far = format!("test:ln-suggest/totallyUnrelated");

    register_predicate(&client, &source).await;
    register_predicate(&client, &near).await;
    register_predicate(&client, &far).await;

    let c = client.pool().get().await.unwrap();
    let rows = c
        .query(
            "select target_iri, similarity from donto_suggest_alignments($1, 0.4, 50)",
            &[&source],
        )
        .await
        .unwrap();

    let near_hit = rows.iter().any(|r| {
        let iri: String = r.get("target_iri");
        iri == near
    });
    let far_hit = rows.iter().any(|r| {
        let iri: String = r.get("target_iri");
        iri == far
    });
    assert!(
        near_hit,
        "suggest_alignments must surface the near-synonym for {source}; got {} rows",
        rows.len()
    );
    assert!(
        !far_hit,
        "unrelated predicate must score below threshold for {source}"
    );
}
