//! Business-only gateway bootstrap for sdkwork-deployments.

use axum::{Extension, Router};
use sdkwork_database_sqlx::DatabasePool;
use sdkwork_deploy_service_host::bootstrap_deploy_service_host_from_env;
use sdkwork_intelligence_deploy_service::DeployService;
use sdkwork_routes_deploy_app_api::{
    build_certificate_management_router, build_cloud_account_router,
    build_domain_management_router, deploy_app_api_domain_context_injectors,
    domain_certificate_route_manifest, gateway_mount as mount_app,
    gateway_route_manifest as app_route_manifest,
    wrap_router_with_web_framework_from_env as wrap_app, AppState,
};
use sdkwork_routes_deploy_backend_api::{
    gateway_mount as mount_backend, gateway_route_manifest as backend_route_manifest,
    wrap_router_with_web_framework_from_env as wrap_backend,
};
use sdkwork_web_bootstrap::{ApiAssemblyContribution, ReadinessCheck, ReadinessFuture, WebModule};
use sdkwork_web_core::{DomainContextInjector, HttpRouteManifest};
use std::sync::Arc;

const APP_OPENAPI_JSON: &str =
    include_str!("../../../apis/app-api/deploy/deploy-app-api.openapi.json");
const BACKEND_OPENAPI_JSON: &str =
    include_str!("../../../apis/backend-api/deploy/deploy-backend-api.openapi.json");

/// Indivisible gateway assembly: the complete host-neutral API contribution
/// plus the resolved Deploy service for gateway-local wiring.
pub struct ApiAssembly {
    pub contribution: ApiAssemblyContribution,
    pub service: Arc<DeployService>,
}

pub async fn assemble_business_routes() -> Result<ApiAssembly, String> {
    let service = bootstrap_deploy_service_host_from_env().await?.service;
    assemble_business_routes_with_service(service).await
}

/// Assemble the Deploy API against a caller-provided database pool so the
/// platform cloud gateway can share its process-wide PostgreSQL pool.
pub async fn assemble_api_router_with_pool(pool: DatabasePool) -> Result<ApiAssembly, String> {
    let service = sdkwork_deploy_service_host::bootstrap_deploy_service_host_with_pool(pool)
        .await?
        .service;
    assemble_business_routes_with_service(service).await
}

async fn assemble_business_routes_with_service(
    service: Arc<DeployService>,
) -> Result<ApiAssembly, String> {
    let app = wrap_app(mount_app(service.clone())).await;
    let backend = wrap_backend(mount_backend(service.clone())).await;
    let router = Router::new()
        .merge(app)
        .merge(backend)
        .layer(Extension(service.clone()));

    let mut routes = app_route_manifest().routes().to_vec();
    routes.extend(backend_route_manifest().routes().iter().cloned());
    let route_manifest = HttpRouteManifest::from_owned_routes(routes);

    let domain_context_injectors = deploy_app_api_domain_context_injectors();
    let readiness_check: Arc<dyn ReadinessCheck> = Arc::new(DeployServiceReadinessCheck {
        service: service.clone(),
    });

    let app_openapi: serde_json::Value = serde_json::from_str(APP_OPENAPI_JSON)
        .map_err(|error| format!("parse deploy app OpenAPI: {error}"))?;
    let backend_openapi: serde_json::Value = serde_json::from_str(BACKEND_OPENAPI_JSON)
        .map_err(|error| format!("parse deploy backend OpenAPI: {error}"))?;

    let contribution = ApiAssemblyContribution::from_openapi_documents(
        "sdkwork-deployments",
        "SDKWork Deploy API",
        router,
        route_manifest,
        vec![app_openapi, backend_openapi],
        domain_context_injectors,
        readiness_check,
    )?;
    Ok(ApiAssembly {
        contribution,
        service,
    })
}

pub async fn assemble_api_router() -> Result<ApiAssembly, String> {
    assemble_business_routes().await
}

