//! API assembly for sdkwork-deployments.
//! Application bootstrap lives in `bootstrap.rs`; route inventory is in `assembly-manifest.json`.
// SDKWORK-ASSEMBLY-LIB-CUSTOM: exports beyond the canonical materializer template.

mod bootstrap;
mod generated;

pub use bootstrap::{
    assemble_api_router, assemble_api_router_with_pool, assemble_business_routes,
    assemble_domain_certificate_blocks, assemble_same_origin_contribution_with_pool,
    migrate_database_from_env, web_module, web_module_with_pool, ApiAssembly,
    DomainCertificateBlocks,
};

// SDKWORK-ASSEMBLY-LIB-CUSTOM: the domain/certificate route manifest is part
// of the assembly integration surface. Host applications (for example
// sdkwork-webserver) compose their served OpenAPI and router inventory from
// this manifest through the assembly boundary (API_ASSEMBLY_SPEC §4/§6.1) and
// must not reach past the assembly into `sdkwork-routes-deploy-app-api`.
pub use sdkwork_routes_deploy_app_api::domain_certificate_route_manifest;

pub fn assembly_route_count() -> usize {
    generated::ROUTE_CRATE_COUNT
}
