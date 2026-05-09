//! `AlertSink` trait — the integration seam between detectors and alert delivery.
//!
//! Detectors call `record_finding(..., Some(&sink))` to emit findings above
//! `severity='info'` to an external channel without coupling to any specific
//! delivery mechanism.
//!
//! Concrete implementations live in `packages/donto-alert-sink`.  This module
//! carries only the trait definition so that `donto-analytics` stays dependency-
//! light and the impls can be opted-in by binaries only.

use crate::findings::Finding;
use crate::SinkError;

/// Deliver a finding to an external channel.
///
/// Implementations must be Send + Sync so that `record_finding` can call them
/// from an async context without boxing.
pub trait AlertSink: Send + Sync {
    fn emit(&self, finding: &Finding) -> Result<(), SinkError>;
}
