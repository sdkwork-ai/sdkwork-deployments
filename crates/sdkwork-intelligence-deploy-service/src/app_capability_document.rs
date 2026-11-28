//! Canonical serialization of the capability document.
//!
//! A revision row stores one JSON object and its SHA-256. Two saves of the same
//! document must therefore produce the same bytes, or the digest unique index
//! (`uk_deploy_app_capability_revision_digest`) would treat identical edits as
//! different revisions.
//!
//! Determinism comes from two facts: `serde_json::Map` is a `BTreeMap` (keys are
//! emitted in sorted order) and every collection is serialized from the request's
//! own order, which the editor controls. The snapshot deliberately excludes the
//! command-only fields (`expectedVersion`, `submitForReview`): they describe a
//! call, not the document, and including them would make an otherwise identical
//! document hash differently on every submit.

use sdkwork_deploy_contract::{
    AppCapabilityResponse, DistributionScope, ReplaceAppCapabilityRequest,
    APP_CAPABILITY_SCHEMA_VERSION,
};
use serde_json::{json, Value};

pub(crate) fn canonical_snapshot(request: &ReplaceAppCapabilityRequest) -> Value {
    let distribution_scope = request
        .distribution_scope
        .unwrap_or(DistributionScope::Private);
    json!({
        "schemaVersion": APP_CAPABILITY_SCHEMA_VERSION,
        "distributionScope": distribution_scope,
        "avatarUrl": request.avatar_url,
        "oauthClientId": request.oauth_client_id,
        "defaultWorkModeKey": request.default_work_mode_key,
        "texts": request.texts,
        "workModes": request.work_modes,
        "scenes": request.scenes,
        "models": request.models,
        "market": request.market,
        "embeds": request.embeds,
    })
}

/// The same document shape, built from a persisted response.
///
/// Used when a submission freezes the *current* document without a replacement
/// request in hand. Both producers must emit the same key set, or a document
/// submitted right after a save would produce a second revision with identical
/// content but a different digest.
pub fn snapshot_from_capability_response(response: &AppCapabilityResponse) -> Value {
    json!({
        "schemaVersion": response.config_schema_version,
        "distributionScope": response.distribution_scope,
        "avatarUrl": response.avatar_url,
        "oauthClientId": response.oauth_client_id,
        "defaultWorkModeKey": response.default_work_mode_key,
        "texts": response.texts,
        "workModes": response.work_modes,
        "scenes": response.scenes,
        "models": response.models,
        "market": response.market,
        "embeds": response.embeds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ReplaceAppCapabilityRequest {
        ReplaceAppCapabilityRequest {
            distribution_scope: Some(DistributionScope::Tenant),
            ..Default::default()
        }
    }

    #[test]
    fn the_snapshot_is_stable_across_equivalent_requests() {
        let first = canonical_snapshot(&request());
        let second = canonical_snapshot(&request());
        assert_eq!(
            serde_json::to_string(&first).expect("serialize"),
            serde_json::to_string(&second).expect("serialize")
        );
    }

    #[test]
    fn command_only_fields_are_not_part_of_the_document() {
        let mut with_command_fields = request();
        with_command_fields.expected_version = Some("7".to_owned());
        with_command_fields.submit_for_review = true;
        assert_eq!(
            canonical_snapshot(&request()),
            canonical_snapshot(&with_command_fields)
        );
    }
}
