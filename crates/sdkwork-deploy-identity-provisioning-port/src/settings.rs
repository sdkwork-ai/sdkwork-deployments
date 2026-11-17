//! Configuration shared by the in-process and HTTP adapters.
//!
//! Only paths and names are configuration. Secrets stay in a protected file
//! whose *path* is named here, never in an environment value.

use std::time::Duration;

/// Selects the adapter. Unset means [`IdentityProvisioningMode::InProcess`].
pub const IAM_PROVISIONING_MODE_ENV: &str = "SDKWORK_DEPLOY_IAM_PROVISIONING_MODE";
/// IAM backend API base URL. Read only by the opt-in HTTP adapter.
pub const IAM_BACKEND_API_URL_ENV: &str = "SDKWORK_DEPLOY_IAM_BACKEND_API_URL";
/// Path of the protected file holding the IAM bootstrap auth token.
pub const IAM_BACKEND_AUTH_TOKEN_FILE_ENV: &str = "SDKWORK_DEPLOY_IAM_BACKEND_AUTH_TOKEN_FILE";
/// Registered IAM application template id the installation is created from.
pub const IAM_APPLICATION_TEMPLATE_ID_ENV: &str = "SDKWORK_DEPLOY_IAM_APPLICATION_TEMPLATE_ID";
/// Registered IAM application key, used when no template id is configured.
pub const IAM_APPLICATION_APP_KEY_ENV: &str = "SDKWORK_DEPLOY_IAM_APPLICATION_APP_KEY";
/// Overrides the per-attempt budget; bounded to 500 ms ..= 30 s.
pub const IAM_PROVISIONING_TIMEOUT_MS_ENV: &str = "SDKWORK_DEPLOY_IAM_PROVISIONING_TIMEOUT_MS";

/// Default per-attempt budget: a provisioning call is an interactive side step
/// of `apps.create`, so it must not hold the request open for long.
pub const DEFAULT_PROVISIONING_TIMEOUT: Duration = Duration::from_secs(5);

const MINIMUM_PROVISIONING_TIMEOUT: Duration = Duration::from_millis(500);
const MAXIMUM_PROVISIONING_TIMEOUT: Duration = Duration::from_secs(30);

/// Which adapter the deployment asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IdentityProvisioningMode {
    /// IAM's own provisioning service, on the shared PostgreSQL pool. The
    /// default, and the only mode that needs no credentials.
    #[default]
    InProcess,
    /// IAM's backend HTTP route, for a genuinely split deployment.
    Http,
}

/// Where the IAM application template comes from.
///
/// IAM requires one of these: a tenant application is an *instance* of a
/// registered `iam_application_template`, and IAM refuses a request that names
/// neither. The deploy module does not register templates, so the reference is
/// configuration rather than something derived from a deploy app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationTemplateReference {
    /// `templateId` of a registered template.
    TemplateId(String),
    /// `appKey` of a registered template.
    AppKey(String),
}

impl ApplicationTemplateReference {
    /// The `templateId` wire field when this reference names one.
    #[must_use]
    pub fn template_id(&self) -> Option<&str> {
        match self {
            Self::TemplateId(value) => Some(value.as_str()),
            Self::AppKey(_) => None,
        }
    }

    /// The `appKey` wire field when this reference names one.
    #[must_use]
    pub fn app_key(&self) -> Option<&str> {
        match self {
            Self::AppKey(value) => Some(value.as_str()),
            Self::TemplateId(_) => None,
        }
    }

    /// The configured value, for logs. Neither value is a secret.
    #[must_use]
    pub fn describe(&self) -> &str {
        match self {
            Self::TemplateId(value) | Self::AppKey(value) => value.as_str(),
        }
    }

    /// Reads the configured reference; `None` means "provisioning is not
    /// configured on this deployment".
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let template_id = std::env::var(IAM_APPLICATION_TEMPLATE_ID_ENV)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if let Some(template_id) = template_id {
            return Some(Self::TemplateId(template_id));
        }
        std::env::var(IAM_APPLICATION_APP_KEY_ENV)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(Self::AppKey)
    }
}

/// Everything one adapter needs besides its transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityProvisioningSettings {
    /// Template the installation is created from.
    pub template: ApplicationTemplateReference,
    /// Budget for one provisioning attempt.
    pub timeout: Duration,
}

