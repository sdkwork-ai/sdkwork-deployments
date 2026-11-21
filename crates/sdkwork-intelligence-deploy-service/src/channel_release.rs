//! Channel release orchestration: command → provider call → observed state.
//!
//! Deploy does not own WeChat's review verdict, so the orchestration layer is
//! deliberately thin:
//!
//! - it plans the state transitions a command may cause, using the contract
//!   state machine, before any provider call;
//! - it calls a [`ChannelReleaseProvider`] port, which is the only place the
//!   provider wire exists;
//! - a missing provider configuration fails loudly instead of reporting a fake
//!   success, so a deployment cannot claim "released" without a released version.
use async_trait::async_trait;
use sdkwork_deploy_contract::{
    validate_channel_release_transition, ChannelReleaseStatus, DeployServiceError,
    DeployServiceResult,
};
use serde_json::Value;

/// Inputs for uploading mini-program code.
#[derive(Clone, Debug, PartialEq)]
pub struct MiniProgramUploadRequest {
    /// IAM resource authorization that owns the authorizer token.
    pub authorization_id: String,
    /// Deploy environment key.
    pub environment: String,
    /// Template id for the component path.
    pub template_id: Option<String>,
    /// User-facing version.
    pub provider_version: Option<String>,
    /// `ext_json` overrides.
    pub ext_json: Option<Value>,
}

/// Upload outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramUploadOutcome {
    /// Provider commit id.
    pub commit_id: String,
}

/// Inputs for submitting a review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramAuditSubmission {
    /// IAM resource authorization that owns the authorizer token.
    pub authorization_id: String,
    /// Uploaded commit id.
    pub commit_id: String,
    /// Version under review.
    pub provider_version: String,
    /// WeChat review item list (categories and page paths).
    pub items: Vec<Value>,
}

/// Submission outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramAuditOutcome {
    /// Provider audit id.
    pub audit_id: String,
}

/// Observed review state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramAuditObservation {
    /// Current provider state.
    pub status: ChannelReleaseStatus,
    /// Provider reason, when rejected or deferred.
    pub detail: Option<String>,
}

/// Release inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramReleaseRequest {
    /// IAM resource authorization that owns the authorizer token.
    pub authorization_id: String,
    /// Version to publish.
    pub provider_version: Option<String>,
}

/// Release outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramReleaseOutcome {
    /// Version the provider reports as live.
    pub released_version: Option<String>,
}

/// Rollback inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramRollbackRequest {
    /// IAM resource authorization that owns the authorizer token.
    pub authorization_id: String,
    /// Version to restore.
    pub target_version: String,
}

/// Gray release inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramGrayReleaseRequest {
    /// IAM resource authorization that owns the authorizer token.
    pub authorization_id: String,
    /// Percentage `0..=100`.
    pub gray_percentage: u16,
}

/// The provider capability Deploy needs for mini-program publishing.
///
/// Implementations live at the provider boundary (the WeChat capability
/// gateway); the service layer only sees typed outcomes.
#[async_trait]
pub trait ChannelReleaseProvider: Send + Sync {
    /// Uploads code and returns the provider commit id.
    async fn upload_mini_program(
        &self,
        request: &MiniProgramUploadRequest,
    ) -> DeployServiceResult<MiniProgramUploadOutcome>;
    /// Submits the review package.
    async fn submit_mini_program_audit(
        &self,
        request: &MiniProgramAuditSubmission,
    ) -> DeployServiceResult<MiniProgramAuditOutcome>;
    /// Queries the review state.
    async fn query_mini_program_audit(
        &self,
        authorization_id: &str,
        audit_id: &str,
    ) -> DeployServiceResult<MiniProgramAuditObservation>;
    /// Publishes the approved version.
    async fn release_mini_program(
        &self,
        request: &MiniProgramReleaseRequest,
    ) -> DeployServiceResult<MiniProgramReleaseOutcome>;
    /// Restores an earlier version.
    async fn rollback_mini_program(
        &self,
        request: &MiniProgramRollbackRequest,
    ) -> DeployServiceResult<()>;
    /// Adjusts the gray rollout percentage.
    async fn gray_release_mini_program(
        &self,
        request: &MiniProgramGrayReleaseRequest,
    ) -> DeployServiceResult<()>;
    /// Adds, sets, or deletes the mini-program server domains.
    async fn modify_mini_program_domains(
        &self,
        authorization_id: &str,
        action: &str,
        domains: &[String],
    ) -> DeployServiceResult<()>;
    /// Sets the mini-program privacy declaration.
    async fn set_mini_program_privacy(
        &self,
        authorization_id: &str,
        privacy: serde_json::Value,
    ) -> DeployServiceResult<()>;
}

/// Fail-closed provider used until a real channel gateway is configured.
pub struct UnavailableChannelReleaseProvider;
const PROVIDER_UNAVAILABLE: &str = "channel release provider is not configured";

