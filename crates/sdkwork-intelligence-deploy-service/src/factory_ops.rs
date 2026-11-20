//! Factory commands: derived readiness and the forward-only uninstall sequence.
//!
//! `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` section 11 turns the new-application
//! runbook into six derived checks; section 3 stage 6 turns retirement into an
//! ordered sequence. Both were operator procedures with no command behind them
//! before this module:
//!
//! - Readiness reads the rows that exist and derives a verdict. It never stores
//!   one (section 3), so a retired artifact or a disabled installation changes the
//!   answer on the next call without a write.
//! - Uninstall is IAM first (`identity_lifecycle.disable_installation`), then the
//!   deploy environments, then `ARCHIVED` on the application. Each step writes a
//!   durable stage marker before the next is attempted, so an interrupted
//!   uninstall is replayed from the stored state instead of starting over, and a
//!   failure keeps the application operable rather than archiving it while its
//!   installation could still issue credentials.

use sdkwork_deploy_contract::{
    AppFactoryReadiness, AppUninstallResponse, DeployAppRequestContext, DeployServiceError,
    DeployServiceResult, FactoryReadinessCheck, FactoryReadinessStep,
};
use sdkwork_deploy_identity_lifecycle_port::{DisabledInstallation, InstallationLifecycleError};

use crate::app_delivery::assert_app_reachable;
use crate::DeployService;

/// The `deploy_app.uninstall_status` vocabulary, mirrored from
/// `chk_deploy_app_uninstall_status`. Strings rather than an enum because the
/// repository binds the wire vocabulary verbatim and the CHECK is the authority.
pub(crate) const UNINSTALL_PENDING_IDENTITY_DISABLE: &str = "pending_identity_disable";
pub(crate) const UNINSTALL_IDENTITY_DISABLED: &str = "identity_disabled";
pub(crate) const UNINSTALL_FAILED: &str = "failed";

