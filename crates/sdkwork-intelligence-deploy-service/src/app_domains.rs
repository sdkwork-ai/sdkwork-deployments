//! App publishing domain services: default-domain auto-provisioning and the
//! hostname → server resolution the Web Server fallback consumes.

use sdkwork_deploy_contract::{
    DeployAppRequestContext, DeployServiceError, DeployServiceResult, ProvisionAppDomainsResult,
    ResolvedDeployServer,
};

use crate::DeployService;

const SUPPORTED_ENVIRONMENTS: [&str; 5] = ["development", "test", "staging", "demo", "production"];

impl DeployService {
    /// Idempotently provision an app's default publishing domains for one
    /// lifecycle environment: platform DNS zones, `<slug>.app[-<env>].<suffix>`
    /// domain rows (auto-verified, the platform owns the apex) and `SERVE`
    /// bindings on the app's site. Called automatically on app creation and
    /// available for explicit re-runs (for example after a site is created).
    pub async fn provision_app_default_domains(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        environment: &str,
    ) -> DeployServiceResult<ProvisionAppDomainsResult> {
        if !SUPPORTED_ENVIRONMENTS.contains(&environment) {
            return Err(DeployServiceError::validation(
                "environment must be development, test, staging, demo, or production",
            ));
        }
        let tenant_id = DeployService::require_tenant(context)?;
        let app = self.repository.retrieve_app(tenant_id, app_id).await?;
        let organization_id = context.organization_id.unwrap_or(0);
        self.repository
            .ensure_platform_app_zones(tenant_id, organization_id, context.actor_id)
            .await?;
        let result = self
            .repository
            .provision_app_default_domains(
                tenant_id,
                organization_id,
                context.actor_id,
                &app.id,
                &app.slug,
                environment,
            )
            .await?;
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

    /// Resolve an active app binding by its exact hostname in one lifecycle
    /// environment and return the app's latest compiled website runtime
    /// descriptor. Both default app domains (`<slug>.app[-<env>].<suffix>`)
    /// and user custom domains are resolved here.
    pub async fn resolve_server_by_hostname(
        &self,
        hostname: &str,
        environment: &str,
    ) -> DeployServiceResult<Option<ResolvedDeployServer>> {
        if !SUPPORTED_ENVIRONMENTS.contains(&environment) {
            return Err(DeployServiceError::validation(
                "environment must be development, test, staging, demo, or production",
            ));
        }
        self.repository
            .resolve_server_by_hostname(hostname, environment)
            .await
    }
}
