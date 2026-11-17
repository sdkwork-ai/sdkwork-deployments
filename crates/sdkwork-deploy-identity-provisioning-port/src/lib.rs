//! Deploy → IAM tenant-application provisioning port.
//!
//! Creating a `deploy_app` row is only half of an application: the runtime
//! identity of that application (its IAM *installation*) lives in sdkwork-iam's
//! `iam_tenant_application` table. This crate owns that cross-module step and
//! nothing else — the service layer decides *when* to provision, the repository
//! owns the `deploy_app.identity_*` columns, and this port turns one deploy
//! application into at most one IAM installation.
//!
//! Two adapters implement the same [`DeployIdentityProvisioningPort`]:
//!
//! - [`in_process::InProcessIdentityProvisioningAdapter`] calls IAM's own
//!   provisioning service on the shared PostgreSQL pool. It is the default,
//!   because `MODULE_SPEC.md` §4.2 and `RUST_CODE_SPEC.md` §17 require a backend
//!   module to reach a sibling in-process rather than through its HTTP surface.
//! - [`http::HttpIdentityProvisioningAdapter`] posts to IAM's backend route
//!   `POST /backend/v3/api/iam/tenant_applications` for a genuinely split
//!   deployment. It is opt-in (`SDKWORK_DEPLOY_IAM_PROVISIONING_MODE=http`) and
//!   the exception is recorded in
//!   `specs/backend-module-call-boundary.allow.json`.
//!
//! Both adapters are replay-safe: `deploy_app.identity_provisioning_key` is
//! stable per application (`deploy:{id}`) and IAM stores it in
//! `iam_tenant_application.provisioning_idempotency_key`, whose partial unique
//! index is what makes a retried create land on the same installation instead of
//! a second one.

pub mod http;
pub mod in_process;
pub mod settings;

use async_trait::async_trait;

pub use http::{HttpIdentityProvisioningAdapter, IAM_TENANT_APPLICATIONS_PATH};
pub use in_process::InProcessIdentityProvisioningAdapter;
pub use settings::{
    resolve_identity_provisioning_mode, resolve_timeout, ApplicationTemplateReference,
    IdentityProvisioningConfigError, IdentityProvisioningMode, IdentityProvisioningSettings,
    DEFAULT_PROVISIONING_TIMEOUT, IAM_APPLICATION_APP_KEY_ENV, IAM_APPLICATION_TEMPLATE_ID_ENV,
    IAM_BACKEND_API_URL_ENV, IAM_BACKEND_AUTH_TOKEN_FILE_ENV, IAM_PROVISIONING_MODE_ENV,
    IAM_PROVISIONING_TIMEOUT_MS_ENV,
};

/// One deploy application, expressed as the inputs IAM's provisioning service
/// accepts.
///
/// The identifiers are the wire strings IAM stores: ids cross module boundaries
/// as decimal strings (`API_SPEC.md` §13.6), never as `i64`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeployApplicationProvisioningCommand {
    /// Owning tenant. IAM scopes the installation to it.
    pub tenant_id: String,
    /// Owning organization, or `"0"` when the application has none — IAM's own
    /// platform bootstrap uses the literal `"0"` for "no organization", so a
    /// deployment application follows the same convention instead of inventing
    /// an organization.
    pub organization_id: String,
    /// Stable installation key. The deploy application's `uuid`, not its slug:
    /// a slug can be renamed, and a rename must not look like a new installation.
    pub instance_key: String,
    /// Human-readable name shown in the IAM console.
    pub display_name: String,
    /// Lifecycle environment recorded on the installation.
    pub environment: String,
    /// IAM application type (`api` | `h5` | `pc` | `other`), derived from the
    /// deploy app kind. Absent lets IAM fall back to the template's own type.
    pub application_type: Option<String>,
    /// `deploy_app.id` (BIGINT) as a decimal string. IAM stores it in
    /// `iam_tenant_application.deploy_application_id` under a unique index, so
    /// one deploy application can never own two installations.
    pub deploy_application_id: String,
    /// Replay-safe key persisted on `deploy_app.identity_provisioning_key`.
    pub provisioning_idempotency_key: String,
}

/// What one provisioning attempt produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvisioningOutcome {
    /// IAM accepted the request (a fresh installation or the replay of it).
    Installed(TenantApplicationReceipt),
    /// No adapter is wired. Nothing was called, and the application's identity
    /// intentionally stays `pending_identity` rather than being reported as a
    /// failure it never had.
    Skipped,
}

/// Identity of the IAM installation a provisioning call resolved to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TenantApplicationReceipt {
    /// `iam_tenant_application.id`, when the adapter could read it back.
    pub tenant_application_id: Option<String>,
    /// `iam_tenant_application.app_id` (the runtime application id).
    pub runtime_app_id: Option<String>,
}

/// Typed failure of one provisioning attempt.
///
/// A failed attempt never rolls back the created deploy application: the service
/// records it on `deploy_app.identity_status` and logs the error.
#[derive(Debug, thiserror::Error)]
pub enum IdentityProvisioningError {
    #[error("IAM application provisioning is not configured")]
    NotConfigured,
    #[error("IAM rejected the application provisioning request with status {status} ({code})")]
    Rejected { status: u16, code: String },
    #[error("IAM application provisioning did not answer within {millis} ms")]
    TimedOut { millis: u64 },
    /// IAM's own service reported a failure. IAM's in-process entrypoint returns
    /// `Result<_, String>`, so the message it produced is carried verbatim; the
    /// HTTP adapter reports the same failures as [`Self::Rejected`] with the
    /// numeric problem code.
    #[error("IAM application provisioning failed: {0}")]
    Store(String),
    #[error("IAM application provisioning transport failed")]
    Transport(#[source] reqwest::Error),
}

/// Provisions the IAM installation that makes a deploy application a runtime
/// identity.
#[async_trait]
pub trait DeployIdentityProvisioningPort: Send + Sync {
    /// Requests (or replays) the installation for one deploy application.
    ///
    /// The call is bounded by the adapter's timeout; it never panics and never
    /// blocks indefinitely.
    async fn provision_tenant_application(
        &self,
        command: &DeployApplicationProvisioningCommand,
    ) -> Result<ProvisioningOutcome, IdentityProvisioningError>;
}

/// The port a deployment that does not host IAM keeps installed.
///
/// It answers [`ProvisioningOutcome::Skipped`] rather than an error: "nobody
/// wired IAM here" is a fact about the deployment, and reporting it as a failed
/// provisioning would mark every application of a standalone installation
/// `failed` for a step that was never attempted.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnconfiguredIdentityProvisioningPort;

#[async_trait]
impl DeployIdentityProvisioningPort for UnconfiguredIdentityProvisioningPort {
    async fn provision_tenant_application(
        &self,
        _command: &DeployApplicationProvisioningCommand,
    ) -> Result<ProvisioningOutcome, IdentityProvisioningError> {
        Ok(ProvisioningOutcome::Skipped)
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
            application_type: Some("pc".to_owned()),
            deploy_application_id: "1845123456789012345".to_owned(),
            provisioning_idempotency_key: "deploy:1845123456789012345".to_owned(),
        }
    }

    #[tokio::test]
    async fn unconfigured_port_skips_instead_of_failing() {
        let outcome = UnconfiguredIdentityProvisioningPort
            .provision_tenant_application(&command())
            .await
            .expect("unconfigured port reports a fact, not an error");
        assert_eq!(outcome, ProvisioningOutcome::Skipped);
    }
}
