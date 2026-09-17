//! Coverage evidence for the composable domain management + certificate
//! management blocks (APP_SDK_INTEGRATION_SPEC §5 same-origin dependency
//! mounts): the block route inventory derived from the generated app API
//! manifest must match exactly the route registrations of the executable
//! blocks.

use sdkwork_routes_deploy_app_api::{
    domain_certificate_manifest::domain_certificate_route_manifest, paths,
};
use sdkwork_web_core::HttpMethod;

fn block_paths() -> Vec<(HttpMethod, &'static str)> {
    vec![
        (HttpMethod::Get, paths::DOMAIN_ZONES),
        (HttpMethod::Post, paths::DOMAIN_ZONES),
        (HttpMethod::Get, paths::DOMAIN_ZONE),
        (HttpMethod::Patch, paths::DOMAIN_ZONE),
        (HttpMethod::Delete, paths::DOMAIN_ZONE),
        (HttpMethod::Get, paths::DOMAIN_ZONE_HOSTNAMES),
        (HttpMethod::Post, paths::DOMAIN_ZONE_HOSTNAMES),
        (HttpMethod::Get, paths::DOMAIN_ZONE_HOSTNAME),
        (HttpMethod::Patch, paths::DOMAIN_ZONE_HOSTNAME),
        (HttpMethod::Delete, paths::DOMAIN_ZONE_HOSTNAME),
        (HttpMethod::Post, paths::DOMAIN_ZONE_HOSTNAME_VERIFY),
        (HttpMethod::Post, paths::DOMAIN_ZONE_HOSTNAME_CLAIMS),
        (HttpMethod::Get, paths::CERTIFICATES),
        (HttpMethod::Post, paths::CERTIFICATES),
        (HttpMethod::Get, paths::CERTIFICATE),
        (HttpMethod::Delete, paths::CERTIFICATE),
        (HttpMethod::Post, paths::CERTIFICATE_RENEW),
        (HttpMethod::Get, paths::CERTIFICATE_RENEWALS),
        // The cloud account surface belongs to this block: it is what the zone
        // and certificate forms open when they need to bind a DNS provider
        // account, so it is mounted and inventoried with the same filter.
        (HttpMethod::Get, paths::CLOUD_ACCOUNTS),
        (HttpMethod::Post, paths::CLOUD_ACCOUNTS),
    ]
}

#[test]
fn domain_certificate_manifest_covers_exactly_the_block_route_registrations() {
    let mut expected = block_paths();
    expected.sort_unstable_by(|left, right| (left.1, left.0 as u8).cmp(&(right.1, right.0 as u8)));

    let mut actual: Vec<(HttpMethod, &'static str)> = domain_certificate_route_manifest()
        .routes()
        .iter()
        .map(|route| (route.method, route.path))
        .collect();
    actual.sort_unstable_by(|left, right| (left.1, left.0 as u8).cmp(&(right.1, right.0 as u8)));

    assert_eq!(
        actual, expected,
        "executable block routes and manifest inventory diverged"
    );
}

#[test]
fn domain_certificate_manifest_is_a_subset_of_the_full_app_api_manifest() {
    let full = sdkwork_routes_deploy_app_api::app_route_manifest();
    for route in domain_certificate_route_manifest().routes() {
        assert!(
            full.routes()
                .iter()
                .any(|candidate| candidate.method == route.method && candidate.path == route.path),
            "block route {} {} missing from the full app API manifest",
            format!("{:?}", route.method),
            route.path,
        );
    }
}
