//! Alexandria §3.3: rule-derived aggregates.
//!
//!   * weight = count(endorses) - count(rejects)
//!   * materializing into a derivation context gives Level-3 statements
//!     with lineage pointing at every input reaction
//!   * re-running over the same inputs is idempotent
//!   * changing inputs closes the prior weight and emits a new one
//!   * ephemeral weight_of read matches the materialized value

use donto_client::{ContextScope, Object, ReactionKind, StatementInput};

mod common;
use common::{cleanup_prefix, connect, tag};

async fn ensure_user_ctx(client: &donto_client::DontoClient, iri: &str) {
    client
        .ensure_context(iri, "user", "permissive", None)
        .await
        .unwrap();
}

#[tokio::test]
async fn endorsement_weight_over_a_scope() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("agg-weight");
    cleanup_prefix(&client, &prefix).await;

    let author = format!("{prefix}/author");
    let alice = format!("{prefix}/user/alice");
    let bob = format!("{prefix}/user/bob");
    let carol = format!("{prefix}/user/carol");
    let deriv = format!("{prefix}/deriv");
    for c in [&author, &alice, &bob, &carol] {
        ensure_user_ctx(&client, c).await;
    }

    let s_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/claim"), "ex:says", Object::iri("ex:fact"))
                .with_context(&author),
        )
        .await
        .unwrap();

    // Two endorsements, one rejection => weight = +1
    client
        .react(s_id, ReactionKind::Endorses, None, &alice, None)
        .await
        .unwrap();
    client
        .react(s_id, ReactionKind::Endorses, None, &bob, None)
        .await
        .unwrap();
    client
        .react(s_id, ReactionKind::Rejects, None, &carol, None)
        .await
        .unwrap();

    let scope = ContextScope {
        include: vec![alice.clone(), bob.clone(), carol.clone()],
        exclude: vec![],
        include_descendants: true,
        include_ancestors: false,
    };

    // Ephemeral read.
    assert_eq!(client.weight_of(s_id, Some(&scope)).await.unwrap(), 1);

    // Materialize into a derivation context.
    let emitted = client
        .compute_endorsement_weights(Some(&scope), &deriv, None)
        .await
        .unwrap();
    assert_eq!(emitted, 1);

    // Inspect the row + its lineage.
    let pool = client.pool().get().await.unwrap();
    let row = pool
        .query_one(
            "select statement_id, (object_lit->>'v')::bigint, donto_maturity(flags) \
             from donto_statement \
             where subject = donto_stmt_iri($1) and predicate = 'donto:weight' \
               and context = $2 and upper(tx_time) is null",
            &[&s_id, &deriv],
        )
        .await
        .unwrap();
    let weight_stmt_id: uuid::Uuid = row.get(0);
    let weight_val: i64 = row.get(1);
    let maturity: i32 = row.get(2);
    assert_eq!(weight_val, 1);
    assert_eq!(maturity, 3, "aggregate must be Level-3 (rule-derived)");

    let lineage_count: i64 = pool
        .query_one(
            "select count(*) from donto_stmt_lineage where statement_id = $1",
            &[&weight_stmt_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        lineage_count, 3,
        "lineage must point at all 3 input reactions"
    );
}

#[tokio::test]
async fn recomputation_is_idempotent_and_tracks_changes() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("agg-recomp");
    cleanup_prefix(&client, &prefix).await;

    let author = format!("{prefix}/author");
    let alice = format!("{prefix}/user/alice");
    let bob = format!("{prefix}/user/bob");
    let deriv = format!("{prefix}/deriv");
    for c in [&author, &alice, &bob] {
        ensure_user_ctx(&client, c).await;
    }
    let s_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/claim"), "ex:p", Object::iri("ex:o"))
                .with_context(&author),
        )
        .await
        .unwrap();

    client
        .react(s_id, ReactionKind::Endorses, None, &alice, None)
        .await
        .unwrap();

    let scope = ContextScope {
        include: vec![alice.clone(), bob.clone()],
        exclude: vec![],
        include_descendants: true,
        include_ancestors: false,
    };

    client
        .compute_endorsement_weights(Some(&scope), &deriv, None)
        .await
        .unwrap();
    client
        .compute_endorsement_weights(Some(&scope), &deriv, None)
        .await
        .unwrap();

    let pool = client.pool().get().await.unwrap();
    let open_rows: i64 = pool
        .query_one(
            "select count(*) from donto_statement \
             where subject = donto_stmt_iri($1) and predicate = 'donto:weight' \
               and context = $2 and upper(tx_time) is null",
            &[&s_id, &deriv],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(open_rows, 1, "idempotent re-run must not duplicate");

    // Add a rejection; weight goes from +1 to 0. Prior row closes.
    client
        .react(s_id, ReactionKind::Rejects, None, &bob, None)
        .await
        .unwrap();
    client
        .compute_endorsement_weights(Some(&scope), &deriv, None)
        .await
        .unwrap();

    let total_history: i64 = pool
        .query_one(
            "select count(*) from donto_statement \
             where subject = donto_stmt_iri($1) and predicate = 'donto:weight' and context = $2",
            &[&s_id, &deriv],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        total_history, 2,
        "prior weight row must persist in tx-history"
    );

    let current: i64 = pool
        .query_one(
            "select (object_lit->>'v')::bigint from donto_statement \
             where subject = donto_stmt_iri($1) and predicate = 'donto:weight' \
               and context = $2 and upper(tx_time) is null",
            &[&s_id, &deriv],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(current, 0);
}
