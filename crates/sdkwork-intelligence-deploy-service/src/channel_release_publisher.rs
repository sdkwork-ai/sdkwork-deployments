//! Publish orchestration for WeChat channel releases.
//!
//! The publisher sequences provider calls and persists each observed step through
//! the store port with the contract state machine and an optimistic version. The
//! sequence is intentionally explicit rather than a blind retry loop: an upload
//! that the provider accepted but whose transition was lost is re-read before the
//! next step, and a release is only ever attempted from `APPROVED`.
use sdkwork_deploy_contract::{
    validate_channel_release_transition, ChannelReleaseStatus, DeployServiceError,
    DeployServiceResult,
};

use crate::channel_release::{
    ChannelReleaseProvider, MiniProgramAuditSubmission, MiniProgramReleaseOutcome,
    MiniProgramReleaseRequest, MiniProgramRollbackRequest, MiniProgramUploadRequest,
};
use crate::channel_release_observer::ChannelReleaseStore;

/// Runs the provider steps of one channel release.
pub struct ChannelReleasePublisher<P: ChannelReleaseProvider, S: ChannelReleaseStore> {
    provider: P,
    store: S,
}

impl<P: ChannelReleaseProvider, S: ChannelReleaseStore> ChannelReleasePublisher<P, S> {
    /// Builds the publisher from a provider and a store.
    pub fn new(provider: P, store: S) -> Self {
        Self { provider, store }
    }

    async fn advance(
        &self,
        tenant_id: i64,
        release_id: &str,
        current: ChannelReleaseStatus,
        version: i64,
        next: ChannelReleaseStatus,
        commit_id: Option<&str>,
        audit_id: Option<&str>,
        detail: Option<&str>,
    ) -> DeployServiceResult<()> {
        validate_channel_release_transition(current, next).map_err(DeployServiceError::conflict)?;
        self.store
            .transition_channel_release(
                tenant_id, release_id, version, next, commit_id, audit_id, detail,
            )
            .await
    }

    /// Uploads code and records the provider commit id.
    ///
    /// A draft first becomes `PROJECTED` (the build artifact is now a
    /// provider-buildable tree) and then `UPLOADED` once the provider accepts it.
    /// @returns the provider commit id.
    pub async fn upload(
        &self,
        tenant_id: i64,
        release_id: &str,
        request: &MiniProgramUploadRequest,
    ) -> DeployServiceResult<String> {
        let (mut current, mut version) = self
            .store
            .load_channel_release_status(tenant_id, release_id)
            .await?;
        if !matches!(
            current,
            ChannelReleaseStatus::Draft | ChannelReleaseStatus::Projected
        ) {
            return Err(DeployServiceError::Conflict(format!(
                "cannot upload from {}",
                current.as_str()
            )));
        }
        if current == ChannelReleaseStatus::Draft {
            self.advance(
                tenant_id,
                release_id,
                current,
                version,
                ChannelReleaseStatus::Projected,
                None,
                None,
                None,
            )
            .await?;
            current = ChannelReleaseStatus::Projected;
            version += 1;
        }
        let outcome = self.provider.upload_mini_program(request).await?;
        self.advance(
            tenant_id,
            release_id,
            current,
            version,
            ChannelReleaseStatus::Uploaded,
            Some(&outcome.commit_id),
            None,
            None,
        )
        .await?;
        Ok(outcome.commit_id)
    }

