//! Static coverage gate for the same-origin dependency entrypoint.
//!
//! Why this is a source assertion and not an HTTP probe: the failure it guards
//! is *which router the assembly hands to the host*, and reproducing that here
//! would need a live PostgreSQL pool (`assemble_same_origin_contribution_with_pool`
//! takes one). The behaviour of the router itself is covered by
//! `sdkwork-routes-deploy-app-api/tests/app_surface_manifest.rs`, which probes
//! every manifest route against the very surface this entrypoint must mount.
//! Together the two tests pin both halves: the surface is complete, and this
//! entrypoint is the one that publishes it.
//!
//! The regression this fixes: the entrypoint hand-enumerated three blocks
//! (`build_domain_management_router` / `build_certificate_management_router` /
//! `build_cloud_account_router`) and published
//! `domain_certificate_route_manifest()`. The router served 11 route
//! registrations while the host's topology declared the whole `/app/v3/api`
//! dependency as served, so `/app/v3/api/apps` answered 404. No gate could see
//! it: the assembly compiled, its manifest matched its own router, and only the
//! *host's* declaration disagreed.

const BOOTSTRAP_SOURCE: &str = include_str!("../src/bootstrap.rs");

const SAME_ORIGIN_ENTRYPOINT: &str = "pub async fn assemble_same_origin_contribution_with_pool(";

fn normalized_source() -> String {
    BOOTSTRAP_SOURCE.replace("\r\n", "\n")
}

fn function_body(name: &str) -> String {
    let source = normalized_source();
    let signature = format!("pub async fn {name}(");
    let start = source.find(&signature).unwrap_or_else(|| {
        panic!(
            "{name} is missing from bootstrap.rs; hosts call it as the same-origin dependency \
             entrypoint (API_ASSEMBLY_SPEC §6.1)"
        )
    });
    let end = source[start..]
        .find("\n}\n")
        .map(|offset| start + offset)
        .unwrap_or_else(|| panic!("{name} has no top-level closing brace"));
    source[start..end].to_owned()
}

#[test]
fn same_origin_entrypoint_does_not_hand_enumerate_route_blocks() {
    let body = function_body("assemble_same_origin_contribution_with_pool");
    for hand_rolled_block in [
        "build_domain_management_router",
        "build_certificate_management_router",
        "build_cloud_account_router",
        "app_delivery_routes",
    ] {
        assert!(
            !body.contains(hand_rolled_block),
            "the same-origin entrypoint composes `{hand_rolled_block}` by hand. A hand-enumerated \
             subset cannot be detected by any gate and silently serves fewer routes than the \
             manifest, OpenAPI document, and permission catalog it publishes; mount the route \
             crate's canonical surface instead."
        );
    }
}

#[test]
fn same_origin_entrypoint_mounts_the_canonical_app_surface() {
    let body = function_body("assemble_same_origin_contribution_with_pool");
    assert!(
        body.contains("mount_app("),
        "the same-origin entrypoint must mount the route crate's canonical app surface"
    );
    assert!(
        body.contains("app_route_manifest()"),
        "the same-origin entrypoint must publish the full app API manifest, not a subset"
    );
    assert!(
        !body.contains("domain_certificate_route_manifest()"),
        "the domain/certificate subset manifest under-reports the mounted surface"
    );
}

/// Anchors the probe used by the routes crate: if the canonical surface is
/// renamed or re-exported through a different symbol, this fails loudly instead
/// of letting the coverage test keep probing a router nobody mounts.
#[test]
fn the_route_crate_still_exports_the_canonical_surface() {
    let source = normalized_source();
    assert!(
        source.contains("gateway_mount as mount_app"),
        "bootstrap.rs must import the route crate's canonical gateway mount"
    );
    assert!(
        source.contains("gateway_route_manifest as app_route_manifest"),
        "bootstrap.rs must import the route crate's canonical route manifest"
    );
}

/// The narrowed title was a symptom of the narrowed surface; hosts publish this
/// string as the composed OpenAPI `info.title`.
#[test]
fn the_contribution_is_titled_for_the_whole_app_surface() {
    let source = normalized_source();
    assert!(
        !source.contains("SDKWork Deployments Domain Certificate API"),
        "the same-origin contribution no longer covers only domain/certificate routes"
    );
    assert!(
        source.contains("\"SDKWork Deploy App API\""),
        "the same-origin contribution must be titled for the whole app API"
    );
}

/// Keeps the sub-surface helper honest: it must stay documented as *not* the
/// same-origin entrypoint, because serving it as the deploy dependency is
/// exactly the 404 regression this file guards.
#[test]
fn the_sub_surface_helper_warns_that_it_is_not_the_entrypoint() {
    let source = normalized_source();
    assert!(
        source.contains(SAME_ORIGIN_ENTRYPOINT),
        "the same-origin entrypoint must remain present"
    );
    assert!(
        source.contains("deliberately **not** the same-origin dependency entrypoint"),
        "assemble_domain_certificate_blocks must document that it is a sub-surface, not the \
         same-origin dependency entrypoint"
    );
}
