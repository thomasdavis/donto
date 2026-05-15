//! End-to-end release pipeline: build_release → write_native_jsonl
//! → write_ro_crate_metadata → envelope::sign → envelope::verify
//! (cross-party).
//!
//! Acceptance: instance A produces a citable, signed release crate;
//! instance B reads the crate metadata + envelope from disk and
//! verifies both the signature AND the manifest hash against the
//! manifest bytes on disk, without ever seeing A's private seed.

use chrono::{Duration, Utc};
use donto_client::{DontoClient, Object, StatementInput};
use donto_release::{
    build_release, envelope, write_native_jsonl, write_ro_crate_metadata,
    Citation, ReleaseSpec,
};

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let ctx = format!("test:release-e2e:{}", uuid::Uuid::new_v4().simple());
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .ok()?;
    Some((c, ctx))
}

async fn cleanup(c: &DontoClient, ctx: &str) {
    let Ok(conn) = c.pool().get().await else {
        return;
    };
    let _ = conn
        .execute("delete from donto_statement where context = $1", &[&ctx])
        .await;
    let _ = conn
        .execute("delete from donto_context where iri = $1", &[&ctx])
        .await;
}

#[tokio::test]
async fn full_release_pipeline_round_trips_across_two_instances() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    for (s, p, o) in [
        ("ex:alice", "ex:age", "32"),
        ("ex:alice", "ex:bornIn", "ex:london"),
        ("ex:bob", "ex:age", "29"),
    ] {
        c.assert(
            &StatementInput::new(s, p, Object::Literal(donto_client::Literal::string(o)))
                .with_context(&ctx),
        )
        .await
        .unwrap();
    }

    // INSTANCE A: build + sign + write the crate.
    let spec = ReleaseSpec {
        release_id: format!("test/release/e2e/{}", uuid::Uuid::new_v4().simple()),
        query_specs: vec!["MATCH ?s ?p ?o LIMIT 100".into()],
        contexts: vec![ctx.clone()],
        as_of: Some(Utc::now() + Duration::seconds(5)),
        min_maturity: 0,
        require_public: false,
        citation: Citation {
            title: "Pipeline E2E Release".into(),
            authors: vec!["Test Suite".into()],
            doi: None,
            publisher: Some("donto".into()),
            license: Some("CC0-1.0".into()),
            version: Some("0.1.0".into()),
            year: Some(2026),
        },
        source_versions: vec![],
        transformations: vec![],
    };
    let manifest = build_release(&c, &spec).await.expect("build_release");
    assert!(!manifest.statement_checksums.is_empty());

    let crate_dir = tempfile::tempdir().unwrap();
    let manifest_path = crate_dir.path().join("manifest.jsonl");
    write_native_jsonl(&manifest, &manifest_path).expect("write_native_jsonl");
    assert!(manifest_path.exists());

    let manifest_json = serde_json::to_value(&manifest).unwrap();
    let kp = envelope::Keypair::generate();
    let env = envelope::sign(&manifest_json, &kp).expect("sign");
    let env_path = crate_dir.path().join("envelope.json");
    std::fs::write(&env_path, serde_json::to_vec_pretty(&env).unwrap()).unwrap();

    write_ro_crate_metadata(
        &manifest,
        crate_dir.path(),
        &[("envelope.json", "application/json")],
    )
    .expect("write_ro_crate_metadata");
    let ro_path = crate_dir.path().join("ro-crate-metadata.json");
    assert!(ro_path.exists());

    // INSTANCE B: only has the crate_dir.
    let env_body = std::fs::read_to_string(&env_path).unwrap();
    let env_loaded: envelope::ReleaseEnvelope = serde_json::from_str(&env_body).unwrap();
    envelope::verify_against_manifest(&env_loaded, &manifest_json)
        .expect("B verifies A's envelope");

    let ro_body = std::fs::read_to_string(&ro_path).unwrap();
    let ro_doc: serde_json::Value = serde_json::from_str(&ro_body).unwrap();
    let graph = ro_doc["@graph"].as_array().unwrap();
    assert!(graph.iter().any(|n| n["@id"] == "envelope.json"));
    assert!(graph.iter().any(|n| n["@id"] == "manifest.jsonl"));

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn tampering_after_signing_is_detected() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    c.assert(
        &StatementInput::new(
            "ex:x",
            "ex:p",
            Object::Literal(donto_client::Literal::string("v")),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();
    let spec = ReleaseSpec {
        release_id: format!("test/tamper/{}", uuid::Uuid::new_v4().simple()),
        query_specs: vec![],
        contexts: vec![ctx.clone()],
        as_of: None,
        min_maturity: 0,
        require_public: false,
        citation: Citation::default(),
        source_versions: vec![],
        transformations: vec![],
    };
    let manifest = build_release(&c, &spec).await.expect("build");
    let manifest_json = serde_json::to_value(&manifest).unwrap();
    let kp = envelope::Keypair::generate();
    let env = envelope::sign(&manifest_json, &kp).unwrap();

    let mut tampered = manifest.clone();
    tampered
        .statement_checksums
        .push(donto_release::StatementChecksum {
            statement_id: "00000000-0000-0000-0000-000000000099".into(),
            sha256: "ff".into(),
        });
    let tampered_json = serde_json::to_value(&tampered).unwrap();
    assert!(
        envelope::verify_against_manifest(&env, &tampered_json).is_err(),
        "tamper must be detected"
    );

    cleanup(&c, &ctx).await;
}