impl IdentityProvisioningSettings {
    /// Resolves the template and the timeout from the environment.
    ///
    /// Returns `None` when no template is configured: the caller then installs
    /// the unconfigured port, so applications keep `pending_identity` instead of
    /// failing a call nobody asked for.
    pub fn from_env() -> Result<Option<Self>, IdentityProvisioningConfigError> {
        let Some(template) = ApplicationTemplateReference::from_env() else {
            return Ok(None);
        };
        Ok(Some(Self {
            template,
            timeout: resolve_timeout()?,
        }))
    }
}

/// Typed configuration failure, reported at startup rather than at the first
/// application create.
#[derive(Debug, thiserror::Error)]
pub enum IdentityProvisioningConfigError {
    #[error("{IAM_PROVISIONING_MODE_ENV} must be one of in_process|http, found {0:?}")]
    UnknownMode(String),
    #[error("{0} is required when {IAM_PROVISIONING_MODE_ENV}=http")]
    MissingHttpSetting(&'static str),
    #[error(
        "{IAM_PROVISIONING_TIMEOUT_MS_ENV} must be an integer between 500 and 30000, found {0:?}"
    )]
    InvalidTimeout(String),
    #[error("the credential file {path} is missing or does not hold one non-blank token")]
    CredentialUnusable { path: String },
    #[error("could not build the IAM provisioning HTTP client")]
    HttpClientUnavailable(#[source] reqwest::Error),
}

/// Reads the requested adapter mode.
pub fn resolve_identity_provisioning_mode(
) -> Result<IdentityProvisioningMode, IdentityProvisioningConfigError> {
    let raw = match std::env::var(IAM_PROVISIONING_MODE_ENV) {
        Ok(value) => value,
        Err(_) => return Ok(IdentityProvisioningMode::InProcess),
    };
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "in_process" | "in-process" | "local" => Ok(IdentityProvisioningMode::InProcess),
        "http" | "remote" => Ok(IdentityProvisioningMode::Http),
        _ => Err(IdentityProvisioningConfigError::UnknownMode(raw)),
    }
}

/// Reads the per-attempt budget, bounded so a typo cannot create an unbounded
/// external await.
pub fn resolve_timeout() -> Result<Duration, IdentityProvisioningConfigError> {
    let raw = match std::env::var(IAM_PROVISIONING_TIMEOUT_MS_ENV) {
        Ok(value) => value,
        Err(_) => return Ok(DEFAULT_PROVISIONING_TIMEOUT),
    };
    let millis: u64 = raw
        .trim()
        .parse()
        .map_err(|_| IdentityProvisioningConfigError::InvalidTimeout(raw.clone()))?;
    let timeout = Duration::from_millis(millis);
    if !(MINIMUM_PROVISIONING_TIMEOUT..=MAXIMUM_PROVISIONING_TIMEOUT).contains(&timeout) {
        return Err(IdentityProvisioningConfigError::InvalidTimeout(raw));
    }
    Ok(timeout)
}

/// Reads the protected auth-token file the HTTP adapter authenticates with.
///
/// IAM's backend route is a `bootstrap-body` surface: the super-admin auth token
/// travels in the request body, and `x-sdkwork-forbid-credential-headers: true`
/// forbids presenting it as a header instead. The **path** is configuration; the
/// token itself never passes through an environment variable or a command line,
/// where every process listing and container inspect would show it.
pub fn resolve_auth_token() -> Result<String, IdentityProvisioningConfigError> {
    let path = std::env::var(IAM_BACKEND_AUTH_TOKEN_FILE_ENV).map_err(|_| {
        IdentityProvisioningConfigError::MissingHttpSetting(IAM_BACKEND_AUTH_TOKEN_FILE_ENV)
    })?;
    let path = path.trim();
    if path.is_empty() {
        return Err(IdentityProvisioningConfigError::MissingHttpSetting(
            IAM_BACKEND_AUTH_TOKEN_FILE_ENV,
        ));
    }
    let value = std::fs::read_to_string(path).map_err(|_| {
        IdentityProvisioningConfigError::CredentialUnusable {
            path: path.to_owned(),
        }
    })?;
    let value = value.trim();
    if value.is_empty() || value.len() > 4_096 || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(IdentityProvisioningConfigError::CredentialUnusable {
            path: path.to_owned(),
        });
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_reference_prefers_explicit_id_and_exposes_one_wire_field() {
        let reference = ApplicationTemplateReference::TemplateId("tmpl_platform".to_owned());
        assert_eq!(reference.template_id(), Some("tmpl_platform"));
        assert_eq!(reference.app_key(), None);

        let reference = ApplicationTemplateReference::AppKey("sdkwork-platform".to_owned());
        assert_eq!(reference.template_id(), None);
        assert_eq!(reference.app_key(), Some("sdkwork-platform"));
    }
}
