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
//!
//! The installation is derived from the deploy application itself. IAM's
//! application-template tables are retired, so the IAM command carries no
//! `templateId`/`appKey`: the required identity inputs are the deploy
//! application id, the instance key, and an application type.

use async_trait::async_trait;
use sqlx::PgPool;

use sdkwork_iam_web_adapter::{provision_tenant_application, TenantApplicationProvisionCommand};

use crate::settings::{IdentityProvisioningConfigError, IdentityProvisioningSettings};
use crate::{
    normalize_application_type, DeployApplicationProvisioningCommand,
    DeployIdentityProvisioningPort, IdentityProvisioningError, ProvisioningOutcome,
    TenantApplicationReceipt,
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

    /// Builds the adapter from the environment.
    pub fn from_env(pool: PgPool) -> Result<Self, IdentityProvisioningConfigError> {
        Ok(Self::new(pool, IdentityProvisioningSettings::from_env()?))
    }
}

/// Maps one deploy application onto IAM's provisioning command.
///
/// Kept free-standing so the mapping can be asserted without a database.
fn provision_command(
    command: &DeployApplicationProvisioningCommand,
) -> TenantApplicationProvisionCommand {
    TenantApplicationProvisionCommand {
        tenant_id: command.tenant_id.clone(),
        organization_id: command.organization_id.clone(),
        // IAM assigns the runtime app id; a deploy application has no
        // pre-allocated one to force.
        app_id: None,
        instance_key: command.instance_key.clone(),
        display_name: command.display_name.clone(),
        environment: command.environment.clone(),
        // IAM requires a type now that templates are gone, so an unrecognised
        // value is normalised to `other` here instead of being forwarded for IAM
        // to reinterpret.
        application_type: Some(
            normalize_application_type(command.application_type.as_str()).to_owned(),
        ),
        primary_domain: None,
        // Deliberately empty, not an omission: this installation grants no API
        // scope of its own. IAM now accepts an empty list, and a future need for
        // scoped access has to name its permissions explicitly.
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
        let provision_command = provision_command(command);
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

    fn command() -> DeployApplicationProvisioningCommand {
        DeployApplicationProvisioningCommand {
            tenant_id: "7".to_owned(),
            organization_id: "0".to_owned(),
            instance_key: "0f8b7f26-2d7c-4f43-8a4b-0c5a51e1f0aa".to_owned(),
            display_name: "Shop".to_owned(),
            environment: "production".to_owned(),
            application_type: "pc".to_owned(),
            deploy_application_id: "1845123456789012345".to_owned(),
            provisioning_idempotency_key: "deploy:1845123456789012345".to_owned(),
        }
    }

    #[test]
    fn provision_command_carries_the_deploy_link_and_the_replay_key() {
        let mapped = provision_command(&command());
        assert_eq!(
            mapped.deploy_application_id.as_deref(),
            Some("1845123456789012345")
        );
        assert_eq!(
            mapped.provisioning_idempotency_key.as_deref(),
            Some("deploy:1845123456789012345")
        );
        assert_eq!(mapped.instance_key, command().instance_key);
        assert_eq!(mapped.display_name, "Shop");
        assert_eq!(mapped.environment, "production");
    }

    #[test]
    fn provision_command_sends_a_normalised_type_and_no_api_scope() {
        let mapped = provision_command(&command());
        assert_eq!(mapped.application_type.as_deref(), Some("pc"));
        // Empty is intentional: the installation grants no API scope of its own.
        assert!(mapped.access_permissions.is_empty());

        let mut unknown = command();
        unknown.application_type = "SPA_WEB".to_owned();
        assert_eq!(
            provision_command(&unknown).application_type.as_deref(),
            Some("other")
        );
    }
}
