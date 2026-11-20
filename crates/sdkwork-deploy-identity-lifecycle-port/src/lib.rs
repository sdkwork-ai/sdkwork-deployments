//! Deploy → IAM installation lifecycle port.
//!
//! `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` section 3 stage 6 makes uninstall a
//! cross-module sequence: **IAM first** (stop issuance and credential
//! resolution), then the hosting carrier (stop traffic), then the deploy
//! application (archive). This crate owns the first hop and nothing else — the
//! service decides the order, the repository records the durable stage, and this
//! port asks IAM to disable the installation that belongs to one deploy
//! application.
//!
//! The port is deliberately small. Uninstall is not a general IAM management
//! surface: deploy may only disable the installation it provisioned, addressed
//! by the deployment anchor it already stores
//! (`deploy_app.id` as a decimal string), never by an IAM row id it would have to
//! look up by a name or a slug.
//!
//! Both adapters apply the same IAM function (`disable_tenant_application_by_deploy_application_id`),
//! so the in-process default and the split-deployment HTTP mode cannot disagree
//! about what "disabled" means.

pub mod http;
pub mod in_process;
pub mod settings;

use async_trait::async_trait;

pub use http::HttpInstallationLifecycleAdapter;
pub use in_process::InProcessInstallationLifecycleAdapter;
pub use settings::{
    resolve_lifecycle_mode, InstallationLifecycleConfigError, InstallationLifecycleMode,
    InstallationLifecycleSettings,
};

/// What one disable attempt produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisabledInstallation {
    /// `iam_application_installation.id` of the installation that was disabled.
    pub installation_id: Option<String>,
    /// Whether the installation had already been disabled before this call.
    pub already_disabled: bool,
}

/// Typed failure of one lifecycle attempt.
///
/// A failure never rolls back the deploy application: the service records the
/// uninstall stage as `failed`, leaves the application operable, and lets the
/// operator retry. The distinction that matters to the caller is
/// [`Self::NotFound`] (no installation is bound to this anchor, which uninstall
/// treats as "already stopped") against the rest (IAM could not answer).
#[derive(Debug, thiserror::Error)]
pub enum InstallationLifecycleError {
    /// No installation of this tenant is bound to the deploy application.
    /// Uninstall treats this as "issuance has already stopped".
    #[error("no IAM installation is bound to deploy application {deploy_application_id}")]
    NotFound { deploy_application_id: String },
    /// IAM's own service reported a failure (in-process) or the backend answered
    /// with a problem document (HTTP). The message is carried verbatim because
    /// the operator needs the named resource from it.
    #[error("IAM installation disable failed: {0}")]
    Rejected(String),
    /// IAM did not answer within the port's per-attempt budget.
    #[error("IAM installation disable did not answer within {millis} ms")]
    TimedOut { millis: u64 },
    /// No adapter is wired. Unlike provisioning (where "skipped" is a fact about
    /// a standalone deployment), uninstall must report this as an error: the
    /// application may not be archived while its installation could still issue
    /// credentials, so the caller has to keep the application operable.
    #[error(
        "no IAM installation-lifecycle adapter is configured; the installation cannot be disabled"
    )]
    Unconfigured,
}

/// Stops the IAM installation that belongs to one deploy application.
#[async_trait]
pub trait DeployInstallationLifecyclePort: Send + Sync {
    /// Disables the installation bound to `deploy_application_id`.
    ///
    /// Idempotent: disabling an installation that is already disabled answers
    /// [`DisabledInstallation::already_disabled`] rather than an error, so an
    /// interrupted uninstall can be replayed.
    async fn disable_installation(
        &self,
        tenant_id: &str,
        deploy_application_id: &str,
    ) -> Result<DisabledInstallation, InstallationLifecycleError>;
}

/// The port a deployment that does not host IAM keeps installed.
///
/// It fails closed with [`InstallationLifecycleError::Unconfigured`]:
/// "nobody wired IAM here" is *not* evidence that issuance stopped, and
/// archiving on that assumption is exactly the ordering violation section 3
/// stage 6 forbids.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnconfiguredInstallationLifecyclePort;

#[async_trait]
impl DeployInstallationLifecyclePort for UnconfiguredInstallationLifecyclePort {
    async fn disable_installation(
        &self,
        _tenant_id: &str,
        _deploy_application_id: &str,
    ) -> Result<DisabledInstallation, InstallationLifecycleError> {
        Err(InstallationLifecycleError::Unconfigured)
    }
}

/// Maps IAM's in-process error string onto the typed port error.
///
/// IAM's management entry points return `Result<_, String>` with a small machine
/// prefix protocol; the deploy port does not parse prose, only the two facts it
/// needs: "no installation for this anchor" and "IAM refused / failed".
#[must_use]
pub fn classify_iam_disable_error(
    deploy_application_id: &str,
    message: &str,
) -> InstallationLifecycleError {
    let lowered = message.to_ascii_lowercase();
    let missing = lowered.contains("no installation")
        || lowered.contains("not found")
        || lowered.contains("was not found");
    if missing {
        InstallationLifecycleError::NotFound {
            deploy_application_id: deploy_application_id.to_owned(),
        }
    } else {
        InstallationLifecycleError::Rejected(message.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_installation_is_distinguishable_from_a_failure() {
        let missing = classify_iam_disable_error(
            "1845123456789012345",
            "no installation of tenant 100001 was found for deploy application 1845123456789012345",
        );
        assert!(matches!(
            missing,
            InstallationLifecycleError::NotFound { .. }
        ));
        let failed = classify_iam_disable_error("1", "database connection reset");
        assert!(matches!(failed, InstallationLifecycleError::Rejected(_)));
    }

    #[tokio::test]
    async fn the_unconfigured_port_fails_closed_instead_of_pretending() {
        let result = UnconfiguredInstallationLifecyclePort
            .disable_installation("7", "1")
            .await;
        assert!(matches!(
            result,
            Err(InstallationLifecycleError::Unconfigured)
        ));
    }
}
