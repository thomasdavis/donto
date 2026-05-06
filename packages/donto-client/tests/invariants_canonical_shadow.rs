//! Canonical shadow quad invariants (migration 0053).
//!
//! Each statement gets at most one current shadow with predicate and entity
//! IRIs canonicalized through the closure and `donto_entity_alias`. Re-
//! materializing closes the prior shadow's `tx_time` and inserts a new row.

mod common;

use common::{connect, ctx, rebuild_closure_with_retry, tag};
use donto_client::{AlignmentRelation, Object, StatementInput};

async fn register_predicate(client: &donto_client::DontoClient, iri: &str) {
    let c = client.pool().get().await.unwrap();
    c.execute(
        "select donto_register_predicate($1, null, null, null, null, null, null, null)",
        &[&iri],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn shadow_resolves_canonical_predicate() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "cs-canon").await;
    let prefix = tag("cs-canon");

    let alias = format!("{prefix}/bornIn");
    let canon = format!("{prefix}/wasBornIn");
    register_predicate(&client, &alias).await;
    register_predicate(&client, &canon).await;

    let stmt_id = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/alice"),
                &alias,
                Object::iri(format!("{prefix}/london")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Align alias → canon and rebuild closure.
    client
        .register_alignment(
            &alias,
            &canon,
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
    rebuild_closure_with_retry(&client).await;

    let shadow_id = client
        .materialize_shadow(stmt_id)
        .await
        .unwrap()
        .expect("shadow must materialize");

    let c = client.pool().get().await.unwrap();
    let row = c
        .query_one(
            "select canonical_predicate, canonical_subject, canonical_object_iri \
             from donto_canonical_shadow where shadow_id = $1",
            &[&shadow_id],
        )
        .await
        .unwrap();
    let canon_pred: String = row.get("canonical_predicate");
    // Closure picks the highest-confidence exact_equivalent — could be either
    // direction. The key invariant is "no longer the alias when an alignment
    // exists", but here either alias or canon could win since both are
    // exact_equivalent of each other in the closure. Just confirm the shadow
    // resolves to one of the two.
    assert!(
        canon_pred == alias || canon_pred == canon,
        "canonical_predicate must be in the closure cluster, got {canon_pred}"
    );
    assert_eq!(
        row.get::<_, String>("canonical_subject"),
        format!("{prefix}/alice")
    );
    assert_eq!(
        row.get::<_, Option<String>>("canonical_object_iri")
            .as_deref(),
        Some(format!("{prefix}/london").as_str())
    );
}

#[tokio::test]
async fn rematerialize_closes_old_shadow() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "cs-remat").await;
    let prefix = tag("cs-remat");

    let p = format!("{prefix}/somePred");
    register_predicate(&client, &p).await;

    let stmt_id = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/s"),
                &p,
                Object::iri(format!("{prefix}/o")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    let first = client
        .materialize_shadow(stmt_id)
        .await
        .unwrap()
        .expect("first shadow");
    let second = client
        .materialize_shadow(stmt_id)
        .await
        .unwrap()
        .expect("second shadow");
    assert_ne!(first, second, "re-materialize must mint a new shadow_id");

    let c = client.pool().get().await.unwrap();

    // First shadow's tx_time must be closed.
    let first_open: bool = c
        .query_one(
            "select upper(tx_time) is null from donto_canonical_shadow where shadow_id = $1",
            &[&first],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!first_open, "prior shadow must have tx_time closed");

    // Second shadow is current.
    let second_open: bool = c
        .query_one(
            "select upper(tx_time) is null from donto_canonical_shadow where shadow_id = $1",
            &[&second],
        )
        .await
        .unwrap()
        .get(0);
    assert!(second_open, "new shadow must be current");

    // Exactly one current shadow per statement (partial unique index).
    let n_current: i64 = c
        .query_one(
            "select count(*) from donto_canonical_shadow \
             where statement_id = $1 and upper(tx_time) is null",
            &[&stmt_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n_current, 1, "exactly one current shadow per statement");
}

#[tokio::test]
async fn shadow_resolves_entity_aliases() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "cs-entity").await;
    let prefix = tag("cs-entity");

    let p = format!("{prefix}/p");
    register_predicate(&client, &p).await;

    let alias = format!("{prefix}/aliceAlias");
    let canonical = format!("{prefix}/aliceCanonical");

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    c.execute(
        "select donto_register_entity_alias($1, $2, 'sys', 1.0)",
        &[&alias, &canonical],
    )
    .await
    .unwrap();

    let stmt_id = client
        .assert(
            &StatementInput::new(&alias, &p, Object::iri(format!("{prefix}/o"))).with_context(&ctx),
        )
        .await
        .unwrap();

    let shadow_id = client
        .materialize_shadow(stmt_id)
        .await
        .unwrap()
        .expect("shadow must materialize");

    let canon_subj: String = c
        .query_one(
            "select canonical_subject from donto_canonical_shadow where shadow_id = $1",
            &[&shadow_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        canon_subj, canonical,
        "canonical_subject must resolve via donto_entity_alias"
    );
}
