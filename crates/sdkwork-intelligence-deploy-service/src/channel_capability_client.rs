//! IAM channel-capability client.
//!
//! Deploy never calls WeChat directly: it asks IAM for one capability through
//! `capability_invocations`, and IAM resolves the authorizer token and performs
//! the provider call. This client is that hop.
//!
//! Credentials are an injected port, not a hard-coded scheme. The production
//! wiring must supply an approved service credential; until then
//! [`UnconfiguredChannelCapabilityBearerAuth`] fails closed instead of sending an
//! anonymous request.
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use serde_json::{json, Value};

const REQUEST_TIMEOUT_SECONDS: u64 = 20;
const ERROR_BODY_LIMIT: usize = 512;

/// Supplies the bearer credential for IAM capability calls.
#[async_trait]
pub trait ChannelCapabilityBearerAuth: Send + Sync {
    /// Returns a bearer token accepted by the IAM backend API.
    async fn bearer_token(&self) -> DeployServiceResult<String>;
}

/// Fail-closed auth used until an approved service credential is wired.
pub struct UnconfiguredChannelCapabilityBearerAuth;

#[async_trait]
impl ChannelCapabilityBearerAuth for UnconfiguredChannelCapabilityBearerAuth {
    async fn bearer_token(&self) -> DeployServiceResult<String> {
        Err(DeployServiceError::Conflict(
            "channel capability credential is not configured".to_string(),
        ))
    }
}

/// Auth backed by one bearer token supplied by the operator configuration.
pub struct StaticChannelCapabilityBearerAuth {
    token: String,
}

impl StaticChannelCapabilityBearerAuth {
    /// Builds auth from a bearer token.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

#[async_trait]
impl ChannelCapabilityBearerAuth for StaticChannelCapabilityBearerAuth {
    async fn bearer_token(&self) -> DeployServiceResult<String> {
        if self.token.trim().is_empty() {
            return Err(DeployServiceError::Conflict(
                "channel capability credential is empty".to_string(),
            ));
        }
        Ok(self.token.clone())
    }
}

/// The HTTP boundary, injectable so the client is testable without a network.
#[async_trait]
pub trait ChannelCapabilityTransport: Send + Sync {
    /// POSTs a JSON body with a bearer token and returns `(status, body)`.
    async fn post_json(
        &self,
        url: &str,
        bearer: &str,
        body: &Value,
    ) -> DeployServiceResult<(u16, String)>;
}

/// Production transport backed by `reqwest`.
pub struct ReqwestChannelCapabilityTransport {
    client: reqwest::Client,
}

impl ReqwestChannelCapabilityTransport {
    /// Builds a transport with a bounded timeout.
    pub fn new() -> DeployServiceResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .build()
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl ChannelCapabilityTransport for ReqwestChannelCapabilityTransport {
    async fn post_json(
        &self,
        url: &str,
        bearer: &str,
        body: &Value,
    ) -> DeployServiceResult<(u16, String)> {
        let response = self
            .client
            .post(url)
            .bearer_auth(bearer)
            .json(body)
            .send()
            .await
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        Ok((status, text))
    }
}

/// Calls one IAM capability.
pub struct IamChannelCapabilityClient {
    base_url: String,
    auth: Arc<dyn ChannelCapabilityBearerAuth>,
    transport: Arc<dyn ChannelCapabilityTransport>,
}

impl IamChannelCapabilityClient {
    /// Builds the client.
    pub fn new(
        base_url: impl Into<String>,
        auth: Arc<dyn ChannelCapabilityBearerAuth>,
        transport: Arc<dyn ChannelCapabilityTransport>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            auth,
            transport,
        }
    }

    /// Invokes one capability for an authorization and returns `data.item.result`.
    pub async fn invoke(
        &self,
        authorization_id: &str,
        capability: &str,
        payload: Value,
    ) -> DeployServiceResult<Value> {
        let authorization_id = authorization_id.trim();
        if authorization_id.is_empty() {
            return Err(DeployServiceError::validation(
                "authorizationId is required",
            ));
        }
        if capability.trim().is_empty() {
            return Err(DeployServiceError::validation("capability is required"));
        }
        let bearer = self.auth.bearer_token().await?;
        // identity:service deploy-publisher — the bearer comes from the declared
        // `ChannelCapabilityAuth` seam (a deployment service credential for
        // publishing a site), not from an end-user request. The publishing
        // command originates from a deploy job, so there is no caller pair.
        // APP_SDK_INTEGRATION_SPEC.md §5.2.2: the IAM backend is another SDKWork
        // surface; resolve it through the shared registry component with the
        // configured base URL as the degradation tier.
        let base = sdkwork_utils_rust::service_base_url::registry_base_url_or(
            "sdkwork-iam",
            &self.base_url,
        );
        let url = format!(
            "{base}/backend/v3/api/iam/oauth/resource_authorizations/{authorization_id}/capability_invocations"
        );
        let body = json!({
            "capability": capability,
            "payload": payload,
        });
        let (status, raw) = self.transport.post_json(&url, &bearer, &body).await?;
        if !(200..300).contains(&status) {
            return Err(self.map_failure(status, &raw));
        }
        let value: Value = serde_json::from_str(&raw).map_err(|_| {
            DeployServiceError::Internal("capability response is not valid json".to_string())
        })?;
        value
            .get("data")
            .and_then(|data| data.get("item"))
            .and_then(|item| item.get("result"))
            .cloned()
            .ok_or_else(|| {
                DeployServiceError::Internal(
                    "capability response is missing data.item.result".to_string(),
                )
            })
    }

