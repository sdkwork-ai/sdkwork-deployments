//! Channel release contract: provider-side publishing for mini programs and
//! official accounts.
//!
//! A `deploy_release` describes what Deploy built. A channel release describes
//! what was pushed to the provider: the template/commit uploaded, the review
//! submitted, the version released, and any rollback. Provider state is always
//! an observation: WeChat owns the review decision, so this state machine only
//! accepts transitions the provider can actually produce and never infers a
//! verdict.
use serde::{Deserialize, Serialize};

/// How the channel account's credentials are obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelAccessMode {
    /// The customer owns the AppID/AppSecret or CI upload key.
    SelfManaged,
    /// The customer authorized a component (third-party platform).
    OperatorAuthorized,
    /// The same authorization flow, component owned by an external provider.
    ExternalProvider,
}

impl ChannelAccessMode {
    /// Stable wire/storage value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SelfManaged => "self_managed",
            Self::OperatorAuthorized => "operator_authorized",
            Self::ExternalProvider => "external_provider",
        }
    }

    /// Parses the wire/storage value.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value.trim() {
            "self_managed" => Some(Self::SelfManaged),
            "operator_authorized" => Some(Self::OperatorAuthorized),
            "external_provider" => Some(Self::ExternalProvider),
            _ => None,
        }
    }

    /// True when publishing goes through a component authorization.
    pub fn requires_component_authorization(self) -> bool {
        !matches!(self, Self::SelfManaged)
    }
}

/// The provider surface a channel release publishes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    /// WeChat mini program.
    WechatMiniProgram,
    /// WeChat official account.
    WechatOfficialAccount,
}

impl ChannelKind {
    /// Stable wire/storage value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WechatMiniProgram => "wechat_mini_program",
            Self::WechatOfficialAccount => "wechat_official_account",
        }
    }

    /// Parses the wire/storage value.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value.trim() {
            "wechat_mini_program" => Some(Self::WechatMiniProgram),
            "wechat_official_account" => Some(Self::WechatOfficialAccount),
            _ => None,
        }
    }
}

/// One provider release attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChannelReleaseStatus {
    /// Created, nothing pushed yet.
    Draft,
    /// The build artifact was projected into a provider-buildable tree.
    Projected,
    /// Code uploaded to the provider (commit / tester build).
    Uploaded,
    /// Review package submitted.
    AuditSubmitted,
    /// Provider is reviewing.
    InReview,
    /// Provider approved the version.
    Approved,
    /// Release command accepted.
    Releasing,
    /// Live on the provider.
    Released,
    /// Provider rejected the review.
    Rejected,
    /// Submission withdrawn before a decision.
    Withdrawn,
    /// Provider deferred the decision; it may return to review.
    Deferred,
    /// Upload, review, or release failed.
    Failed,
    /// An earlier released version was restored.
    RolledBack,
    /// A newer release replaced this one.
    Superseded,
}

impl ChannelReleaseStatus {
    /// Stable wire/storage value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Projected => "PROJECTED",
            Self::Uploaded => "UPLOADED",
            Self::AuditSubmitted => "AUDIT_SUBMITTED",
            Self::InReview => "IN_REVIEW",
            Self::Approved => "APPROVED",
            Self::Releasing => "RELEASING",
            Self::Released => "RELEASED",
            Self::Rejected => "REJECTED",
            Self::Withdrawn => "WITHDRAWN",
            Self::Deferred => "DEFERRED",
            Self::Failed => "FAILED",
            Self::RolledBack => "ROLLED_BACK",
            Self::Superseded => "SUPERSEDED",
        }
    }

    /// Parses the wire/storage value.
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value.trim() {
            "DRAFT" => Self::Draft,
            "PROJECTED" => Self::Projected,
            "UPLOADED" => Self::Uploaded,
            "AUDIT_SUBMITTED" => Self::AuditSubmitted,
            "IN_REVIEW" => Self::InReview,
            "APPROVED" => Self::Approved,
            "RELEASING" => Self::Releasing,
            "RELEASED" => Self::Released,
            "REJECTED" => Self::Rejected,
            "WITHDRAWN" => Self::Withdrawn,
            "DEFERRED" => Self::Deferred,
            "FAILED" => Self::Failed,
            "ROLLED_BACK" => Self::RolledBack,
            "SUPERSEDED" => Self::Superseded,
            _ => return None,
        })
    }

    /// True when no further transition is possible.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Rejected | Self::Withdrawn | Self::Failed | Self::RolledBack | Self::Superseded
        )
    }
}

