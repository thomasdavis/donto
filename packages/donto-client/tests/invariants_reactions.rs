//! Alexandria §3.2: reaction meta-statements.
//!
//!   * reactions are ordinary statements; they don't mutate their target
//!   * kind -> canonical predicate + polarity mapping is enforced
//!   * context = who reacted (provenance for free)
//!   * supersedes/cites require an object; endorses/rejects don't

use donto_client::{Object, Polarity, ReactionKind, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

#[tokio::test]
async fn endorsement_is_a_sibling_not_a_mutation() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("rx-endorse");
    cleanup_prefix(&client, &prefix).await;

    let author_ctx = format!("{prefix}/author");
    let reader_ctx = format!("{prefix}/reader/alice");
    client
        .ensure_context(&author_ctx, "user", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&reader_ctx, "user", "permissive", None)
        .await
        .unwrap();

    let s_id = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/claim"),
                "ex:says",
                Object::Literal(donto_client::Literal::string("earth is warming")),
            )
            .with_context(&author_ctx),
        )
        .await
        .unwrap();

    // Pre-reaction fingerprint.
    let pool = client.pool().get().await.unwrap();
    let hash_before: Vec<u8> = pool
        .query_one(
            "select content_hash from donto_statement where statement_id = $1",
            &[&s_id],
        )
        .await
        .unwrap()
        .get(0);

    let r_id = client
        .react(s_id, ReactionKind::Endorses, None, &reader_ctx, None)
        .await
        .unwrap();

    let hash_after: Vec<u8> = pool
        .query_one(
            "select content_hash from donto_statement where statement_id = $1",
            &[&s_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        hash_before, hash_after,
        "reacted-to statement must not change"
    );

    // reactions_for reports it under the reader's context.
    let reactions = client.reactions_for(s_id).await.unwrap();
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].reaction_id, r_id);
    assert_eq!(reactions[0].kind, ReactionKind::Endorses);
    assert_eq!(reactions[0].context, reader_ctx);
    assert_eq!(reactions[0].polarity, Polarity::Asserted);
}

#[tokio::test]
async fn rejection_carries_negated_polarity() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("rx-reject");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "rx-reject").await;

    let s_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/claim"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();
    client
        .react(s_id, ReactionKind::Rejects, None, &ctx, None)
        .await
        .unwrap();

    let reactions = client.reactions_for(s_id).await.unwrap();
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].kind, ReactionKind::Rejects);
    assert_eq!(reactions[0].polarity, Polarity::Negated);
}

#[tokio::test]
async fn cites_and_supersedes_require_object() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("rx-obj");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "rx-obj").await;

    let s_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/s"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    for kind in [ReactionKind::Cites, ReactionKind::Supersedes] {
        let err = client
            .react(s_id, kind, None, &ctx, None)
            .await
            .err()
            .unwrap_or_else(|| panic!("{kind:?} without object must error"));
        let msg = format!("{err:?}");
        assert!(
            msg.contains("object"),
            "expected object requirement in error, got: {msg}"
        );
    }

    // Same kinds succeed with an object.
    let t_id = client
        .assert(
            &StatementInput::new(format!("{prefix}/t"), "ex:p", Object::iri("ex:o2"))
                .with_context(&ctx),
        )
        .await
        .unwrap();
    let t_iri = format!("donto:stmt/{t_id}");
    client
        .react(s_id, ReactionKind::Supersedes, Some(&t_iri), &ctx, None)
        .await
        .unwrap();

    let reactions = client.reactions_for(s_id).await.unwrap();
    assert!(reactions
        .iter()
        .any(|r| r.kind == ReactionKind::Supersedes && r.object_iri.as_deref() == Some(&t_iri)));
}

#[tokio::test]
async fn reaction_to_nonexistent_statement_errors() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("rx-miss");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "rx-miss").await;

    let fake = uuid::Uuid::new_v4();
    let err = client
        .react(fake, ReactionKind::Endorses, None, &ctx, None)
        .await
        .err()
        .expect("reacting to a non-existent stmt must error");
    assert!(format!("{err:?}").contains("not found"));
}
