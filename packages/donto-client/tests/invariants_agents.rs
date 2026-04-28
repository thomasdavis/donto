//! Evidence substrate: agent registry and workspace bindings.
//!
//!   * ensure_agent is idempotent
//!   * agents can be bound to contexts with roles
//!   * context_agents and agent_contexts return correct results
//!   * agent types are validated

mod common;
use common::{connect, ctx, tag};

#[tokio::test]
async fn ensure_agent_is_idempotent() {
    let client = pg_or_skip!(connect().await);
    let iri = format!("test:agent/{}", tag("ag-idem"));

    let id1 = client
        .ensure_agent(&iri, "llm", Some("Claude"), Some("claude-sonnet-4-6"))
        .await.unwrap();
    let id2 = client
        .ensure_agent(&iri, "llm", Some("Claude"), Some("claude-sonnet-4-6"))
        .await.unwrap();
    assert_eq!(id1, id2);
}

#[tokio::test]
async fn bind_agent_to_context() {
    let client = pg_or_skip!(connect().await);
    let iri = format!("test:agent/{}", tag("ag-bind"));
    let ctx = ctx(&client, "ag-bind").await;

    let agent_id = client
        .ensure_agent(&iri, "extractor", Some("NER Bot"), None)
        .await.unwrap();

    client.bind_agent_context(agent_id, &ctx, "owner").await.unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();

    // agent_contexts lists the binding.
    let rows = c
        .query(
            "select context, role from donto_agent_contexts($1)",
            &[&agent_id],
        )
        .await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, String>("context"), ctx);
    assert_eq!(rows[0].get::<_, String>("role"), "owner");

    // context_agents lists the agent.
    let rows = c
        .query(
            "select agent_id, iri, agent_type, role from donto_context_agents($1)",
            &[&ctx],
        )
        .await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, uuid::Uuid>("agent_id"), agent_id);
    assert_eq!(rows[0].get::<_, String>("agent_type"), "extractor");
}

#[tokio::test]
async fn role_upgrade_on_rebind() {
    let client = pg_or_skip!(connect().await);
    let iri = format!("test:agent/{}", tag("ag-role"));
    let ctx = ctx(&client, "ag-role").await;

    let agent_id = client.ensure_agent(&iri, "human", None, None).await.unwrap();

    client.bind_agent_context(agent_id, &ctx, "reader").await.unwrap();
    client.bind_agent_context(agent_id, &ctx, "contributor").await.unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let role: String = c
        .query_one(
            "select role from donto_agent_context where agent_id = $1 and context = $2",
            &[&agent_id, &ctx],
        )
        .await.unwrap().get(0);
    assert_eq!(role, "contributor", "rebind must update the role");
}

#[tokio::test]
async fn invalid_agent_type_rejected() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let iri = format!("test:agent/{}", tag("ag-badtype"));
    let err = c
        .execute(
            "insert into donto_agent (iri, agent_type) values ($1, 'robot')",
            &[&iri],
        )
        .await
        .err()
        .expect("invalid agent_type must violate check");
    let msg = format!("{err:?}");
    assert!(msg.contains("agent_type"), "expected agent_type in error, got: {msg}");
}

#[tokio::test]
async fn multiple_agents_per_context() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ag-multi").await;

    let a1_iri = format!("test:agent/{}/a", tag("ag-multi"));
    let a2_iri = format!("test:agent/{}/b", tag("ag-multi"));
    let a1 = client.ensure_agent(&a1_iri, "llm", None, None).await.unwrap();
    let a2 = client.ensure_agent(&a2_iri, "human", None, None).await.unwrap();

    client.bind_agent_context(a1, &ctx, "owner").await.unwrap();
    client.bind_agent_context(a2, &ctx, "contributor").await.unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let count: i64 = c
        .query_one(
            "select count(*) from donto_context_agents($1)",
            &[&ctx],
        )
        .await.unwrap().get(0);
    assert_eq!(count, 2);
}
