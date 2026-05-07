//! Trust Kernel HTTP middleware.
//!
//! Reads `X-Donto-Caller: <agent-iri>` and (optionally)
//! `X-Donto-Action: <action>` from the incoming request. For routes
//! that declare a required action, calls `donto_authorise()` and
//! returns 403 when the caller cannot perform the action against
//! the target.
//!
//! The middleware is **opt-in per route**: handlers that need
//! authorisation declare the requirement explicitly via
//! [`require_action`]. Routes that don't declare one continue to
//! work without authentication (preserving M0's "substrate exists,
//! application-layer enforcement is incremental" posture).
//!
//! Caller identity is conveyed via `X-Donto-Caller` (no signing yet —
//! that's a v1010 verifiable-credentials integration). Treat this
//! as advisory in untrusted networks; production deployments should
//! pair it with an authenticating reverse proxy.

use crate::AppState;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

/// Anonymous caller IRI used when no `X-Donto-Caller` header is set.
/// Only the default-restricted policy applies; no attestation lookup
/// can succeed.
pub const ANONYMOUS_CALLER: &str = "agent:anonymous";

/// Extract the caller IRI from `X-Donto-Caller`, or fall back to the
/// anonymous identity. Trims whitespace; rejects empty or whitespace-
/// only headers (returns anonymous so the request still proceeds
/// against fail-closed defaults).
pub fn caller_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-donto-caller")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| ANONYMOUS_CALLER.to_string())
}

/// Required-action declaration for an endpoint.
///
/// `target_kind` and `target_id` describe the protected resource;
/// `action` is one of the 15 PRD actions
/// (`read_metadata` | `read_content` | `quote` | … | `train_model`
/// | `publish_release` | …). See migration 0111.
#[derive(Debug, Clone)]
pub struct ActionRequirement<'a> {
    pub target_kind: &'a str,
    pub target_id: &'a str,
    pub action: &'a str,
}

/// Run the authorisation check. Returns `Ok(())` on allow, `Err(...)`
/// on deny. The Err response is a JSON 403 with structured detail
/// suitable for client-side handling.
pub async fn require_action(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    req: ActionRequirement<'_>,
) -> Result<(), Response> {
    let caller = caller_from_headers(headers);
    match state
        .client
        .authorise(&caller, req.target_kind, req.target_id, req.action)
        .await
    {
        Ok(true) => Ok(()),
        Ok(false) => Err(forbidden(&caller, &req)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "authorisation_check_failed",
                "detail": e.to_string(),
            })),
        )
            .into_response()),
    }
}

fn forbidden(caller: &str, req: &ActionRequirement<'_>) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "forbidden",
            "rule": "donto.authorise",
            "caller": caller,
            "target_kind": req.target_kind,
            "target_id": req.target_id,
            "action": req.action,
            "remedy": "obtain a non-revoked, non-expired attestation \
                      for one of the policies assigned to this target",
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn caller_from_present_header() {
        let mut h = HeaderMap::new();
        h.insert("x-donto-caller", HeaderValue::from_static("agent:alice"));
        assert_eq!(caller_from_headers(&h), "agent:alice");
    }

    #[test]
    fn caller_falls_back_to_anonymous_when_missing() {
        let h = HeaderMap::new();
        assert_eq!(caller_from_headers(&h), ANONYMOUS_CALLER);
    }

    #[test]
    fn caller_falls_back_when_header_is_empty() {
        let mut h = HeaderMap::new();
        h.insert("x-donto-caller", HeaderValue::from_static(""));
        assert_eq!(caller_from_headers(&h), ANONYMOUS_CALLER);
    }

    #[test]
    fn caller_falls_back_when_header_is_whitespace() {
        let mut h = HeaderMap::new();
        h.insert("x-donto-caller", HeaderValue::from_static("   "));
        assert_eq!(caller_from_headers(&h), ANONYMOUS_CALLER);
    }

    #[test]
    fn caller_trims_whitespace() {
        let mut h = HeaderMap::new();
        h.insert("x-donto-caller", HeaderValue::from_static("  agent:bob  "));
        assert_eq!(caller_from_headers(&h), "agent:bob");
    }
}
