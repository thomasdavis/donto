//! Local-filesystem blob store. Writes blobs to
//! `<root>/sha256/<hex>` atomically (write to `<hex>.tmp` then
//! rename). The root directory is created on demand.

use crate::{key_for, BlobError, BlobStore, BlobSummary};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct LocalFsBlobStore {
    root: PathBuf,
}

impl LocalFsBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn path_for(&self, sha256: &[u8; 32]) -> PathBuf {
        self.root.join(key_for(sha256))
    }
}

#[async_trait]
impl BlobStore for LocalFsBlobStore {
    fn backend(&self) -> &'static str {
        "local-fs"
    }

    async fn put_bytes_at_hash(
        &self,
        sha256: &[u8; 32],
        bytes: &[u8],
        _mime_type: Option<&str>,
    ) -> Result<BlobSummary, BlobError> {
        let final_path = self.path_for(sha256);
        if tokio::fs::try_exists(&final_path).await? {
            return Ok(BlobSummary {
                sha256: *sha256,
                byte_size: bytes.len() as u64,
                uri: self.uri_for(sha256),
                mime_type: _mime_type.map(String::from),
                already_present: true,
            });
        }
        if let Some(parent) = final_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        // Atomic write: temp then rename.
        let tmp_path = {
            let mut p = final_path.clone();
            let name = p
                .file_name()
                .ok_or_else(|| BlobError::Backend("no file name".into()))?
                .to_string_lossy()
                .to_string();
            p.set_file_name(format!("{name}.tmp"));
            p
        };
        let mut f = tokio::fs::File::create(&tmp_path).await?;
        f.write_all(bytes).await?;
        f.sync_all().await?;
        tokio::fs::rename(&tmp_path, &final_path).await?;
        Ok(BlobSummary {
            sha256: *sha256,
            byte_size: bytes.len() as u64,
            uri: self.uri_for(sha256),
            mime_type: _mime_type.map(String::from),
            already_present: false,
        })
    }

    async fn exists(&self, sha256: &[u8; 32]) -> Result<bool, BlobError> {
        Ok(tokio::fs::try_exists(self.path_for(sha256)).await?)
    }

    async fn fetch(&self, sha256: &[u8; 32]) -> Result<Vec<u8>, BlobError> {
        Ok(tokio::fs::read(self.path_for(sha256)).await?)
    }

    fn uri_for(&self, sha256: &[u8; 32]) -> String {
        format!("file://{}", self.path_for(sha256).display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[tokio::test]
    async fn round_trip_put_exists_fetch() {
        let tmp = tempfile::tempdir().unwrap();
        let store = LocalFsBlobStore::new(tmp.path());
        let summary = store
            .put_bytes(b"hello donto", Some("text/plain"))
            .await
            .unwrap();
        assert!(!summary.already_present);
        assert_eq!(summary.byte_size, 11);
        assert!(store.exists(&summary.sha256).await.unwrap());
        let bytes = store.fetch(&summary.sha256).await.unwrap();
        assert_eq!(bytes, b"hello donto");
    }

    #[tokio::test]
    async fn put_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let store = LocalFsBlobStore::new(tmp.path());
        let a = store.put_bytes(b"same bytes", None).await.unwrap();
        let b = store.put_bytes(b"same bytes", None).await.unwrap();
        assert_eq!(a.sha256, b.sha256);
        assert!(!a.already_present);
        assert!(b.already_present);
    }

    #[tokio::test]
    async fn put_file_matches_put_bytes_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let store = LocalFsBlobStore::new(tmp.path());
        let src = tmp.path().join("src.md");
        tokio::fs::write(&src, b"contents").await.unwrap();
        let from_file = store.put_file(&src, Some("text/markdown")).await.unwrap();
        let mut h = Sha256::new();
        h.update(b"contents");
        let expected: [u8; 32] = h.finalize().into();
        assert_eq!(from_file.sha256, expected);
    }

    #[tokio::test]
    async fn exists_is_false_for_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = LocalFsBlobStore::new(tmp.path());
        assert!(!store.exists(&[0u8; 32]).await.unwrap());
    }
}
