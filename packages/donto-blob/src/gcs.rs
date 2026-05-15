//! Google Cloud Storage blob backend, implemented by shelling out
//! to the `gcloud storage` CLI. This sidesteps the Rust GCS-client
//! ecosystem's metadata-server / workload-identity gymnastics —
//! `gcloud` handles auth correctly for VM service accounts, user
//! accounts, and impersonation alike.
//!
//! Caveats:
//!   * Each `put` spawns a `gcloud storage cp` process. Fine for
//!     dozens / hundreds of blobs; for >10K, swap in a native
//!     Rust client.
//!   * The VM service account on this box has `devstorage.read_only`
//!     scope today, so `put` errors with 403. Migrate to GCS by
//!     running this backend from a machine with write auth, or
//!     extend the VM's scopes.

use crate::{key_for, BlobError, BlobStore, BlobSummary};
use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct GcsBlobStore {
    bucket: String,
    /// Optional `--configuration=<name>` for `gcloud` calls.
    /// Useful when the active config doesn't have write scope but
    /// a sibling config does (e.g. `apex-personal`).
    configuration: Option<String>,
}

impl GcsBlobStore {
    pub fn new(bucket: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            configuration: None,
        }
    }
    pub fn with_configuration(mut self, config: impl Into<String>) -> Self {
        self.configuration = Some(config.into());
        self
    }
    fn key(&self, sha256: &[u8; 32]) -> PathBuf {
        key_for(sha256)
    }
    fn gs_uri(&self, sha256: &[u8; 32]) -> String {
        format!("gs://{}/{}", self.bucket, self.key(sha256).display())
    }
    fn gcloud(&self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new("gcloud");
        if let Some(cfg) = &self.configuration {
            cmd.arg(format!("--configuration={cfg}"));
        }
        cmd
    }
}

#[async_trait]
impl BlobStore for GcsBlobStore {
    fn backend(&self) -> &'static str {
        "gcs"
    }

    async fn put_bytes_at_hash(
        &self,
        sha256: &[u8; 32],
        bytes: &[u8],
        mime_type: Option<&str>,
    ) -> Result<BlobSummary, BlobError> {
        if self.exists(sha256).await? {
            return Ok(BlobSummary {
                sha256: *sha256,
                byte_size: bytes.len() as u64,
                uri: self.gs_uri(sha256),
                mime_type: mime_type.map(String::from),
                already_present: true,
            });
        }
        // Write to a temp file then `gcloud storage cp`. Streaming
        // via gcloud's --content-type doesn't accept stdin reliably
        // across versions, so a tempfile is the portable answer.
        let tmp = tempfile::NamedTempFile::new()?;
        tokio::fs::write(tmp.path(), bytes).await?;
        let mut cmd = self.gcloud();
        cmd.args(["storage", "cp", "--no-clobber"]);
        if let Some(mt) = mime_type {
            cmd.args(["--content-type", mt]);
        }
        cmd.arg(tmp.path()).arg(self.gs_uri(sha256));
        let out = cmd.output().await?;
        if !out.status.success() {
            return Err(BlobError::Backend(format!(
                "gcloud storage cp failed (exit {}): {}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(BlobSummary {
            sha256: *sha256,
            byte_size: bytes.len() as u64,
            uri: self.gs_uri(sha256),
            mime_type: mime_type.map(String::from),
            already_present: false,
        })
    }

    async fn exists(&self, sha256: &[u8; 32]) -> Result<bool, BlobError> {
        let mut cmd = self.gcloud();
        cmd.args(["storage", "ls", &self.gs_uri(sha256)]);
        let out = cmd.output().await?;
        if out.status.success() {
            Ok(true)
        } else {
            // gcloud returns non-zero for missing objects. Distinguish
            // between "missing" (OK, return false) and other errors.
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("not found") || stderr.contains("404") {
                Ok(false)
            } else {
                Err(BlobError::Backend(format!(
                    "gcloud storage ls failed: {}",
                    stderr.trim()
                )))
            }
        }
    }

    async fn fetch(&self, sha256: &[u8; 32]) -> Result<Vec<u8>, BlobError> {
        let tmp = tempfile::NamedTempFile::new()?;
        let mut cmd = self.gcloud();
        cmd.args(["storage", "cp", &self.gs_uri(sha256)])
            .arg(tmp.path());
        let out = cmd.output().await?;
        if !out.status.success() {
            return Err(BlobError::Backend(format!(
                "gcloud storage cp (fetch) failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(tokio::fs::read(tmp.path()).await?)
    }

    fn uri_for(&self, sha256: &[u8; 32]) -> String {
        self.gs_uri(sha256)
    }
}
