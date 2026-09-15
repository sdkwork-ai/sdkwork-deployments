//! App publishing domain services: default-domain auto-provisioning and the
//! hostname → server resolution the Web Server fallback consumes.

use sdkwork_deploy_contract::{
    DeployAppRequestContext, DeployServiceError, DeployServiceResult, ProvisionAppDomainsResult,
    ResolvedDeployServer, APP_PUBLISH_ENVIRONMENTS,
};
use sdkwork_deploy_core::{effective_app_domain_suffixes, PLATFORM_APP_DOMAIN_SUFFIXES};

use crate::DeployService;

/// Lifecycle environments an app can be published on. Single source of truth
/// for both provisioning and lookup so a hostname can never be created for an
/// environment the lookup refuses to query (the pre-`demo` drift).
pub const SUPPORTED_APP_ENVIRONMENTS: [&str; 5] =
    ["development", "test", "staging", "demo", "production"];

impl DeployService {
    /// Idempotently reconcile an app's default publishing domains for one
    /// lifecycle environment: platform DNS zones, `<appDomainLabel>.app[-<env>].<suffix>`
    /// domain rows (auto-verified, the platform owns the apex) and `SERVE`
    /// bindings on the app. Called automatically on app creation and available
    /// for explicit re-runs (for example after a site is created or after the
    /// app's `appDomainLabel` / `appDomainSuffixes` change).
    pub async fn provision_app_default_domains(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment: &str,
    ) -> DeployServiceResult<ProvisionAppDomainsResult> {
        if !SUPPORTED_APP_ENVIRONMENTS.contains(&environment) {
            return Err(DeployServiceError::validation(format!(
                "environment must be one of {}",
                SUPPORTED_APP_ENVIRONMENTS.join(", ")
            )));
        }
        let tenant_id = DeployService::require_tenant(context)?;
        let app = self.repository.retrieve_app(tenant_id, app_id).await?;
        let organization_id = context.organization_id.unwrap_or(0);
        let suffixes = self.app_domain_suffixes(tenant_id, &app.id).await?;
        let created_zones = self
            .repository
            .ensure_platform_app_zones(tenant_id, organization_id, context.actor_id, &suffixes)
            .await?;
        let reconciled = self
            .repository
            .provision_app_default_domains(
                tenant_id,
                organization_id,
                context.actor_id,
                &app.id,
                environment,
            )
            .await?;
        let result = ProvisionAppDomainsResult {
            created_zones: created_zones + reconciled.created_zones,
            ..reconciled
        };
        tracing::info!(
            tenant_id,
            app_id = %app.id,
            environment,
            created_zones = result.created_zones,
            created_domains = result.created_domains,
            created_bindings = result.created_bindings,
            "provisioned app default publishing domains"
        );
        Ok(result)
    }

    /// Reconcile the app's default publishing domains across **every**
    /// lifecycle environment so all five hostname families
    /// (`app` / `app-dev` / `app-test` / `app-staging` / `app-demo`) exist from
    /// the moment the app is created. Per-environment failures are logged and
    /// do not abort the remaining environments; the first failure is returned
    /// after the loop so the caller still observes it.
    pub async fn provision_app_default_domains_all_environments(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<Vec<(String, ProvisionAppDomainsResult)>> {
        let mut results = Vec::with_capacity(SUPPORTED_APP_ENVIRONMENTS.len());
        let mut first_error: Option<DeployServiceError> = None;
        for environment in SUPPORTED_APP_ENVIRONMENTS {
            match self
                .provision_app_default_domains(context, app_id, environment)
                .await
            {
                Ok(result) => results.push((environment.to_owned(), result)),
                Err(error) => {
                    tracing::warn!(
                        app_id,
                        environment,
                        error = %error,
                        "app default publishing domain reconciliation failed for environment"
                    );
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(results),
        }
    }

    /// The suffix catalog an app publishes on: its `appDomainSuffixes`
    /// override when present, otherwise the platform catalog.
    async fn app_domain_suffixes(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<Vec<String>> {
        let override_suffixes = self
            .repository
            .app_domain_suffix_override(tenant_id, app_id)
            .await?;
        Ok(effective_app_domain_suffixes(override_suffixes.as_deref()))
    }

    /// Resolve an active app binding by its exact hostname in one lifecycle
    /// environment and return that environment's newest VALID compiled website
    /// runtime descriptor plus the app's nginx configuration. Both default app
    /// domains (`<appDomainLabel>.app[-<env>].<suffix>`) and user custom
    /// domains are resolved here.
    pub async fn resolve_server_by_hostname(
        &self,
        hostname: &str,
        environment: &str,
    ) -> DeployServiceResult<Option<ResolvedDeployServer>> {
        if !SUPPORTED_APP_ENVIRONMENTS.contains(&environment) {
            return Err(DeployServiceError::validation(format!(
                "environment must be one of {}",
                SUPPORTED_APP_ENVIRONMENTS.join(", ")
            )));
        }
        self.repository
            .resolve_server_by_hostname(hostname, environment)
            .await
    }
}

/// The platform suffix catalog, exposed for callers that only need the
/// default set.
pub fn platform_app_domain_suffixes() -> Vec<String> {
    PLATFORM_APP_DOMAIN_SUFFIXES
        .iter()
        .map(|suffix| (*suffix).to_owned())
        .collect()
}

/// The lifecycle environments an app is published on, in promotion order.
pub fn app_publish_environments() -> Vec<String> {
    APP_PUBLISH_ENVIRONMENTS
        .iter()
        .map(|environment| environment.as_str().to_owned())
        .collect()
}