#[async_trait]
impl ChannelReleaseProvider for UnavailableChannelReleaseProvider {
    async fn upload_mini_program(
        &self,
        _request: &MiniProgramUploadRequest,
    ) -> DeployServiceResult<MiniProgramUploadOutcome> {
        Err(DeployServiceError::Conflict(
            PROVIDER_UNAVAILABLE.to_string(),
        ))
    }

    async fn submit_mini_program_audit(
        &self,
        _request: &MiniProgramAuditSubmission,
    ) -> DeployServiceResult<MiniProgramAuditOutcome> {
        Err(DeployServiceError::Conflict(
            PROVIDER_UNAVAILABLE.to_string(),
        ))
    }

    async fn query_mini_program_audit(
        &self,
        _authorization_id: &str,
        _audit_id: &str,
    ) -> DeployServiceResult<MiniProgramAuditObservation> {
        Err(DeployServiceError::Conflict(
            PROVIDER_UNAVAILABLE.to_string(),
        ))
    }

    async fn release_mini_program(
        &self,
        _request: &MiniProgramReleaseRequest,
    ) -> DeployServiceResult<MiniProgramReleaseOutcome> {
        Err(DeployServiceError::Conflict(
            PROVIDER_UNAVAILABLE.to_string(),
        ))
    }

    async fn rollback_mini_program(
        &self,
        _request: &MiniProgramRollbackRequest,
    ) -> DeployServiceResult<()> {
        Err(DeployServiceError::Conflict(
            PROVIDER_UNAVAILABLE.to_string(),
        ))
    }

    async fn gray_release_mini_program(
        &self,
        _request: &MiniProgramGrayReleaseRequest,
    ) -> DeployServiceResult<()> {
        Err(DeployServiceError::Conflict(
            PROVIDER_UNAVAILABLE.to_string(),
        ))
    }

    async fn modify_mini_program_domains(
        &self,
        _authorization_id: &str,
        _action: &str,
        _domains: &[String],
    ) -> DeployServiceResult<()> {
        Err(DeployServiceError::Conflict(
            PROVIDER_UNAVAILABLE.to_string(),
        ))
    }

    async fn set_mini_program_privacy(
        &self,
        _authorization_id: &str,
        _privacy: serde_json::Value,
    ) -> DeployServiceResult<()> {
        Err(DeployServiceError::Conflict(
            PROVIDER_UNAVAILABLE.to_string(),
        ))
    }
}

/// One publisher command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelReleaseCommand {
    /// Upload code.
    Upload,
    /// Submit the review.
    SubmitAudit,
    /// Apply an observed audit state.
    QueryAudit(ChannelReleaseStatus),
    /// Publish the approved version.
    Release,
    /// Restore an earlier version.
    Rollback,
    /// Change the gray percentage without changing the state.
    Gray(u16),
}

/// Plans the status transitions one command may cause.
///
/// @param current - current provider release state.
/// @param command - publisher command.
/// @returns the ordered target states, empty when the command does not change
/// the state, or an error when the command is not valid from `current`.
pub fn plan_command_transitions(
    current: ChannelReleaseStatus,
    command: &ChannelReleaseCommand,
) -> Result<Vec<ChannelReleaseStatus>, String> {
    match command {
        ChannelReleaseCommand::Upload => step(current, ChannelReleaseStatus::Uploaded),
        ChannelReleaseCommand::SubmitAudit => step(current, ChannelReleaseStatus::AuditSubmitted),
        ChannelReleaseCommand::QueryAudit(next) => step(current, *next),
        ChannelReleaseCommand::Release => {
            let mut targets = step(current, ChannelReleaseStatus::Releasing)?;
            targets.extend(step(
                ChannelReleaseStatus::Releasing,
                ChannelReleaseStatus::Released,
            )?);
            Ok(targets)
        }
        ChannelReleaseCommand::Rollback => step(current, ChannelReleaseStatus::RolledBack),
        ChannelReleaseCommand::Gray(percentage) => {
            if *percentage > 100 {
                return Err("grayPercentage must be between 0 and 100".to_string());
            }
            if current != ChannelReleaseStatus::Released {
                return Err(format!(
                    "gray release requires a released version, current state is {}",
                    current.as_str()
                ));
            }
            Ok(Vec::new())
        }
    }
}

