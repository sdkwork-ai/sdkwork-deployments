//! Exchange the caller's session for an application access credential at IAM.
//!
//! The deploy side never signs: it posts the deployment anchor to
//! `appAccessTokens.create` and receives a signed, revocable, one-hour credential.

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

use crate::error::AppTokenError;
use crate::model::{AppAccessToken, CallerSession};
use crate::IAM_APP_ACCESS_TOKENS_PATH;

/// Exchanges a session for one application's signed credential.
#[async_trait]
pub trait AppTokenExchange: Send + Sync {
    /// Requests the credential of `deploy_application_id`.
    ///
    /// @param session - the caller's authenticated session.
    /// @param deploy_application_id - deployment anchor to request.
    /// @param environment - installation lifecycle to initialize, or `None` to let
    ///   IAM resolve it from the session and the installation's registered rows.
    /// @returns the issued credential.
    /// @throws [`AppTokenError`] typed by IAM's status class.
    async fn exchange(
        &self,
        session: &CallerSession,
        deploy_application_id: &str,
        environment: Option<&str>,
    ) -> Result<AppAccessToken, AppTokenError>;
}

/// HTTP implementation against the IAM app-api.
#[derive(Clone, Debug)]
pub struct HttpAppTokenExchange {
    client: reqwest::Client,
    base_url: String,
}

impl HttpAppTokenExchange {
    /// Builds the client for one platform origin.
    ///
    /// @param base_url - IAM app-api origin, for example `https://api.example.com`.
    /// @returns the exchange client.
    /// @throws [`AppTokenError::Configuration`] when the URL is not an http(s)
    ///   origin.
    pub fn try_new(base_url: impl AsRef<str>) -> Result<Self, AppTokenError> {
        let trimmed = base_url.as_ref().trim().trim_end_matches('/');
        let parsed = reqwest::Url::parse(trimmed)
            .map_err(|error| AppTokenError::Configuration(error.to_string()))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(AppTokenError::Configuration(
                "base URL must be an http(s) origin".to_owned(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|error| AppTokenError::Configuration(error.to_string()))?;
        Ok(Self {
            client,
            base_url: trimmed.to_owned(),
        })
    }

    /// The route this client calls.
    /// @returns the absolute URL.
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("{}{IAM_APP_ACCESS_TOKENS_PATH}", self.base_url)
    }
}

#[async_trait]
impl AppTokenExchange for HttpAppTokenExchange {
    async fn exchange(
        &self,
        session: &CallerSession,
        deploy_application_id: &str,
        environment: Option<&str>,
    ) -> Result<AppAccessToken, AppTokenError> {
        if !session.is_complete() {
            return Err(AppTokenError::Unauthorized);
        }
        let access_token = HeaderValue::from_str(session.access_token())
            .map_err(|_| AppTokenError::Unauthorized)?;
        let auth_token = HeaderValue::from_str(&format!("Bearer {}", session.auth_token()))
            .map_err(|_| AppTokenError::Unauthorized)?;
        let mut headers = HeaderMap::new();
        headers.insert("Access-Token", access_token);
        headers.insert(AUTHORIZATION, auth_token);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let response = self
            .client
            .post(self.endpoint())
            .headers(headers)
            .json(&exchange_request_body(deploy_application_id, environment))
            .send()
            .await
            .map_err(|error| AppTokenError::Transport(error.to_string()))?;
        let status = response.status().as_u16();
        let body: Value = response
            .json()
            .await
            .map_err(|error| AppTokenError::Protocol(error.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(AppTokenError::from_iam_status(status, problem_detail(&body)));
        }
        parse_envelope(&body)
    }
}

/// Builds the `appAccessTokens.create` body.
///
/// `environment` is omitted when the caller did not choose one, so IAM resolves it
/// from the session and the installation's registered rows.
/// @param deploy_application_id - deployment anchor.
/// @param environment - installation lifecycle, when chosen.
/// @returns the JSON body.
#[must_use]
pub fn exchange_request_body(
    deploy_application_id: &str,
    environment: Option<&str>,
) -> Value {
    let mut body = json!({ "appId": deploy_application_id });
    if let Some(environment) = environment.map(str::trim).filter(|value| !value.is_empty()) {
        body["environment"] = Value::String(environment.to_owned());
    }
    body
}

/// Reads `detail` from a `ProblemDetail`, falling back to a compact rendering.
fn problem_detail(body: &Value) -> String {
    body.get("detail")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| body.to_string())
}

/// Extracts `data.item` from the SDKWork success envelope.
///
/// Exposed for tests so the contract can be asserted without a socket.
/// @param body - decoded response body.
/// @returns the typed credential.
/// @throws [`AppTokenError::Protocol`] when the envelope is not the documented one.
pub fn parse_envelope(body: &Value) -> Result<AppAccessToken, AppTokenError> {
    let item = body
        .get("data")
        .and_then(|data| data.get("item"))
        .ok_or_else(|| {
            AppTokenError::Protocol("response has no data.item credential".to_owned())
        })?;
    serde_json::from_value(item.clone())
        .map_err(|error| AppTokenError::Protocol(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_origins() {
        assert!(matches!(
            HttpAppTokenExchange::try_new("ftp://example.com"),
            Err(AppTokenError::Configuration(_))
        ));
        assert!(HttpAppTokenExchange::try_new("https://api.example.com/").is_ok());
    }

    #[test]
    fn endpoint_has_no_double_slash() {
        let exchange = HttpAppTokenExchange::try_new("https://api.example.com/")
            .expect("valid origin");
        assert_eq!(
            exchange.endpoint(),
            "https://api.example.com/app/v3/api/iam/app_access_tokens"
        );
    }

    #[test]
    fn envelope_requires_data_item() {
        assert!(matches!(
            parse_envelope(&json!({ "code": 0, "data": {} })),
            Err(AppTokenError::Protocol(_))
        ));
    }

    #[test]
    fn request_body_carries_the_anchor_and_an_optional_environment() {
        assert_eq!(
            exchange_request_body("381", None),
            json!({ "appId": "381" })
        );
        assert_eq!(
            exchange_request_body("381", Some(" production ")),
            json!({ "appId": "381", "environment": "production" })
        );
        assert_eq!(
            exchange_request_body("381", Some("  ")),
            json!({ "appId": "381" }),
            "a blank environment is omitted, not sent empty"
        );
    }

    #[test]
    fn envelope_parses_the_credential() {
        let token = parse_envelope(&json!({
            "code": 0,
            "data": {
                "item": {
                    "accessToken": "a",
                    "authToken": "b",
                    "expiresAt": "2026-11-19T12:00:00Z",
                    "appId": "381764687714324480",
                    "tenantApplicationId": "tapp_100001_0_381764687714324480",
                    "organizationId": "0",
                    "ownershipScope": "TENANT",
                    "sessionId": "session-1"
                }
            }
        }))
        .expect("documented envelope");
        assert_eq!(token.app_id, "381764687714324480");
        assert_eq!(token.ownership_scope, "TENANT");
    }

    #[test]
    fn empty_session_is_refused_before_any_request() {
        let exchange = HttpAppTokenExchange::try_new("https://api.example.com")
            .expect("valid origin");
        let session = CallerSession::new("", "b");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let outcome = runtime.block_on(exchange.exchange(&session, "381", None));
        assert!(matches!(outcome, Err(AppTokenError::Unauthorized)));
    }
}
