//! v1000 policy + attestation HTTP endpoints.
//!
//! These map directly onto the SQL substrate from migrations 0111
//! (policy capsule) and 0112 (attestation). They form the read/write
//! surface for the M0 Trust Kernel.

use crate::auth::{caller_from_headers, require_action, ActionRequirement};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct AssignPolicyReq {
    pub target_kind: String,
    pub target_id: String,
    pub policy_iri: String,
    #[serde(default = "default_assigned_by")]
    pub assigned_by: String,
}
fn default_assigned_by() -> String {
    "system".to_string()
}

pub async fn assign(
    State(s): State<Arc<AppState>>,
    Json(req): Json<AssignPolicyReq>,
) -> impl IntoResponse {
    match s
        .client
        .assign_policy(
            &req.target_kind,
            &req.target_id,
            &req.policy_iri,
            &req.assigned_by,
        )
        .await
    {
        Ok(id) => Json(json!({ "assignment_id": id })).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct IssueAttestationReq {
    pub holder: String,
    pub issuer: String,
    pub policy_iri: String,
    pub actions: Vec<String>,
    pub purpose: String,
    pub rationale: String,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn issue_attestation(
    State(s): State<Arc<AppState>>,
    Json(req): Json<IssueAttestationReq>,
) -> impl IntoResponse {
    let actions: Vec<&str> = req.actions.iter().map(String::as_str).collect();
    match s
        .client
        .issue_attestation(
            &req.holder,
            &req.issuer,
            &req.policy_iri,
            &actions,
            &req.purpose,
            &req.rationale,
            req.expires_at,
        )
        .await
    {
        Ok(id) => Json(json!({ "attestation_id": id })).into_response(),
        Err(e) => Json(json!({
            "error": "issue_failed",
            "detail": e.to_string(),
        }))
        .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct RevokeAttestationReq {
    #[serde(default = "default_revoked_by")]
    pub revoked_by: String,
    #[serde(default)]
    pub reason: Option<String>,
}
fn default_revoked_by() -> String {
    "system".to_string()
}

pub async fn revoke_attestation(
    State(s): State<Arc<AppState>>,
    Path(attestation_id): Path<Uuid>,
    Json(req): Json<RevokeAttestationReq>,
) -> impl IntoResponse {
    match s
        .client
        .revoke_attestation(attestation_id, &req.revoked_by, req.reason.as_deref())
        .await
    {
        Ok(true) => Json(json!({ "revoked": true, "attestation_id": attestation_id })).into_response(),
        Ok(false) => Json(json!({ "revoked": false, "reason": "already revoked or not found" }))
            .into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

/// Authorisation probe. Lets a client check whether the calling
/// agent can perform an action against a target without actually
/// performing the action. Useful for UIs that want to grey-out
/// disallowed buttons.
#[derive(Debug, Deserialize)]
pub struct AuthorisationProbeReq {
    pub target_kind: String,
    pub target_id: String,
    pub action: String,
}

pub async fn authorise_probe(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<AuthorisationProbeReq>,
) -> impl IntoResponse {
    let caller = caller_from_headers(&headers);
    match s
        .client
        .authorise(&caller, &req.target_kind, &req.target_id, &req.action)
        .await
    {
        Ok(allowed) => Json(json!({
            "caller": caller,
            "target_kind": req.target_kind,
            "target_id": req.target_id,
            "action": req.action,
            "allowed": allowed,
        }))
        .into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

/// Effective allowed_actions for a target without considering caller
/// attestations. Surfaces "what does this source permit anyone to
/// do" for inspection.
pub async fn effective_actions(
    State(s): State<Arc<AppState>>,
    Path((target_kind, target_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match s.client.effective_actions(&target_kind, &target_id).await {
        Ok(actions) => Json(actions).into_response(),
        Err(e) => Json(json!({ "error": e.to_string() })).into_response(),
    }
}

/// Demo endpoint that exercises `require_action` end-to-end. Use this
/// from tests to verify the middleware. Production callers should not
/// hit this directly.
#[derive(Debug, Deserialize)]
pub struct ProtectedReadReq {
    pub target_kind: String,
    pub target_id: String,
    pub action: String,
}

pub async fn protected_read(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ProtectedReadReq>,
) -> impl IntoResponse {
    let r = ActionRequirement {
        target_kind: &req.target_kind,
        target_id: &req.target_id,
        action: &req.action,
    };
    if let Err(resp) = require_action(&s, &headers, r).await {
        return resp;
    }
    Json(json!({
        "ok": true,
        "caller": caller_from_headers(&headers),
    }))
    .into_response()
}
