//! Policy gate for releases.
//!
//! Skeleton shape: `evaluate_policy` walks every distinct context that
//! contributed to the release and asks the database whether
//! `read+export` is permitted for the *anonymous* caller (the most
//! restrictive case — that's how a public release is gated).
//!
//! When `donto_authorise` returns false for any contributing context
//! and the spec demands a public release, `releasable` is set to
//! `false`. Private/internal releases can still be built; the policy
//! report records the failures so reviewers can see them.

use crate::manifest::{PolicyDecision, PolicyReport};
use crate::ReleaseError;
use donto_client::DontoClient;
use std::collections::{BTreeMap, BTreeSet};

/// Caller IRI used to probe whether a release would be public-readable.
const PUBLIC_PROBE_CALLER: &str = "agent:anonymous";

pub async fn evaluate_policy(
    client: &DontoClient,
    contexts: &BTreeSet<String>,
    require_public: bool,
) -> Result<PolicyReport, ReleaseError> {
    let mut decisions: BTreeMap<String, PolicyDecision> = BTreeMap::new();
    let mut all_cleared = true;

    for ctx in contexts {
        let allowed = client
            .authorise(PUBLIC_PROBE_CALLER, "context", ctx, "read")
            .await
            .unwrap_or(false);
        if !allowed {
            all_cleared = false;
        }
        decisions.insert(
            ctx.clone(),
            PolicyDecision {
                cleared: allowed,
                policy_iri: None,
                reason: if allowed {
                    "donto_authorise(anonymous, context, read) granted".into()
                } else {
                    "donto_authorise(anonymous, context, read) denied".into()
                },
            },
        );
    }

    let releasable = if require_public { all_cleared } else { true };
    let note = if require_public {
        format!(
            "public release: {}/{} contexts cleared",
            decisions.values().filter(|d| d.cleared).count(),
            decisions.len()
        )
    } else {
        format!("internal release over {} contexts", decisions.len())
    };

    Ok(PolicyReport {
        releasable,
        decisions,
        note,
    })
}
