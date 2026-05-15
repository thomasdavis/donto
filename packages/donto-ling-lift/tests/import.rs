use donto_client::{DontoClient, Polarity};
use donto_ling_lift::{ImportOptions, Importer};
use std::fs;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let ctx = format!("test:lift:{}", uuid::Uuid::new_v4().simple());
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

const TINY_LIFT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<lift version="0.13">
  <entry id="cat-1">
    <lexical-unit>
      <form lang="en"><text>cat</text></form>
    </lexical-unit>
    <sense id="cat-1-sense-1">
      <grammatical-info value="noun"/>
      <gloss lang="es"><text>gato</text></gloss>
      <definition><form lang="en"><text>a small domesticated carnivorous mammal</text></form></definition>
    </sense>
  </entry>
  <entry id="dog-1">
    <lexical-unit>
      <form lang="en"><text>dog</text></form>
    </lexical-unit>
    <sense id="dog-1-sense-1">
      <gloss lang="es"><text>perro</text></gloss>
    </sense>
  </entry>
</lift>
"#;

#[tokio::test]
async fn imports_minimal_lift_dictionary() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("tiny.lift");
    fs::write(&path, TINY_LIFT).unwrap();

    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(&path, ImportOptions::default())
        .await
        .expect("import");

    assert_eq!(report.entries_seen, 2);
    assert_eq!(report.senses_seen, 2);
    assert!(report.statements_inserted > 0);

    // cat-1 sense has grammaticalCategory = noun
    let g = c
        .match_pattern(
            Some("lift:sense/cat-1-sense-1"),
            Some("lift:grammaticalCategory"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(g.len(), 1);
    match &g[0].object {
        donto_client::Object::Literal(l) => assert_eq!(l.v, serde_json::json!("noun")),
        other => panic!("got {other:?}"),
    }

    // Entry → sense link
    let link = c
        .match_pattern(
            Some("lift:entry/cat-1"),
            Some("lift:hasSense"),
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

    // Spanish gloss on cat sense
    let gloss = c
        .match_pattern(
            Some("lift:sense/cat-1-sense-1"),
            Some("lift:gloss/es"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(gloss.len(), 1);
    match &gloss[0].object {
        donto_client::Object::Literal(l) => assert_eq!(l.v, serde_json::json!("gato")),
        other => panic!("got {other:?}"),
    }

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn unhandled_lift_elements_are_recorded_as_loss() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // Include a <pronunciation> element that this v1 doesn't map.
    let body = r#"<?xml version="1.0"?>
<lift version="0.13">
  <entry id="word-1">
    <lexical-unit><form lang="en"><text>word</text></form></lexical-unit>
    <pronunciation><form lang="en"><text>/wɜːrd/</text></form></pronunciation>
    <sense id="word-1-s1"><gloss lang="es"><text>palabra</text></gloss></sense>
  </entry>
</lift>"#;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("with-pron.lift");
    fs::write(&path, body).unwrap();

    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(&path, ImportOptions::default())
        .await
        .unwrap();
    assert!(
        report.losses.iter().any(|l| l.contains("pronunciation")),
        "expected pronunciation loss, got {:?}",
        report.losses
    );
    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn entry_without_id_is_skipped_with_loss() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let body = r#"<?xml version="1.0"?>
<lift>
  <entry>
    <lexical-unit><form lang="en"><text>nameless</text></form></lexical-unit>
  </entry>
</lift>"#;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("noid.lift");
    fs::write(&path, body).unwrap();
    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(&path, ImportOptions::default())
        .await
        .unwrap();
    assert!(
        report.losses.iter().any(|l| l.contains("without id")),
        "expected loss line, got {:?}",
        report.losses
    );
    cleanup(&c, &ctx).await;
}
