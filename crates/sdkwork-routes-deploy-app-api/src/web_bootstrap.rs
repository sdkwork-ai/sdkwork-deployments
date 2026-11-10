use std::sync::Arc;

use axum::Router;
use sdkwork_deploy_contract::DeployAppRequestContext;
use sdkwork_iam_web_adapter::IamWebRequestContextResolver;
use sdkwork_routes_deploy_common::{
    deploy_web_auth_mode_from_env, with_problem_correlation, DeployWebAuthMode,
    ProductionFailClosedResolver,
};
use sdkwork_web_axum::{with_web_request_context, WebFrameworkLayer};
use sdkwork_web_core::{
    DefaultWebRequestContextResolver, DomainContextInjector, WebRequestContext,
    WebRequestContextProfile,
};

use crate::http_route_manifest::app_route_manifest;
use crate::paths;

pub fn deploy_app_api_public_path_prefixes() -> Vec<String> {
    // The download catalog, update configuration, feed and artifact endpoints
    // are read by anonymous visitors and by desktop clients without a session.
    vec!["/app/v3/api/downloads/".to_owned()]
}

pub fn deploy_app_api_prefixes() -> Vec<String> {
    vec![paths::PREFIX.to_owned()]
}

#[derive(Clone, Default)]
pub struct DeployAppContextInjector;

impl DomainContextInjector for DeployAppContextInjector {
    fn inject(&self, request: &mut axum::extract::Request, context: &WebRequestContext) {
        if let Some(app_context) = deploy_app_context_from_request(request, context) {
            request.extensions_mut().insert(app_context);
        }
    }
}

/// Domain context injectors registered by hosts that mount Deployments
/// app-API blocks (for example the Web Server standalone gateway composing
/// the domain/certificate blocks). The injector derives the Deployments
/// request context from the host Web Framework principal.
pub fn deploy_app_api_domain_context_injectors() -> Vec<std::sync::Arc<dyn DomainContextInjector>> {
    vec![std::sync::Arc::new(DeployAppContextInjector)]
}

fn deploy_app_context_from_web_request(
    context: &WebRequestContext,
) -> Option<DeployAppRequestContext> {
    let principal = context.principal.as_ref()?;
    let tenant_id = principal.tenant_id().parse().ok()?;
    let actor_id = principal.user_id().parse().ok();
    let organization_id = principal
        .organization_id()
        .and_then(|value| value.parse().ok());
    let session_id = principal.session_id().map(str::to_owned);
    Some(DeployAppRequestContext {
        tenant_id,
        actor_id,
        organization_id,
        session_id,
        auth_token: None,
        access_token: None,
    })
}

fn deploy_app_context_from_request(
    request: &axum::extract::Request,
    context: &WebRequestContext,
) -> Option<DeployAppRequestContext> {
    let mut app_context = deploy_app_context_from_web_request(context)?;
    app_context.auth_token = sdkwork_web_core::extractors::bearer_token(request.headers());
    app_context.access_token = sdkwork_web_core::extractors::access_token(request.headers());
    Some(app_context)
}

pub fn wrap_router_with_web_framework(
    resolver: DefaultWebRequestContextResolver,
    router: Router,
) -> Router {
    with_web_request_context(
        with_problem_correlation(router),
        build_deploy_app_api_framework_layer(resolver),
    )
}

pub fn wrap_router_with_iam_database_web_framework(
    resolver: IamWebRequestContextResolver,
    router: Router,
) -> Router {
    with_web_request_context(
        with_problem_correlation(router),
        build_deploy_app_api_framework_layer(resolver),
    )
}

fn build_deploy_app_api_framework_layer<R>(resolver: R) -> WebFrameworkLayer<R>
where
    R: sdkwork_web_core::WebRequestContextResolver + Clone,
{
    let route_manifest = app_route_manifest();
    route_manifest
        .validate_public_path_prefixes(&deploy_app_api_public_path_prefixes())
        .expect("deploy app-api public prefixes must not cover protected manifest routes");

    WebFrameworkLayer::new(resolver)
        .with_profile(WebRequestContextProfile {
            app_api_prefix: paths::PREFIX.to_owned(),
            public_path_prefixes: deploy_app_api_public_path_prefixes(),
            ..WebRequestContextProfile::default()
        })
        .with_route_manifest(route_manifest)
        .with_domain_injector(Arc::new(DeployAppContextInjector))
}

pub async fn wrap_router_with_web_framework_from_env(router: Router) -> Router {
    match deploy_web_auth_mode_from_env().await {
        DeployWebAuthMode::DevInline => {
            wrap_router_with_web_framework(DefaultWebRequestContextResolver::default(), router)
        }
        DeployWebAuthMode::ProductionFailClosed => with_web_request_context(
            with_problem_correlation(router),
            build_deploy_app_api_framework_layer(ProductionFailClosedResolver),
        ),
        DeployWebAuthMode::IamDatabase(resolver) => {
            wrap_router_with_iam_database_web_framework(*resolver, router)
        }
    }
}
