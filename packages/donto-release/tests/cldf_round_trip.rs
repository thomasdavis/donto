//! CLDF release-export round-trip:
//!
//!   write_cldf_release on a manifest's statements → CLDF directory
//!   → contents recover the same Languages / Parameters / Codes /
//!     Values that originally went in via donto-ling-cldf
//!
//! Note: this test does not pull the donto-ling-cldf crate (would be
//! a cyclic-ish workspace edge); instead it verifies the CSV files
//! parse with the standard `csv` crate and contain the expected
//! rows.

use chrono::{Duration, Utc};
use donto_client::{ContextScope, DontoClient, Object, Polarity, StatementInput};
use donto_release::{
    build_release, write_cldf_release, Citation, ReleaseSpec,
};
use std::collections::HashSet;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let ctx = format!("test:cldf-export:{}", uuid::Uuid::new_v4().simple());
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
async fn cldf_export_recovers_languages_parameters_codes_values() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };

    // Seed a minimal CLDF-shaped donto-quad graph by hand:
    //   2 languages: en, es
    //   1 parameter: cldf:wals-toy/param/bwo
    //   3 codes: svo, sov, vso
    //   2 values: en→svo, es→svo
    let lang_en = "cldf:wals-toy/lang/en";
    let lang_es = "cldf:wals-toy/lang/es";
    let param_bwo = "cldf:wals-toy/param/bwo";
    let code_svo = "cldf:wals-toy/code/svo";
    let code_sov = "cldf:wals-toy/code/sov";
    let code_vso = "cldf:wals-toy/code/vso";

    let seed = [
        (lang_en, "rdf:type", "ling:Language"),
        (lang_es, "rdf:type", "ling:Language"),
        (code_svo, "rdf:type", "ling:Code"),
        (code_sov, "rdf:type", "ling:Code"),
        (code_vso, "rdf:type", "ling:Code"),
        (code_svo, "ling:codeFor", param_bwo),
        (code_sov, "ling:codeFor", param_bwo),
        (code_vso, "ling:codeFor", param_bwo),
        (lang_en, param_bwo, code_svo),
        (lang_es, param_bwo, code_svo),
    ];
    for (s, p, o) in seed {
        c.assert(&StatementInput::new(s, p, Object::iri(o)).with_context(&ctx))
            .await
            .unwrap();
    }
    // Language names — emitted as literals.
    c.assert(
        &StatementInput::new(
            lang_en,
            "ling:name",
            Object::Literal(donto_client::Literal::string("English")),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new(
            lang_es,
            "ling:name",
            Object::Literal(donto_client::Literal::string("Spanish")),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();

    // Build a release manifest + pull the statements separately for the exporter.
    let spec = ReleaseSpec {
        release_id: format!("wals-toy/{}", uuid::Uuid::new_v4().simple()),
        query_specs: vec!["MATCH ?s ?p ?o LIMIT 10000".into()],
        contexts: vec![ctx.clone()],
        as_of: Some(Utc::now() + Duration::seconds(5)),
        min_maturity: 0,
        require_public: false,
        citation: Citation {
            title: "WALS toy round-trip".into(),
            ..Citation::default()
        },
        source_versions: vec![],
        transformations: vec![],
        adapter_losses: Default::default(),
        auto_citation: false,
    };
    let manifest = build_release(&c, &spec).await.expect("build_release");
    let scope = ContextScope::any_of(vec![ctx.clone()]);
    let stmts = c
        .match_pattern(None, None, None, Some(&scope), Some(Polarity::Asserted), 0, None, None)
        .await
        .unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let summary = write_cldf_release(&manifest, &stmts, tmp.path()).expect("export");

    assert_eq!(summary.languages, 2);
    assert_eq!(summary.parameters, 1);
    assert_eq!(summary.codes, 3);
    assert_eq!(summary.values, 2);
    assert_eq!(summary.lossy_count, 0);

    // Verify the four CSV files exist and contain the right IDs.
    let lang_csv = std::fs::read_to_string(tmp.path().join("languages.csv")).unwrap();
    assert!(lang_csv.contains("en,English"));
    assert!(lang_csv.contains("es,Spanish"));

    let param_csv = std::fs::read_to_string(tmp.path().join("parameters.csv")).unwrap();
    assert!(param_csv.contains("bwo"));

    let code_csv = std::fs::read_to_string(tmp.path().join("codes.csv")).unwrap();
    let lines: HashSet<&str> = code_csv.lines().collect();
    assert!(lines.iter().any(|l| l.starts_with("svo,bwo")));
    assert!(lines.iter().any(|l| l.starts_with("sov,bwo")));
    assert!(lines.iter().any(|l| l.starts_with("vso,bwo")));

    let value_csv = std::fs::read_to_string(tmp.path().join("values.csv")).unwrap();
    assert!(value_csv.contains("en,bwo,svo"));
    assert!(value_csv.contains("es,bwo,svo"));

    // Metadata JSON.
    let meta_path = tmp.path().join(format!(
        "{}-metadata.json",
        manifest
            .release_id
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>()
    ));
    let meta_body = std::fs::read_to_string(&meta_path).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_body).unwrap();
    let tables = meta["tables"].as_array().unwrap();
    assert_eq!(tables.len(), 4);

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn cldf_export_records_non_canonical_statements_as_lossy() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };

    // A language with one parameter value (canonical) plus a free-form
    // statement that doesn't fit the four CLDF tables.
    c.assert(
        &StatementInput::new(
            "cldf:t/lang/x",
            "rdf:type",
            Object::iri("ling:Language"),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();
    c.assert(
        &StatementInput::new("cldf:t/lang/x", "cldf:t/param/p", Object::iri("cldf:t/code/v"))
            .with_context(&ctx),
    )
    .await
    .unwrap();
    // Non-canonical: subject is NOT a registered Language.
    c.assert(
        &StatementInput::new(
            "cldf:t/orphan",
            "cldf:t/param/p",
            Object::iri("cldf:t/code/v"),
        )
        .with_context(&ctx),
    )
    .await
    .unwrap();

    let spec = ReleaseSpec {
        release_id: format!("lossy/{}", uuid::Uuid::new_v4().simple()),
        query_specs: vec![],
        contexts: vec![ctx.clone()],
        as_of: None,
        min_maturity: 0,
        require_public: false,
        citation: Citation::default(),
        source_versions: vec![],
        transformations: vec![],
        adapter_losses: Default::default(),
        auto_citation: false,
    };
    let manifest = build_release(&c, &spec).await.unwrap();
    let scope = ContextScope::any_of(vec![ctx.clone()]);
    let stmts = c
        .match_pattern(None, None, None, Some(&scope), Some(Polarity::Asserted), 0, None, None)
        .await
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let summary = write_cldf_release(&manifest, &stmts, tmp.path()).unwrap();
    assert!(
        summary.lossy_count >= 1,
        "expected at least one lossy statement, got {}",
        summary.lossy_count
    );
    cleanup(&c, &ctx).await;
}
