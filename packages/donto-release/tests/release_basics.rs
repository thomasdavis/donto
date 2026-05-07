//! Release builder skeleton — integration tests over a live Postgres.
//!
//! Tests assert the headline reproducibility property (same data → same
//! `manifest_sha256`), the policy gate (public release blocks unless
//! every contributing context permits anonymous read), and the
//! native JSONL export (deterministic line-by-line bytes).

use chrono::Utc;
use donto_client::{DontoClient, Object, StatementInput};
use donto_release::{build_release, write_native_jsonl, Citation, ReleaseSpec};

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let prefix = format!("test:release:{}", uuid::Uuid::new_v4().simple());
    let ctx = format!("{prefix}/ctx");
    c.ensure_context(&ctx, "custom", "permissive", None)
        .await
        .ok()?;
    Some((c, ctx, prefix))
}

#[tokio::test]
async fn empty_spec_produces_stable_manifest_with_no_statements() {
    let Some((client, _ctx, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let spec = ReleaseSpec {
        release_id: format!("{prefix}/empty"),
        contexts: vec![format!("{prefix}/no-such-context")],
        ..ReleaseSpec::new(format!("{prefix}/empty"))
    };
    let m = build_release(&client, &spec).await.unwrap();
    assert!(m.statement_checksums.is_empty());
    assert!(!m.manifest_sha256.is_empty());
    assert_eq!(m.manifest_sha256.len(), 64); // hex sha256
}

#[tokio::test]
async fn manifest_is_reproducible_over_unchanged_data() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };

    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx_iri),
        )
        .await
        .unwrap();

    let spec = ReleaseSpec {
        release_id: format!("{prefix}/r1"),
        contexts: vec![ctx_iri.clone()],
        ..ReleaseSpec::new(format!("{prefix}/r1"))
    };

    let m1 = build_release(&client, &spec).await.unwrap();
    let m2 = build_release(&client, &spec).await.unwrap();
    assert_eq!(
        m1.manifest_sha256, m2.manifest_sha256,
        "rebuilding the same spec over unchanged data must reproduce the hash"
    );
    assert_eq!(m1.statement_checksums.len(), 1);
}

#[tokio::test]
async fn manifest_changes_when_a_statement_is_added() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s1"),
                "ex:p",
                Object::iri(format!("{prefix}/o1")),
            )
            .with_context(&ctx_iri),
        )
        .await
        .unwrap();

    let spec = ReleaseSpec {
        release_id: format!("{prefix}/r"),
        contexts: vec![ctx_iri.clone()],
        ..ReleaseSpec::new(format!("{prefix}/r"))
    };
    let m1 = build_release(&client, &spec).await.unwrap();

    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s2"),
                "ex:p",
                Object::iri(format!("{prefix}/o2")),
            )
            .with_context(&ctx_iri),
        )
        .await
        .unwrap();

    let m2 = build_release(&client, &spec).await.unwrap();
    assert_ne!(
        m1.manifest_sha256, m2.manifest_sha256,
        "adding a statement must change the manifest hash"
    );
    assert_eq!(m2.statement_checksums.len(), 2);
}

#[tokio::test]
async fn require_public_blocks_release_when_anonymous_read_denied() {
    let Some((client, _ctx, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };

    // Use a private/restricted context: assign the private_research
    // policy if it exists, otherwise the test still proves the gate
    // works because anonymous read is denied by default.
    let private = format!("{prefix}/private");
    client
        .ensure_context(&private, "custom", "permissive", None)
        .await
        .unwrap();
    let _ = client
        .assign_policy("context", &private, "policy:private_research", "agent:test")
        .await;

    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                Object::iri(format!("{prefix}/o")),
            )
            .with_context(&private),
        )
        .await
        .unwrap();

    let spec = ReleaseSpec {
        release_id: format!("{prefix}/public-release"),
        contexts: vec![private.clone()],
        require_public: true,
        ..ReleaseSpec::new(format!("{prefix}/public-release"))
    };
    let res = build_release(&client, &spec).await;
    assert!(
        res.is_err(),
        "public release over private context must be refused"
    );
}

