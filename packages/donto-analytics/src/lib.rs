//! donto-analytics — telemetry analysis and anomaly detection for donto.
//!
//! Four public modules:
//! - `time_series` — rolling-window statistics (median, MAD, z-equivalent).
//! - `features`    — feature extractors that query the DB.
//! - `findings`    — typed wrapper over `donto_detector_finding`.
//! - `alert`       — `AlertSink` trait; wire sinks without coupling detectors.

#![warn(missing_debug_implementations, rust_2018_idioms)]

pub mod alert;
pub mod analyzer_paraconsistency;
pub mod analyzer_reviewer_acceptance;
pub mod detector_rule_duration;
pub mod features;
pub mod findings;
pub mod time_series;

/// Unified error type for analytics operations.
#[derive(Debug, thiserror::Error)]
pub enum AnalyticsError {
    #[error("database error: {0}")]
    Db(#[from] donto_client::Error),

    #[error("pool error: {0}")]
    Pool(#[from] deadpool_postgres::PoolError),

    #[error("alert sink error: {0}")]
    Sink(#[from] SinkError),
}

/// Error returned by `AlertSink::emit`.
#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("unrecognized sink spec: {0:?} (supported: \"stdout\", \"file:///abs/path\")")]
    UnrecognizedSpec(String),
}