    /// Records an upload performed outside Deploy (direct-mode `miniprogram-ci`).
    ///
    /// The provider commit id is a deterministic receipt (`ci:<version>`), so the
    /// record still has a stable handle without inventing a WeChat commit id.
    pub async fn record_upload(
        &self,
        tenant_id: i64,
        release_id: &str,
        commit_id: &str,
    ) -> DeployServiceResult<()> {
        if commit_id.trim().is_empty() {
            return Err(DeployServiceError::validation("commit id is required"));
        }
        let (mut current, mut version) = self
            .store
            .load_channel_release_status(tenant_id, release_id)
            .await?;
        if current == ChannelReleaseStatus::Draft {
            self.advance(
                tenant_id,
                release_id,
                current,
                version,
                ChannelReleaseStatus::Projected,
                None,
                None,
                None,
            )
            .await?;
            current = ChannelReleaseStatus::Projected;
            version += 1;
        }
        if current != ChannelReleaseStatus::Projected {
            return Err(DeployServiceError::Conflict(format!(
                "cannot record an upload from {}",
                current.as_str()
            )));
        }
        self.advance(
            tenant_id,
            release_id,
            current,
            version,
            ChannelReleaseStatus::Uploaded,
            Some(commit_id),
            None,
            None,
        )
        .await
    }

    /// Marks a non-terminal release `FAILED` with the provider reason.
    ///
    /// Best effort by design: the caller reports the original provider error, and
    /// a state the machine cannot move to `FAILED` from (for example `RELEASED`)
    /// keeps its current state instead of being forced.
    pub async fn mark_failed(
        &self,
        tenant_id: i64,
        release_id: &str,
        detail: &str,
    ) -> DeployServiceResult<()> {
        let (current, version) = self
            .store
            .load_channel_release_status(tenant_id, release_id)
            .await?;
        if current.is_terminal() {
            return Ok(());
        }
        self.advance(
            tenant_id,
            release_id,
            current,
            version,
            ChannelReleaseStatus::Failed,
            None,
            None,
            Some(detail),
        )
        .await
    }

    /// Submits the uploaded code for review and records the audit id.
    pub async fn submit_audit(
        &self,
        tenant_id: i64,
        release_id: &str,
        request: &MiniProgramAuditSubmission,
    ) -> DeployServiceResult<String> {
        let (current, version) = self
            .store
            .load_channel_release_status(tenant_id, release_id)
            .await?;
        if current != ChannelReleaseStatus::Uploaded {
            return Err(DeployServiceError::Conflict(format!(
                "cannot submit a review from {}",
                current.as_str()
            )));
        }
        let outcome = self.provider.submit_mini_program_audit(request).await?;
        self.advance(
            tenant_id,
            release_id,
            current,
            version,
            ChannelReleaseStatus::AuditSubmitted,
            None,
            Some(&outcome.audit_id),
            None,
        )
        .await?;
        Ok(outcome.audit_id)
    }

    /// Publishes the approved version.
    ///
    /// A provider rejection after `RELEASING` records `FAILED` with the provider
    /// reason, then returns the original error so the caller cannot report a
    /// release that did not happen.
    pub async fn release(
        &self,
        tenant_id: i64,
        release_id: &str,
        request: &MiniProgramReleaseRequest,
    ) -> DeployServiceResult<MiniProgramReleaseOutcome> {
        let (current, version) = self
            .store
            .load_channel_release_status(tenant_id, release_id)
            .await?;
        if current != ChannelReleaseStatus::Approved {
            return Err(DeployServiceError::Conflict(format!(
                "cannot publish from {}; a release requires APPROVED",
                current.as_str()
            )));
        }
        self.advance(
            tenant_id,
            release_id,
            current,
            version,
            ChannelReleaseStatus::Releasing,
            None,
            None,
            None,
        )
        .await?;
        self.release_provider_step(tenant_id, release_id, version + 1, request)
            .await
    }

