//! Composition: resolve the application, then exchange the session.
//!
//! The service holds no state and performs no I/O of its own; it is the seam an
//! application wires its own repository and HTTP client into. That keeps the
//! library usable by a service host, a CLI, or a test double with the same code.

use crate::error::AppTokenError;
use crate::exchange::AppTokenExchange;
use crate::lookup::DeployApplicationLookup;
use crate::model::{AppAccessToken, AppTokenRequest, CallerSession};

/// Resolves a deploy application and mints its credential through IAM.
#[derive(Clone, Debug)]
pub struct DeployAppTokenService<L, E> {
    lookup: L,
    exchange: E,
}

impl<L, E> DeployAppTokenService<L, E>
where
    L: DeployApplicationLookup,
    E: AppTokenExchange,
{
    /// @param lookup - deploy-owned application resolver.
    /// @param exchange - IAM `appAccessTokens.create` client.
    /// @returns the service.
    #[must_use]
    pub fn new(lookup: L, exchange: E) -> Self {
        Self { lookup, exchange }
    }

    /// Issues the credential of the application the caller named.
    ///
    /// Order matters: the session is checked before the inventory is read, the
    /// lookup is always scoped to the caller's tenant, and IAM receives the
    /// resolved anchor rather than the caller's raw selector. IAM then applies the
    /// entitlement matrix (owner / organization membership / tenant membership) — this
    /// service never decides it.
    /// @param tenant_id - the caller's tenant, from its session.
    /// @param session - the caller's authenticated session pair.
    /// @param request - the selector the caller supplied.
    /// @returns the issued credential.
    /// @throws [`AppTokenError`] when the session is incomplete, the application is
    ///   absent from the tenant, or IAM refuses.
    pub async fn issue(
        &self,
        tenant_id: &str,
        session: &CallerSession,
        request: &AppTokenRequest,
    ) -> Result<AppAccessToken, AppTokenError> {
        if !session.is_complete() {
            return Err(AppTokenError::Unauthorized);
        }
        let application = self
            .lookup
            .find(tenant_id, &request.selector)
            .await?
            .ok_or_else(|| {
                AppTokenError::application_not_found(tenant_id, request.selector.to_string())
            })?;
        self.exchange
            .exchange(
                session,
                application.anchor(),
                request.environment.as_deref(),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::model::{DeployApplicationRef, DeployApplicationSelector};

    #[derive(Clone, Debug)]
    struct FakeLookup {
        found: Option<DeployApplicationRef>,
        seen_tenant: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    }

    #[async_trait]
    impl DeployApplicationLookup for FakeLookup {
        async fn find(
            &self,
            tenant_id: &str,
            _selector: &DeployApplicationSelector,
        ) -> Result<Option<DeployApplicationRef>, AppTokenError> {
            if let Ok(mut guard) = self.seen_tenant.lock() {
                *guard = Some(tenant_id.to_owned());
            }
            Ok(self.found.clone())
        }
    }

    #[derive(Clone, Debug, Default)]
    struct FakeExchange {
        seen_anchor: std::sync::Arc<std::sync::Mutex<Option<String>>>,
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait]
    impl AppTokenExchange for FakeExchange {
        async fn exchange(
            &self,
            _session: &CallerSession,
            deploy_application_id: &str,
            environment: Option<&str>,
        ) -> Result<AppAccessToken, AppTokenError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if let Ok(mut guard) = self.seen_anchor.lock() {
                *guard = Some(deploy_application_id.to_owned());
            }
            Ok(AppAccessToken {
                access_token: "a".to_owned(),
                auth_token: "b".to_owned(),
                expires_at: "2026-11-19T12:00:00Z".to_owned(),
                app_id: deploy_application_id.to_owned(),
                tenant_application_id: "tapp_1".to_owned(),
                organization_id: "0".to_owned(),
                ownership_scope: "TENANT".to_owned(),
                environment: environment.unwrap_or("production").to_owned(),
                session_id: "session".to_owned(),
            })
        }
    }

    fn application() -> DeployApplicationRef {
        DeployApplicationRef {
            uuid: "ef45ca0e-0093-4eaf-ab03-964d8ef6227c".to_owned(),
            deploy_application_id: "381764687714324480".to_owned(),
            tenant_id: "100001".to_owned(),
            slug: "sdkwork-cloudrouter-pc".to_owned(),
            app_kind: "SPA_WEB".to_owned(),
            owner_type: "TENANT".to_owned(),
            owner_user_id: None,
        }
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
    }

    #[test]
    fn issues_against_the_resolved_anchor_not_the_raw_selector() {
        let exchange = FakeExchange::default();
        let service = DeployAppTokenService::new(
            FakeLookup {
                found: Some(application()),
                seen_tenant: std::sync::Arc::new(std::sync::Mutex::new(None)),
            },
            exchange.clone(),
        );
        let issued = runtime()
            .block_on(service.issue(
                "100001",
                &CallerSession::new("access", "auth"),
                &AppTokenRequest::new(DeployApplicationSelector::slug("sdkwork-cloudrouter-pc")),
            ))
            .expect("issued");
        assert_eq!(issued.app_id, "381764687714324480");
        assert_eq!(
            exchange.seen_anchor.lock().expect("anchor").as_deref(),
            Some("381764687714324480")
        );
    }

    #[test]
    fn missing_application_is_refused_before_any_exchange() {
        let exchange = FakeExchange::default();
        let service = DeployAppTokenService::new(
            FakeLookup {
                found: None,
                seen_tenant: std::sync::Arc::new(std::sync::Mutex::new(None)),
            },
            exchange.clone(),
        );
        let outcome = runtime().block_on(service.issue(
            "100001",
            &CallerSession::new("access", "auth"),
            &AppTokenRequest::new(DeployApplicationSelector::slug("missing")),
        ));
        assert!(matches!(
            outcome,
            Err(AppTokenError::ApplicationNotFound { .. })
        ));
        assert_eq!(exchange.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn incomplete_session_never_reads_the_inventory() {
        let exchange = FakeExchange::default();
        let seen_tenant = std::sync::Arc::new(std::sync::Mutex::new(None));
        let service = DeployAppTokenService::new(
            FakeLookup {
                found: Some(application()),
                seen_tenant: seen_tenant.clone(),
            },
            exchange.clone(),
        );
        let outcome = runtime().block_on(service.issue(
            "100001",
            &CallerSession::new("", ""),
            &AppTokenRequest::new(DeployApplicationSelector::slug("app")),
        ));
        assert!(matches!(outcome, Err(AppTokenError::Unauthorized)));
        assert!(seen_tenant.lock().expect("tenant").is_none());
        assert_eq!(exchange.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn environment_hint_reaches_the_exchange() {
        let service = DeployAppTokenService::new(
            FakeLookup {
                found: Some(application()),
                seen_tenant: std::sync::Arc::new(std::sync::Mutex::new(None)),
            },
            FakeExchange::default(),
        );
        let issued = runtime()
            .block_on(
                service.issue(
                    "100001",
                    &CallerSession::new("access", "auth"),
                    &AppTokenRequest::new(DeployApplicationSelector::slug(
                        "sdkwork-cloudrouter-pc",
                    ))
                    .with_environment("development"),
                ),
            )
            .expect("issued");
        assert_eq!(issued.environment, "development");
    }

    #[test]
    fn lookup_is_scoped_to_the_caller_tenant() {
        let seen_tenant = std::sync::Arc::new(std::sync::Mutex::new(None));
        let service = DeployAppTokenService::new(
            FakeLookup {
                found: Some(application()),
                seen_tenant: seen_tenant.clone(),
            },
            FakeExchange::default(),
        );
        runtime()
            .block_on(service.issue(
                "100001",
                &CallerSession::new("access", "auth"),
                &AppTokenRequest::new(DeployApplicationSelector::slug("app")),
            ))
            .expect("issued");
        assert_eq!(
            seen_tenant.lock().expect("tenant").as_deref(),
            Some("100001")
        );
    }
}