fn step(
    current: ChannelReleaseStatus,
    next: ChannelReleaseStatus,
) -> Result<Vec<ChannelReleaseStatus>, String> {
    validate_channel_release_transition(current, next)?;
    Ok(vec![next])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_requires_a_projected_release() {
        assert_eq!(
            plan_command_transitions(
                ChannelReleaseStatus::Projected,
                &ChannelReleaseCommand::Upload
            )
            .expect("plan"),
            vec![ChannelReleaseStatus::Uploaded]
        );
        assert!(plan_command_transitions(
            ChannelReleaseStatus::Draft,
            &ChannelReleaseCommand::Upload
        )
        .is_err());
    }

    #[test]
    fn release_is_two_ordered_transitions() {
        assert_eq!(
            plan_command_transitions(
                ChannelReleaseStatus::Approved,
                &ChannelReleaseCommand::Release
            )
            .expect("plan"),
            vec![
                ChannelReleaseStatus::Releasing,
                ChannelReleaseStatus::Released
            ]
        );
        assert!(plan_command_transitions(
            ChannelReleaseStatus::InReview,
            &ChannelReleaseCommand::Release
        )
        .is_err());
    }

    #[test]
    fn query_audit_only_accepts_reachable_states() {
        assert_eq!(
            plan_command_transitions(
                ChannelReleaseStatus::InReview,
                &ChannelReleaseCommand::QueryAudit(ChannelReleaseStatus::Approved)
            )
            .expect("plan"),
            vec![ChannelReleaseStatus::Approved]
        );
        assert!(plan_command_transitions(
            ChannelReleaseStatus::Uploaded,
            &ChannelReleaseCommand::QueryAudit(ChannelReleaseStatus::Approved)
        )
        .is_err());
    }

    #[test]
    fn rollback_requires_a_released_version() {
        assert_eq!(
            plan_command_transitions(
                ChannelReleaseStatus::Released,
                &ChannelReleaseCommand::Rollback
            )
            .expect("plan"),
            vec![ChannelReleaseStatus::RolledBack]
        );
        assert!(plan_command_transitions(
            ChannelReleaseStatus::Approved,
            &ChannelReleaseCommand::Rollback
        )
        .is_err());
    }

    #[test]
    fn gray_release_keeps_the_state_but_bounds_the_percentage() {
        assert_eq!(
            plan_command_transitions(
                ChannelReleaseStatus::Released,
                &ChannelReleaseCommand::Gray(25)
            )
            .expect("plan"),
            Vec::<ChannelReleaseStatus>::new()
        );
        assert!(plan_command_transitions(
            ChannelReleaseStatus::Released,
            &ChannelReleaseCommand::Gray(101)
        )
        .is_err());
        assert!(plan_command_transitions(
            ChannelReleaseStatus::InReview,
            &ChannelReleaseCommand::Gray(50)
        )
        .is_err());
    }

    #[tokio::test]
    async fn unavailable_provider_fails_closed() {
        let provider = UnavailableChannelReleaseProvider;
        let error = provider
            .upload_mini_program(&MiniProgramUploadRequest {
                authorization_id: "auth-1".to_string(),
                environment: "production".to_string(),
                template_id: None,
                provider_version: None,
                ext_json: None,
            })
            .await
            .expect_err("must fail closed");
        assert!(format!("{error}").contains("not configured"));
    }
}

#[async_trait]
impl<T: ChannelReleaseProvider + ?Sized> ChannelReleaseProvider for std::sync::Arc<T> {
    async fn upload_mini_program(
        &self,
        request: &MiniProgramUploadRequest,
    ) -> DeployServiceResult<MiniProgramUploadOutcome> {
        (**self).upload_mini_program(request).await
    }

    async fn submit_mini_program_audit(
        &self,
        request: &MiniProgramAuditSubmission,
    ) -> DeployServiceResult<MiniProgramAuditOutcome> {
        (**self).submit_mini_program_audit(request).await
    }

    async fn query_mini_program_audit(
        &self,
        authorization_id: &str,
        audit_id: &str,
    ) -> DeployServiceResult<MiniProgramAuditObservation> {
        (**self)
            .query_mini_program_audit(authorization_id, audit_id)
            .await
    }

    async fn release_mini_program(
        &self,
        request: &MiniProgramReleaseRequest,
    ) -> DeployServiceResult<MiniProgramReleaseOutcome> {
        (**self).release_mini_program(request).await
    }

    async fn rollback_mini_program(
        &self,
        request: &MiniProgramRollbackRequest,
    ) -> DeployServiceResult<()> {
        (**self).rollback_mini_program(request).await
    }

    async fn gray_release_mini_program(
        &self,
        request: &MiniProgramGrayReleaseRequest,
    ) -> DeployServiceResult<()> {
        (**self).gray_release_mini_program(request).await
    }

    async fn modify_mini_program_domains(
        &self,
        authorization_id: &str,
        action: &str,
        domains: &[String],
    ) -> DeployServiceResult<()> {
        (**self)
            .modify_mini_program_domains(authorization_id, action, domains)
            .await
    }

    async fn set_mini_program_privacy(
        &self,
        authorization_id: &str,
        privacy: serde_json::Value,
    ) -> DeployServiceResult<()> {
        (**self)
            .set_mini_program_privacy(authorization_id, privacy)
            .await
    }
}