/// All statuses, in lifecycle order.
pub const CHANNEL_RELEASE_STATUSES: [ChannelReleaseStatus; 14] = [
    ChannelReleaseStatus::Draft,
    ChannelReleaseStatus::Projected,
    ChannelReleaseStatus::Uploaded,
    ChannelReleaseStatus::AuditSubmitted,
    ChannelReleaseStatus::InReview,
    ChannelReleaseStatus::Approved,
    ChannelReleaseStatus::Releasing,
    ChannelReleaseStatus::Released,
    ChannelReleaseStatus::Rejected,
    ChannelReleaseStatus::Withdrawn,
    ChannelReleaseStatus::Deferred,
    ChannelReleaseStatus::Failed,
    ChannelReleaseStatus::RolledBack,
    ChannelReleaseStatus::Superseded,
];

/// Validates one provider release transition.
///
/// The provider is the authority: `Approved` is only reachable from review, and
/// a terminal state never advances. `Deferred` may return to review because
/// WeChat can resume a postponed audit.
pub fn validate_channel_release_transition(
    current: ChannelReleaseStatus,
    next: ChannelReleaseStatus,
) -> Result<(), String> {
    if current.is_terminal() {
        return Err(format!(
            "channel release is terminal at {} and cannot move to {}",
            current.as_str(),
            next.as_str()
        ));
    }
    let allowed = matches!(
        (current, next),
        (ChannelReleaseStatus::Draft, ChannelReleaseStatus::Projected)
            | (ChannelReleaseStatus::Draft, ChannelReleaseStatus::Failed)
            | (
                ChannelReleaseStatus::Projected,
                ChannelReleaseStatus::Uploaded
            )
            | (
                ChannelReleaseStatus::Projected,
                ChannelReleaseStatus::Failed
            )
            | (
                ChannelReleaseStatus::Uploaded,
                ChannelReleaseStatus::AuditSubmitted
            )
            | (ChannelReleaseStatus::Uploaded, ChannelReleaseStatus::Failed)
            | (
                ChannelReleaseStatus::AuditSubmitted,
                ChannelReleaseStatus::InReview
            )
            | (
                ChannelReleaseStatus::AuditSubmitted,
                ChannelReleaseStatus::Rejected
            )
            | (
                ChannelReleaseStatus::AuditSubmitted,
                ChannelReleaseStatus::Withdrawn
            )
            | (
                ChannelReleaseStatus::AuditSubmitted,
                ChannelReleaseStatus::Deferred
            )
            | (
                ChannelReleaseStatus::AuditSubmitted,
                ChannelReleaseStatus::Failed
            )
            | (
                ChannelReleaseStatus::InReview,
                ChannelReleaseStatus::Approved
            )
            | (
                ChannelReleaseStatus::InReview,
                ChannelReleaseStatus::Rejected
            )
            | (
                ChannelReleaseStatus::InReview,
                ChannelReleaseStatus::Withdrawn
            )
            | (
                ChannelReleaseStatus::InReview,
                ChannelReleaseStatus::Deferred
            )
            | (ChannelReleaseStatus::InReview, ChannelReleaseStatus::Failed)
            | (
                ChannelReleaseStatus::Deferred,
                ChannelReleaseStatus::InReview
            )
            | (
                ChannelReleaseStatus::Deferred,
                ChannelReleaseStatus::Withdrawn
            )
            | (ChannelReleaseStatus::Deferred, ChannelReleaseStatus::Failed)
            | (
                ChannelReleaseStatus::Approved,
                ChannelReleaseStatus::Releasing
            )
            | (ChannelReleaseStatus::Approved, ChannelReleaseStatus::Failed)
            | (
                ChannelReleaseStatus::Releasing,
                ChannelReleaseStatus::Released
            )
            | (
                ChannelReleaseStatus::Releasing,
                ChannelReleaseStatus::Failed
            )
            | (
                ChannelReleaseStatus::Released,
                ChannelReleaseStatus::RolledBack
            )
            | (
                ChannelReleaseStatus::Released,
                ChannelReleaseStatus::Superseded
            )
    );
    if allowed {
        Ok(())
    } else {
        Err(format!(
            "invalid channel release transition {} -> {}",
            current.as_str(),
            next.as_str()
        ))
    }
}

