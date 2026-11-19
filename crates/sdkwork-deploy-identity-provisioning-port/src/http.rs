//! Opt-in adapter that provisions through IAM's backend HTTP route.
//!
//! identity:service - this is a background machine surface. It authenticates the
//! IAM provisioning route with the deployment's dedicated backend provisioning
//! token (read from a secret file and carried in the provisioning request body);
//! there is no end-user request whose dual tokens could be borrowed, and the
//! tenant/application scope is the deploy application being provisioned.
//!
//! Route authority (read from sdkwork-iam, not guessed):
//!
//! - `crates/sdkwork-routes-iam-backend-api/src/handlers.rs:186-189` mounts
//!   `POST /backend/v3/api/iam/tenant_applications` on
//!   `provision_tenant_application_handler` (handler at `handlers.rs:476`).
//! - `crates/sdkwork-routes-iam-backend-api/src/manifest.rs:607-612` declares the
//!   route as `HttpMethod::Post` with permission token `tenantApplications.create`;
//!   `sdks/sdkwork-iam-backend-sdk/openapi/sdkwork-iam-backend-api.openapi.yaml`
//!   publishes it as operation `tenantApplications.create` with permission
//!   `iam.tenant_applications.provision` and `x-sdkwork-auth-mode:
//!   bootstrap-body`.
//! - The request body parser
//!   (`crates/sdkwork-iam-web-adapter/src/application_registry.rs:310-369`)
//!   accepts `deployApplicationId` / `deploy_application_id` and
//!   `provisioningIdempotencyKey` / `provisioning_idempotency_key`.
//!
//! A backend module reaching a sibling module over HTTP is the exception, not the
//! rule (`RUST_CODE_SPEC.md` §17): this adapter exists for a genuinely split
//! deployment, is selected only by `SDKWORK_DEPLOY_IAM_PROVISIONING_MODE=http`,
//! and is recorded in `specs/backend-module-call-boundary.allow.json`. Its
//! default counterpart is the in-process adapter.

use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::settings::{
    resolve_auth_token, IdentityProvisioningConfigError, IdentityProvisioningSettings,
    IAM_BACKEND_API_URL_ENV, IAM_BACKEND_AUTH_TOKEN_FILE_ENV,
};
use crate::{
    normalize_application_type, normalize_ownership_scope, DeployApplicationProvisioningCommand,
    DeployIdentityProvisioningPort, IdentityProvisioningError, ProvisioningOutcome,
    TenantApplicationReceipt,
};

/// IAM backend route that provisions one tenant application.
pub const IAM_TENANT_APPLICATIONS_PATH: &str = "/backend/v3/api/iam/tenant_applications";

/// Provisions tenant applications over IAM's backend HTTP surface.
#[derive(Clone, Debug)]
pub struct HttpIdentityProvisioningAdapter {
    base_url: String,
    auth_token: String,
    settings: IdentityProvisioningSettings,
    client: reqwest::Client,
}

