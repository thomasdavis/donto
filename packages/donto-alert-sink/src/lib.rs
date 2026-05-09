//! donto-alert-sink — pluggable alert delivery for detector findings.
//!
//! Implements `donto_analytics::alert::AlertSink` with two concrete sinks:
//!
//! - `StdoutSink`  — writes newline-delimited JSON to stdout.
//! - `FileSink`    — appends newline-delimited JSON to a file path.
//!
//! # Selection
//!
//! Call `sink_from_env()` (or `AlertSinkBox::from_env()`) to read
//! `$DONTO_ALERT_SINK`:
//!
//! ```text
//! DONTO_ALERT_SINK=stdout          # StdoutSink
//! DONTO_ALERT_SINK=file:///var/log/donto-alerts.jsonl
//! DONTO_ALERT_SINK=file://alerts.jsonl   # relative path
//! ```
//!
//! # CLI wiring
//!
//! Detectors that accept `--alert-sink` pass the value through
//! `sink_from_spec(spec)`, which returns `Err(SinkError::UnrecognizedSpec)`
//! for any scheme that is not `"stdout"` or `"file://"`.  When `--alert-sink`
//! is omitted the default is DB-only (no emit).

use donto_analytics::alert::AlertSink;
use donto_analytics::findings::Finding;
use donto_analytics::SinkError;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

// ── StdoutSink ────────────────────────────────────────────────────────────────

/// Writes every finding as a JSON line to stdout.
pub struct StdoutSink;

impl AlertSink for StdoutSink {
    fn emit(&self, finding: &Finding) -> Result<(), SinkError> {
        let line = serde_json::to_string(finding)?;
        // stdout is line-buffered in a terminal; use write! + flush for
        // robustness when piped.
        let stdout = std::io::stdout();
        let mut h = stdout.lock();
        writeln!(h, "{line}")?;
        Ok(())
    }
}

// ── FileSink ──────────────────────────────────────────────────────────────────

/// Appends findings as JSON lines to a file.
///
/// The file is opened in append mode on each `emit` call so that rotations
/// work without restart.
pub struct FileSink {
    path: PathBuf,
    // Mutex only guards the open+write sequence; no persistent fd so file
    // rotation is transparent.
    _lock: Mutex<()>,
}

impl FileSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            _lock: Mutex::new(()),
        }
    }
}

impl AlertSink for FileSink {
    fn emit(&self, finding: &Finding) -> Result<(), SinkError> {
        let line = serde_json::to_string(finding)?;
        let _g = self._lock.lock().expect("FileSink lock poisoned");
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{line}")?;
        Ok(())
    }
}

// ── Factory ───────────────────────────────────────────────────────────────────

/// Heap-allocated sink, erasing the concrete type.
pub type AlertSinkBox = Box<dyn AlertSink>;

/// Build a sink from a spec string.
///
/// - `"stdout"` → `StdoutSink`
/// - `"file://path"` → `FileSink` at `path`
/// - anything else → `Err(SinkError::UnrecognizedSpec)`
///
/// The previous behaviour of silently falling through to `StdoutSink` for
/// unknown specs is intentionally removed: an unrecognized spec almost always
/// means a misconfigured pipeline, and silent stdout emission in that case
/// would produce unexpected output with no indication that the spec was ignored.
pub fn sink_from_spec(spec: &str) -> Result<AlertSinkBox, SinkError> {
    let spec = spec.trim();
    if spec == "stdout" {
        Ok(Box::new(StdoutSink))
    } else if let Some(path) = spec.strip_prefix("file://") {
        Ok(Box::new(FileSink::new(path)))
    } else {
        Err(SinkError::UnrecognizedSpec(spec.to_owned()))
    }
}

/// Read `$DONTO_ALERT_SINK` and build the appropriate sink.
///
/// Returns `Ok(None)` when the variable is absent or empty (DB-only, no
/// external emit).  Returns `Err(SinkError::UnrecognizedSpec)` when the
/// variable is set to an unrecognized scheme.
pub fn sink_from_env() -> Result<Option<AlertSinkBox>, SinkError> {
    match std::env::var("DONTO_ALERT_SINK") {
        Ok(v) if !v.is_empty() => sink_from_spec(&v).map(Some),
        _ => Ok(None),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn dummy_finding() -> Finding {
        Finding {
            finding_id: 1,
            detector_iri: "donto:detector/test".into(),
            target_kind: "rule".into(),
            target_id: "ex:rule/1".into(),
            severity: donto_analytics::findings::Severity::Warning,
            observed_at: Utc::now(),
            payload: serde_json::json!({"test": true}),
        }
    }

    #[test]
    fn file_sink_appends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("findings.jsonl");
        let sink = FileSink::new(&path);
        let f = dummy_finding();
        sink.emit(&f).unwrap();
        sink.emit(&f).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents.lines().count(), 2, "should append two lines");
    }

    #[test]
    fn sink_from_spec_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("out.jsonl");
        let spec = format!("file://{}", p.display());
        let sink = sink_from_spec(&spec).expect("file:// should parse");
        sink.emit(&dummy_finding()).unwrap();
        assert!(p.exists());
    }

    #[test]
    fn sink_from_spec_stdout() {
        let sink = sink_from_spec("stdout").expect("stdout should parse");
        // Just confirm it doesn't panic — we can't capture stdout in unit tests.
        let _ = sink; // StdoutSink is zero-sized
    }

    #[test]
    fn sink_from_spec_unrecognized_returns_err() {
        // Unknown schemes must not silently fall back to stdout.
        // Note: Result::unwrap_err / expect_err both require T: Debug, and
        // Box<dyn AlertSink> does not implement Debug.  Use match instead.
        match sink_from_spec("slack://hooks.example.com/xyz") {
            Ok(_) => panic!("expected Err for unrecognized scheme, got Ok"),
            Err(donto_analytics::SinkError::UnrecognizedSpec(s)) => {
                assert!(
                    s.contains("slack://"),
                    "error should carry the original spec, got: {s:?}"
                );
            }
            Err(other) => panic!("expected UnrecognizedSpec, got {other:?}"),
        }

        // Empty string is also unrecognized (caller should omit --alert-sink, not pass "").
        assert!(sink_from_spec("").is_err(), "empty spec should return Err");

        // Whitespace-only is trimmed to empty, also unrecognized.
        assert!(
            sink_from_spec("   ").is_err(),
            "blank spec should return Err"
        );
    }
}