/// Host-neutral Deploy App API contribution for composing gateways that install
/// their own single Web Framework layer (for example the BirdCoder standalone
/// gateway). Exports the bare app router plus its complete inventory so
/// consumers never import `sdkwork-routes-deploy-app-api` or
/// `sdkwork-intelligence-deploy-service` directly (API_ASSEMBLY_SPEC §3/§6.1).
pub async fn assemble_app_api_contribution_from_env() -> Result<ApiAssemblyContribution, String> {
    let service = bootstrap_deploy_service_host_from_env().await?.service;
    let router = sdkwork_routes_deploy_app_api::build_router_with_shared_app_api(service.clone());
    let app_openapi: serde_json::Value = serde_json::from_str(APP_OPENAPI_JSON)
        .map_err(|error| format!("parse deploy app OpenAPI: {error}"))?;
    ApiAssemblyContribution::from_openapi_documents(
        "sdkwork-deployments",
        "SDKWork Deploy App API",
        router,
        sdkwork_routes_deploy_app_api::app_route_manifest(),
        vec![app_openapi],
        sdkwork_routes_deploy_app_api::deploy_app_api_domain_context_injectors(),
        Arc::new(DeployServiceReadinessCheck { service }),
    )
}

/// Composable domain management + certificate management contribution for
/// consuming hosts (for example the Web Server standalone gateway).
///
/// The blocks are host-neutral business routers plus their complete route
/// inventory (manifest, domain context injectors, readiness). Consuming
/// gateways merge them before installing their single Web Framework layer, so
/// the blocks stay un-wrapped and authenticate through the host framework.
pub struct DomainCertificateBlocks {
    pub router: Router,
    pub route_manifest: HttpRouteManifest,
    pub domain_context_injectors: Vec<Arc<dyn DomainContextInjector>>,
    pub readiness_check: Arc<dyn ReadinessCheck>,
}

/// Composes the domain / certificate / cloud-account **sub-surface** only.
///
/// This is deliberately **not** the same-origin dependency entrypoint. It
/// mounts a subset of the `/app/v3/api` routes, so a host that serves it as its
/// whole deploy dependency answers 404 for every app, upload session, artifact,
/// signing identity, build template, and usage-event route — the exact failure
/// API_ASSEMBLY_SPEC §6.1 describes. Consuming gateways call
/// [`assemble_same_origin_contribution_with_pool`] instead; this helper exists
/// for a host that genuinely mounts the domain / certificate blocks alone.
///
/// It is kept regardless of callers because
/// `sdkwork-specs/tools/materialize-api-assembly.mjs` probes for its presence
/// when rendering this crate's `lib.rs` export list, so removing it would
/// silently drop an export from the generated assembly surface.
pub async fn assemble_domain_certificate_blocks() -> Result<DomainCertificateBlocks, String> {
    let service = bootstrap_deploy_service_host_from_env().await?.service;
    let router = Router::new()
        .merge(build_domain_management_router())
        .merge(build_certificate_management_router())
        .merge(build_cloud_account_router())
        .with_state(AppState {
            api: service.clone(),
        });
    let route_manifest = domain_certificate_route_manifest();
    let domain_context_injectors = deploy_app_api_domain_context_injectors();
    let readiness_check: Arc<dyn ReadinessCheck> =
        Arc::new(DeployServiceReadinessCheck { service });
    Ok(DomainCertificateBlocks {
        router,
        route_manifest,
        domain_context_injectors,
        readiness_check,
    })
}