/// Maps WeChat's `get_auditstatus` code to the provider release state.
///
/// Codes are the documented `0..=4`; anything else is not an audit verdict and
/// must be treated as an unknown provider answer rather than guessed.
pub fn wechat_audit_status_to_release(code: i64) -> Option<ChannelReleaseStatus> {
    match code {
        0 => Some(ChannelReleaseStatus::Approved),
        1 => Some(ChannelReleaseStatus::Rejected),
        2 => Some(ChannelReleaseStatus::InReview),
        3 => Some(ChannelReleaseStatus::Withdrawn),
        4 => Some(ChannelReleaseStatus::Deferred),
        _ => None,
    }
}

/// Creates one channel release attempt.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateChannelReleaseRequest {
    /// Fleet lifecycle environment key.
    pub environment: String,
    /// `deploy_application_resource_link` row id.
    #[serde(rename = "resourceLinkId")]
    pub resource_link_id: String,
    /// Provider surface.
    pub channel: ChannelKind,
    /// Credential mode.
    #[serde(rename = "accessMode")]
    pub access_mode: ChannelAccessMode,
    /// WeChat template id for the component path.
    #[serde(rename = "templateId")]
    pub template_id: Option<String>,
    /// Version shown to users.
    #[serde(rename = "providerVersion")]
    pub provider_version: Option<String>,
    /// `ext_json` overrides for the component path.
    #[serde(rename = "extJson")]
    pub ext_json: Option<serde_json::Value>,
}

/// Grayscale release request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GrayChannelReleaseRequest {
    /// Percentage of users, `0..=100`.
    #[serde(rename = "grayPercentage")]
    pub gray_percentage: u16,
}

impl GrayChannelReleaseRequest {
    /// Rejects values WeChat cannot express.
    pub fn validate(&self) -> Result<(), String> {
        if self.gray_percentage > 100 {
            return Err("grayPercentage must be between 0 and 100".to_string());
        }
        Ok(())
    }
}

/// Rollback request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RollbackChannelReleaseRequest {
    /// Provider version to restore.
    #[serde(rename = "targetVersion")]
    pub target_version: String,
}