impl HttpIdentityProvisioningAdapter {
    /// Builds the adapter from an explicit endpoint, credential, and settings.
    ///
    /// The per-attempt budget is installed on the client as well as applied
    /// around the call, so neither a connect stall nor a slow response body can
    /// outlive it.
    pub fn new(
        base_url: impl Into<String>,
        auth_token: impl Into<String>,
        settings: IdentityProvisioningSettings,
    ) -> Result<Self, IdentityProvisioningConfigError> {
        let base_url = base_url.into().trim().trim_end_matches('/').to_owned();
        if base_url.is_empty() {
            return Err(IdentityProvisioningConfigError::MissingHttpSetting(
                IAM_BACKEND_API_URL_ENV,
            ));
        }
        let auth_token = auth_token.into().trim().to_owned();
        if auth_token.is_empty() {
            return Err(IdentityProvisioningConfigError::MissingHttpSetting(
                IAM_BACKEND_AUTH_TOKEN_FILE_ENV,
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(settings.timeout)
            .connect_timeout(settings.timeout)
            .build()
            .map_err(IdentityProvisioningConfigError::HttpClientUnavailable)?;
        Ok(Self {
            base_url,
            auth_token,
            settings,
            client,
        })
    }

    /// Builds the adapter from the environment.
    ///
    /// Every setting is required in HTTP mode: this adapter is only ever built
    /// because an operator asked for a split deployment, so a deployment that
    /// asked and is missing a setting fails at startup instead of at the first
    /// application create.
    pub fn from_env() -> Result<Self, IdentityProvisioningConfigError> {
        let settings = IdentityProvisioningSettings::from_env()?;
        let base_url = std::env::var(IAM_BACKEND_API_URL_ENV).map_err(|_| {
            IdentityProvisioningConfigError::MissingHttpSetting(IAM_BACKEND_API_URL_ENV)
        })?;
        let auth_token = resolve_auth_token()?;
        Self::new(base_url, auth_token, settings)
    }

    /// Absolute URL of the provisioning route, resolved through service
    /// discovery first and the configured URL as the degradation
    /// (`DISCOVERY_SPEC` §12.1).
    fn endpoint(&self) -> String {
        let base = sdkwork_utils_rust::service_base_url::registry_base_url_or(
            "sdkwork-iam",
            &self.base_url,
        );
        format!(
            "{}{}",
            base.trim().trim_end_matches('/'),
            IAM_TENANT_APPLICATIONS_PATH
        )
    }
}

#[async_trait]
impl DeployIdentityProvisioningPort for HttpIdentityProvisioningAdapter {
    async fn provision_tenant_application(
        &self,
        command: &DeployApplicationProvisioningCommand,
    ) -> Result<ProvisioningOutcome, IdentityProvisioningError> {
        let body = provision_request_body(&self.auth_token, command);
        let endpoint = self.endpoint();
        let attempt = async {
            let response = self
                .client
                .post(endpoint.as_str())
                .json(&body)
                .send()
                .await
                .map_err(IdentityProvisioningError::Transport)?;
            let status = response.status();
            // The response is IAM's own problem/response envelope, capped by the
            // route's own body handling; reading it is what lets a 4xx be
            // classified instead of reported as a bare status code.
            let text = response
                .text()
                .await
                .map_err(IdentityProvisioningError::Transport)?;
            Ok::<_, IdentityProvisioningError>((status, text))
        };
        let (status, text) = match tokio::time::timeout(self.settings.timeout, attempt).await {
            Ok(result) => result?,
            Err(_) => {
                return Err(IdentityProvisioningError::TimedOut {
                    millis: u64::try_from(self.settings.timeout.as_millis()).unwrap_or(u64::MAX),
                })
            }
        };
        if !status.is_success() {
            return Err(IdentityProvisioningError::Rejected {
                status: status.as_u16(),
                code: problem_code(&text),
            });
        }
        Ok(ProvisioningOutcome::Installed(receipt_from_body(&text)))
    }
}

/// Builds the provisioning body IAM's parser accepts (`application_registry.rs`
/// accepts the camelCase spelling of every field).
///
/// There is no `templateId`/`appKey`: IAM's application templates are retired,
/// so the installation is derived from this deploy application.
fn provision_request_body(
    auth_token: &str,
    command: &DeployApplicationProvisioningCommand,
) -> Value {
    let mut body = Map::new();
    body.insert("authToken".to_owned(), Value::String(auth_token.to_owned()));
    body.insert(
        "tenantId".to_owned(),
        Value::String(command.tenant_id.clone()),
    );
    body.insert(
        "organizationId".to_owned(),
        Value::String(command.organization_id.clone()),
    );
    body.insert(
        "instanceKey".to_owned(),
        Value::String(command.instance_key.clone()),
    );
    body.insert(
        "displayName".to_owned(),
        Value::String(command.display_name.clone()),
    );
    body.insert(
        "environment".to_owned(),
        Value::String(command.environment.clone()),
    );
    body.insert(
        "deployApplicationId".to_owned(),
        Value::String(command.deploy_application_id.clone()),
    );
    body.insert(
        "provisioningIdempotencyKey".to_owned(),
        Value::String(command.provisioning_idempotency_key.clone()),
    );
    // IAM requires a type; normalise here so the wire never carries a value IAM
    // would have to reinterpret as `other`.
    body.insert(
        "applicationType".to_owned(),
        Value::String(normalize_application_type(command.application_type.as_str()).to_owned()),
    );
    // The installation's ownership is the application's ownership, normalised
    // onto IAM's installation scopes (`PLATFORM` becomes `TENANT`: an
    // installation has no platform-wide scope).
    body.insert(
        "ownershipScope".to_owned(),
        Value::String(normalize_ownership_scope(command.ownership_scope.as_str()).to_owned()),
    );
    if let Some(owner_user_id) = command.owner_user_id.as_deref() {
        body.insert(
            "ownerUserId".to_owned(),
            Value::String(owner_user_id.to_owned()),
        );
    }
    // Deliberately empty, not an omission: this installation grants no API scope
    // of its own. IAM now accepts the empty list.
    body.insert("accessPermissions".to_owned(), Value::Array(Vec::new()));
    Value::Object(body)
}

/// Reads the numeric problem code out of an `application/problem+json` body.
///
/// Only the code is kept: the human-readable `detail` of an IAM failure can
/// quote stored values, and this module's log line is not the place for them.
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

/// Reads the installation identity back out of IAM's success envelope
/// (`{ "code": 0, "data": { "item": { "tenantApplicationId", "appId" } } }`).
///
/// A missing or unexpected envelope is not a failure: the call itself succeeded,
/// and losing the echo must not mark an installed application `failed`.
fn receipt_from_body(text: &str) -> TenantApplicationReceipt {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return TenantApplicationReceipt::default();
    };
    let item = value.get("data").and_then(|data| data.get("item"));
    let read = |key: &str| {
        item.and_then(|item| item.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    TenantApplicationReceipt {
        tenant_application_id: read("tenantApplicationId").or_else(|| read("id")),
        runtime_app_id: read("appId"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> IdentityProvisioningSettings {
        IdentityProvisioningSettings {
            timeout: crate::settings::DEFAULT_PROVISIONING_TIMEOUT,
        }
    }

    fn adapter() -> HttpIdentityProvisioningAdapter {
        HttpIdentityProvisioningAdapter::new("http://127.0.0.1:3901", "token-value", settings())
            .expect("adapter")
    }

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
    fn endpoint_targets_the_iam_backend_tenant_applications_route() {
        let endpoint = adapter().endpoint();
        assert!(endpoint.ends_with("/backend/v3/api/iam/tenant_applications"));
        assert!(endpoint.starts_with("http"));
    }

    #[test]
    fn request_body_carries_the_deploy_link_and_the_replay_key() {
        let body = provision_request_body("token-value", &command());
        assert_eq!(body["deployApplicationId"], "1845123456789012345");
        assert_eq!(
            body["provisioningIdempotencyKey"],
            "deploy:1845123456789012345"
        );
        assert_eq!(body["tenantId"], "7");
        assert_eq!(body["organizationId"], "0");
        assert_eq!(body["instanceKey"], command().instance_key);
        assert_eq!(body["displayName"], "Shop");
        assert_eq!(body["environment"], "production");
        assert_eq!(body["authToken"], "token-value");
    }

    #[test]
    fn request_body_has_no_retired_template_fields_and_no_api_scope() {
        let body = provision_request_body("token-value", &command());
        assert!(body.get("templateId").is_none());
        assert!(body.get("appKey").is_none());
        // Empty on purpose: the installation grants no API scope of its own.
        assert_eq!(body["accessPermissions"], serde_json::json!([]));
    }

    #[test]
    fn request_body_normalises_the_application_type() {
        let mut unknown = command();
        unknown.application_type = "NOT_A_TYPE".to_owned();
        let body = provision_request_body("token-value", &unknown);
        assert_eq!(body["applicationType"], "other");

        let mut mini = command();
        mini.application_type = "MINIPROGRAM".to_owned();
        assert_eq!(
            provision_request_body("token-value", &mini)["applicationType"],
            "miniprogram"
        );

        let mut api = command();
        api.application_type = " API ".to_owned();
        assert_eq!(
            provision_request_body("token-value", &api)["applicationType"],
            "api"
        );
    }

    #[test]
    fn request_body_carries_the_ownership_scope_and_only_a_user_owner() {
        let tenant = provision_request_body("token-value", &command());
        assert_eq!(tenant["ownershipScope"], "TENANT");
        assert!(tenant.get("ownerUserId").is_none());

        let mut personal = command();
        personal.ownership_scope = "USER".to_owned();
        personal.owner_user_id = Some("11001".to_owned());
        let personal_body = provision_request_body("token-value", &personal);
        assert_eq!(personal_body["ownershipScope"], "USER");
        assert_eq!(personal_body["ownerUserId"], "11001");

        let mut platform = command();
        platform.ownership_scope = "PLATFORM".to_owned();
        assert_eq!(
            provision_request_body("token-value", &platform)["ownershipScope"],
            "TENANT"
        );
    }

    #[test]
    fn blank_settings_are_refused_instead_of_building_an_unusable_client() {
        let error = HttpIdentityProvisioningAdapter::new("   ", "token-value", settings())
            .expect_err("blank endpoint");
        assert!(matches!(
            error,
            IdentityProvisioningConfigError::MissingHttpSetting(_)
        ));
    }

    #[test]
    fn success_envelope_yields_the_installation_identity() {
        let receipt = receipt_from_body(
            r#"{"code":0,"data":{"item":{"tenantApplicationId":"tapp_7_0_platform","appId":"app_7"}},"traceId":"t"}"#,
        );
        assert_eq!(
            receipt.tenant_application_id.as_deref(),
            Some("tapp_7_0_platform")
        );
        assert_eq!(receipt.runtime_app_id.as_deref(), Some("app_7"));
    }

    #[test]
    fn unreadable_success_body_keeps_the_installation_without_an_echo() {
        assert_eq!(
            receipt_from_body("not json"),
            TenantApplicationReceipt::default()
        );
    }

    #[test]
    fn problem_body_is_reduced_to_its_numeric_code() {
        assert_eq!(
            problem_code(r#"{"code":40401,"detail":"secret value"}"#),
            "40401"
        );
        assert_eq!(
            problem_code(r#"{"code":"iam_tenant_application_provision_failed"}"#),
            "iam_tenant_application_provision_failed"
        );
        assert_eq!(problem_code("<html>"), "unknown");
    }
}