    fn map_failure(&self, status: u16, raw: &str) -> DeployServiceError {
        if status == 401 || status == 403 {
            return DeployServiceError::forbidden(
                "IAM rejected the channel capability credential".to_string(),
            );
        }
        if status == 404 {
            return DeployServiceError::not_found("resource authorization not found or not active");
        }
        if status == 409 {
            return DeployServiceError::conflict("authorization is not active for this capability");
        }
        if status == 422 {
            return DeployServiceError::validation("capability request was rejected");
        }
        let detail = failure_code(raw).unwrap_or_else(|| {
            raw.chars()
                .take(ERROR_BODY_LIMIT)
                .collect::<String>()
                .trim()
                .to_string()
        });
        DeployServiceError::Internal(format!(
            "channel capability call failed ({status}): {detail}"
        ))
    }
}

/// Extracts the numeric problem code from a ProblemDetail body when present.
fn failure_code(raw: &str) -> Option<String> {
    let value: Value = serde_json::from_str(raw).ok()?;
    value
        .get("code")
        .and_then(Value::as_i64)
        .map(|code| format!("problem code {code}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct FakeAuth {
        token: Result<String, String>,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ChannelCapabilityBearerAuth for FakeAuth {
        async fn bearer_token(&self) -> DeployServiceResult<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.token.clone().map_err(DeployServiceError::Conflict)
        }
    }

    struct FakeTransport {
        response: Result<(u16, String), String>,
        last: Mutex<Option<(String, String, Value)>>,
    }

    impl FakeTransport {
        fn new(status: u16, body: &str) -> Self {
            Self {
                response: Ok((status, body.to_string())),
                last: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl ChannelCapabilityTransport for FakeTransport {
        async fn post_json(
            &self,
            url: &str,
            bearer: &str,
            body: &Value,
        ) -> DeployServiceResult<(u16, String)> {
            *self.last.lock().expect("last") =
                Some((url.to_string(), bearer.to_string(), body.clone()));
            self.response.clone().map_err(DeployServiceError::Internal)
        }
    }

    fn client(
        token: Result<String, String>,
        status: u16,
        body: &str,
    ) -> (
        IamChannelCapabilityClient,
        Arc<FakeTransport>,
        Arc<FakeAuth>,
    ) {
        let auth = Arc::new(FakeAuth {
            token,
            calls: AtomicUsize::new(0),
        });
        let transport = Arc::new(FakeTransport::new(status, body));
        (
            IamChannelCapabilityClient::new("https://iam.example", auth.clone(), transport.clone()),
            transport,
            auth,
        )
    }

    #[tokio::test]
    async fn invokes_the_capability_and_unwraps_item_result() {
        let (client, transport, _auth) = client(
            Ok("bearer-1".to_string()),
            200,
            r#"{"code":0,"data":{"item":{"capability":"mini_program.release","result":{"released":true}}}}"#,
        );
        let result = client
            .invoke("auth-1", "mini_program.release", json!({}))
            .await
            .expect("invoke");
        assert_eq!(result["released"], true);
        let last = transport.last.lock().expect("last").clone().expect("call");
        assert!(last
            .0
            .ends_with("/resource_authorizations/auth-1/capability_invocations"));
        assert_eq!(last.1, "bearer-1");
        assert_eq!(last.2["capability"], "mini_program.release");
    }

    #[tokio::test]
    async fn missing_auth_fails_closed_without_a_request() {
        let (client, transport, _auth) =
            client(Err("not configured".to_string()), 200, r#"{"code":0}"#);
        let error = client
            .invoke("auth-1", "mini_program.release", json!({}))
            .await
            .expect_err("must fail");
        assert!(matches!(error, DeployServiceError::Conflict(_)));
        assert!(transport.last.lock().expect("last").is_none());
    }

    #[tokio::test]
    async fn rejected_credentials_do_not_leak_the_bearer() {
        let (client, _transport, _auth) = client(
            Ok("secret-bearer".to_string()),
            401,
            r#"{"code":40101,"title":"unauthorized"}"#,
        );
        let error = client
            .invoke("auth-1", "mini_program.release", json!({}))
            .await
            .expect_err("must fail");
        let message = format!("{error}");
        assert!(matches!(error, DeployServiceError::Forbidden(_)));
        assert!(!message.contains("secret-bearer"));
    }

    #[tokio::test]
    async fn provider_failure_maps_to_internal_without_body_leak() {
        let (client, _transport, _auth) = client(
            Ok("bearer-1".to_string()),
            502,
            r#"{"code":50201,"title":"provider unavailable"}"#,
        );
        let error = client
            .invoke("auth-1", "mini_program.release", json!({}))
            .await
            .expect_err("must fail");
        let message = format!("{error}");
        assert!(matches!(error, DeployServiceError::Internal(_)));
        assert!(message.contains("problem code 50201"));
    }

    #[tokio::test]
    async fn missing_result_is_an_internal_error() {
        let (client, _transport, _auth) = client(
            Ok("bearer-1".to_string()),
            200,
            r#"{"code":0,"data":{"item":{"capability":"x"}}}"#,
        );
        assert!(client.invoke("auth-1", "x", json!({})).await.is_err());
    }

    #[tokio::test]
    async fn blank_authorization_is_rejected_before_auth() {
        let (client, _transport, auth) = client(Ok("bearer-1".to_string()), 200, r#"{"code":0}"#);
        assert!(client.invoke("  ", "x", json!({})).await.is_err());
        assert_eq!(auth.calls.load(Ordering::SeqCst), 0);
    }
}
