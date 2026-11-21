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

/// Advances one channel release to a new observed state.
///
/// `expected_version` is the optimistic-concurrency version read from the
/// previous response; a stale value is rejected instead of overwriting a newer
/// provider observation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransitionChannelReleaseRequest {
    /// Optimistic version the caller read, decimal string on the wire.
    #[serde(rename = "expectedVersion")]
    pub expected_version: String,
    /// Target state.
    #[serde(rename = "releaseStatus")]
    pub release_status: ChannelReleaseStatus,
    /// Provider commit id, when the transition records an upload.
    #[serde(rename = "commitId", skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
    /// Provider audit id, when the transition records a submission.
    #[serde(rename = "auditId", skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<String>,
    /// Stable provider error code, when the transition records a failure.
    #[serde(rename = "errorCode", skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Upload action for a publish command.
pub const CHANNEL_RELEASE_ACTION_UPLOAD: &str = "upload";
/// Submit-review action for a publish command.
pub const CHANNEL_RELEASE_ACTION_SUBMIT_AUDIT: &str = "submit_audit";
/// Publish action for a publish command.
pub const CHANNEL_RELEASE_ACTION_RELEASE: &str = "release";
/// Rollback action for a publish command.
pub const CHANNEL_RELEASE_ACTION_ROLLBACK: &str = "rollback";
/// Gray-rollout action for a publish command.
pub const CHANNEL_RELEASE_ACTION_GRAY: &str = "gray";
/// Server-domain edit action for a publish command.
pub const CHANNEL_RELEASE_ACTION_MODIFY_DOMAINS: &str = "modify_domains";
/// Privacy-declaration action for a publish command.
pub const CHANNEL_RELEASE_ACTION_SET_PRIVACY: &str = "set_privacy";

/// One publish step for a channel release.
///
/// A single command surface keeps the five provider steps on the same
/// authorization, version, and audit path instead of five near-identical
/// endpoints that would each need their own state guards.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelReleasePublishCommandRequest {
    /// `upload` | `submit_audit` | `release` | `rollback` | `gray`.
    pub action: String,
    /// IAM resource authorization that owns the authorizer token.
    #[serde(rename = "authorizationId")]
    pub authorization_id: String,
    /// WeChat template id, required by `upload` when the release has none.
    #[serde(rename = "templateId", skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    /// User-facing version, required by `upload` when the release has none.
    #[serde(rename = "providerVersion", skip_serializing_if = "Option::is_none")]
    pub provider_version: Option<String>,
    /// `ext_json` overrides for the component path.
    #[serde(rename = "extJson", skip_serializing_if = "Option::is_none")]
    pub ext_json: Option<serde_json::Value>,
    /// Version to restore on `rollback`.
    #[serde(rename = "targetVersion", skip_serializing_if = "Option::is_none")]
    pub target_version: Option<String>,
    /// Percentage `0..=100` on `gray`.
    #[serde(rename = "grayPercentage", skip_serializing_if = "Option::is_none")]
    pub gray_percentage: Option<u16>,
    /// WeChat review item list, required by `submit_audit`.
    #[serde(rename = "items", skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<serde_json::Value>>,
    /// Domain action (`add` / `set` / `delete`), required by `modify_domains`.
    #[serde(rename = "domainAction", skip_serializing_if = "Option::is_none")]
    pub domain_action: Option<String>,
    /// Request domains, required by `modify_domains`.
    #[serde(rename = "domains", skip_serializing_if = "Option::is_none")]
    pub domains: Option<Vec<String>>,
    /// Privacy declaration, required by `set_privacy`.
    #[serde(rename = "privacy", skip_serializing_if = "Option::is_none")]
    pub privacy: Option<serde_json::Value>,
    /// WeChat AppID, required by a direct-mode (`self_managed`) upload.
    #[serde(rename = "providerAppId", skip_serializing_if = "Option::is_none")]
    pub provider_app_id: Option<String>,
    /// Project directory, required by a direct-mode upload.
    #[serde(rename = "projectDir", skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
    /// Upload private key path, required by a direct-mode upload.
    #[serde(rename = "privateKeyPath", skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<String>,
    /// Optional CI robot number, `1..=30`.
    #[serde(rename = "robot", skip_serializing_if = "Option::is_none")]
    pub robot: Option<u8>,
}

