//! Evidence substrate: vector/embedding layer.
//!
//!   * store_vector upserts by (subject_type, subject_id, model_id)
//!   * cosine_similarity is correct
//!   * nearest_vectors returns ranked results
//!   * dimension mismatch returns null similarity

use donto_client::{Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

#[tokio::test]
async fn store_and_retrieve_vector() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("vec-basic");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "vec-basic").await;

    let stmt_id = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"), "ex:p", Object::iri("ex:o"),
            ).with_context(&ctx),
        )
        .await.unwrap();

    let embedding: Vec<f32> = vec![1.0, 0.0, 0.0, 0.0];
    let vec_id = client
        .store_vector("statement", stmt_id, "test-model", Some("v1"), &embedding)
        .await.unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let row = c
        .query_one(
            "select subject_type, subject_id, model_id, dimensions, embedding \
             from donto_vector where vector_id = $1",
            &[&vec_id],
        )
        .await.unwrap();
    assert_eq!(row.get::<_, String>("subject_type"), "statement");
    assert_eq!(row.get::<_, uuid::Uuid>("subject_id"), stmt_id);
    assert_eq!(row.get::<_, String>("model_id"), "test-model");
    assert_eq!(row.get::<_, i32>("dimensions"), 4);
    let stored: Vec<f32> = row.get("embedding");
    assert_eq!(stored, embedding);
}

#[tokio::test]
async fn store_vector_upserts() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("vec-upsert");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "vec-upsert").await;

    let stmt_id = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"), "ex:p", Object::iri("ex:o"),
            ).with_context(&ctx),
        )
        .await.unwrap();

    let v1: Vec<f32> = vec![1.0, 0.0, 0.0];
    let v2: Vec<f32> = vec![0.0, 1.0, 0.0];
    client.store_vector("statement", stmt_id, "model-A", Some("v1"), &v1).await.unwrap();
    client.store_vector("statement", stmt_id, "model-A", Some("v2"), &v2).await.unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let count: i64 = c
        .query_one(
            "select count(*) from donto_vector \
             where subject_type = 'statement' and subject_id = $1 and model_id = 'model-A'",
            &[&stmt_id],
        )
        .await.unwrap().get(0);
    assert_eq!(count, 1, "upsert must not create duplicates");

    let stored: Vec<f32> = c
        .query_one(
            "select embedding from donto_vector \
             where subject_type = 'statement' and subject_id = $1 and model_id = 'model-A'",
            &[&stmt_id],
        )
        .await.unwrap().get(0);
    assert_eq!(stored, v2, "upsert must use the latest embedding");
}

#[tokio::test]
async fn cosine_similarity_correct() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    // Identical vectors → 1.0
    let sim: f64 = c
        .query_one(
            "select donto_cosine_similarity(array[1,0,0]::float4[], array[1,0,0]::float4[])",
            &[],
        )
        .await.unwrap().get(0);
    assert!((sim - 1.0).abs() < 1e-6);

    // Orthogonal vectors → 0.0
    let sim: f64 = c
        .query_one(
            "select donto_cosine_similarity(array[1,0,0]::float4[], array[0,1,0]::float4[])",
            &[],
        )
        .await.unwrap().get(0);
    assert!(sim.abs() < 1e-6);

    // Opposite vectors → -1.0
    let sim: f64 = c
        .query_one(
            "select donto_cosine_similarity(array[1,0]::float4[], array[-1,0]::float4[])",
            &[],
        )
        .await.unwrap().get(0);
    assert!((sim + 1.0).abs() < 1e-6);
}

#[tokio::test]
async fn cosine_dimension_mismatch_is_null() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let sim: Option<f64> = c
        .query_one(
            "select donto_cosine_similarity(array[1,0]::float4[], array[1,0,0]::float4[])",
            &[],
        )
        .await.unwrap().get(0);
    assert!(sim.is_none(), "dimension mismatch must return null");
}

#[tokio::test]
async fn nearest_vectors_ranked() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("vec-nn");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "vec-nn").await;

    let model_id = format!("test-nn-{}", uuid::Uuid::new_v4().simple());
    let mut ids = Vec::new();
    let embeddings: Vec<Vec<f32>> = vec![
        vec![1.0, 0.0, 0.0],   // close to query
        vec![0.0, 1.0, 0.0],   // orthogonal
        vec![0.9, 0.1, 0.0],   // very close to query
    ];
    for (i, emb) in embeddings.iter().enumerate() {
        let stmt_id = client
            .assert(
                &StatementInput::new(
                    format!("{prefix}/s{i}"), "ex:p", Object::iri(format!("ex:o{i}")),
                ).with_context(&ctx),
            )
            .await.unwrap();
        client.store_vector("statement", stmt_id, &model_id, None, emb).await.unwrap();
        ids.push(stmt_id);
    }

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let query_vec: Vec<f32> = vec![1.0, 0.0, 0.0];
    let rows = c
        .query(
            "select subject_id, similarity \
             from donto_nearest_vectors('statement', $2, $1::float4[], 3)",
            &[&query_vec, &model_id.as_str()],
        )
        .await.unwrap();

    assert!(rows.len() >= 3, "should return all 3 vectors");
    // First result should be the identical vector.
    let first_id: uuid::Uuid = rows[0].get("subject_id");
    let first_sim: f64 = rows[0].get("similarity");
    assert_eq!(first_id, ids[0]);
    assert!((first_sim - 1.0).abs() < 1e-6);

    // Second should be the 0.9/0.1 vector (high cosine sim).
    let second_id: uuid::Uuid = rows[1].get("subject_id");
    assert_eq!(second_id, ids[2]);
}
