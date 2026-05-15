use donto_client::{DontoClient, Polarity};
use donto_ling_eaf::{ImportOptions, Importer};
use std::fs;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let ctx = format!("test:eaf:{}", uuid::Uuid::new_v4().simple());
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

// Minimal EAF document: one media, one tier with two annotations,
// and a referring tier with one REF annotation.
const TINY_EAF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ANNOTATION_DOCUMENT AUTHOR="test" FORMAT="3.0">
  <HEADER MEDIA_FILE="" TIME_UNITS="milliseconds">
    <MEDIA_DESCRIPTOR MEDIA_URL="file:///tmp/audio.wav" MIME_TYPE="audio/x-wav"/>
  </HEADER>
  <TIME_ORDER>
    <TIME_SLOT TIME_SLOT_ID="ts1" TIME_VALUE="0"/>
    <TIME_SLOT TIME_SLOT_ID="ts2" TIME_VALUE="1500"/>
    <TIME_SLOT TIME_SLOT_ID="ts3" TIME_VALUE="3000"/>
  </TIME_ORDER>
  <TIER LINGUISTIC_TYPE_REF="default" TIER_ID="utterance" PARTICIPANT="speaker-A">
    <ANNOTATION>
      <ALIGNABLE_ANNOTATION ANNOTATION_ID="a1" TIME_SLOT_REF1="ts1" TIME_SLOT_REF2="ts2">
        <ANNOTATION_VALUE>hello</ANNOTATION_VALUE>
      </ALIGNABLE_ANNOTATION>
    </ANNOTATION>
    <ANNOTATION>
      <ALIGNABLE_ANNOTATION ANNOTATION_ID="a2" TIME_SLOT_REF1="ts2" TIME_SLOT_REF2="ts3">
        <ANNOTATION_VALUE>world</ANNOTATION_VALUE>
      </ALIGNABLE_ANNOTATION>
    </ANNOTATION>
  </TIER>
  <TIER LINGUISTIC_TYPE_REF="gloss" TIER_ID="gloss">
    <ANNOTATION>
      <REF_ANNOTATION ANNOTATION_ID="g1" ANNOTATION_REF="a1">
        <ANNOTATION_VALUE>greeting</ANNOTATION_VALUE>
      </REF_ANNOTATION>
    </ANNOTATION>
  </TIER>
  <LINGUISTIC_TYPE LINGUISTIC_TYPE_ID="default" GRAPHIC_REFERENCES="false" TIME_ALIGNABLE="true"/>
</ANNOTATION_DOCUMENT>
"#;

#[tokio::test]
async fn imports_minimal_eaf_document() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("tiny.eaf");
    fs::write(&path, TINY_EAF).unwrap();

    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(&path, ImportOptions::default())
        .await
        .expect("import");

    assert_eq!(report.tiers_seen, 2);
    assert_eq!(report.annotations_seen, 3);
    assert!(report.statements_inserted > 0);
    assert!(
        report.losses.iter().any(|l| l.contains("LINGUISTIC_TYPE")),
        "expected LINGUISTIC_TYPE loss, got {:?}",
        report.losses
    );

    // a1 should have startMs=0, endMs=1500.
    let start_rows = c
        .match_pattern(
            Some("eaf:ann/a1"),
            Some("eaf:startMs"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(start_rows.len(), 1);
    match &start_rows[0].object {
        donto_client::Object::Literal(l) => assert_eq!(l.v, serde_json::json!(0)),
        other => panic!("got {other:?}"),
    }

    // a1 should have value="hello".
    let val_rows = c
        .match_pattern(
            Some("eaf:ann/a1"),
            Some("eaf:value"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(val_rows.len(), 1);

    // g1 (REF) should refer back to a1.
    let ref_rows = c
        .match_pattern(
            Some("eaf:ann/g1"),
            Some("eaf:refersTo"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(ref_rows.len(), 1);
    match &ref_rows[0].object {
        donto_client::Object::Iri(i) => assert_eq!(i, "eaf:ann/a1"),
        other => panic!("got {other:?}"),
    }

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn media_descriptors_recorded_on_document() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("tiny.eaf");
    fs::write(&path, TINY_EAF).unwrap();
    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(&path, ImportOptions::default())
        .await
        .unwrap();

    let media = c
        .match_pattern(
            Some(report.doc_iri.as_str()),
            Some("eaf:media"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(media.len(), 1);
    match &media[0].object {
        donto_client::Object::Literal(l) => {
            assert_eq!(l.v, serde_json::json!("file:///tmp/audio.wav"))
        }
        other => panic!("got {other:?}"),
    }

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn strict_mode_aborts_on_lint_loss() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("tiny.eaf");
    fs::write(&path, TINY_EAF).unwrap();
    let importer = Importer::new(&c, &ctx);
    let opts = ImportOptions {
        strict: true,
        ..ImportOptions::default()
    };
    let err = importer.import(&path, opts).await.err();
    assert!(
        err.is_some(),
        "strict mode must abort when LINGUISTIC_TYPE loss is recorded"
    );
    cleanup(&c, &ctx).await;
}
