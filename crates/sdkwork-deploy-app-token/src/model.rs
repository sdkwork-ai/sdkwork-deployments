//! Deployment-owned models of the exchange.
//!
//! These types carry identifiers and non-secret metadata. Credential values live
//! in [`AppAccessToken`] with a redacting `Debug`, so a stray `{:?}` can never
//! publish a live token into a log.

use std::fmt;

use serde::{Deserialize, Serialize};

/// How a caller names the application.
///
/// The deploy module speaks three identifiers (`deploy_app.uuid` for addressing,
/// `deploy_app.id` as the token anchor, `slug` for humans), so the library accepts
/// all three and resolves them against deploy-owned columns only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeployApplicationSelector {
    /// Public application slug (`deploy_app.slug`), for example
    /// `sdkwork-cloudrouter-pc`.
    Slug(String),
    /// Addressable application uuid (`deploy_app.uuid`).
    Uuid(String),
    /// Deployment anchor (`deploy_app.id`) written as a decimal string.
    DeployApplicationId(String),
}

impl DeployApplicationSelector {
    /// @param value - application slug.
    /// @returns the selector.
    #[must_use]
    pub fn slug(value: impl Into<String>) -> Self {
        Self::Slug(value.into())
    }

    /// @param value - application uuid.
    /// @returns the selector.
    #[must_use]
    pub fn uuid(value: impl Into<String>) -> Self {
        Self::Uuid(value.into())
    }

    /// @param value - decimal deployment anchor.
    /// @returns the selector.
    #[must_use]
    pub fn deploy_application_id(value: impl Into<String>) -> Self {
        Self::DeployApplicationId(value.into())
    }

    /// The raw value, independent of which column it addresses.
    /// @returns the trimmed selector value.
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            Self::Slug(value) | Self::Uuid(value) | Self::DeployApplicationId(value) => value,
        }
    }
}

impl fmt::Display for DeployApplicationSelector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Slug(value) => write!(formatter, "slug:{value}"),
            Self::Uuid(value) => write!(formatter, "uuid:{value}"),
            Self::DeployApplicationId(value) => write!(formatter, "deploy-app:{value}"),
        }
    }
}

/// One resolved deploy application.
///
/// `owner_type` and `owner_user_id` are deploy's own launch-policy facts. They are
/// **informational** here: the authority for a release scope is the installation
/// (`iam_application_installation.ownership_scope`), so IAM decides entitlement from
/// its own row and the library never pre-empts that decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeployApplicationRef {
    /// Addressable uuid.
    pub uuid: String,
    /// Deployment anchor (`deploy_app.id`), the identity IAM signs into the token.
    pub deploy_application_id: String,
    /// Owning tenant.
    pub tenant_id: String,
    /// Public slug.
    pub slug: String,
    /// Application kind (`SPA_WEB`, `DESKTOP_APP`, ...).
    pub app_kind: String,
    /// Deploy launch policy owner type (`PLATFORM`, `TENANT`, `ORGANIZATION`, `USER`).
    pub owner_type: String,
    /// Owner user id for a `USER` application.
    pub owner_user_id: Option<String>,
}

impl DeployApplicationRef {
    /// The anchor to send to IAM.
    /// @returns the decimal deploy application id.
    #[must_use]
    pub fn anchor(&self) -> &str {
        &self.deploy_application_id
    }
}

/// The caller's already-authenticated session.
///
/// The library never mints, refreshes, or persists a session: the caller's
/// application owns its session lifecycle and hands the current pair in.
#[derive(Clone, PartialEq, Eq)]
pub struct CallerSession {
    access_token: String,
    auth_token: String,
}

impl CallerSession {
    /// @param access_token - the session `Access-Token`.
    /// @param auth_token - the session `Authorization: Bearer` value.
    /// @returns the session bag.
    #[must_use]
    pub fn new(access_token: impl Into<String>, auth_token: impl Into<String>) -> Self {
        Self {
            access_token: access_token.into(),
            auth_token: auth_token.into(),
        }
    }

