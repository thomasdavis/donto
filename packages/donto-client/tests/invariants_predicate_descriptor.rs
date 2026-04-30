//! Predicate descriptor invariants (migration 0049).
//!
//! `donto_upsert_descriptor` stores rich metadata (label, gloss, types, domain,
//! embedding) for a predicate. Upsert is idempotent: a second call updates the
//! existing row instead of duplicating it. `donto_nearest_predicates` returns
//! candidates ranked by cosine similarity over the embedding column.

mod common;

use common::{connect, tag};

#[tokio::test]
async fn upsert_and_read_back() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pd-rw");

    let iri = format!("{prefix}/myPred");
    let returned = client
        .upsert_descriptor(
            &iri,
            "my pred",
            Some("a custom predicate"),
            Some("Person"),
            Some("Place"),
            Some("custom"),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(returned, iri);

    let c = client.pool().get().await.unwrap();
    let row = c
        .query_one(
            "select label, gloss, subject_type, object_type, domain \
             from donto_predicate_descriptor where iri = $1",
            &[&iri],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("label"), "my pred");
    assert_eq!(
        row.get::<_, Option<String>>("gloss").as_deref(),
        Some("a custom predicate")
    );
    assert_eq!(
        row.get::<_, Option<String>>("subject_type").as_deref(),
        Some("Person")
    );
    assert_eq!(
        row.get::<_, Option<String>>("object_type").as_deref(),
        Some("Place")
    );
    assert_eq!(
        row.get::<_, Option<String>>("domain").as_deref(),
        Some("custom")
    );
}

#[tokio::test]
async fn upsert_is_idempotent() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pd-idem");

    let iri = format!("{prefix}/myPred");

    client
        .upsert_descriptor(&iri, "first label", None, None, None, None, None, None)
        .await
        .unwrap();
    client
        .upsert_descriptor(
            &iri,
            "second label",
            Some("now with gloss"),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let c = client.pool().get().await.unwrap();

    // Exactly one row.
    let n: i64 = c
        .query_one(
            "select count(*) from donto_predicate_descriptor where iri = $1",
            &[&iri],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1, "upsert must not duplicate");

    // Updated values.
    let row = c
        .query_one(
            "select label, gloss from donto_predicate_descriptor where iri = $1",
            &[&iri],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("label"), "second label");
    assert_eq!(
        row.get::<_, Option<String>>("gloss").as_deref(),
        Some("now with gloss")
    );
}

#[tokio::test]
async fn nearest_predicates_ranks_by_similarity() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pd-near");

    let model = format!("test-emb-{}", uuid::Uuid::new_v4().simple());

    // Three predicates with embeddings: near, mid, far from query.
    let near = format!("{prefix}/near");
    let mid = format!("{prefix}/mid");
    let far = format!("{prefix}/far");

    let q: Vec<f32> = vec![1.0, 0.0, 0.0];
    let near_emb: Vec<f32> = vec![1.0, 0.0, 0.0];
    let mid_emb: Vec<f32> = vec![0.7, 0.7, 0.0];
    let far_emb: Vec<f32> = vec![0.0, 0.0, 1.0];

    client
        .upsert_descriptor(
            &near,
            "near",
            None,
            None,
            None,
            Some(prefix.as_str()),
            Some(&model),
            Some(&near_emb),
        )
        .await
        .unwrap();
    client
        .upsert_descriptor(
            &mid,
            "mid",
            None,
            None,
            None,
            Some(prefix.as_str()),
            Some(&model),
            Some(&mid_emb),
        )
        .await
        .unwrap();
    client
        .upsert_descriptor(
            &far,
            "far",
            None,
            None,
            None,
            Some(prefix.as_str()),
            Some(&model),
            Some(&far_emb),
        )
        .await
        .unwrap();

    let results = client
        .nearest_predicates(&q, &model, Some(prefix.as_str()), 10)
        .await
        .unwrap();

    assert!(
        results.len() >= 3,
        "expected >= 3 predicates in domain, got {}",
        results.len()
    );

    // Results must be ordered by similarity desc.
    for w in results.windows(2) {
        assert!(
            w[0].similarity >= w[1].similarity,
            "results must be ordered by similarity desc: {results:?}"
        );
    }

    // The top result for q == near's embedding must be the near IRI.
    assert_eq!(results[0].iri, near, "highest similarity must be `near`");
}
