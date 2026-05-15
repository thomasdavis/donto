//! End-to-end tests for the CLDF importer. Each test writes a
//! synthetic CLDF dataset to a tempdir, runs the importer against
//! a live donto-pg-test, and asserts the resulting quads.

use donto_client::{DontoClient, Polarity};
use donto_ling_cldf::{ImportOptions, Importer};
use std::fs;
use std::path::Path;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let ctx = format!("test:cldf:{}", uuid::Uuid::new_v4().simple());
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

/// Build a minimal CLDF StructureDataset fixture in `dir`:
///   - 2 languages (English, Spanish)
///   - 1 parameter (basicWordOrder)
///   - 3 codes (SVO, SOV, VSO)
///   - 2 values (English → SVO, Spanish → SVO)
fn write_synthetic_dataset(dir: &Path) {
    let meta = r#"{
        "@context": ["http://www.w3.org/ns/csvw", {"@language": "en"}],
        "dc:title": "WALS-toy",
        "dc:identifier": "cldf:wals-toy",
        "tables": [
            {"url": "languages.csv", "dc:conformsTo": "http://cldf.clld.org/v1.0/terms.rdf#LanguageTable"},
            {"url": "parameters.csv", "dc:conformsTo": "http://cldf.clld.org/v1.0/terms.rdf#ParameterTable"},
            {"url": "codes.csv", "dc:conformsTo": "http://cldf.clld.org/v1.0/terms.rdf#CodeTable"},
            {"url": "values.csv", "dc:conformsTo": "http://cldf.clld.org/v1.0/terms.rdf#ValueTable"}
        ]
    }"#;
    fs::write(dir.join("wals-toy-metadata.json"), meta).unwrap();

    fs::write(
        dir.join("languages.csv"),
        "ID,Name,Glottocode\nen,English,stan1293\nes,Spanish,stan1288\n",
    )
    .unwrap();

    fs::write(
        dir.join("parameters.csv"),
        "ID,Name,Description\nbwo,basicWordOrder,Basic word order of the language\n",
    )
    .unwrap();

    fs::write(
        dir.join("codes.csv"),
        "ID,Parameter_ID,Name\nsvo,bwo,SVO\nsov,bwo,SOV\nvso,bwo,VSO\n",
    )
    .unwrap();

    fs::write(
        dir.join("values.csv"),
        "ID,Language_ID,Parameter_ID,Value\nen-bwo,en,bwo,svo\nes-bwo,es,bwo,svo\n",
    )
    .unwrap();
}

#[tokio::test]
async fn imports_minimal_structure_dataset() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    write_synthetic_dataset(tmp.path());

    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(tmp.path(), ImportOptions::default())
        .await
        .expect("import");

    assert_eq!(report.languages_seen, 2);
    assert_eq!(report.parameters_seen, 1);
    assert_eq!(report.codes_seen, 3);
    assert_eq!(report.values_seen, 2);
    assert!(report.statements_inserted >= 11); // 2 langs × 3 + 3 codes × 3 + 2 values
    assert!(
        report.losses.is_empty(),
        "expected no losses, got {:?}",
        report.losses
    );
    assert_eq!(report.dataset_iri.as_deref(), Some("cldf:wals-toy"));

    // Concretely: English should have predicate basicWordOrder
    // pointing at code SVO.
    let rows = c
        .match_pattern(
            Some("cldf:wals-toy/lang/en"),
            Some("cldf:wals-toy/param/bwo"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "exactly one value claim");
    match &rows[0].object {
        donto_client::Object::Iri(iri) => {
            assert_eq!(iri, "cldf:wals-toy/code/svo");
        }
        other => panic!("expected IRI object, got {other:?}"),
    }

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn reports_extra_tables_as_loss() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    write_synthetic_dataset(tmp.path());
    // Hack: append an ExampleTable entry to the metadata so the
    // importer must report it as loss.
    let meta_path = tmp.path().join("wals-toy-metadata.json");
    let mut meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["tables"].as_array_mut().unwrap().push(serde_json::json!({
        "url": "examples.csv",
        "dc:conformsTo": "http://cldf.clld.org/v1.0/terms.rdf#ExampleTable"
    }));
    fs::write(meta_path, serde_json::to_string(&meta).unwrap()).unwrap();
    // Also create an empty examples.csv so the parser doesn't error
    // looking for it (it skips missing files).
    fs::write(tmp.path().join("examples.csv"), "ID,Primary_Text\n").unwrap();

    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(tmp.path(), ImportOptions::default())
        .await
        .unwrap();
    assert!(
        report.losses.iter().any(|l| l.contains("ExampleTable")),
        "expected ExampleTable in losses, got {:?}",
        report.losses
    );

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn strict_mode_aborts_on_loss() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    write_synthetic_dataset(tmp.path());
    // Add an unrepresented table.
    let meta_path = tmp.path().join("wals-toy-metadata.json");
    let mut meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["tables"].as_array_mut().unwrap().push(serde_json::json!({
        "url": "borrowings.csv",
        "dc:conformsTo": "http://cldf.clld.org/v1.0/terms.rdf#BorrowingTable"
    }));
    fs::write(meta_path, serde_json::to_string(&meta).unwrap()).unwrap();
    fs::write(tmp.path().join("borrowings.csv"), "ID\n").unwrap();

    let importer = Importer::new(&c, &ctx);
    let opts = ImportOptions {
        strict: true,
        ..ImportOptions::default()
    };
    let err = importer.import(tmp.path(), opts).await.err();
    assert!(err.is_some(), "strict mode must abort");

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn missing_metadata_errors_cleanly() {
    let Some((c, _ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    // No metadata file written.
    let importer = Importer::new(&c, "test:cldf:missing");
    let err = importer
        .import(tmp.path(), ImportOptions::default())
        .await
        .err();
    assert!(err.is_some());
}

#[tokio::test]
async fn unknown_language_id_in_value_row_reports_loss() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    write_synthetic_dataset(tmp.path());
    // Append a value row referencing a non-existent language ID.
    fs::write(
        tmp.path().join("values.csv"),
        "ID,Language_ID,Parameter_ID,Value\n\
         en-bwo,en,bwo,svo\n\
         es-bwo,es,bwo,svo\n\
         ghost-bwo,ghost,bwo,svo\n",
    )
    .unwrap();

    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(tmp.path(), ImportOptions::default())
        .await
        .unwrap();
    assert_eq!(
        report.values_seen, 3,
        "still counts the row as 'seen'"
    );
    assert!(
        report.losses.iter().any(|l| l.contains("ghost-bwo")),
        "ghost row should be in losses, got {:?}",
        report.losses
    );
    // Only the two valid rows produce statements (plus the entity
    // type assertions for langs and codes).
    let value_rows = c
        .match_pattern(
            None,
            Some("cldf:wals-toy/param/bwo"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(value_rows.len(), 2);

    cleanup(&c, &ctx).await;
}