    /// The `Access-Token` header value.
    /// @returns the access token.
    #[must_use]
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    /// The `Authorization: Bearer` value.
    /// @returns the auth token.
    #[must_use]
    pub fn auth_token(&self) -> &str {
        &self.auth_token
    }

    /// Whether both halves are present.
    /// @returns true when the session can be sent.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self.access_token.trim().is_empty() && !self.auth_token.trim().is_empty()
    }
}

impl fmt::Debug for CallerSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CallerSession")
            .field("access_token", &"<redacted>")
            .field("auth_token", &"<redacted>")
            .finish()
    }
}

/// One exchange request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppTokenRequest {
    /// How the caller named the application.
    pub selector: DeployApplicationSelector,
}

impl AppTokenRequest {
    /// @param selector - how the caller named the application.
    /// @returns the request.
    #[must_use]
    pub fn new(selector: DeployApplicationSelector) -> Self {
        Self { selector }
    }
}

/// The issued credential.
///
/// Mirrors `IamAppAccessToken` from the IAM app-api contract.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppAccessToken {
    /// Signed access token, send as `Access-Token`.
    #[serde(rename = "accessToken")]
    pub access_token: String,
    /// Signed auth token, send as `Authorization: Bearer`.
    #[serde(rename = "authToken")]
    pub auth_token: String,
    /// Expiry instant (RFC 3339).
    #[serde(rename = "expiresAt")]
    pub expires_at: String,
    /// Deploy application anchor carried by the token.
    #[serde(rename = "appId")]
    pub app_id: String,
    /// Installation the credential was issued for.
    #[serde(rename = "tenantApplicationId")]
    pub tenant_application_id: String,
    /// Organization claim of the installation.
    #[serde(rename = "organizationId", default)]
    pub organization_id: String,
    /// Release scope of the installation.
    #[serde(rename = "ownershipScope")]
    pub ownership_scope: String,
    /// Revocable session id.
    #[serde(rename = "sessionId", default)]
    pub session_id: String,
}

impl fmt::Debug for AppAccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppAccessToken")
            .field("access_token", &"<redacted>")
            .field("auth_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("app_id", &self.app_id)
            .field("tenant_application_id", &self.tenant_application_id)
            .field("organization_id", &self.organization_id)
            .field("ownership_scope", &self.ownership_scope)
            .field("session_id", &self.session_id)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_display_names_the_identity_plane() {
        assert_eq!(
            DeployApplicationSelector::slug("app").to_string(),
            "slug:app"
        );
        assert_eq!(
            DeployApplicationSelector::deploy_application_id("381764687714324480").value(),
            "381764687714324480"
        );
    }

    #[test]
    fn session_and_token_debug_never_leak_credentials() {
        let session = CallerSession::new("secret-access", "secret-auth");
        let rendered = format!("{session:?}");
        assert!(!rendered.contains("secret-access"), "{rendered}");
        assert!(!rendered.contains("secret-auth"), "{rendered}");

        let token = AppAccessToken {
            access_token: "secret-access".to_owned(),
            auth_token: "secret-auth".to_owned(),
            expires_at: "2026-11-19T12:00:00Z".to_owned(),
            app_id: "381".to_owned(),
            tenant_application_id: "tapp_1".to_owned(),
            organization_id: "0".to_owned(),
            ownership_scope: "TENANT".to_owned(),
            session_id: "session".to_owned(),
        };
        let rendered = format!("{token:?}");
        assert!(!rendered.contains("secret-access"), "{rendered}");
        assert!(!rendered.contains("secret-auth"), "{rendered}");
        assert!(rendered.contains("381"), "{rendered}");
    }

    #[test]
    fn session_completeness_requires_both_halves() {
        assert!(CallerSession::new("a", "b").is_complete());
        assert!(!CallerSession::new("a", " ").is_complete());
    }
}