impl ChannelReleasePublishCommandRequest {
    /// Rejects an action or field set the publisher cannot apply.
    pub fn validate(&self) -> Result<(), String> {
        if self.authorization_id.trim().is_empty() {
            return Err("authorizationId is required".to_string());
        }
        match self.action.trim() {
            CHANNEL_RELEASE_ACTION_UPLOAD
            | CHANNEL_RELEASE_ACTION_SUBMIT_AUDIT
            | CHANNEL_RELEASE_ACTION_RELEASE => Ok(()),
            CHANNEL_RELEASE_ACTION_ROLLBACK => {
                if self
                    .target_version
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    return Err("targetVersion is required for a rollback".to_string());
                }
                Ok(())
            }
            CHANNEL_RELEASE_ACTION_GRAY => match self.gray_percentage {
                Some(value) if value <= 100 => Ok(()),
                Some(_) => Err("grayPercentage must be between 0 and 100".to_string()),
                None => Err("grayPercentage is required for a gray rollout".to_string()),
            },
            CHANNEL_RELEASE_ACTION_MODIFY_DOMAINS => {
                let action = self
                    .domain_action
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| matches!(*value, "add" | "set" | "delete"))
                    .ok_or_else(|| "domainAction must be add, set, or delete".to_string())?;
                let _ = action;
                match self.domains.as_ref() {
                    Some(domains) if !domains.is_empty() => Ok(()),
                    _ => Err("domains is required for a domain edit".to_string()),
                }
            }
            CHANNEL_RELEASE_ACTION_SET_PRIVACY => match self.privacy.as_ref() {
                Some(privacy) if privacy.is_object() => Ok(()),
                Some(_) => Err("privacy must be a JSON object".to_string()),
                None => Err("privacy is required for a privacy update".to_string()),
            },
            other => Err(format!("unknown channel release action {other}")),
        }
    }
}

impl TransitionChannelReleaseRequest {
    /// Rejects a body the repository cannot apply.
    pub fn validate(&self) -> Result<i64, String> {
        let expected_version = self
            .expected_version
            .trim()
            .parse::<i64>()
            .map_err(|_| "expectedVersion must be a decimal integer".to_string())?;
        if expected_version < 1 {
            return Err("expectedVersion must be positive".to_string());
        }
        Ok(expected_version)
    }
}

/// Plans a direct-mode (`self_managed`) mini-program upload.
///
/// The customer keeps their own WeChat account and upload key; Deploy returns
/// the exact `miniprogram-ci` invocation and never reads the key itself.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateChannelDirectUploadPlanRequest {
    /// WeChat AppID of the customer's mini program.
    #[serde(rename = "providerAppId")]
    pub provider_app_id: String,
    /// Path to the projected mini-program project.
    #[serde(rename = "projectDir")]
    pub project_dir: String,
    /// Path to the customer's upload private key.
    #[serde(rename = "privateKeyPath")]
    pub private_key_path: String,
    /// Version shown to users.
    pub version: String,
    /// Upload description (build identity).
    pub description: String,
    /// Optional CI robot number, `1..=30`.
    pub robot: Option<u8>,
}

/// The deterministic direct-mode upload invocation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelDirectUploadPlan {
    /// Executable name.
    pub program: String,
    /// Ordered argument vector.
    pub args: Vec<String>,
}

