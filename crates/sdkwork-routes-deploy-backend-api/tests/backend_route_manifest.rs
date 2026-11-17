//! Route-manifest contract for the application identity retry entry
//! (`apps.identityProvision`).
//!
//! The permission is generated, not hand-written:
//! `tools/materialize_deploy_phase1_contracts.mjs` derives
//! `deploy.<operationId root>.<read|write>` from the operationId's **last**
//! segment. Renaming the operation to something whose final segment contains
//! `list`/`retrieve` would silently relax the requirement to
//! `deploy.apps.read`, and nothing else in the crate would notice. This test
//! pins the operation id, the path, the verb, and the generated permission.
//!
//! It is a pure manifest assertion: no database and no HTTP request, so it runs
//! in the crate's ordinary `cargo test`.

use sdkwork_routes_deploy_backend_api::backend_route_manifest;
use sdkwork_web_core::HttpMethod;

const IDENTITY_PROVISION_PATH: &str = "/backend/v3/api/apps/{appId}/identity_provision";

#[test]
fn identity_provision_route_requires_deploy_apps_write() {
    let manifest = backend_route_manifest();
    let route = manifest
        .routes()
        .iter()
        .find(|route| route.path == IDENTITY_PROVISION_PATH)
        .expect("apps.identityProvision must be materialized into the backend route manifest");

    assert_eq!(route.method, HttpMethod::Post);
    assert_eq!(route.operation_id, "apps.identityProvision");
    // A retry re-provisions a runtime identity: it is a write, never a read.
    assert_eq!(route.required_permission, Some("deploy.apps.write"));
}

#[test]
fn identity_provision_route_matches_a_concrete_application_path() {
    let manifest = backend_route_manifest();
    let matched = manifest
        .match_route(
            "POST",
            "/backend/v3/api/apps/0f8b7f26-2d7c-4f43-8a4b-0c5a51e1f0aa/identity_provision",
        )
        .expect("the manifest template must match a real request path");
    assert_eq!(matched.operation_id, "apps.identityProvision");
    // `GET` on the same path is not this command.
    assert!(manifest
        .match_route(
            "GET",
            "/backend/v3/api/apps/0f8b7f26-2d7c-4f43-8a4b-0c5a51e1f0aa/identity_provision",
        )
        .is_none());
}
