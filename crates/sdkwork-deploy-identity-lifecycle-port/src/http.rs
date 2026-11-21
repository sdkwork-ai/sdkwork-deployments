//! Split-deployment adapter for the installation lifecycle.
//!
//! `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` section 3 stage 6 makes uninstall a
//! cross-module sequence: **IAM first** (stop issuance and credential
//! resolution), then the hosting carrier, then the deploy application. The
//! in-process adapter is the default; this adapter is the exception for a
//! genuinely split deployment, selected by the same switch as provisioning
//! (`SDKWORK_DEPLOY_IAM_PROVISIONING_MODE=http`).
//!
//! Route authority (read from sdkwork-iam, not guessed):
//!
//! - `POST /backend/v3/api/iam/installations/by_deploy_application/{deployApplicationId}/disable`
//!   is declared in `crates/sdkwork-routes-iam-backend-api/src/manifest.rs` as a
//!   `bootstrap_body` route with permission `iam.installations.disable`
//!   (the permission code keeps its historical spelling until the IAM migration),
//!   handled by `disable_tenant_application_by_deploy_application_handler`, and
//!   published in `apis/backend-api/iam/sdkwork-iam-backend-api.openapi.yaml` as
//!   `installations.management.byDeployApplication.disable`.
//! - Authentication is the deployment's dedicated backend bootstrap token, carried
//!   in the request body (`authToken`) exactly like the provisioning route; there
//!   is no end-user request whose dual tokens could be borrowed.

use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::settings::{InstallationLifecycleConfigError, InstallationLifecycleSettings};
use crate::{DeployInstallationLifecyclePort, DisabledInstallation, InstallationLifecycleError};

/// IAM backend route that disables the installation bound to a deploy anchor.
pub const IAM_DISABLE_BY_DEPLOY_APPLICATION_PATH: &str =
    "/backend/v3/api/iam/installations/by_deploy_application/{deployApplicationId}/disable";

/// Split-deployment adapter that disables through IAM's backend HTTP surface.
#[derive(Clone, Debug)]
pub struct HttpInstallationLifecycleAdapter {
    base_url: String,
    auth_token: String,
    settings: InstallationLifecycleSettings,
    client: reqwest::Client,
}

impl HttpInstallationLifecycleAdapter {
    /// Builds the adapter from an explicit endpoint, credential, and settings.
    ///
    /// The per-attempt budget is installed on the client as well as applied
    /// around the call, so neither a connect stall nor a slow response body can
    /// outlive it.
    pub fn new(
        base_url: impl Into<String>,
        auth_token: impl Into<String>,
        settings: InstallationLifecycleSettings,
    ) -> Result<Self, InstallationLifecycleConfigError> {
        let base_url = base_url.into().trim().trim_end_matches('/').to_owned();
        let auth_token = auth_token.into().trim().to_owned();
        let client = reqwest::Client::builder()
            .timeout(settings.timeout)
            .connect_timeout(settings.timeout)
            .build()
            .map_err(InstallationLifecycleConfigError::HttpClientUnavailable)?;
        Ok(Self {
            base_url,
            auth_token,
            settings,
            client,
        })
    }

    /// Builds the adapter from the environment.
    ///
    /// Every setting is required in HTTP mode: the adapter only exists because an
    /// operator asked for a split deployment, so a missing setting fails at
    /// startup instead of at the first uninstall.
    pub fn from_env() -> Result<Self, InstallationLifecycleConfigError> {
        let settings = InstallationLifecycleSettings::from_env()?;
        let base_url = std::env::var(crate::settings::IAM_BACKEND_API_URL_ENV).map_err(|_| {
            InstallationLifecycleConfigError::MissingHttpSetting(
                crate::settings::IAM_BACKEND_API_URL_ENV,
            )
        })?;
        let auth_token = sdkwork_deploy_identity_provisioning_port::settings::resolve_auth_token()?;
        Self::new(base_url, auth_token, settings)
    }

    /// The configured base URL, exposed for the startup log.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The per-attempt budget, exposed for the startup log.
    #[must_use]
    pub fn timeout(&self) -> std::time::Duration {
        self.settings.timeout
    }

    fn endpoint(&self, deploy_application_id: &str) -> String {
        let base = sdkwork_utils_rust::service_base_url::registry_base_url_or(
            "sdkwork-iam",
            &self.base_url,
        );
        let anchor = deploy_application_id.trim();
        format!(
            "{}{}",
            base.trim().trim_end_matches('/'),
            IAM_DISABLE_BY_DEPLOY_APPLICATION_PATH.replace("{deployApplicationId}", anchor)
        )
    }
}

