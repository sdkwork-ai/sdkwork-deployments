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
    resolve_identity_provisioning_mode, resolve_timeout, IdentityProvisioningConfigError,
    IdentityProvisioningMode, IdentityProvisioningSettings, DEFAULT_PROVISIONING_TIMEOUT,
    IAM_BACKEND_API_URL_ENV, IAM_BACKEND_AUTH_TOKEN_FILE_ENV, IAM_PROVISIONING_MODE_ENV,
    IAM_PROVISIONING_TIMEOUT_MS_ENV,
};

/// The application types IAM accepts (`api` | `h5` | `pc` | `flutter` |
/// `miniprogram` | `ios` | `android` | `harmonyos` | `desktop` | `other`).
pub const APPLICATION_TYPES: &[&str] = &[
    "api",
    "h5",
    "pc",
    "flutter",
    "miniprogram",
    "ios",
    "android",
    "harmonyos",
    "desktop",
    "other",
];

/// Normalises an application type onto IAM's closed vocabulary.
///
/// IAM requires the field and coerces anything it does not recognise to
/// `other`; doing the same here means the port never sends a value the receiver
/// would have to reinterpret, and the mapping is testable without IAM.
#[must_use]
pub fn normalize_application_type(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "api" => "api",
        "h5" => "h5",
        "pc" => "pc",
        "flutter" => "flutter",
        "miniprogram" => "miniprogram",
        "ios" => "ios",
        "android" => "android",
        "harmonyos" => "harmonyos",
        "desktop" => "desktop",
        _ => "other",
    }
}

/// The ownership scopes IAM accepts on an installation
/// (`TENANT` | `ORGANIZATION` | `USER`).
pub const OWNERSHIP_SCOPES: &[&str] = &["TENANT", "ORGANIZATION", "USER"];

/// Normalises a deploy ownership level onto IAM's installation scopes.
///
/// Deploy has a fourth, platform-internal level (`PLATFORM`) that IAM does not
/// model on an installation: a platform application is readable by every tenant
/// and has no per-tenant owner, which is exactly IAM's `TENANT` scope. The
/// mapping is therefore explicit and total, never a pass-through of a value the
/// receiver would have to reinterpret.
#[must_use]
pub fn normalize_ownership_scope(value: &str) -> &'static str {
    match value.trim().to_ascii_uppercase().as_str() {
        "ORGANIZATION" => "ORGANIZATION",
        "USER" => "USER",
        _ => "TENANT",
    }
}

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
    /// IAM application type, derived from the deploy app kind and normalised onto
    /// IAM's closed vocabulary (`api` | `h5` | `pc` | `flutter` | `other`) by the
    /// adapters. IAM requires the field; there is no template to fall back to.
    pub application_type: String,
    /// The deploy application's ownership level (`TENANT`, `ORGANIZATION`,
    /// `USER`, or the platform-internal `PLATFORM`). The adapters normalise it
    /// onto [`OWNERSHIP_SCOPES`], so the installation's stored scope always
    /// agrees with the application's own.
    pub ownership_scope: String,
    /// The owning user, required for `USER` and absent otherwise.
    pub owner_user_id: Option<String>,
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
            application_type: "pc".to_owned(),
            ownership_scope: "TENANT".to_owned(),
            owner_user_id: None,
            deploy_application_id: "1845123456789012345".to_owned(),
            provisioning_idempotency_key: "deploy:1845123456789012345".to_owned(),
        }
    }

    #[test]
    fn application_types_are_normalised_onto_iams_closed_vocabulary() {
        for accepted in APPLICATION_TYPES {
            assert_eq!(normalize_application_type(accepted), *accepted);
        }
        // Case and padding are the only tolerated variation; anything else is
        // IAM's own `other`, never a value IAM would have to reinterpret.
        assert_eq!(normalize_application_type("  API_SERVICE "), "other");
        assert_eq!(normalize_application_type(""), "other");
        assert_eq!(normalize_application_type("  PC "), "pc");
        assert_eq!(normalize_application_type("flutter"), "flutter");
    }

    #[test]
    fn ownership_scope_is_normalised_onto_iams_installation_scopes() {
        for accepted in OWNERSHIP_SCOPES {
            assert_eq!(normalize_ownership_scope(accepted), *accepted);
        }
        assert_eq!(normalize_ownership_scope(" organization "), "ORGANIZATION");
        assert_eq!(normalize_ownership_scope("user"), "USER");
        // The deploy-only platform level has no per-tenant owner; its honest IAM
        // representation is the tenant-wide scope.
        assert_eq!(normalize_ownership_scope("PLATFORM"), "TENANT");
        assert_eq!(normalize_ownership_scope(""), "TENANT");
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
