//! Evidence substrate: mentions and coreference resolution.

mod common;
use common::{connect, tag};

async fn setup_span(client: &donto_client::DontoClient, name: &str) -> (uuid::Uuid, uuid::Uuid) {
    let iri = format!("test:doc/{}", tag(name));
    let doc_id = client.ensure_document(&iri, "text/plain", None, None, None).await.unwrap();
    let rev_id = client.add_revision(doc_id, Some("The model outperforms it on all benchmarks."), None, None).await.unwrap();
    (doc_id, rev_id)
}

#[tokio::test]
async fn create_mention_and_query() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let (_, rev_id) = setup_span(&client, "men-basic").await;
    let span_id = client.create_char_span(rev_id, 4, 9, Some("model")).await.unwrap();

    let mention_id: uuid::Uuid = c.query_one(
        "select donto_create_mention($1, 'entity', $2, null, $3::double precision)",
        &[&span_id, &"model:mistral-7b", &0.95f64],
    ).await.unwrap().get(0);

    let rows = c.query(
        "select * from donto_mentions_in_revision($1, 'entity')", &[&rev_id],
    ).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, uuid::Uuid>("mention_id"), mention_id);
    assert_eq!(rows[0].get::<_, Option<String>>("entity_iri").as_deref(), Some("model:mistral-7b"));
}

#[tokio::test]
async fn mention_with_candidates() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let (_, rev_id) = setup_span(&client, "men-cand").await;
    let span_id = client.create_char_span(rev_id, 4, 9, Some("model")).await.unwrap();

    let candidates = vec!["model:mistral-7b".to_string(), "model:llama2-13b".to_string()];
    let mention_id: uuid::Uuid = c.query_one(
        "select donto_create_mention($1, 'entity', null, $2::text[], $3::double precision)",
        &[&span_id, &candidates, &0.6f64],
    ).await.unwrap().get(0);

    let stored: Vec<String> = c.query_one(
        "select candidate_iris from donto_mention where mention_id = $1", &[&mention_id],
    ).await.unwrap().get(0);
    assert_eq!(stored.len(), 2);
    assert!(stored.contains(&"model:mistral-7b".to_string()));
}

#[tokio::test]
async fn coref_cluster() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let (_, rev_id) = setup_span(&client, "men-coref").await;

    // "The model" at offset 4-9 and "it" at offset 22-24
    let span1 = client.create_char_span(rev_id, 4, 9, Some("model")).await.unwrap();
    let span2 = client.create_char_span(rev_id, 22, 24, Some("it")).await.unwrap();

    let m1: uuid::Uuid = c.query_one(
        "select donto_create_mention($1, 'entity')", &[&span1],
    ).await.unwrap().get(0);
    let m2: uuid::Uuid = c.query_one(
        "select donto_create_mention($1, 'entity')", &[&span2],
    ).await.unwrap().get(0);

    let cluster_id: uuid::Uuid = c.query_one(
        "select donto_create_coref_cluster($1, $2::uuid[], $3, $4::double precision)",
        &[&rev_id, &vec![m1, m2], &"model:mistral-7b", &0.9f64],
    ).await.unwrap().get(0);

    // Verify membership
    let members: i64 = c.query_one(
        "select count(*) from donto_coref_member where cluster_id = $1", &[&cluster_id],
    ).await.unwrap().get(0);
    assert_eq!(members, 2);

    // First mention is representative
    let is_rep: bool = c.query_one(
        "select is_representative from donto_coref_member where cluster_id = $1 and mention_id = $2",
        &[&cluster_id, &m1],
    ).await.unwrap().get(0);
    assert!(is_rep);

    let is_rep2: bool = c.query_one(
        "select is_representative from donto_coref_member where cluster_id = $1 and mention_id = $2",
        &[&cluster_id, &m2],
    ).await.unwrap().get(0);
    assert!(!is_rep2);

    // Resolved IRI
    let resolved: Option<String> = c.query_one(
        "select resolved_iri from donto_coref_cluster where cluster_id = $1", &[&cluster_id],
    ).await.unwrap().get(0);
    assert_eq!(resolved.as_deref(), Some("model:mistral-7b"));
}

#[tokio::test]
async fn mention_type_validated() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let (_, rev_id) = setup_span(&client, "men-type").await;
    let span_id = client.create_char_span(rev_id, 0, 3, None).await.unwrap();

    let err = c.execute(
        "insert into donto_mention (span_id, mention_type) values ($1, 'invalid_type')",
        &[&span_id],
    ).await.err().expect("invalid mention_type must error");
    assert!(format!("{err:?}").contains("mention_type"));
}

#[tokio::test]
async fn mentions_filter_by_type() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let (_, rev_id) = setup_span(&client, "men-filt").await;
    let s1 = client.create_char_span(rev_id, 0, 3, None).await.unwrap();
    let s2 = client.create_char_span(rev_id, 4, 9, None).await.unwrap();

    c.execute("select donto_create_mention($1, 'entity')", &[&s1]).await.unwrap();
    c.execute("select donto_create_mention($1, 'temporal')", &[&s2]).await.unwrap();

    let all: i64 = c.query_one(
        "select count(*) from donto_mentions_in_revision($1)", &[&rev_id],
    ).await.unwrap().get(0);
    assert_eq!(all, 2);

    let entities: i64 = c.query_one(
        "select count(*) from donto_mentions_in_revision($1, 'entity')", &[&rev_id],
    ).await.unwrap().get(0);
    assert_eq!(entities, 1);
}
