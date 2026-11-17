//! Shared Deploy router auth wiring for sdkwork-web-framework integration.

pub mod app_filter;
pub mod correlation;
pub mod envelope;
pub mod pagination;
pub mod problem;
pub mod response;

pub use app_filter::app_kind_filter;

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    deploy_is_production_like_environment, deploy_use_dev_inline_auth_resolver,
};
use sdkwork_iam_web_adapter::IamWebRequestContextResolver;
use sdkwork_web_core::{WebFrameworkError, WebRequestContextResolver, WebRequestPrincipal};

pub use correlation::{with_problem_correlation, DeployProblemCorrelation};
pub use problem::{DeployApiError, DeployApiResult};
pub use response::{
    finish_api_json, finish_created_api_json, finish_no_content, ok_json, service_result,
    ApiProblem, ApiResult,
};

const PRODUCTION_AUTH_UNAVAILABLE: &str = "production deploy auth requires IAM PostgreSQL database";

pub enum DeployWebAuthMode {
    DevInline,
    IamDatabase(Box<IamWebRequestContextResolver>),
    ProductionFailClosed,
}

pub async fn deploy_web_auth_mode_from_env() -> DeployWebAuthMode {
    if deploy_use_dev_inline_auth_resolver() {
        return DeployWebAuthMode::DevInline;
    }

    let workspace_database_explicitly_configured = std::env::var("SDKWORK_DATABASE_URL")
        .or_else(|_| std::env::var("SDKWORK_DATABASE_ENGINE"))
        .is_ok();

    if deploy_is_production_like_environment() && !workspace_database_explicitly_configured {
        return DeployWebAuthMode::ProductionFailClosed;
    }

    DeployWebAuthMode::IamDatabase(Box::new(
        sdkwork_iam_web_adapter::iam_web_request_context_resolver_from_env().await,
    ))
}

#[derive(Clone, Default)]
pub struct ProductionFailClosedResolver;

#[async_trait]
impl WebRequestContextResolver for ProductionFailClosedResolver {
    async fn resolve_api_key(
        &self,
        _raw_api_key: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        Err(WebFrameworkError::invalid_credentials(
            PRODUCTION_AUTH_UNAVAILABLE,
        ))
    }

    async fn resolve_dual_token(
        &self,
        _raw_auth_token: &str,
        _raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        Err(WebFrameworkError::invalid_credentials(
            PRODUCTION_AUTH_UNAVAILABLE,
        ))
    }

    async fn resolve_access_token(
        &self,
        _raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        Err(WebFrameworkError::invalid_credentials(
            PRODUCTION_AUTH_UNAVAILABLE,
        ))
    }

    async fn resolve_oauth_bearer(
        &self,
        _raw_bearer_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        Err(WebFrameworkError::invalid_credentials(
            PRODUCTION_AUTH_UNAVAILABLE,
        ))
    }
}
