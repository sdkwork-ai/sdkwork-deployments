//! Review observation for channel releases.
//!
//! WeChat owns the review verdict. This observer asks the provider for the
//! current audit state, applies the contract state machine, and writes the
//! observed transition through a store port with an optimistic version. It never
//! infers a verdict from elapsed time or from a previous observation:
//!
//! - a terminal release is not queried again and is returned unchanged;
//! - a provider answer equal to the current state is a no-op;
//! - a provider answer the state machine cannot reach is a conflict, not an
//!   overwrite.
use async_trait::async_trait;

use sdkwork_deploy_contract::{
    validate_channel_release_transition, ChannelReleaseStatus, DeployServiceError,
    DeployServiceResult,
};

use crate::channel_release::ChannelReleaseProvider;

/// The narrow persistence surface the observer needs.
#[async_trait]
pub trait ChannelReleaseStore: Send + Sync {
    /// Returns the current status and optimistic version.
    async fn load_channel_release_status(
        &self,
        tenant_id: i64,
        release_id: &str,
    ) -> DeployServiceResult<(ChannelReleaseStatus, i64)>;
    /// Applies one observed transition.
    ///
    /// `commit_id` and `audit_id` are provider identifiers the publisher records;
    /// the review observer passes `None` for both.
    async fn transition_channel_release(
        &self,
        tenant_id: i64,
        release_id: &str,
        expected_version: i64,
        next: ChannelReleaseStatus,
        commit_id: Option<&str>,
        audit_id: Option<&str>,
        detail: Option<&str>,
    ) -> DeployServiceResult<()>;

    /// Records the provider's active gray percentage for a released version.
    async fn set_channel_release_gray_percentage(
        &self,
        tenant_id: i64,
        release_id: &str,
        gray_percentage: u16,
    ) -> DeployServiceResult<()>;
}

/// Applies provider review observations to a channel release.
pub struct ChannelReleaseReviewObserver<P: ChannelReleaseProvider, S: ChannelReleaseStore> {
    provider: P,
    store: S,
}

impl<P: ChannelReleaseProvider, S: ChannelReleaseStore> ChannelReleaseReviewObserver<P, S> {
    /// Builds the observer from a provider and a store.
    pub fn new(provider: P, store: S) -> Self {
        Self { provider, store }
    }