/// Builds the body IAM's handler reads: the bootstrap token and the tenant the
/// anchor belongs to. Nothing else — a machine call must not carry a user's
/// session or permission scope.
fn disable_request_body(auth_token: &str, tenant_id: &str) -> Value {
    let mut body = Map::new();
    body.insert("authToken".to_owned(), Value::String(auth_token.to_owned()));
    body.insert("tenantId".to_owned(), Value::String(tenant_id.to_owned()));
    Value::Object(body)
}

#[async_trait]
impl DeployInstallationLifecyclePort for HttpInstallationLifecycleAdapter {
    async fn disable_installation(
        &self,
        tenant_id: &str,
        deploy_application_id: &str,
    ) -> Result<DisabledInstallation, InstallationLifecycleError> {
        let body = disable_request_body(&self.auth_token, tenant_id);
        let endpoint = self.endpoint(deploy_application_id);
        let attempt = async {
            let response = self
                .client
                .post(endpoint.as_str())
                .json(&body)
                .send()
                .await
                .map_err(|_| {
                    InstallationLifecycleError::Rejected(
                        "IAM installation disable transport failed".to_owned(),
                    )
                })?;
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Ok::<_, InstallationLifecycleError>((status, text))
        };
        let (status, text) = match tokio::time::timeout(self.settings.timeout, attempt).await {
            Ok(result) => result?,
            Err(_) => {
                return Err(InstallationLifecycleError::TimedOut {
                    millis: u64::try_from(self.settings.timeout.as_millis()).unwrap_or(u64::MAX),
                })
            }
        };
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(InstallationLifecycleError::NotFound {
                deploy_application_id: deploy_application_id.to_owned(),
            });
        }
        if !status.is_success() {
            return Err(InstallationLifecycleError::Rejected(format!(
                "IAM disable answered {} ({})",
                status.as_u16(),
                problem_code(&text)
            )));
        }
        Ok(DisabledInstallation {
            installation_id: installation_id_from_body(&text),
            // The handler is idempotent and answers the stored state; the adapter
            // cannot distinguish "already disabled" from "just disabled" without a
            // second read, and the deploy flow treats both as success.
            already_disabled: text.contains("\"status\":\"disabled\"")
                || text.contains("\"status\": \"disabled\""),
        })
    }
}

/// Reads the numeric problem code out of an `application/problem+json` body.
fn problem_code(text: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return "unknown".to_owned();
    };
    match value.get("code") {
        Some(Value::String(code)) => code.clone(),
        Some(code) => code.to_string(),
        None => "unknown".to_owned(),
    }
}

/// Reads the installation id back out of the success envelope.
fn installation_id_from_body(text: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    value
        .get("data")
        .and_then(|data| data.get("item"))
        .and_then(|item| {
            item.get("installationId")
                .or_else(|| item.get("tenantApplicationId"))
                .or_else(|| item.get("id"))
        })
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> InstallationLifecycleSettings {
        InstallationLifecycleSettings {
            timeout: std::time::Duration::from_secs(5),
        }
    }

    fn adapter() -> HttpInstallationLifecycleAdapter {
        HttpInstallationLifecycleAdapter::new("http://127.0.0.1:3901", "token-value", settings())
            .expect("adapter")
    }

    #[test]
    fn the_endpoint_is_the_anchor_addressed_disable_route() {
        let endpoint = adapter().endpoint("1845123456789012345");
        assert!(endpoint.ends_with(
            "/backend/v3/api/iam/installations/by_deploy_application/1845123456789012345/disable"
        ));
        assert!(endpoint.starts_with("http"));
    }

    #[test]
    fn the_body_carries_only_the_bootstrap_token_and_the_tenant() {
        let body = disable_request_body("token-value", "100001");
        assert_eq!(body["authToken"], "token-value");
        assert_eq!(body["tenantId"], "100001");
        assert_eq!(body.as_object().map(Map::len), Some(2));
    }

    #[test]
    fn the_success_envelope_yields_the_installation_id() {
        let id = installation_id_from_body(
            r#"{"code":0,"data":{"item":{"installationId":"tapp_7"}},"traceId":"t"}"#,
        );
        assert_eq!(id.as_deref(), Some("tapp_7"));
    }

    #[test]
    fn a_problem_body_is_reduced_to_its_numeric_code() {
        assert_eq!(
            problem_code(r#"{"code":40301,"detail":"secret value"}"#),
            "40301"
        );
    }
}
