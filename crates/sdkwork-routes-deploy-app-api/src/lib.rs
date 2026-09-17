//! App API route boundary for SDKWork Deploy.

pub mod app_delivery_routes;
pub mod auth;
pub mod domain_certificate_manifest;
pub mod http_route_manifest;
pub mod paths;
pub mod routes;
pub mod web_bootstrap;

pub use domain_certificate_manifest::domain_certificate_route_manifest;
pub use http_route_manifest::app_route_manifest;
pub use routes::{
    build_certificate_management_router, build_cloud_account_router,
    build_domain_management_router, build_router_with_app_api, build_router_with_shared_app_api,
    AppState,
};
pub use sdkwork_deploy_contract::{DeployAppApi, DeployAppRequestContext};
pub use web_bootstrap::{
    deploy_app_api_domain_context_injectors, deploy_app_api_prefixes,
    deploy_app_api_public_path_prefixes, wrap_router_with_web_framework_from_env,
};

use sdkwork_web_core::HttpRouteManifest;
use std::sync::Arc;

pub fn gateway_route_manifest() -> HttpRouteManifest {
    app_route_manifest()
}

pub fn gateway_mount(api: Arc<dyn DeployAppApi>) -> axum::Router {
    build_router_with_shared_app_api(api)
}
