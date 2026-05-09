//! Paraconsistency density analyzer (C2).
//!
//! Algorithm:
//! 1. Call `fetch_paraconsistency_features` to aggregate (s,p) pairs with
//!    ≥2 distinct polarities over the requested window.
//! 2. Upsert each result into `donto_paraconsistency_density` using the
//!    correct partial-index `on conflict (subject, predicate, window_start)`
//!    form (CLAUDE.md SQL idiom — named constraint not available for partial
//!    unique index).
//! 3. Return a summary of rows upserted.

use chrono::{DateTime, Utc};
use donto_client::DontoClient;

use crate::features::fetch_paraconsistency_features;

/// Configuration for the paraconsistency analyzer.
#[derive(Debug, Clone)]
pub struct ParaconsistencyConfig {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
}

/// Summary returned after a run.
#[derive(Debug)]
pub struct ParaconsistencyRunReport {
    pub pairs_examined: u64,
    pub pairs_upserted: u64,
}

/// Run the paraconsistency analyzer: aggregate features and upsert into
/// `donto_paraconsistency_density`.
pub async fn run(
    client: &DontoClient,
    cfg: &ParaconsistencyConfig,
) -> Result<ParaconsistencyRunReport, donto_client::Error> {
    let features =
        fetch_paraconsistency_features(client, &cfg.window_start, &cfg.window_end).await?;

    let pairs_examined = features.len() as u64;
    let mut pairs_upserted = 0u64;

    let c = client.pool().get().await?;

    for f in &features {
        // sample_statements is Vec<Uuid>; we need to pass it as a slice.
        let samples: Vec<uuid::Uuid> = f.sample_statements.clone();

        c.execute(
            "insert into donto_paraconsistency_density
                 (subject, predicate, window_start, window_end,
                  distinct_polarities, distinct_contexts, conflict_score,
                  sample_statements, computed_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, now())
             on conflict (subject, predicate, window_start)
             do update set
                 window_end          = excluded.window_end,
                 distinct_polarities = excluded.distinct_polarities,
                 distinct_contexts   = excluded.distinct_contexts,
                 conflict_score      = excluded.conflict_score,
                 sample_statements   = excluded.sample_statements,
                 computed_at         = now()",
            &[
                &f.subject,
                &f.predicate,
                &cfg.window_start,
                &cfg.window_end,
                &(f.distinct_polarities as i32),
                &(f.distinct_contexts as i32),
                &f.conflict_score,
                &samples,
            ],
        )
        .await?;

        pairs_upserted += 1;
    }

    Ok(ParaconsistencyRunReport {
        pairs_examined,
        pairs_upserted,
    })
}