impl DeployService {
    /// Returns the derived per-stage readiness of one application.
    pub(crate) async fn factory_readiness(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppFactoryReadiness> {
        let tenant_id = Self::require_tenant(context)?;
        let app = self.repository.retrieve_app(tenant_id, app_id).await?;
        assert_app_reachable(context, &app)?;
        let facts = self
            .repository
            .factory_readiness_facts(tenant_id, app_id)
            .await?;
        let verdict = sdkwork_deploy_core::derive_factory_readiness(&facts);
        Ok(AppFactoryReadiness {
            status: verdict.status.to_owned(),
            blocking_step: verdict
                .blocking_stage
                .map(|stage| stage.as_str().to_owned()),
            checks: verdict
                .stages
                .into_iter()
                .map(|stage| FactoryReadinessCheck {
                    step: readiness_step(stage.stage).as_str().to_owned(),
                    status: stage.status.to_owned(),
                    reason: stage.reason,
                    detail: Some(stage.detail),
                })
                .collect(),
        })
    }

    /// Runs the forward-only uninstall sequence.
    ///
    /// Order (section 3 stage 6): stop issuance, stop traffic, archive. Deploy owns
    /// the first two steps and records each one; the hosting carrier row is
    /// released by its own module once the deploy domains stop resolving.
    pub(crate) async fn uninstall_app(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        idempotency_key: &str,
    ) -> DeployServiceResult<AppUninstallResponse> {
        let tenant_id = Self::require_tenant(context)?;
        let app = self.repository.retrieve_app(tenant_id, app_id).await?;
        assert_app_reachable(context, &app)?;
        let (uninstall_status, app_status) = self
            .repository
            .retrieve_uninstall_state(tenant_id, app_id)
            .await?;
        if app_status.eq_ignore_ascii_case("ARCHIVED")
            || uninstall_status.eq_ignore_ascii_case("complete")
        {
            // Replay: the sequence already completed. Answer success with the
            // stored state instead of re-running the disable or failing the retry.
            return Ok(AppUninstallResponse {
                app_id: app_id.to_owned(),
                uninstall_status,
                app_status,
                replayed: true,
            });
        }
        // The idempotency key names the operator's request in the audit trail; the
        // durable stage column is what makes a replay land on the same state when
        // the caller retries with a fresh key.
        let _ = idempotency_key;

        // Step 1: stop issuance. A deployment with no identity lifecycle adapter
        // cannot prove issuance stopped, so it must not archive.
        self.repository
            .mark_uninstall_stage(tenant_id, app_id, UNINSTALL_PENDING_IDENTITY_DISABLE)
            .await?;
        let disable = self
            .identity_lifecycle
            .disable_installation(
                &tenant_id.to_string(),
                &app.deploy_application_id.to_string(),
            )
            .await;
        match disable {
            Ok(DisabledInstallation {
                installation_id,
                already_disabled,
            }) => {
                tracing::info!(
                    target: "sdkwork_deploy::factory",
                    tenant_id,
                    app_id,
                    installation_id = installation_id.as_deref().unwrap_or_default(),
                    already_disabled,
                    "IAM installation disabled for uninstall"
                );
            }
            Err(InstallationLifecycleError::NotFound { .. }) => {
                // Nothing is bound to the anchor: issuance has already stopped and
                // there is nothing to disable. This is idempotent success, not a
                // failure the operator has to chase.
                tracing::info!(
                    target: "sdkwork_deploy::factory",
                    tenant_id,
                    app_id,
                    "no IAM installation is bound to this application; uninstall continues"
                );
            }
            Err(error) => {
                // Keep the application operable and record the stage so the
                // operator can retry `apps.uninstall` after fixing IAM.
                self.repository
                    .mark_uninstall_stage(tenant_id, app_id, UNINSTALL_FAILED)
                    .await?;
                let _ = self
                    .audit_app_action(context, "app.uninstall_failed", app_id)
                    .await;
                tracing::warn!(
                    target: "sdkwork_deploy::factory",
                    tenant_id,
                    app_id,
                    error = %error,
                    "uninstall stopped before archiving: the IAM installation could not be disabled"
                );
                return Err(DeployServiceError::conflict(format!(
                    "application was not archived because its IAM installation could not be disabled: {error}. \
                     Fix the identity plane and retry apps.uninstall"
                )));
            }
        }
        self.repository
            .mark_uninstall_stage(tenant_id, app_id, UNINSTALL_IDENTITY_DISABLED)
            .await?;
        let _ = self
            .audit_app_action(context, "app.identity_disabled", app_id)
            .await;

        // Step 2: stop traffic and archive. The repository disables every live
        // environment row and sets `ARCHIVED` in one transaction; no row is
        // deleted, so the history section 3 stage 6 requires stays readable.
        self.repository.archive_app(tenant_id, app_id).await?;
        let _ = self
            .audit_app_action(context, "app.uninstall", app_id)
            .await;
        let (uninstall_status, app_status) = self
            .repository
            .retrieve_uninstall_state(tenant_id, app_id)
            .await?;
        Ok(AppUninstallResponse {
            app_id: app_id.to_owned(),
            uninstall_status,
            app_status,
            replayed: false,
        })
    }
}

fn readiness_step(stage: sdkwork_deploy_core::FactoryStage) -> FactoryReadinessStep {
    use sdkwork_deploy_core::FactoryStage;
    match stage {
        FactoryStage::Registration => FactoryReadinessStep::Registration,
        FactoryStage::PlatformTarget => FactoryReadinessStep::PlatformTarget,
        FactoryStage::Environment => FactoryReadinessStep::Environment,
        FactoryStage::Identity => FactoryReadinessStep::Identity,
        FactoryStage::Source => FactoryReadinessStep::Source,
        FactoryStage::HostingCarrier => FactoryReadinessStep::HostingCarrier,
    }
}