#[tokio::test]
async fn internal_release_records_policy_decisions_without_blocking() {
    let Some((client, ctx, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"),
                "ex:p",
                Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    let spec = ReleaseSpec {
        release_id: format!("{prefix}/internal"),
        contexts: vec![ctx.clone()],
        require_public: false,
        ..ReleaseSpec::new(format!("{prefix}/internal"))
    };
    let m = build_release(&client, &spec).await.unwrap();
    // Even if the context isn't anonymously-readable, internal releases
    // still build; the report just records the per-context decision.
    assert!(m.policy_report.decisions.contains_key(&ctx));
}

#[tokio::test]
async fn jsonl_export_is_deterministic() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    for i in 0..5 {
        client
            .assert(
                &StatementInput::new(
                    format!("{prefix}/s{i}"),
                    "ex:p",
                    Object::iri(format!("{prefix}/o{i}")),
                )
                .with_context(&ctx_iri),
            )
            .await
            .unwrap();
    }

    let spec = ReleaseSpec {
        release_id: format!("{prefix}/exp"),
        contexts: vec![ctx_iri.clone()],
        citation: Citation {
            title: "Test release".into(),
            authors: vec!["A. Tester".into()],
            year: Some(2026),
            ..Citation::default()
        },
        ..ReleaseSpec::new(format!("{prefix}/exp"))
    };
    let m = build_release(&client, &spec).await.unwrap();

    let dir = tempfile::tempdir().unwrap();
    let p1 = dir.path().join("a.jsonl");
    let p2 = dir.path().join("b.jsonl");
    write_native_jsonl(&m, &p1).unwrap();
    write_native_jsonl(&m, &p2).unwrap();
    let bytes_a = std::fs::read(&p1).unwrap();
    let bytes_b = std::fs::read(&p2).unwrap();
    assert_eq!(bytes_a, bytes_b, "JSONL export must be deterministic");

    let lines: Vec<_> = std::str::from_utf8(&bytes_a)
        .unwrap()
        .lines()
        .collect();
    assert_eq!(lines.len(), 1 + 5, "header + 5 statements");
}

#[tokio::test]
async fn citation_metadata_is_carried_through() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let spec = ReleaseSpec {
        release_id: format!("{prefix}/cite"),
        contexts: vec![ctx_iri.clone()],
        citation: Citation {
            title: "Donto Sample Release".into(),
            authors: vec!["Alice".into(), "Bob".into()],
            doi: Some("10.0000/donto.test".into()),
            license: Some("CC-BY-4.0".into()),
            year: Some(2026),
            ..Citation::default()
        },
        ..ReleaseSpec::new(format!("{prefix}/cite"))
    };
    let m = build_release(&client, &spec).await.unwrap();
    assert_eq!(m.citation.title, "Donto Sample Release");
    assert_eq!(m.citation.authors.len(), 2);
    assert_eq!(m.citation.doi.as_deref(), Some("10.0000/donto.test"));
}

#[tokio::test]
async fn source_versions_and_transformations_are_sorted_for_stable_hash() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let spec_a = ReleaseSpec {
        release_id: format!("{prefix}/sort"),
        contexts: vec![ctx_iri.clone()],
        source_versions: vec!["doc:b".into(), "doc:a".into(), "doc:c".into()],
        transformations: vec!["run:y".into(), "run:x".into()],
        ..ReleaseSpec::new(format!("{prefix}/sort"))
    };
    let spec_b = ReleaseSpec {
        source_versions: vec!["doc:c".into(), "doc:a".into(), "doc:b".into()],
        transformations: vec!["run:x".into(), "run:y".into()],
        ..spec_a.clone()
    };
    let m_a = build_release(&client, &spec_a).await.unwrap();
    let m_b = build_release(&client, &spec_b).await.unwrap();
    assert_eq!(
        m_a.manifest_sha256, m_b.manifest_sha256,
        "source/transformation ordering must not perturb the hash"
    );
}

#[tokio::test]
async fn as_of_lens_excludes_later_inserts() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };

    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/early"),
                "ex:p",
                Object::iri(format!("{prefix}/o-early")),
            )
            .with_context(&ctx_iri),
        )
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let cutoff = Utc::now();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/late"),
                "ex:p",
                Object::iri(format!("{prefix}/o-late")),
            )
            .with_context(&ctx_iri),
        )
        .await
        .unwrap();

    let spec = ReleaseSpec {
        release_id: format!("{prefix}/asof"),
        contexts: vec![ctx_iri.clone()],
        as_of: Some(cutoff),
        ..ReleaseSpec::new(format!("{prefix}/asof"))
    };
    let m = build_release(&client, &spec).await.unwrap();
    assert_eq!(
        m.statement_checksums.len(),
        1,
        "as_of lens before the late insert sees only the early row"
    );
}

#[tokio::test]
async fn min_maturity_filter_excludes_below_floor() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/raw"),
                "ex:p",
                Object::iri(format!("{prefix}/o-raw")),
            )
            .with_context(&ctx_iri)
            .with_maturity(0),
        )
        .await
        .unwrap();
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/curated"),
                "ex:p",
                Object::iri(format!("{prefix}/o-curated")),
            )
            .with_context(&ctx_iri)
            .with_maturity(2),
        )
        .await
        .unwrap();

    let spec = ReleaseSpec {
        release_id: format!("{prefix}/curated"),
        contexts: vec![ctx_iri.clone()],
        min_maturity: 2,
        ..ReleaseSpec::new(format!("{prefix}/curated"))
    };
    let m = build_release(&client, &spec).await.unwrap();
    assert_eq!(m.statement_checksums.len(), 1);
}

#[tokio::test]
async fn manifest_carries_release_id_and_query_specs() {
    let Some((client, ctx_iri, prefix)) = boot().await else {
        eprintln!("skip");
        return;
    };
    let spec = ReleaseSpec {
        release_id: format!("{prefix}/labelled"),
        contexts: vec![ctx_iri.clone()],
        query_specs: vec![
            "MATCH ?s ex:p ?o\nPROJECT ?s, ?o".into(),
            "MATCH ?s ex:q ?o".into(),
        ],
        ..ReleaseSpec::new(format!("{prefix}/labelled"))
    };
    let m = build_release(&client, &spec).await.unwrap();
    assert_eq!(m.release_id, format!("{prefix}/labelled"));
    assert_eq!(m.query_specs.len(), 2);
}
