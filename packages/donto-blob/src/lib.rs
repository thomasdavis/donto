//! Content-addressed blob store for donto.
//!
//! The substrate (`donto_blob` + `donto_document_revision.blob_hash`,
//! migration `0125`) is content-addressed by SHA-256. This crate
//! provides the storage layer above it: trait `BlobStore` with two
//! shipping backends and a third trivial mock for tests.
//!
//! | Backend            | Where bytes land                       | Auth needed |
//! |--------------------|----------------------------------------|-------------|
//! | `LocalFsBlobStore` | `/var/<root>/sha256/<hex>` on disk     | none        |
//! | `GcsBlobStore`     | `gs://<bucket>/sha256/<hex>` via gcloud| gcloud SA   |
//! | `MockBlobStore`    | in-memory `HashMap`                    | none (test) |
//!
//! The trait is small on purpose — every backend implements four
//! operations: `put` (idempotent, content-addressed), `exists`,
//! `fetch`, `list_uris`. Anything fancier (presigned URLs,
//! lifecycle rules, multipart upload) lives in the backend impl,
//! not the trait.
//!
//! ## Usage
//!
//! ```no_run
//! # async fn run() -> anyhow::Result<()> {
//! use std::path::Path;
//! use donto_blob::{BlobStore, LocalFsBlobStore};
//! let store = LocalFsBlobStore::new("/mnt/donto-data/blobs");
//! let summary = store
//!     .put_file(Path::new("/path/to/source.md"), Some("text/markdown"))
//!     .await?;
//! println!("uploaded {}: {} ({} bytes)",
//!     hex::encode(summary.sha256), summary.uri, summary.byte_size);
//! # Ok(()) }
//! ```
//!
//! For donto-client integration, see `register_with_db`.

use async_trait::async_trait;
use donto_client::DontoClient;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::io::AsyncReadExt;

pub mod gcs;
pub mod local;
#[cfg(any(test, feature = "mock"))]
pub mod mock;

pub use gcs::GcsBlobStore;
pub use local::LocalFsBlobStore;
#[cfg(any(test, feature = "mock"))]
pub use mock::MockBlobStore;

/// What a `put` returns: the canonical record of one blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobSummary {
    /// SHA-256 over the bytes (the content-address).
    pub sha256: [u8; 32],
    /// Total byte count.
    pub byte_size: u64,
    /// Bucket / filesystem URI where the bytes live now.
    pub uri: String,
    /// MIME type if known (sniffed from file extension or caller-supplied).
    pub mime_type: Option<String>,
    /// True if this `put` was a no-op (blob already existed).
    pub already_present: bool,
}

#[derive(Debug, Error)]
pub enum BlobError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid sha256: {0}")]
    BadHash(String),
    #[error("backend: {0}")]
    Backend(String),
    #[error("db: {0}")]
    Db(#[from] donto_client::Error),
}

/// Idempotent, content-addressed blob storage. All four operations
/// take or return a 32-byte SHA-256 — never a file path or URI
/// directly — so backend swaps are transparent to callers.
#[async_trait]
pub trait BlobStore: Send + Sync {
    /// Backend name for debug / status output.
    fn backend(&self) -> &'static str;

    /// Upload `bytes` if the blob isn't already present. The
    /// trait-level implementation hashes + dispatches; backend
    /// impls override `put_bytes_at_hash` instead.
    async fn put_bytes(
        &self,
        bytes: &[u8],
        mime_type: Option<&str>,
    ) -> Result<BlobSummary, BlobError> {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let sha = hasher.finalize();
        let mut sha_arr = [0u8; 32];
        sha_arr.copy_from_slice(&sha);
        self.put_bytes_at_hash(&sha_arr, bytes, mime_type).await
    }

    /// Implementation hook — caller has already computed the hash.
    /// Backend MUST check `exists(&sha)` before re-uploading.
    async fn put_bytes_at_hash(
        &self,
        sha256: &[u8; 32],
        bytes: &[u8],
        mime_type: Option<&str>,
    ) -> Result<BlobSummary, BlobError>;

