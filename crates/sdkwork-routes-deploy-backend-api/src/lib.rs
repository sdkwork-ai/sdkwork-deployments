//! Backend API route boundary for SDKWork Deploy.

pub mod auth;
pub mod downloads_routes;
pub mod http_route_manifest;
pub mod paths;
pub mod routes;
pub mod web_bootstrap;

pub use downloads_routes::{build_downloads_backend_router, DownloadsBackendState};
pub use http_route_manifest::backend_route_manifest;
pub use routes::{build_router_with_backend_api, build_router_with_shared_backend_api};
pub use sdkwork_deploy_contract::{DeployBackendApi, DeployBackendRequestContext};
pub use web_bootstrap::wrap_router_with_web_framework_from_env;

use sdkwork_web_core::HttpRouteManifest;
use std::sync::Arc;

pub fn gateway_route_manifest() -> HttpRouteManifest {
    backend_route_manifest()
}

pub fn gateway_mount(api: Arc<dyn DeployBackendApi>) -> axum::Router {
    build_router_with_shared_backend_api(api)
}
