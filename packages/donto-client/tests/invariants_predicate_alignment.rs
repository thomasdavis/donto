//! Predicate alignment registration invariants (migration 0048).
//!
//! Six relation types — exact_equivalent, inverse_equivalent, sub_property_of,
//! close_match, decomposition, not_equivalent — recorded in
//! `donto_predicate_alignment`. Bitemporal and append-only: `retract_alignment`
//! closes `tx_time` rather than deleting.

mod common;

use chrono::NaiveDate;
use common::{connect, tag};
use donto_client::AlignmentRelation;

#[tokio::test]
async fn register_exact_equivalent_returns_id() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pa-exact");

    let source = format!("{prefix}/bornIn");
    let target = format!("{prefix}/wasBornIn");

    let id = client
        .register_alignment(
            &source,
            &target,
            AlignmentRelation::ExactEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let relation: String = c
        .query_one(
            "select relation from donto_predicate_alignment where alignment_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(relation, "exact_equivalent");
}

#[tokio::test]
async fn register_inverse_equivalent() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pa-inv");

    let source = format!("{prefix}/parentOf");
    let target = format!("{prefix}/childOf");

    let id = client
        .register_alignment(
            &source,
            &target,
            AlignmentRelation::InverseEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let row = c
        .query_one(
            "select source_iri, target_iri, relation from donto_predicate_alignment \
             where alignment_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("source_iri"), source);
    assert_eq!(row.get::<_, String>("target_iri"), target);
    assert_eq!(row.get::<_, String>("relation"), "inverse_equivalent");
}

#[tokio::test]
async fn register_all_six_relation_types() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pa-all");

    let relations = [
        AlignmentRelation::ExactEquivalent,
        AlignmentRelation::InverseEquivalent,
        AlignmentRelation::SubPropertyOf,
        AlignmentRelation::CloseMatch,
        AlignmentRelation::Decomposition,
        AlignmentRelation::NotEquivalent,
    ];

    for (i, rel) in relations.iter().enumerate() {
        let source = format!("{prefix}/s{i}");
        let target = format!("{prefix}/t{i}");
        let id = client
            .register_alignment(&source, &target, *rel, 0.9, None, None, None, None, None)
            .await
            .unwrap_or_else(|e| panic!("relation {} should register: {e:?}", rel.as_str()));

        let pool = client.pool();
        let c = pool.get().await.unwrap();
        let stored: String = c
            .query_one(
                "select relation from donto_predicate_alignment where alignment_id = $1",
                &[&id],
            )
            .await
            .unwrap()
            .get(0);
        assert_eq!(stored, rel.as_str());
    }
}

#[tokio::test]
async fn self_alignment_rejected() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pa-self");

    let iri = format!("{prefix}/p");
    let err = client
        .register_alignment(
            &iri,
            &iri,
            AlignmentRelation::ExactEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .err()
        .expect("self-alignment must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("source cannot equal target") || msg.contains("donto_pa_distinct"),
        "expected self-alignment rejection, got: {msg}"
    );
}

#[tokio::test]
async fn retract_closes_tx_time() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pa-retract");

    let id = client
        .register_alignment(
            &format!("{prefix}/a"),
            &format!("{prefix}/b"),
            AlignmentRelation::ExactEquivalent,
            1.0,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // First retract returns true.
    let first = client.retract_alignment(id).await.unwrap();
    assert!(first, "first retract must return true");

    // Second retract returns false (no current row).
    let second = client.retract_alignment(id).await.unwrap();
    assert!(!second, "second retract must return false");

    // Row's tx_time has been closed.
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let open: bool = c
        .query_one(
            "select upper(tx_time) is null from donto_predicate_alignment where alignment_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!open, "tx_time must be closed after retract");
}

#[tokio::test]
async fn registration_with_confidence_validity_provenance() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("pa-rich");

    let source = format!("{prefix}/sourcePred");
    let target = format!("{prefix}/targetPred");
    let valid_lo = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
    let valid_hi = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    let provenance = serde_json::json!({"method": "manual", "reviewer": "alice"});

    let id = client
        .register_alignment(
            &source,
            &target,
            AlignmentRelation::CloseMatch,
            0.85,
            Some(valid_lo),
            Some(valid_hi),
            None,
            Some(&provenance),
            Some("alice"),
        )
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let row = c
        .query_one(
            "select confidence, lower(valid_time) as vlo, upper(valid_time) as vhi, \
                    provenance, registered_by \
             from donto_predicate_alignment where alignment_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let conf: f64 = row.get("confidence");
    assert!((conf - 0.85).abs() < 1e-9);
    assert_eq!(row.get::<_, Option<NaiveDate>>("vlo"), Some(valid_lo));
    assert_eq!(row.get::<_, Option<NaiveDate>>("vhi"), Some(valid_hi));
    let prov: serde_json::Value = row.get("provenance");
    assert_eq!(prov["method"], "manual");
    assert_eq!(prov["reviewer"], "alice");
    assert_eq!(
        row.get::<_, Option<String>>("registered_by").as_deref(),
        Some("alice")
    );
}
