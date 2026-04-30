//! Alignment run invariants (migration 0050).
//!
//! `donto_start_alignment_run` opens a run in 'running' status;
//! `donto_complete_alignment_run` sets the terminal status, completed_at, and
//! the proposed/accepted/rejected counters. Alignment registrations carry a
//! run_id pointer back to the run that produced them.

mod common;

use common::{connect, tag};
use donto_client::AlignmentRelation;

#[tokio::test]
async fn start_returns_uuid_with_running_status() {
    let client = pg_or_skip!(connect().await);

    let run_id = client
        .start_alignment_run("manual", Some("test-model"), None, None)
        .await
        .unwrap();

    let c = client.pool().get().await.unwrap();
    let row = c
        .query_one(
            "select run_type, model_id, status, completed_at \
             from donto_alignment_run where run_id = $1",
            &[&run_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("run_type"), "manual");
    assert_eq!(
        row.get::<_, Option<String>>("model_id").as_deref(),
        Some("test-model")
    );
    assert_eq!(row.get::<_, String>("status"), "running");
    assert!(
        row.get::<_, Option<chrono::DateTime<chrono::Utc>>>("completed_at")
            .is_none(),
        "completed_at must be null on a running run"
    );
}

#[tokio::test]
async fn complete_sets_status_and_counts() {
    let client = pg_or_skip!(connect().await);

    let run_id = client
        .start_alignment_run("lexical", None, None, None)
        .await
        .unwrap();
    client
        .complete_alignment_run(run_id, "completed", Some(10), Some(7), Some(3))
        .await
        .unwrap();

    let c = client.pool().get().await.unwrap();
    let row = c
        .query_one(
            "select status, alignments_proposed, alignments_accepted, \
                    alignments_rejected, completed_at \
             from donto_alignment_run where run_id = $1",
            &[&run_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>("status"), "completed");
    assert_eq!(row.get::<_, i32>("alignments_proposed"), 10);
    assert_eq!(row.get::<_, i32>("alignments_accepted"), 7);
    assert_eq!(row.get::<_, i32>("alignments_rejected"), 3);
    assert!(
        row.get::<_, Option<chrono::DateTime<chrono::Utc>>>("completed_at")
            .is_some(),
        "completed_at must be set after completion"
    );
}

#[tokio::test]
async fn alignment_registration_can_reference_run_id() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ar-fk");

    let run_id = client
        .start_alignment_run("manual", Some("test"), None, None)
        .await
        .unwrap();

    let alignment_id = client
        .register_alignment(
            &format!("{prefix}/a"),
            &format!("{prefix}/b"),
            AlignmentRelation::ExactEquivalent,
            1.0,
            None,
            None,
            Some(run_id),
            None,
            None,
        )
        .await
        .unwrap();

    let c = client.pool().get().await.unwrap();
    let stored: Option<uuid::Uuid> = c
        .query_one(
            "select run_id from donto_predicate_alignment where alignment_id = $1",
            &[&alignment_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(stored, Some(run_id), "alignment must point to its run");
}
