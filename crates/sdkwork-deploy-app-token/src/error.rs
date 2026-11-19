//! Typed failures of the deploy-side token exchange.

use thiserror::Error;

/// Failure of one step of the exchange.
///
/// Variants mirror the IAM app-api status classes, so a caller can turn them into
/// an HTTP status without parsing a string.
#[derive(Debug, Error)]
pub enum AppTokenError {
    /// The selector names no deploy application of the caller's tenant.
    #[error("deploy application {selector} was not found in tenant {tenant_id}")]
    ApplicationNotFound {
        /// Tenant the lookup was scoped to.
        tenant_id: String,
        /// Selector as supplied by the caller.
        selector: String,
    },

    /// The caller's session is missing or expired.
    #[error("application access token exchange requires an authenticated session")]
    Unauthorized,

    /// IAM refused the caller for this application.
    #[error("caller is not entitled to this application: {detail}")]
    Forbidden {
        /// IAM problem detail.
        detail: String,
    },

    /// The installation cannot issue right now (disabled, environment not
    /// registered, personal installation without its owner).
    #[error("application cannot issue a credential: {detail}")]
    Conflict {
        /// IAM problem detail.
        detail: String,
    },

    /// Transport failure: no response at all.
    #[error("application access token transport failed: {0}")]
    Transport(String),

    /// A response arrived but the contract was not honoured.
    #[error("application access token response was not understood: {0}")]
    Protocol(String),

    /// The deploy inventory could not be read.
    #[error("deploy application lookup failed: {0}")]
    Storage(String),

    /// The client is misconfigured (base URL).
    #[error("invalid IAM base URL: {0}")]
    Configuration(String),
}

impl AppTokenError {
    /// Builds the not-found error for one selector.
    /// @param tenant_id - tenant the lookup was scoped to.
    /// @param selector - selector as supplied by the caller.
    /// @returns the typed error.
    #[must_use]
    pub fn application_not_found(
        tenant_id: impl Into<String>,
        selector: impl Into<String>,
    ) -> Self {
        Self::ApplicationNotFound {
            tenant_id: tenant_id.into(),
            selector: selector.into(),
        }
    }

    /// Maps an IAM app-api status and problem detail onto this enum.
    ///
    /// The mapping follows `appAccessTokens.create`: 401 needs a session, 403 is
    /// an entitlement refusal, 404/409 mean the installation is absent or not
    /// issuable, and everything else is a protocol/server failure.
    /// @param status - HTTP status of the IAM response.
    /// @param detail - problem detail text, when present.
    /// @returns the typed error.
    #[must_use]
    pub fn from_iam_status(status: u16, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        match status {
            401 => Self::Unauthorized,
            403 => Self::Forbidden { detail },
            404 | 409 | 422 => Self::Conflict { detail },
            _ if (500..=599).contains(&status) => {
                Self::Protocol(format!("IAM answered HTTP {status}: {detail}"))
            }
            _ => Self::Protocol(format!("IAM answered HTTP {status}: {detail}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_iam_statuses_onto_classes() {
        assert!(matches!(
            AppTokenError::from_iam_status(401, "missing session"),
            AppTokenError::Unauthorized
        ));
        assert!(matches!(
            AppTokenError::from_iam_status(403, "not a member"),
            AppTokenError::Forbidden { .. }
        ));
        assert!(matches!(
            AppTokenError::from_iam_status(409, "must be enabled"),
            AppTokenError::Conflict { .. }
        ));
        assert!(matches!(
            AppTokenError::from_iam_status(503, "unavailable"),
            AppTokenError::Protocol(_)
        ));
    }

    #[test]
    fn not_found_names_the_tenant_and_selector() {
        let error = AppTokenError::application_not_found("100001", "sdkwork-cloudrouter-pc");
        let rendered = error.to_string();
        assert!(rendered.contains("100001"), "{rendered}");
        assert!(rendered.contains("sdkwork-cloudrouter-pc"), "{rendered}");
    }
}
