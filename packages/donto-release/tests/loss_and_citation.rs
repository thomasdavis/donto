//! Loss-report population + citation auto-extraction in
//! build_release. Both features land in the same spec / same
//! manifest so we test them together.

use chrono::Utc;
use donto_client::{DontoClient, Object, StatementInput};
use donto_release::{build_release, Citation, ReleaseSpec};
use std::collections::BTreeMap;

fn dsn() -> String {
    std::env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".into())
}

async fn boot() -> Option<(DontoClient, String)> {
    let c = DontoClient::from_dsn(&dsn()).ok()?;
    let _ = c.pool().get().await.ok()?;
    c.migrate().await.ok()?;
    let ctx = format!("test:loss-citation:{}", uuid::Uuid::new_v4().simple());
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
        .execute("delete from donto_document where iri = $1", &[&ctx])
        .await;
    let _ = conn
        .execute("delete from donto_context where iri = $1", &[&ctx])
        .await;
}

#[tokio::test]
async fn adapter_losses_fold_into_loss_report() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    c.assert(
        &StatementInput::new("ex:s", "ex:p", Object::iri("ex:o")).with_context(&ctx),
    )
    .await
    .unwrap();

    let mut losses = BTreeMap::new();
    losses.insert(
        "cldf".into(),
        "12 rows dropped: unmapped ExampleTable".into(),
    );
    losses.insert(
        "ud".into(),
        "3 rows dropped: empty-node placeholders skipped".into(),
    );

    let spec = ReleaseSpec {
        release_id: format!("test/loss/{}", uuid::Uuid::new_v4().simple()),
        query_specs: vec![],
        contexts: vec![ctx.clone()],
        as_of: Some(Utc::now() + chrono::Duration::seconds(5)),
        min_maturity: 0,
        require_public: false,
        citation: Citation::default(),
        source_versions: vec![],
        transformations: vec![],
        adapter_losses: losses,
        auto_citation: false,
    };
    let manifest = build_release(&c, &spec).await.unwrap();

    assert_eq!(
        manifest.loss_report.adapter_versions.len(),
        2,
        "both adapters recorded"
    );
    assert!(manifest.loss_report.adapter_versions.contains_key("cldf"));
    assert!(manifest.loss_report.adapter_versions.contains_key("ud"));
    assert!(
        manifest.loss_report.note.contains("cldf"),
        "note must mention cldf, got `{}`",
        manifest.loss_report.note
    );
    // 12 + 3 from the leading-integer parse.
    assert_eq!(manifest.loss_report.dropped_rows, 15);

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn citation_auto_fills_authors_and_year_from_documents() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    c.assert(
        &StatementInput::new("ex:s", "ex:p", Object::iri("ex:o")).with_context(&ctx),
    )
    .await
    .unwrap();
    // Register a donto_document whose IRI matches the context, so
    // the auto-extractor finds it.
    let conn = c.pool().get().await.unwrap();
    conn.execute(
        "insert into donto_document (iri, media_type, status, creators, source_date) \
         values ($1, 'text/plain', 'registered', \
                 jsonb_build_array(jsonb_build_object('name', 'Ada Lovelace'), 'Charles Babbage'), \
                 jsonb_build_object('year', 1843))",
        &[&ctx],
    )
    .await
    .unwrap();

    let spec = ReleaseSpec {
        release_id: format!("test/cite/{}", uuid::Uuid::new_v4().simple()),
        query_specs: vec![],
        contexts: vec![ctx.clone()],
        as_of: Some(Utc::now() + chrono::Duration::seconds(5)),
        min_maturity: 0,
        require_public: false,
        // Title supplied by caller — must NOT be overwritten.
        citation: Citation {
            title: "Original title (not overwritten)".into(),
            ..Citation::default()
        },
        source_versions: vec![],
        transformations: vec![],
        adapter_losses: BTreeMap::new(),
        auto_citation: true,
    };
    let manifest = build_release(&c, &spec).await.unwrap();

    assert_eq!(
        manifest.citation.title, "Original title (not overwritten)",
        "caller's title must not be overwritten"
    );
    assert_eq!(manifest.citation.year, Some(1843));
    let authors = manifest.citation.authors;
    assert_eq!(authors.len(), 2);
    assert!(authors.iter().any(|a| a == "Ada Lovelace"));
    assert!(authors.iter().any(|a| a == "Charles Babbage"));

    cleanup(&c, &ctx).await;
}

#[tokio::test]
async fn auto_citation_off_does_not_query_documents() {
    let Some((c, ctx)) = boot().await else {
        eprintln!("skip: no DB");
        return;
    };
    c.assert(
        &StatementInput::new("ex:s", "ex:p", Object::iri("ex:o")).with_context(&ctx),
    )
    .await
    .unwrap();
    let spec = ReleaseSpec {
        release_id: format!("test/noauto/{}", uuid::Uuid::new_v4().simple()),
        query_specs: vec![],
        contexts: vec![ctx.clone()],
        as_of: None,
        min_maturity: 0,
        require_public: false,
        citation: Citation::default(),
        source_versions: vec![],
        transformations: vec![],
        adapter_losses: BTreeMap::new(),
        auto_citation: false,
    };
    let manifest = build_release(&c, &spec).await.unwrap();
    assert!(manifest.citation.authors.is_empty());
    assert!(manifest.citation.year.is_none());
    cleanup(&c, &ctx).await;
}
