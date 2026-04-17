use anyhow::Result;
use donto_client::{DontoClient, StatementInput};
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct IngestReport {
    pub source: String,
    pub format: String,
    pub default_context: String,
    pub batches: u64,
    pub statements_in: u64,
    pub statements_inserted: u64,
    pub quarantined: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug)]
pub struct Pipeline<'a> {
    pub client: &'a DontoClient,
    pub default_context: String,
    pub batch_size: usize,
}

impl<'a> Pipeline<'a> {
    pub fn new(client: &'a DontoClient, default_context: impl Into<String>) -> Self {
        Self { client, default_context: default_context.into(), batch_size: 1000 }
    }

    pub fn batch_size(mut self, n: usize) -> Self { self.batch_size = n; self }

    pub async fn run<I>(&self, source: &str, format: &str, iter: I) -> Result<IngestReport>
    where
        I: IntoIterator<Item = StatementInput>,
    {
        let start = Instant::now();
        let stmts: Vec<StatementInput> = iter.into_iter().collect();
        let total = stmts.len() as u64;
        let mut batches = 0u64;
        let mut inserted = 0u64;
        for chunk in stmts.chunks(self.batch_size) {
            let n = self.client.assert_batch(chunk).await?;
            batches += 1;
            inserted += n as u64;
        }
        Ok(IngestReport {
            source: source.into(),
            format: format.into(),
            default_context: self.default_context.clone(),
            batches,
            statements_in: total,
            statements_inserted: inserted,
            quarantined: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
        })
    }
}
