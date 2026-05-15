use donto_client::{DontoClient, Polarity};
use donto_ling_unimorph::{ImportOptions, Importer};
use std::fs;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let ctx = format!("test:unimorph:{}", uuid::Uuid::new_v4().simple());
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
async fn imports_minimal_paradigm() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // Tiny English verb paradigm.
    let body = "speak\tspeak\tV;NFIN\nspeak\tspeaks\tV;PRS;3;SG\nspeak\tspoke\tV;PST\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("eng.unimorph");
    fs::write(&path, body).unwrap();

    let importer = Importer::new(&c, &ctx);
    let opts = ImportOptions {
        language: "eng".into(),
        ..ImportOptions::default()
    };
    let report = importer.import(&path, opts).await.expect("import");

    assert_eq!(report.lexemes_seen, 1, "one lemma → one lexeme");
    assert_eq!(report.forms_seen, 3);
    assert!(report.statements_inserted > 0);

    // Verify the lexeme IRI exists with a citation form.
    let rows = c
        .match_pattern(
            Some("unimorph:eng/lex/speak"),
            Some("unimorph:citationForm"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    // First form (idx 0) should link to that lexeme.
    let link = c
        .match_pattern(
            Some("unimorph:eng/form/speak/0"),
            Some("unimorph:lemma"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(link.len(), 1);
    match &link[0].object {
        donto_client::Object::Iri(i) => assert_eq!(i, "unimorph:eng/lex/speak"),
        other => panic!("expected IRI, got {other:?}"),
    }
    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn malformed_two_column_line_errors() {
    let Some((c, _)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let body = "speak\tspeaks\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("bad.unimorph");
    fs::write(&path, body).unwrap();
    let importer = Importer::new(&c, "test:unimorph:bad");
    assert!(importer
        .import(&path, ImportOptions::default())
        .await
        .is_err());
}

#[tokio::test]
async fn empty_tag_segments_recorded_as_loss() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // Trailing semicolon → empty tag segment.
    let body = "go\twent\tV;PST;\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("loss.unimorph");
    fs::write(&path, body).unwrap();
    let importer = Importer::new(&c, &ctx);
    let opts = ImportOptions {
        language: "eng".into(),
        ..ImportOptions::default()
    };
    let report = importer.import(&path, opts).await.unwrap();
    assert!(
        report.losses.iter().any(|l| l.contains("empty tag")),
        "expected empty-tag loss, got {:?}",
        report.losses
    );
    cleanup(&c, &ctx).await;
}