    /// Restores an earlier released version.
    ///
    /// Only a live `RELEASED` record can move to `ROLLED_BACK`. A provider
    /// failure leaves the record `RELEASED` (the live version is unchanged) and
    /// returns the provider error; it never fabricates a rollback.
    pub async fn rollback(
        &self,
        tenant_id: i64,
        release_id: &str,
        request: &MiniProgramRollbackRequest,
    ) -> DeployServiceResult<()> {
        let (current, version) = self
            .store
            .load_channel_release_status(tenant_id, release_id)
            .await?;
        if current != ChannelReleaseStatus::Released {
            return Err(DeployServiceError::Conflict(format!(
                "cannot roll back from {}; a rollback requires RELEASED",
                current.as_str()
            )));
        }
        self.provider.rollback_mini_program(request).await?;
        self.advance(
            tenant_id,
            release_id,
            current,
            version,
            ChannelReleaseStatus::RolledBack,
            None,
            None,
            None,
        )
        .await
    }

    /// Adjusts the provider's gray rollout for a live release.
    ///
    /// The percentage is only persisted after the provider accepted it, so the
    /// board never shows a rollout the provider rejected.
    pub async fn gray(
        &self,
        tenant_id: i64,
        release_id: &str,
        request: &crate::channel_release::MiniProgramGrayReleaseRequest,
    ) -> DeployServiceResult<()> {
        if request.gray_percentage > 100 {
            return Err(DeployServiceError::validation(
                "grayPercentage must be between 0 and 100",
            ));
        }
        let (current, _version) = self
            .store
            .load_channel_release_status(tenant_id, release_id)
            .await?;
        if current != ChannelReleaseStatus::Released {
            return Err(DeployServiceError::Conflict(format!(
                "cannot adjust a gray rollout from {}; a rollout requires RELEASED",
                current.as_str()
            )));
        }
        self.provider.gray_release_mini_program(request).await?;
        self.store
            .set_channel_release_gray_percentage(tenant_id, release_id, request.gray_percentage)
            .await
    }