/// One page of channel releases.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelReleasePage {
    /// Page items, newest first.
    pub items: Vec<ChannelReleaseResponse>,
    /// Total rows for the filtered scope.
    pub total: i64,
    /// 1-based page number.
    pub page: i32,
    /// Requested page size.
    #[serde(rename = "pageSize")]
    pub page_size: i32,
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
    fn transition_request_requires_a_positive_decimal_version() {
        let valid = TransitionChannelReleaseRequest {
            expected_version: "7".to_string(),
            release_status: ChannelReleaseStatus::Uploaded,
            commit_id: None,
            audit_id: None,
            error_code: None,
        };
        assert_eq!(valid.validate(), Ok(7));
        for invalid in ["", "abc", "0", "-1", "1.5"] {
            let request = TransitionChannelReleaseRequest {
                expected_version: invalid.to_string(),
                release_status: ChannelReleaseStatus::Uploaded,
                commit_id: None,
                audit_id: None,
                error_code: None,
            };
            assert!(request.validate().is_err(), "{invalid} must be rejected");
        }
    }

    #[test]
    fn publish_command_requires_action_specific_fields() {
        let base = ChannelReleasePublishCommandRequest {
            action: CHANNEL_RELEASE_ACTION_UPLOAD.to_string(),
            authorization_id: "auth-1".to_string(),
            template_id: None,
            provider_version: None,
            ext_json: None,
            target_version: None,
            gray_percentage: None,
            items: None,
            domain_action: None,
            domains: None,
            privacy: None,
            provider_app_id: None,
            project_dir: None,
            private_key_path: None,
            robot: None,
        };
        assert!(base.validate().is_ok());
        let missing_authorization = ChannelReleasePublishCommandRequest {
            authorization_id: String::new(),
            ..base.clone()
        };
        assert!(missing_authorization.validate().is_err());

        let rollback = ChannelReleasePublishCommandRequest {
            action: CHANNEL_RELEASE_ACTION_ROLLBACK.to_string(),
            ..base.clone()
        };
        assert!(rollback.validate().is_err());
        let rollback_with_target = ChannelReleasePublishCommandRequest {
            target_version: Some("0.9.0".to_string()),
            ..rollback.clone()
        };
        assert!(rollback_with_target.validate().is_ok());

        let gray_without_percentage = ChannelReleasePublishCommandRequest {
            action: CHANNEL_RELEASE_ACTION_GRAY.to_string(),
            ..base.clone()
        };
        assert!(gray_without_percentage.validate().is_err());
        let gray_out_of_range = ChannelReleasePublishCommandRequest {
            gray_percentage: Some(101),
            ..gray_without_percentage.clone()
        };
        assert!(gray_out_of_range.validate().is_err());
        let gray_ok = ChannelReleasePublishCommandRequest {
            gray_percentage: Some(30),
            ..gray_without_percentage
        };
        assert!(gray_ok.validate().is_ok());

        let unknown = ChannelReleasePublishCommandRequest {
            action: "delete_everything".to_string(),
            ..base
        };
        assert!(unknown.validate().is_err());
    }

    #[test]
    fn domain_and_privacy_actions_validate_their_fields() {
        let base = ChannelReleasePublishCommandRequest {
            action: CHANNEL_RELEASE_ACTION_MODIFY_DOMAINS.to_string(),
            authorization_id: "auth-1".to_string(),
            template_id: None,
            provider_version: None,
            ext_json: None,
            target_version: None,
            gray_percentage: None,
            items: None,
            domain_action: None,
            domains: None,
            privacy: None,
            provider_app_id: None,
            project_dir: None,
            private_key_path: None,
            robot: None,
        };
        assert!(base.validate().is_err());
        let with_domains = ChannelReleasePublishCommandRequest {
            domain_action: Some("add".to_string()),
            domains: Some(vec!["https://api.example.com".to_string()]),
            ..base.clone()
        };
        assert!(with_domains.validate().is_ok());
        let bad_action = ChannelReleasePublishCommandRequest {
            domain_action: Some("replace".to_string()),
            ..with_domains.clone()
        };
        assert!(bad_action.validate().is_err());

        let privacy = ChannelReleasePublishCommandRequest {
            action: CHANNEL_RELEASE_ACTION_SET_PRIVACY.to_string(),
            ..base
        };
        assert!(privacy.validate().is_err());
        let privacy_object = ChannelReleasePublishCommandRequest {
            privacy: Some(serde_json::json!({"privacy_ver": 1})),
            ..privacy.clone()
        };
        assert!(privacy_object.validate().is_ok());
        let privacy_scalar = ChannelReleasePublishCommandRequest {
            privacy: Some(serde_json::json!("text")),
            ..privacy
        };
        assert!(privacy_scalar.validate().is_err());
    }

    #[test]
    fn direct_upload_plan_request_round_trips() {
        let request = CreateChannelDirectUploadPlanRequest {
            provider_app_id: "wx123".to_string(),
            project_dir: "/srv/apps/demo".to_string(),
            private_key_path: "/run/secrets/ci.key".to_string(),
            version: "1.0.0".to_string(),
            description: "build 42".to_string(),
            robot: Some(7),
        };
        let encoded = serde_json::to_string(&request).expect("serialize");
        assert!(encoded.contains("\"providerAppId\":\"wx123\""));
        assert!(encoded.contains("\"robot\":7"));
        let decoded: CreateChannelDirectUploadPlanRequest =
            serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded.provider_app_id, "wx123");
        assert_eq!(decoded.robot, Some(7));
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