/// One channel release as returned by the API.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelReleaseResponse {
    /// Release row id.
    pub id: String,
    /// Deploy application id.
    #[serde(rename = "appId")]
    pub app_id: String,
    /// Fleet lifecycle environment key.
    pub environment: String,
    /// Provider surface.
    pub channel: ChannelKind,
    /// Credential mode.
    #[serde(rename = "accessMode")]
    pub access_mode: ChannelAccessMode,
    /// User-facing provider version.
    #[serde(rename = "providerVersion")]
    pub provider_version: Option<String>,
    /// WeChat template id.
    #[serde(rename = "templateId")]
    pub template_id: Option<String>,
    /// Uploaded commit id.
    #[serde(rename = "commitId")]
    pub commit_id: Option<String>,
    /// Submitted audit id.
    #[serde(rename = "auditId")]
    pub audit_id: Option<String>,
    /// Current status.
    #[serde(rename = "releaseStatus")]
    pub release_status: ChannelReleaseStatus,
    /// Active gray percentage.
    #[serde(rename = "grayPercentage")]
    pub gray_percentage: u16,
    /// Release this row rolled back from.
    #[serde(rename = "rollbackOfReleaseId")]
    pub rollback_of_release_id: Option<String>,
    /// Provider error code, stable for support.
    #[serde(rename = "errorCode")]
    pub error_code: Option<String>,
    /// Submission time, RFC 3339.
    #[serde(rename = "submittedAt")]
    pub submitted_at: Option<String>,
    /// Release time, RFC 3339.
    #[serde(rename = "releasedAt")]
    pub released_at: Option<String>,
    /// Creation time, RFC 3339.
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// Update time, RFC 3339.
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    /// Optimistic concurrency version, decimal string on the wire.
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_reaches_released() {
        let path = [
            (ChannelReleaseStatus::Draft, ChannelReleaseStatus::Projected),
            (
                ChannelReleaseStatus::Projected,
                ChannelReleaseStatus::Uploaded,
            ),
            (
                ChannelReleaseStatus::Uploaded,
                ChannelReleaseStatus::AuditSubmitted,
            ),
            (
                ChannelReleaseStatus::AuditSubmitted,
                ChannelReleaseStatus::InReview,
            ),
            (
                ChannelReleaseStatus::InReview,
                ChannelReleaseStatus::Approved,
            ),
            (
                ChannelReleaseStatus::Approved,
                ChannelReleaseStatus::Releasing,
            ),
            (
                ChannelReleaseStatus::Releasing,
                ChannelReleaseStatus::Released,
            ),
        ];
        for (current, next) in path {
            assert!(
                validate_channel_release_transition(current, next).is_ok(),
                "{} -> {} must be allowed",
                current.as_str(),
                next.as_str()
            );
        }
    }

    #[test]
    fn rejects_shortcuts_and_backwards_moves() {
        assert!(validate_channel_release_transition(
            ChannelReleaseStatus::Draft,
            ChannelReleaseStatus::Released
        )
        .is_err());
        assert!(validate_channel_release_transition(
            ChannelReleaseStatus::Approved,
            ChannelReleaseStatus::Uploaded
        )
        .is_err());
        assert!(validate_channel_release_transition(
            ChannelReleaseStatus::Released,
            ChannelReleaseStatus::InReview
        )
        .is_err());
        assert!(validate_channel_release_transition(
            ChannelReleaseStatus::Rejected,
            ChannelReleaseStatus::Approved
        )
        .is_err());
    }

    #[test]
    fn deferred_audit_may_resume() {
        assert!(validate_channel_release_transition(
            ChannelReleaseStatus::Deferred,
            ChannelReleaseStatus::InReview
        )
        .is_ok());
    }

    #[test]
    fn rollout_can_be_superseded_or_rolled_back() {
        assert!(validate_channel_release_transition(
            ChannelReleaseStatus::Released,
            ChannelReleaseStatus::Superseded
        )
        .is_ok());
        assert!(validate_channel_release_transition(
            ChannelReleaseStatus::Released,
            ChannelReleaseStatus::RolledBack
        )
        .is_ok());
    }

    #[test]
    fn maps_documented_wechat_audit_codes_only() {
        assert_eq!(
            wechat_audit_status_to_release(0),
            Some(ChannelReleaseStatus::Approved)
        );
        assert_eq!(
            wechat_audit_status_to_release(1),
            Some(ChannelReleaseStatus::Rejected)
        );
        assert_eq!(
            wechat_audit_status_to_release(2),
            Some(ChannelReleaseStatus::InReview)
        );
        assert_eq!(
            wechat_audit_status_to_release(3),
            Some(ChannelReleaseStatus::Withdrawn)
        );
        assert_eq!(
            wechat_audit_status_to_release(4),
            Some(ChannelReleaseStatus::Deferred)
        );
        assert_eq!(wechat_audit_status_to_release(9), None);
    }

    #[test]
    fn wire_values_round_trip() {
        for status in CHANNEL_RELEASE_STATUSES {
            assert_eq!(
                ChannelReleaseStatus::from_wire(status.as_str()),
                Some(status)
            );
        }
        for mode in [
            ChannelAccessMode::SelfManaged,
            ChannelAccessMode::OperatorAuthorized,
            ChannelAccessMode::ExternalProvider,
        ] {
            assert_eq!(ChannelAccessMode::from_wire(mode.as_str()), Some(mode));
        }
        for kind in [
            ChannelKind::WechatMiniProgram,
            ChannelKind::WechatOfficialAccount,
        ] {
            assert_eq!(ChannelKind::from_wire(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn gray_percentage_is_bounded() {
        assert!(GrayChannelReleaseRequest { gray_percentage: 0 }
            .validate()
            .is_ok());
        assert!(GrayChannelReleaseRequest {
            gray_percentage: 100
        }
        .validate()
        .is_ok());
        assert!(GrayChannelReleaseRequest {
            gray_percentage: 101
        }
        .validate()
        .is_err());
    }
}