    async fn release_provider_step(
        &self,
        tenant_id: i64,
        release_id: &str,
        version: i64,
        request: &MiniProgramReleaseRequest,
    ) -> DeployServiceResult<MiniProgramReleaseOutcome> {
        match self.provider.release_mini_program(request).await {
            Ok(outcome) => {
                self.advance(
                    tenant_id,
                    release_id,
                    ChannelReleaseStatus::Releasing,
                    version,
                    ChannelReleaseStatus::Released,
                    None,
                    None,
                    None,
                )
                .await?;
                Ok(outcome)
            }
            Err(error) => {
                let detail = error.to_string();
                // Best effort: the original provider error is the reported
                // failure, and the FAILED marker must not mask it.
                let _ = self
                    .advance(
                        tenant_id,
                        release_id,
                        ChannelReleaseStatus::Releasing,
                        version,
                        ChannelReleaseStatus::Failed,
                        None,
                        None,
                        Some(&detail),
                    )
                    .await;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_release::{
        MiniProgramAuditObservation, MiniProgramAuditOutcome, MiniProgramUploadOutcome,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeProvider {
        upload_fails: bool,
        release_fails: bool,
        rollback_fails: bool,
        gray_fails: bool,
        upload_calls: AtomicUsize,
        release_calls: AtomicUsize,
    }

    #[async_trait]
    impl ChannelReleaseProvider for FakeProvider {
        async fn upload_mini_program(
            &self,
            _request: &MiniProgramUploadRequest,
        ) -> DeployServiceResult<MiniProgramUploadOutcome> {
            self.upload_calls.fetch_add(1, Ordering::SeqCst);
            if self.upload_fails {
                return Err(DeployServiceError::Conflict("upload rejected".to_string()));
            }
            Ok(MiniProgramUploadOutcome {
                commit_id: "commit-1".to_string(),
            })
        }

        async fn submit_mini_program_audit(
            &self,
            _request: &MiniProgramAuditSubmission,
        ) -> DeployServiceResult<MiniProgramAuditOutcome> {
            Ok(MiniProgramAuditOutcome {
                audit_id: "audit-1".to_string(),
            })
        }

        async fn query_mini_program_audit(
            &self,
            _authorization_id: &str,
            _audit_id: &str,
        ) -> DeployServiceResult<MiniProgramAuditObservation> {
            Ok(MiniProgramAuditObservation {
                status: ChannelReleaseStatus::InReview,
                detail: None,
            })
        }

        async fn release_mini_program(
            &self,
            _request: &MiniProgramReleaseRequest,
        ) -> DeployServiceResult<MiniProgramReleaseOutcome> {
            self.release_calls.fetch_add(1, Ordering::SeqCst);
            if self.release_fails {
                return Err(DeployServiceError::Conflict("release rejected".to_string()));
            }
            Ok(MiniProgramReleaseOutcome {
                released_version: Some("1.0.0".to_string()),
            })
        }

        async fn rollback_mini_program(
            &self,
            _request: &MiniProgramRollbackRequest,
        ) -> DeployServiceResult<()> {
            if self.rollback_fails {
                return Err(DeployServiceError::Conflict(
                    "rollback rejected".to_string(),
                ));
            }
            Ok(())
        }

        async fn gray_release_mini_program(
            &self,
            _request: &crate::channel_release::MiniProgramGrayReleaseRequest,
        ) -> DeployServiceResult<()> {
            if self.gray_fails {
                return Err(DeployServiceError::Conflict("gray rejected".to_string()));
            }
            Ok(())
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

    #[derive(Clone)]
    struct FakeStore {
        state: Arc<Mutex<(ChannelReleaseStatus, i64)>>,
        transitions: Arc<Mutex<Vec<(ChannelReleaseStatus, Option<String>, Option<String>)>>>,
        gray_percentage: Arc<Mutex<Option<u16>>>,
    }

    impl FakeStore {
        fn new(status: ChannelReleaseStatus) -> Self {
            Self {
                state: Arc::new(Mutex::new((status, 1))),
                transitions: Arc::new(Mutex::new(Vec::new())),
                gray_percentage: Arc::new(Mutex::new(None)),
            }
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
            commit_id: Option<&str>,
            audit_id: Option<&str>,
            _detail: Option<&str>,
        ) -> DeployServiceResult<()> {
            let mut state = self.state.lock().expect("state");
            if state.1 != expected_version {
                return Err(DeployServiceError::Conflict("stale version".to_string()));
            }
            *state = (next, state.1 + 1);
            self.transitions.lock().expect("transitions").push((
                next,
                commit_id.map(str::to_string),
                audit_id.map(str::to_string),
            ));
            Ok(())
        }

        async fn set_channel_release_gray_percentage(
            &self,
            _tenant_id: i64,
            _release_id: &str,
            gray_percentage: u16,
        ) -> DeployServiceResult<()> {
            let state = self.state.lock().expect("state");
            if state.0 != ChannelReleaseStatus::Released {
                return Err(DeployServiceError::Conflict(
                    "channel release is not released".to_string(),
                ));
            }
            *self.gray_percentage.lock().expect("gray") = Some(gray_percentage);
            Ok(())
        }
    }

    fn upload_request() -> MiniProgramUploadRequest {
        MiniProgramUploadRequest {
            authorization_id: "auth-1".to_string(),
            environment: "production".to_string(),
            template_id: Some("tpl-1".to_string()),
            provider_version: Some("1.0.0".to_string()),
            ext_json: None,
        }
    }

    fn release_request() -> MiniProgramReleaseRequest {
        MiniProgramReleaseRequest {
            authorization_id: "auth-1".to_string(),
            provider_version: Some("1.0.0".to_string()),
        }
    }

    #[tokio::test]
    async fn upload_walks_draft_to_projected_to_uploaded() {
        let store = FakeStore::new(ChannelReleaseStatus::Draft);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store.clone());
        let commit = publisher
            .upload(7, "release-1", &upload_request())
            .await
            .expect("upload");
        assert_eq!(commit, "commit-1");
        let transitions = store.transitions.lock().expect("transitions").clone();
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].0, ChannelReleaseStatus::Projected);
        assert_eq!(transitions[1].0, ChannelReleaseStatus::Uploaded);
        assert_eq!(transitions[1].1.as_deref(), Some("commit-1"));
        assert_eq!(
            store.state.lock().expect("state").0,
            ChannelReleaseStatus::Uploaded
        );
    }

    #[tokio::test]
    async fn upload_refuses_a_released_record() {
        let store = FakeStore::new(ChannelReleaseStatus::Released);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store);
        let error = publisher
            .upload(7, "release-1", &upload_request())
            .await
            .expect_err("must refuse");
        assert!(matches!(error, DeployServiceError::Conflict(_)));
    }

    #[tokio::test]
    async fn submit_requires_uploaded_and_records_the_audit_id() {
        let store = FakeStore::new(ChannelReleaseStatus::Uploaded);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store.clone());
        let audit_id = publisher
            .submit_audit(
                7,
                "release-1",
                &MiniProgramAuditSubmission {
                    authorization_id: "auth-1".to_string(),
                    commit_id: "commit-1".to_string(),
                    provider_version: "1.0.0".to_string(),
                    items: Vec::new(),
                },
            )
            .await
            .expect("submit");
        assert_eq!(audit_id, "audit-1");
        let transitions = store.transitions.lock().expect("transitions").clone();
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].0, ChannelReleaseStatus::AuditSubmitted);
        assert_eq!(transitions[0].2.as_deref(), Some("audit-1"));
    }

    #[tokio::test]
    async fn release_requires_approved() {
        let store = FakeStore::new(ChannelReleaseStatus::AuditSubmitted);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store);
        let error = publisher
            .release(7, "release-1", &release_request())
            .await
            .expect_err("must refuse");
        assert!(matches!(error, DeployServiceError::Conflict(_)));
    }

    #[tokio::test]
    async fn release_marks_failed_when_the_provider_rejects() {
        let provider = FakeProvider {
            release_fails: true,
            ..FakeProvider::default()
        };
        let store = FakeStore::new(ChannelReleaseStatus::Approved);
        let publisher = ChannelReleasePublisher::new(provider, store.clone());
        let error = publisher
            .release(7, "release-1", &release_request())
            .await
            .expect_err("must fail");
        assert!(matches!(error, DeployServiceError::Conflict(_)));
        assert_eq!(
            store.state.lock().expect("state").0,
            ChannelReleaseStatus::Failed
        );
    }

    #[tokio::test]
    async fn release_marks_released_on_success() {
        let store = FakeStore::new(ChannelReleaseStatus::Approved);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store.clone());
        let outcome = publisher
            .release(7, "release-1", &release_request())
            .await
            .expect("release");
        assert_eq!(outcome.released_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            store.state.lock().expect("state").0,
            ChannelReleaseStatus::Released
        );
    }

    #[tokio::test]
    async fn records_a_direct_mode_upload() {
        let store = FakeStore::new(ChannelReleaseStatus::Draft);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store.clone());
        publisher
            .record_upload(7, "release-1", "ci:1.0.0")
            .await
            .expect("record");
        let transitions = store.transitions.lock().expect("transitions").clone();
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].0, ChannelReleaseStatus::Projected);
        assert_eq!(transitions[1].0, ChannelReleaseStatus::Uploaded);
        assert_eq!(transitions[1].1.as_deref(), Some("ci:1.0.0"));
    }

    #[tokio::test]
    async fn direct_mode_upload_requires_a_commit_receipt() {
        let store = FakeStore::new(ChannelReleaseStatus::Draft);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store);
        assert!(publisher.record_upload(7, "release-1", "  ").await.is_err());
    }

    #[tokio::test]
    async fn marks_a_failed_provider_step() {
        let store = FakeStore::new(ChannelReleaseStatus::Projected);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store.clone());
        publisher
            .mark_failed(7, "release-1", "provider rejected the upload")
            .await
            .expect("mark failed");
        let transitions = store.transitions.lock().expect("transitions").clone();
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].0, ChannelReleaseStatus::Failed);
        assert_eq!(
            store.state.lock().expect("state").0,
            ChannelReleaseStatus::Failed
        );
    }

    #[tokio::test]
    async fn mark_failed_ignores_a_terminal_release() {
        let store = FakeStore::new(ChannelReleaseStatus::RolledBack);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store.clone());
        publisher
            .mark_failed(7, "release-1", "late observation")
            .await
            .expect("no-op");
        assert_eq!(
            store.state.lock().expect("state").0,
            ChannelReleaseStatus::RolledBack
        );
    }

    fn rollback_request() -> MiniProgramRollbackRequest {
        MiniProgramRollbackRequest {
            authorization_id: "auth-1".to_string(),
            target_version: "0.9.0".to_string(),
        }
    }

    #[tokio::test]
    async fn rollback_requires_a_live_release() {
        let store = FakeStore::new(ChannelReleaseStatus::Approved);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store);
        let error = publisher
            .rollback(7, "release-1", &rollback_request())
            .await
            .expect_err("must refuse");
        assert!(matches!(error, DeployServiceError::Conflict(_)));
    }

    #[tokio::test]
    async fn rollback_moves_released_to_rolled_back() {
        let store = FakeStore::new(ChannelReleaseStatus::Released);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store.clone());
        publisher
            .rollback(7, "release-1", &rollback_request())
            .await
            .expect("rollback");
        assert_eq!(
            store.state.lock().expect("state").0,
            ChannelReleaseStatus::RolledBack
        );
    }

    #[tokio::test]
    async fn rollback_provider_failure_keeps_the_live_release() {
        let provider = FakeProvider {
            rollback_fails: true,
            ..FakeProvider::default()
        };
        let store = FakeStore::new(ChannelReleaseStatus::Released);
        let publisher = ChannelReleasePublisher::new(provider, store.clone());
        publisher
            .rollback(7, "release-1", &rollback_request())
            .await
            .expect_err("must fail");
        assert_eq!(
            store.state.lock().expect("state").0,
            ChannelReleaseStatus::Released
        );
    }

    fn gray_request(gray_percentage: u16) -> crate::channel_release::MiniProgramGrayReleaseRequest {
        crate::channel_release::MiniProgramGrayReleaseRequest {
            authorization_id: "auth-1".to_string(),
            gray_percentage,
        }
    }

    #[tokio::test]
    async fn gray_requires_a_live_release() {
        let store = FakeStore::new(ChannelReleaseStatus::Approved);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store.clone());
        let error = publisher
            .gray(7, "release-1", &gray_request(30))
            .await
            .expect_err("must refuse");
        assert!(matches!(error, DeployServiceError::Conflict(_)));
        assert_eq!(*store.gray_percentage.lock().expect("gray"), None);
    }

    #[tokio::test]
    async fn gray_records_the_provider_percentage() {
        let store = FakeStore::new(ChannelReleaseStatus::Released);
        let publisher = ChannelReleasePublisher::new(FakeProvider::default(), store.clone());
        publisher
            .gray(7, "release-1", &gray_request(30))
            .await
            .expect("gray");
        assert_eq!(*store.gray_percentage.lock().expect("gray"), Some(30));
    }

    #[tokio::test]
    async fn gray_provider_failure_does_not_persist() {
        let provider = FakeProvider {
            gray_fails: true,
            ..FakeProvider::default()
        };
        let store = FakeStore::new(ChannelReleaseStatus::Released);
        let publisher = ChannelReleasePublisher::new(provider, store.clone());
        publisher
            .gray(7, "release-1", &gray_request(30))
            .await
            .expect_err("must fail");
        assert_eq!(*store.gray_percentage.lock().expect("gray"), None);
    }
}
