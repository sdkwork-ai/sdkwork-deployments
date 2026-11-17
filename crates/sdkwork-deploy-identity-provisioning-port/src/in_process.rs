//! IAM's own provisioning service, called on the shared PostgreSQL pool.
//!
//! This is the default adapter. `MODULE_SPEC.md` §4.2 and `RUST_CODE_SPEC.md`
//! §17 require a backend module to reach a sibling module through a normal
//! Cargo dependency on the shared pool, not through the sibling's HTTP surface —
//! a loopback origin is a separate socket hop with its own authentication and
//! its own failure timing. sdkwork-iam exposes the provisioning write as a
//! service function on its web-adapter crate (`provision_tenant_application`),
//! which is the same function its own backend route calls, so both adapters
//! apply identical rules, idempotency included.

use async_trait::async_trait;
use sqlx::PgPool;

use sdkwork_iam_web_adapter::{provision_tenant_application, TenantApplicationProvisionCommand};

use crate::settings::{IdentityProvisioningConfigError, IdentityProvisioningSettings};
use crate::{
    DeployApplicationProvisioningCommand, DeployIdentityProvisioningPort,
    IdentityProvisioningError, ProvisioningOutcome, TenantApplicationReceipt,
};

/// Provisions tenant applications through IAM's service layer on the host pool.
#[derive(Clone, Debug)]
pub struct InProcessIdentityProvisioningAdapter {
    pool: PgPool,
    settings: IdentityProvisioningSettings,
}

impl InProcessIdentityProvisioningAdapter {
    /// Builds the adapter for a deployment whose IAM schema lives in the same
    /// PostgreSQL database as its deploy schema.
    pub fn new(pool: PgPool, settings: IdentityProvisioningSettings) -> Self {
        Self { pool, settings }
    }

    /// Builds the adapter from the environment, or reports that provisioning is
    /// not configured on this deployment.
    pub fn from_env(pool: PgPool) -> Result<Option<Self>, IdentityProvisioningConfigError> {
        Ok(IdentityProvisioningSettings::from_env()?
            .map(|settings| Self::new(pool.clone(), settings)))
    }

    /// The configured template reference, for startup logging.
    #[must_use]
    pub fn template(&self) -> &str {
        self.settings.template.describe()
    }
}

/// Maps one deploy application onto IAM's provisioning command.
///
/// Kept free-standing so the mapping can be asserted without a database.
fn provision_command(
    settings: &IdentityProvisioningSettings,
    command: &DeployApplicationProvisioningCommand,
) -> TenantApplicationProvisionCommand {
    TenantApplicationProvisionCommand {
        tenant_id: command.tenant_id.clone(),
        organization_id: command.organization_id.clone(),
        template_id: settings.template.template_id().map(str::to_owned),
        app_key: settings.template.app_key().map(str::to_owned),
        // IAM assigns the runtime app id; a deploy application has no
        // pre-allocated one to force.
        app_id: None,
        instance_key: command.instance_key.clone(),
        display_name: command.display_name.clone(),
        environment: command.environment.clone(),
        application_type: command.application_type.clone(),
        primary_domain: None,
        // Empty lists let IAM apply the template's own defaults instead of
        // Deploy inventing a permission or runtime scope for it.
        access_permissions: Vec::new(),
        runtime_config: serde_json::json!({}),
        deploy_application_id: Some(command.deploy_application_id.clone()),
        provisioning_idempotency_key: Some(command.provisioning_idempotency_key.clone()),
    }
}

#[async_trait]
impl DeployIdentityProvisioningPort for InProcessIdentityProvisioningAdapter {
    async fn provision_tenant_application(
        &self,
        command: &DeployApplicationProvisioningCommand,
    ) -> Result<ProvisioningOutcome, IdentityProvisioningError> {
        let provision_command = provision_command(&self.settings, command);
        // Every external await is bounded, including an in-process one: a pool
        // waiting on a saturated or half-failed database is exactly the shape
        // that turns one stuck request into a stuck request queue.
        let attempt = tokio::time::timeout(
            self.settings.timeout,
            provision_tenant_application(&self.pool, &provision_command),
        )
        .await
        .map_err(|_| IdentityProvisioningError::TimedOut {
            millis: u64::try_from(self.settings.timeout.as_millis()).unwrap_or(u64::MAX),
        })?;
        let application = attempt.map_err(IdentityProvisioningError::Store)?;
        Ok(ProvisioningOutcome::Installed(TenantApplicationReceipt {
            tenant_application_id: Some(application.id),
            runtime_app_id: Some(application.app_id),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ApplicationTemplateReference;

    fn settings() -> IdentityProvisioningSettings {
        IdentityProvisioningSettings {
            template: ApplicationTemplateReference::TemplateId("tmpl_platform".to_owned()),
            timeout: crate::settings::DEFAULT_PROVISIONING_TIMEOUT,
        }
    }

    fn command() -> DeployApplicationProvisioningCommand {
        DeployApplicationProvisioningCommand {
            tenant_id: "7".to_owned(),
            organization_id: "0".to_owned(),
            instance_key: "0f8b7f26-2d7c-4f43-8a4b-0c5a51e1f0aa".to_owned(),
            display_name: "Shop".to_owned(),
            environment: "production".to_owned(),
            application_type: Some("pc".to_owned()),
            deploy_application_id: "1845123456789012345".to_owned(),
            provisioning_idempotency_key: "deploy:1845123456789012345".to_owned(),
        }
    }

    #[test]
    fn provision_command_carries_the_deploy_link_and_the_replay_key() {
        let mapped = provision_command(&settings(), &command());
        assert_eq!(
            mapped.deploy_application_id.as_deref(),
            Some("1845123456789012345")
        );
        assert_eq!(
            mapped.provisioning_idempotency_key.as_deref(),
            Some("deploy:1845123456789012345")
        );
        assert_eq!(mapped.template_id.as_deref(), Some("tmpl_platform"));
        assert_eq!(mapped.app_key, None);
        assert_eq!(mapped.instance_key, command().instance_key);
        assert!(mapped.access_permissions.is_empty());
    }

    #[test]
    fn app_key_reference_reaches_iam_without_a_template_id() {
        let settings = IdentityProvisioningSettings {
            template: ApplicationTemplateReference::AppKey("sdkwork-platform".to_owned()),
            timeout: crate::settings::DEFAULT_PROVISIONING_TIMEOUT,
        };
        let mapped = provision_command(&settings, &command());
        assert_eq!(mapped.template_id, None);
        assert_eq!(mapped.app_key.as_deref(), Some("sdkwork-platform"));
    }
}
