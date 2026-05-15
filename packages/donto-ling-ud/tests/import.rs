//! End-to-end tests for the CoNLL-U importer.

use donto_client::{DontoClient, Polarity};
use donto_ling_ud::{ImportOptions, Importer};
use std::fs;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let ctx = format!("test:ud:{}", uuid::Uuid::new_v4().simple());
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

const TINY_CONLLU: &str = "\
# sent_id = s1
# text = The cat sleeps.
1\tThe\tthe\tDET\tDT\tDefinite=Def|PronType=Art\t2\tdet\t_\t_
2\tcat\tcat\tNOUN\tNN\tNumber=Sing\t3\tnsubj\t_\tSpaceAfter=No
3\tsleeps\tsleep\tVERB\tVBZ\tNumber=Sing|Tense=Pres\t0\troot\t_\t_

# sent_id = s2
# text = Dogs bark.
1\tDogs\tdog\tNOUN\tNNS\tNumber=Plur\t2\tnsubj\t_\t_
2\tbark\tbark\tVERB\tVBP\tNumber=Plur\t0\troot\t_\tSpaceAfter=No
";

#[tokio::test]
async fn imports_minimal_two_sentence_corpus() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("tiny.conllu");
    fs::write(&path, TINY_CONLLU).unwrap();

    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(&path, ImportOptions::default())
        .await
        .expect("import");

    assert_eq!(report.sentences_seen, 2);
    assert_eq!(report.tokens_seen, 5);
    assert!(report.statements_inserted > 0);

    // Sentence s1's root token should have ud:isRoot = true.
    let rows = c
        .match_pattern(
            Some("ud:tok/s1/3"),
            Some("ud:isRoot"),
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

    // The DET token's UPOS should be upos:DET (IRI form).
    let upos_rows = c
        .match_pattern(
            Some("ud:tok/s1/1"),
            Some("ud:upos"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(upos_rows.len(), 1);
    match &upos_rows[0].object {
        donto_client::Object::Iri(iri) => assert_eq!(iri, "upos:DET"),
        other => panic!("expected IRI, got {other:?}"),
    }

    // Head links: the DET should have ud:head pointing at token 2.
    let head_rows = c
        .match_pattern(
            Some("ud:tok/s1/1"),
            Some("ud:head"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(head_rows.len(), 1);
    match &head_rows[0].object {
        donto_client::Object::Iri(iri) => assert_eq!(iri, "ud:tok/s1/2"),
        other => panic!("expected IRI, got {other:?}"),
    }

    // FEATS are split into ud:feat:<Name> predicates.
    let plural_rows = c
        .match_pattern(
            Some("ud:tok/s2/1"),
            Some("ud:feat:Number"),
            None,
            Some(&donto_client::ContextScope::just(&ctx)),
            Some(Polarity::Asserted),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(plural_rows.len(), 1);

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn records_mwt_ranges_as_loss() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let body = "\
# sent_id = es1
# text = al perro
1-2\tal\t_\t_\t_\t_\t_\t_\t_\t_
1\ta\ta\tADP\t_\t_\t3\tcase\t_\t_
2\tel\tel\tDET\t_\t_\t3\tdet\t_\t_
3\tperro\tperro\tNOUN\t_\t_\t0\troot\t_\t_
";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("mwt.conllu");
    fs::write(&path, body).unwrap();

    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(&path, ImportOptions::default())
        .await
        .unwrap();
    assert!(
        report.losses.iter().any(|l| l.contains("multi-word token range `1-2`")),
        "expected MWT loss line, got {:?}",
        report.losses
    );
    // Three tokens (not counting the MWT range).
    assert_eq!(report.tokens_seen, 3);

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn records_empty_nodes_as_loss() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let body = "\
# sent_id = enh1
# text = … (gapping)
1\tShe\tshe\tPRON\t_\t_\t2\tnsubj\t_\t_
2\tate\teat\tVERB\t_\t_\t0\troot\t_\t_
3\tfish\tfish\tNOUN\t_\t_\t2\tobj\t_\t_
4\tand\tand\tCCONJ\t_\t_\t6\tcc\t_\t_
5\the\the\tPRON\t_\t_\t6\tnsubj\t_\t_
5.1\tate\teat\tVERB\t_\t_\t_\t_\t_\t_
6\tpork\tpork\tNOUN\t_\t_\t2\tconj\t_\t_
";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("empty.conllu");
    fs::write(&path, body).unwrap();

    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(&path, ImportOptions::default())
        .await
        .unwrap();
    assert!(
        report.losses.iter().any(|l| l.contains("empty node `5.1`")),
        "expected empty-node loss line, got {:?}",
        report.losses
    );

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn malformed_line_returns_parse_error() {
    let Some((c, _ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // Only 9 columns instead of 10.
    let body = "1\tHi\thi\tINTJ\t_\t_\t0\troot\t_\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("bad.conllu");
    fs::write(&path, body).unwrap();

    let importer = Importer::new(&c, "test:ud:malformed");
    let err = importer
        .import(&path, ImportOptions::default())
        .await
        .err();
    assert!(err.is_some(), "expected parse error");
}

#[tokio::test]
async fn strict_mode_aborts_on_unhandled_comment() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    let body = "\
# sent_id = q1
# text = Hello
# annotator = test-user
1\tHello\thello\tINTJ\t_\t_\t0\troot\t_\t_
";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("strict.conllu");
    fs::write(&path, body).unwrap();

    let importer = Importer::new(&c, &ctx);
    let opts = ImportOptions {
        strict: true,
        ..ImportOptions::default()
    };
    let err = importer.import(&path, opts).await.err();
    assert!(err.is_some(), "strict mode must abort on unhandled comments");

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn trailing_sentence_without_blank_line_still_parses() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    // No trailing newline; last sentence has no blank-line terminator.
    let body = "1\tWord\tword\tNOUN\t_\t_\t0\troot\t_\t_";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("trail.conllu");
    fs::write(&path, body).unwrap();

    let importer = Importer::new(&c, &ctx);
    let report = importer
        .import(&path, ImportOptions::default())
        .await
        .unwrap();
    assert_eq!(report.sentences_seen, 1);
    assert_eq!(report.tokens_seen, 1);
    cleanup(&c, &ctx).await;
}