/// The complete Deploy App API as **one** host-neutral contribution on the
/// caller's process-shared PostgreSQL pool.
///
/// API_ASSEMBLY_SPEC §6.1: a gateway that selects this dependency declares it
/// as a same-origin required component port and calls this owner entrypoint; it
/// must not rebuild `ApiAssemblyContribution` from [`DomainCertificateBlocks`]
/// (§6.2.1 forbids the host projecting dependency fields into a hand-written
/// contribution). The owner defines its own contribution, so every consuming
/// host publishes the same manifest, OpenAPI document, and permission catalog.
///
/// **Coverage.** The contribution spans every route the consuming host's
/// `/app/v3/api` dependency entry declares as served — apps, domain zones,
/// certificates, cloud accounts, upload sessions, artifacts, signing
/// identities, build templates, and usage events — because API_ASSEMBLY_SPEC
/// §6.1 makes a declared-but-unserved same-origin surface an opaque 404 on an
/// authenticated operator action.
///
/// It is therefore composed from the route crate's own
/// `gateway_mount` / `gateway_route_manifest` pair instead of enumerating
/// blocks here: a hand-enumerated subset cannot be detected by any gate, and
/// silently serves fewer routes than this manifest, the synthesized OpenAPI
/// document, and the permission catalog all claim. Mounting the route crate's
/// canonical surface keeps the executable router and the published inventory
/// equal by construction.
///
/// The router is deliberately un-wrapped, exactly as
/// [`assemble_domain_certificate_blocks`] documents: the consuming host applies
/// the single Web Framework layer for its whole composed surface.
pub async fn assemble_same_origin_contribution_with_pool(
    pool: DatabasePool,
) -> Result<ApiAssemblyContribution, String> {
    let service = sdkwork_deploy_service_host::bootstrap_deploy_service_host_with_pool(pool)
        .await?
        .service;
    let router = mount_app(service.clone());
    ApiAssemblyContribution::from_manifest(
        "sdkwork-deployments",
        "SDKWork Deploy App API",
        router,
        app_route_manifest(),
        deploy_app_api_domain_context_injectors(),
        Arc::new(DeployServiceReadinessCheck { service }),
    )
}

/// Lifecycle-convergence entrypoint for the Deployments database module,
/// reusable by consuming hosts (for example the Web Server standalone gateway).
/// Baseline applies on empty databases; versioned forward migrations converge
/// existing databases; the drift gate then fails loudly if the schema still
/// diverges from the contract.
///
/// Whether forward migrations run is governed by `SDKWORK_DATABASE_AUTO_MIGRATE`
/// (falling back to the manifest's `lifecycle.autoMigrate`), per
/// DATABASE_FRAMEWORK_SPEC §4.4 — this entrypoint must not force it, because the
/// serve path uses it. [`migrate_database_from_env`] is the explicit migration
/// entrypoint that does.
pub async fn ensure_database_lifecycle_from_env() -> Result<(), String> {
    sdkwork_deploy_database_host::bootstrap_deploy_database_from_env()
        .await
        .map(|_| ())
        .map_err(|detail| format!("deploy database lifecycle failed: {detail}"))
}

/// Explicit migration entrypoint for the Deployments database module. Running
/// this command **is** the operator's authorization to apply forward migrations,
/// so it forces `SDKWORK_DATABASE_AUTO_MIGRATE` on for this process and then
/// converges the module.
pub async fn migrate_database_from_env() -> Result<(), String> {
    std::env::set_var("SDKWORK_DATABASE_AUTO_MIGRATE", "true");
    ensure_database_lifecycle_from_env().await
}

struct DeployServiceReadinessCheck {
    service: Arc<DeployService>,
}

impl ReadinessCheck for DeployServiceReadinessCheck {
    fn check(&self) -> ReadinessFuture<'_> {
        let service = self.service.clone();
        Box::pin(async move {
            service
                .ready_check()
                .await
                .map_err(|error| error.to_string())
        })
    }
}

/// Canonical Web Module definition for this application
/// (API_ASSEMBLY_SPEC §4.1.1): the complete HTTP surface — every route,
/// manifest, and OpenAPI document of this owner — as one installable module.
pub async fn web_module() -> Result<WebModule, String> {
    Ok(WebModule::from_contribution(
        assemble_api_router().await?.contribution,
    ))
}

/// Same as [`web_module`] but composed on a process-shared database pool
/// (platform gateways, API_ASSEMBLY_SPEC §4.1.1).
pub async fn web_module_with_pool(pool: DatabasePool) -> Result<WebModule, String> {
    Ok(WebModule::from_contribution(
        assemble_api_router_with_pool(pool).await?.contribution,
    ))
}