    /// Stream a file from disk; saves on memory for large blobs.
    async fn put_file(
        &self,
        path: &Path,
        mime_type: Option<&str>,
    ) -> Result<BlobSummary, BlobError> {
        let mut f = tokio::fs::File::open(path).await?;
        // Two-pass: hash, then check, then upload. Avoids holding
        // the whole file in RAM for the hash step.
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            let n = f.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            total += n as u64;
        }
        let sha = hasher.finalize();
        let mut sha_arr = [0u8; 32];
        sha_arr.copy_from_slice(&sha);

        if self.exists(&sha_arr).await? {
            return Ok(BlobSummary {
                sha256: sha_arr,
                byte_size: total,
                uri: self.uri_for(&sha_arr),
                mime_type: mime_type.map(String::from).or_else(|| sniff_mime(path)),
                already_present: true,
            });
        }

        // Re-open and stream upload. For backends that support it,
        // a streaming put is the right answer; this fallback reads
        // the file again — fine for the local / GCS shell backends.
        let mut f2 = tokio::fs::File::open(path).await?;
        let mut buf2 = Vec::with_capacity(total as usize);
        f2.read_to_end(&mut buf2).await?;
        let mime = mime_type.map(String::from).or_else(|| sniff_mime(path));
        let mut summary = self
            .put_bytes_at_hash(&sha_arr, &buf2, mime.as_deref())
            .await?;
        summary.mime_type = mime;
        Ok(summary)
    }

    async fn exists(&self, sha256: &[u8; 32]) -> Result<bool, BlobError>;
    async fn fetch(&self, sha256: &[u8; 32]) -> Result<Vec<u8>, BlobError>;
    fn uri_for(&self, sha256: &[u8; 32]) -> String;
}

/// After a successful `put_*`, register the blob in the donto DB.
/// Two-step on purpose: backends never touch the DB; bookkeeping is
/// always the caller's choice.
pub async fn register_with_db(
    client: &DontoClient,
    summary: &BlobSummary,
) -> Result<(), BlobError> {
    let conn = client
        .pool()
        .get()
        .await
        .map_err(|e| BlobError::Db(donto_client::Error::Pool(e)))?;
    conn.execute(
        "select donto_register_blob($1, $2, $3, $4)",
        &[
            &summary.sha256.as_slice(),
            &(summary.byte_size as i64),
            &summary.mime_type,
            &summary.uri,
        ],
    )
    .await
    .map_err(|e| BlobError::Db(donto_client::Error::Postgres(e)))?;
    Ok(())
}

/// Best-effort MIME from file extension. Returns None for unknown
/// extensions — donto_blob.mime_type is nullable on purpose.
pub fn sniff_mime(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "md" | "markdown" => "text/markdown",
        "txt" | "text" | "log" => "text/plain",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "jsonl" | "ndjson" => "application/x-ndjson",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "yaml" | "yml" => "application/yaml",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "tiff" | "tif" => "image/tiff",
        "svg" => "image/svg+xml",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "ttl" => "text/turtle",
        "nt" | "nq" => "application/n-quads",
        "rdf" => "application/rdf+xml",
        "eaf" => "application/xml",
        "lift" => "application/xml",
        "conllu" => "text/plain",
        "ged" => "application/x-gedcom",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        _ => return None,
    }.to_string())
}

/// Hex-encode the 32-byte SHA. Convenience wrapper.
pub fn sha_hex(sha: &[u8; 32]) -> String {
    hex::encode(sha)
}

/// Decode a hex SHA-256 back to its 32-byte form. Errors if length
/// or character set is wrong.
pub fn sha_from_hex(s: &str) -> Result<[u8; 32], BlobError> {
    let bytes = hex::decode(s.trim()).map_err(|e| BlobError::BadHash(e.to_string()))?;
    if bytes.len() != 32 {
        return Err(BlobError::BadHash(format!(
            "expected 32 bytes (64 hex chars), got {}",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Standard relative key under any backend.
pub fn key_for(sha256: &[u8; 32]) -> PathBuf {
    let hex = hex::encode(sha256);
    PathBuf::from("sha256").join(hex)
}
