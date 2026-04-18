//! Lean engine client (PRD §15: sidecar operational contract).
//!
//! Owns a long-lived `donto_engine` child process, exchanges DIR
//! envelopes (one JSON per line) over its stdin/stdout. Serialises
//! requests with a mutex so we never interleave reads/writes on the
//! single stdio pair.
//!
//! Design choices:
//!   * If the engine binary path is not configured, `LeanClient::new`
//!     returns `Ok(None)` — the rest of dontosrv keeps working with the
//!     Rust built-in shapes only. The sidecar contract guarantees donto
//!     stays usable when Lean is absent.
//!   * Per-request timeout is enforced. If Lean wedges, we return
//!     `sidecar_unavailable` rather than blocking the HTTP handler
//!     forever.
//!   * The first line the engine emits on launch is its `ready` banner;
//!     we read and discard it during init so the first real request
//!     gets the right response.

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const READY_TIMEOUT:   Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct LeanClient {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug)]
struct Inner {
    /// `Some` while the child is alive. We swap to `None` on a write or
    /// read failure so subsequent requests fail fast with
    /// `sidecar_unavailable` instead of blocking on a dead pipe.
    child: Option<ChildHandle>,
    /// Path to the engine binary, kept for restart attempts (Phase 6+).
    bin: String,
}

#[derive(Debug)]
struct ChildHandle {
    /// We keep the Child to reap it; killed on Drop.
    _child: Child,
    stdin:  ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl LeanClient {
    /// Spawn `donto_engine` and read its readiness banner. Returns
    /// `Ok(None)` if no path is configured (sidecar absent path); errors
    /// only on a real spawn failure.
    pub async fn try_spawn(bin: Option<&str>) -> Result<Option<Self>> {
        let Some(bin) = bin else { return Ok(None); };
        let mut cmd = Command::new(bin);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn()
            .with_context(|| format!("spawning lean engine at {bin}"))?;
        let stdin  = child.stdin.take().ok_or_else(|| anyhow!("no stdin on lean child"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout on lean child"))?;
        let mut stdout = BufReader::new(stdout);

        // Read banner line.
        let mut banner = String::new();
        let read = time::timeout(READY_TIMEOUT, stdout.read_line(&mut banner)).await;
        match read {
            Ok(Ok(0)) => return Err(anyhow!("lean engine exited before ready")),
            Ok(Ok(_)) => {
                let v: Value = serde_json::from_str(banner.trim())
                    .with_context(|| format!("parse banner: {banner:?}"))?;
                if v.get("kind").and_then(|x| x.as_str()) != Some("ready") {
                    return Err(anyhow!("lean engine did not greet with `ready`: {v}"));
                }
            }
            Ok(Err(e)) => return Err(anyhow!("read banner: {e}")),
            Err(_)     => return Err(anyhow!("lean engine did not greet within {:?}", READY_TIMEOUT)),
        }

        let handle = ChildHandle { _child: child, stdin, stdout };
        Ok(Some(Self {
            inner: Arc::new(Mutex::new(Inner { child: Some(handle), bin: bin.into() })),
        }))
    }

    /// Send one envelope, await one response. Closes the child on any I/O
    /// error so callers see a clean `sidecar_unavailable` thereafter.
    pub async fn send(&self, envelope: Value) -> Result<Value> {
        let mut g = self.inner.lock().await;
        let bin = g.bin.clone();
        let Some(handle) = g.child.as_mut() else {
            return Err(anyhow!("lean engine offline (binary: {bin})"));
        };

        let line = serde_json::to_string(&envelope)? + "\n";
        let result = time::timeout(REQUEST_TIMEOUT, async {
            handle.stdin.write_all(line.as_bytes()).await?;
            handle.stdin.flush().await?;
            let mut resp = String::new();
            let n = handle.stdout.read_line(&mut resp).await?;
            if n == 0 {
                return Err(anyhow!("lean engine closed stdout"));
            }
            let v: Value = serde_json::from_str(resp.trim())
                .with_context(|| format!("parse lean response: {resp:?}"))?;
            Ok::<_, anyhow::Error>(v)
        }).await;

        match result {
            Ok(Ok(v))   => Ok(v),
            Ok(Err(e))  => { g.child = None; Err(e) }
            Err(_)      => { g.child = None; Err(anyhow!("lean engine timeout after {:?}", REQUEST_TIMEOUT)) }
        }
    }

    /// `true` iff the child is still considered alive.
    pub async fn is_alive(&self) -> bool {
        self.inner.lock().await.child.is_some()
    }
}
