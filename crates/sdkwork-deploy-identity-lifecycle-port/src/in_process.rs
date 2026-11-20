//! IAM's own disable function, called on the shared PostgreSQL pool.
//!
//! This is the default adapter, mirroring the provisioning port's in-process
//! choice (`MODULE_SPEC.md` §4.2: a backend module reaches a sibling through a
//! normal Cargo dependency on the shared pool). IAM exposes the disable write as
//! a service function on its web-adapter crate, which is the same function its
//! own management handler calls, so the deploy uninstall path and the operator's
//! disable button cannot apply different rules.

use async_trait::async_trait;
use sqlx::PgPool;

use sdkwork_iam_web_adapter::disable_tenant_application_by_deploy_application_id;

use crate::settings::{InstallationLifecycleConfigError, InstallationLifecycleSettings};
use crate::{
    classify_iam_disable_error, DeployInstallationLifecyclePort, DisabledInstallation,
    InstallationLifecycleError,
};

/// Disables installations through IAM's service layer on the host pool.
#[derive(Clone, Debug)]
pub struct InProcessInstallationLifecycleAdapter {
    pool: PgPool,
    settings: InstallationLifecycleSettings,
}

impl InProcessInstallationLifecycleAdapter {
    /// Builds the adapter for a deployment whose IAM schema lives in the same
    /// PostgreSQL database as its deploy schema.
    pub fn new(pool: PgPool, settings: InstallationLifecycleSettings) -> Self {
        Self { pool, settings }
    }

    /// Builds the adapter from the environment.
    pub fn from_env(pool: PgPool) -> Result<Self, InstallationLifecycleConfigError> {
        Ok(Self::new(pool, InstallationLifecycleSettings::from_env()?))
    }
}

#[async_trait]
impl DeployInstallationLifecyclePort for InProcessInstallationLifecycleAdapter {
    async fn disable_installation(
        &self,
        tenant_id: &str,
        deploy_application_id: &str,
    ) -> Result<DisabledInstallation, InstallationLifecycleError> {
        let attempt = tokio::time::timeout(
            self.settings.timeout,
            disable_tenant_application_by_deploy_application_id(
                &self.pool,
                tenant_id,
                deploy_application_id,
            ),
        )
        .await;
        let attempt = match attempt {
            Ok(result) => result,
            Err(_) => {
                return Err(InstallationLifecycleError::TimedOut {
                    millis: u64::try_from(self.settings.timeout.as_millis()).unwrap_or(u64::MAX),
                })
            }
        };
        match attempt {
            Ok(application) => Ok(DisabledInstallation {
                already_disabled: application.status.eq_ignore_ascii_case("disabled"),
                installation_id: Some(application.id),
            }),
            Err(message) => Err(classify_iam_disable_error(deploy_application_id, &message)),
        }
    }
}
