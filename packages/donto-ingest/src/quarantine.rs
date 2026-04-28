//! Quarantine helper. When a curated context rejects a statement (Phase 5+
//! shape enforcement), we redirect the rejected statements into a
//! per-source quarantine context. Operators promote or discard from there.

use anyhow::Result;
use donto_client::{DontoClient, StatementInput};

pub fn quarantine_iri(source: &str) -> String {
    format!("ctx:quarantine/{}", sanitize(source))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub async fn route(
    client: &DontoClient,
    source: &str,
    stmts: Vec<StatementInput>,
) -> Result<usize> {
    let qctx = quarantine_iri(source);
    client
        .ensure_context(&qctx, "quarantine", "permissive", None)
        .await?;
    let mut rerouted = Vec::with_capacity(stmts.len());
    for s in stmts {
        rerouted.push(StatementInput {
            context: qctx.clone(),
            ..s
        });
    }
    let n = client.assert_batch(&rerouted).await?;
    Ok(n)
}
