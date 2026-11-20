//! Lifecycle settings shared by the in-process and HTTP adapters.
//!
//! Uninstall is an interactive operator command, so it reuses the provisioning
//! port's bounded attempt budget (`SDKWORK_DEPLOY_IAM_PROVISIONING_TIMEOUT_MS`,
//! 500 ms ..= 30 s) instead of introducing a second knob that would have to be
//! kept in step with it.

use sdkwork_deploy_identity_provisioning_port::{
    resolve_timeout, IdentityProvisioningConfigError, IdentityProvisioningMode,
};

pub use sdkwork_deploy_identity_provisioning_port::{
    IAM_BACKEND_API_URL_ENV, IAM_BACKEND_AUTH_TOKEN_FILE_ENV, IAM_PROVISIONING_MODE_ENV,
    IAM_PROVISIONING_TIMEOUT_MS_ENV,
};

/// Which adapter the deployment asked for. Same switch as provisioning: one
/// deployment hosts IAM in-process or reaches it over its backend route, never
/// both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InstallationLifecycleMode {
    #[default]
    InProcess,
    Http,
}

/// Typed configuration failure, reported at startup.
#[derive(Debug, thiserror::Error)]
pub enum InstallationLifecycleConfigError {
    #[error(transparent)]
    Provisioning(#[from] IdentityProvisioningConfigError),
    /// A split-deployment adapter was requested but its endpoint is missing.
    #[error("{0} is required when the IAM lifecycle runs over HTTP")]
    MissingHttpSetting(&'static str),
    /// A split-deployment adapter was requested but its HTTP client could not be
    /// built.
    #[error("could not build the IAM lifecycle HTTP client")]
    HttpClientUnavailable(#[source] reqwest::Error),
}

/// Everything an adapter needs besides its transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstallationLifecycleSettings {
    /// Budget for one disable attempt.
    pub timeout: std::time::Duration,
}

impl InstallationLifecycleSettings {
    /// Resolves the per-attempt budget from the environment.
    pub fn from_env() -> Result<Self, InstallationLifecycleConfigError> {
        Ok(Self {
            timeout: resolve_timeout()?,
        })
    }
}

/// Reads the requested adapter mode from the provisioning switch, so a split
/// deployment cannot end up provisioning in-process and uninstalling over HTTP.
pub fn resolve_lifecycle_mode(
) -> Result<InstallationLifecycleMode, InstallationLifecycleConfigError> {
    match sdkwork_deploy_identity_provisioning_port::resolve_identity_provisioning_mode()? {
        IdentityProvisioningMode::InProcess => Ok(InstallationLifecycleMode::InProcess),
        IdentityProvisioningMode::Http => Ok(InstallationLifecycleMode::Http),
    }
}