    /// Observes one audit and applies the resulting transition.
    ///
    /// @returns the provider status that is now persisted.
    pub async fn observe(
        &self,
        tenant_id: i64,
        release_id: &str,
        authorization_id: &str,
        audit_id: &str,
    ) -> DeployServiceResult<ChannelReleaseStatus> {
        let (current, version) = self
            .store
            .load_channel_release_status(tenant_id, release_id)
            .await?;
        if current.is_terminal() {
            return Ok(current);
        }
        let observation = self
            .provider
            .query_mini_program_audit(authorization_id, audit_id)
            .await?;
        if observation.status == current {
            return Ok(current);
        }
        validate_channel_release_transition(current, observation.status)
            .map_err(DeployServiceError::conflict)?;
        self.store
            .transition_channel_release(
                tenant_id,
                release_id,
                version,
                observation.status,
                None,
                None,
                observation.detail.as_deref(),
            )
            .await?;
        Ok(observation.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_release::{
        MiniProgramAuditObservation, MiniProgramAuditOutcome, MiniProgramAuditSubmission,
        MiniProgramGrayReleaseRequest, MiniProgramReleaseOutcome, MiniProgramReleaseRequest,
        MiniProgramRollbackRequest, MiniProgramUploadOutcome, MiniProgramUploadRequest,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[derive(Clone)]
    struct FakeProvider {
        observation: Result<MiniProgramAuditObservation, String>,
        queries: std::sync::Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ChannelReleaseProvider for FakeProvider {
        async fn upload_mini_program(
            &self,
            _request: &MiniProgramUploadRequest,
        ) -> DeployServiceResult<MiniProgramUploadOutcome> {
            Err(DeployServiceError::Internal("unused".to_string()))
        }

        async fn submit_mini_program_audit(
            &self,
            _request: &MiniProgramAuditSubmission,
        ) -> DeployServiceResult<MiniProgramAuditOutcome> {
            Err(DeployServiceError::Internal("unused".to_string()))
        }

        async fn query_mini_program_audit(
            &self,
            _authorization_id: &str,
            _audit_id: &str,
        ) -> DeployServiceResult<MiniProgramAuditObservation> {
            self.queries.fetch_add(1, Ordering::SeqCst);
            self.observation
                .clone()
                .map_err(DeployServiceError::Internal)
        }

        async fn release_mini_program(
            &self,
            _request: &MiniProgramReleaseRequest,
        ) -> DeployServiceResult<MiniProgramReleaseOutcome> {
            Err(DeployServiceError::Internal("unused".to_string()))
        }

        async fn rollback_mini_program(
            &self,
            _request: &MiniProgramRollbackRequest,
        ) -> DeployServiceResult<()> {
            Err(DeployServiceError::Internal("unused".to_string()))
        }

        async fn gray_release_mini_program(
            &self,
            _request: &MiniProgramGrayReleaseRequest,
        ) -> DeployServiceResult<()> {
            Err(DeployServiceError::Internal("unused".to_string()))
        }

        async fn modify_mini_program_domains(
            &self,
            _authorization_id: &str,
            _action: &str,
            _domains: &[String],
        ) -> DeployServiceResult<()> {
            Err(DeployServiceError::Internal("unused".to_string()))
        }

        async fn set_mini_program_privacy(
            &self,
            _authorization_id: &str,
            _privacy: serde_json::Value,
        ) -> DeployServiceResult<()> {
            Err(DeployServiceError::Internal("unused".to_string()))
        }
    }

    struct FakeStore {
        state: Mutex<(ChannelReleaseStatus, i64)>,
        transitions: Mutex<Vec<(i64, ChannelReleaseStatus, Option<String>)>>,
    }

    impl FakeStore {
        fn with(state: ChannelReleaseStatus, version: i64) -> Self {
            Self {
                state: Mutex::new((state, version)),
                transitions: Mutex::new(Vec::new()),
            }
        }

        fn transitions(&self) -> Vec<(i64, ChannelReleaseStatus, Option<String>)> {
            self.transitions.lock().expect("transitions").clone()
        }
    }

    #[async_trait]
    impl ChannelReleaseStore for FakeStore {
        async fn load_channel_release_status(
            &self,
            _tenant_id: i64,
            _release_id: &str,
        ) -> DeployServiceResult<(ChannelReleaseStatus, i64)> {
            Ok(*self.state.lock().expect("state"))
        }

        async fn transition_channel_release(
            &self,
            _tenant_id: i64,
            _release_id: &str,
            expected_version: i64,
            next: ChannelReleaseStatus,
            _commit_id: Option<&str>,
            _audit_id: Option<&str>,
            detail: Option<&str>,
        ) -> DeployServiceResult<()> {
            let mut state = self.state.lock().expect("state");
            if state.1 != expected_version {
                return Err(DeployServiceError::Conflict("stale version".to_string()));
            }
            *state = (next, state.1 + 1);
            self.transitions.lock().expect("transitions").push((
                expected_version,
                next,
                detail.map(str::to_string),
            ));
            Ok(())
        }

        async fn set_channel_release_gray_percentage(
            &self,
            _tenant_id: i64,
            _release_id: &str,
            _gray_percentage: u16,
        ) -> DeployServiceResult<()> {
            Ok(())
        }
    }

    fn provider(
        status: ChannelReleaseStatus,
        detail: Option<&str>,
    ) -> (FakeProvider, std::sync::Arc<AtomicUsize>) {
        let queries = std::sync::Arc::new(AtomicUsize::new(0));
        (
            FakeProvider {
                observation: Ok(MiniProgramAuditObservation {
                    status,
                    detail: detail.map(str::to_string),
                }),
                queries: queries.clone(),
            },
            queries,
        )
    }

    #[tokio::test]
    async fn applies_a_review_approval() {
        let (provider, _queries) = provider(ChannelReleaseStatus::Approved, None);
        let store = FakeStore::with(ChannelReleaseStatus::InReview, 3);
        let observer = ChannelReleaseReviewObserver::new(provider, store);
        let status = observer
            .observe(7, "release-1", "auth-1", "audit-1")
            .await
            .expect("observe");
        assert_eq!(status, ChannelReleaseStatus::Approved);
        let transitions = observer.store.transitions();
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].0, 3);
        assert_eq!(transitions[0].1, ChannelReleaseStatus::Approved);
    }

    #[tokio::test]
    async fn terminal_releases_are_not_queried() {
        let (provider, queries) = provider(ChannelReleaseStatus::Approved, None);
        let store = FakeStore::with(ChannelReleaseStatus::Rejected, 1);
        let observer = ChannelReleaseReviewObserver::new(provider, store);
        let status = observer
            .observe(7, "release-1", "auth-1", "audit-1")
            .await
            .expect("observe");
        assert_eq!(status, ChannelReleaseStatus::Rejected);
        assert_eq!(queries.load(Ordering::SeqCst), 0);
        assert!(observer.store.transitions().is_empty());
    }

    #[tokio::test]
    async fn unchanged_observation_is_a_no_op() {
        let (provider, _queries) = provider(ChannelReleaseStatus::InReview, None);
        let store = FakeStore::with(ChannelReleaseStatus::InReview, 2);
        let observer = ChannelReleaseReviewObserver::new(provider, store);
        let status = observer
            .observe(7, "release-1", "auth-1", "audit-1")
            .await
            .expect("observe");
        assert_eq!(status, ChannelReleaseStatus::InReview);
        assert!(observer.store.transitions().is_empty());
    }

    #[tokio::test]
    async fn unreachable_provider_state_is_a_conflict() {
        let (provider, _queries) = provider(ChannelReleaseStatus::Approved, None);
        let store = FakeStore::with(ChannelReleaseStatus::Uploaded, 1);
        let observer = ChannelReleaseReviewObserver::new(provider, store);
        let error = observer
            .observe(7, "release-1", "auth-1", "audit-1")
            .await
            .expect_err("must reject");
        assert!(format!("{error}").contains("invalid channel release transition"));
        assert!(observer.store.transitions().is_empty());
    }

    #[tokio::test]
    async fn provider_failure_surfaces_without_a_transition() {
        let queries = std::sync::Arc::new(AtomicUsize::new(0));
        let provider = FakeProvider {
            observation: Err("provider down".to_string()),
            queries,
        };
        let store = FakeStore::with(ChannelReleaseStatus::InReview, 1);
        let observer = ChannelReleaseReviewObserver::new(provider, store);
        let error = observer
            .observe(7, "release-1", "auth-1", "audit-1")
            .await
            .expect_err("must fail");
        assert!(format!("{error}").contains("provider down"));
        assert!(observer.store.transitions().is_empty());
    }
}
