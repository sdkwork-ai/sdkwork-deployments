//! Domain management and certificate management route inventory subset.
//!
//! The full app API manifest stays the generated single source of truth
//! (`http_route_manifest.rs`). This file derives the composable domain
//! management + certificate management inventory from it so the same
//! normalized routes back the executable blocks
//! (`build_domain_management_router` / `build_certificate_management_router` /
//! `build_cloud_account_router`) in every host that mounts them.
//!
//! The cloud account collection belongs to this subset rather than to a manifest
//! of its own because it is not independently mountable in any useful sense: a
//! host that serves the domain or certificate blocks needs the picker's backing
//! endpoint, since that is where both forms get the account they pin.

use sdkwork_web_core::HttpRouteManifest;

use crate::http_route_manifest::app_route_manifest;

pub fn domain_certificate_route_manifest() -> HttpRouteManifest {
    HttpRouteManifest::from_owned_routes(
        app_route_manifest()
            .routes()
            .iter()
            .filter(|route| {
                route.path.starts_with("/app/v3/api/domain_zones")
                    || route.path.starts_with("/app/v3/api/certificates")
                    || route.path.starts_with("/app/v3/api/cloud_accounts")
            })
            .copied()
            .collect(),
    )
}
