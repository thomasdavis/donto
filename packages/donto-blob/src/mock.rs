//! In-memory blob backend for tests.

use crate::{BlobError, BlobStore, BlobSummary};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct MockBlobStore {
    inner: Arc<Mutex<HashMap<[u8; 32], Vec<u8>>>>,
}

impl MockBlobStore {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl BlobStore for MockBlobStore {
    fn backend(&self) -> &'static str {
        "mock"
    }
    async fn put_bytes_at_hash(
        &self,
        sha256: &[u8; 32],
        bytes: &[u8],
        mime_type: Option<&str>,
    ) -> Result<BlobSummary, BlobError> {
        let mut g = self.inner.lock().unwrap();
        let already = g.contains_key(sha256);
        if !already {
            g.insert(*sha256, bytes.to_vec());
        }
        Ok(BlobSummary {
            sha256: *sha256,
            byte_size: bytes.len() as u64,
            uri: self.uri_for(sha256),
            mime_type: mime_type.map(String::from),
            already_present: already,
        })
    }
    async fn exists(&self, sha256: &[u8; 32]) -> Result<bool, BlobError> {
        Ok(self.inner.lock().unwrap().contains_key(sha256))
    }
    async fn fetch(&self, sha256: &[u8; 32]) -> Result<Vec<u8>, BlobError> {
        self.inner
            .lock()
            .unwrap()
            .get(sha256)
            .cloned()
            .ok_or_else(|| BlobError::Backend("mock: not found".into()))
    }
    fn uri_for(&self, sha256: &[u8; 32]) -> String {
        format!("mock://{}", hex::encode(sha256))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_round_trip() {
        let s = MockBlobStore::new();
        let summary = s.put_bytes(b"x", None).await.unwrap();
        assert!(s.exists(&summary.sha256).await.unwrap());
        assert_eq!(s.fetch(&summary.sha256).await.unwrap(), b"x");
        let again = s.put_bytes(b"x", None).await.unwrap();
        assert!(again.already_present);
        assert_eq!(s.len(), 1);
    }
}
