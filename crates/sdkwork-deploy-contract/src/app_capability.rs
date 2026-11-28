//! Application **capability** document: the six configurable dimensions an
//! application exposes when it is opened as an AI workspace.
//!
//! # What this module owns
//!
//! * the wire shape of the capability document (`AppCapabilityResponse`) and of
//!   its write (`ReplaceAppCapabilityRequest`),
//! * the closed vocabularies that the DDL CHECK constraints also encode, so a
//!   value never has to be translated between DTO, SQL parameter and SDK,
//! * normalization: every rule the database enforces is checked once here so the
//!   operator sees a `40003` with a sentence instead of a masked SQLSTATE.
//!
//! # Why the write is a whole-document replace
//!
//! The six dimensions are one document with internal references (a default work
//! mode key, scene-to-asset bindings, market shelves). Editing them row by row
//! passes through states that are not merely incomplete but invalid — delete the
//! default mode and, until the replacement is created, the application has a
//! default pointer to nothing. `PUT .../capability` therefore replaces the whole
//! document in one transaction, which is also the shape WorkBuddy's export/import
//! configuration file has.
//!
//! # Localized text
//!
//! Every user-facing string is a `locale → text` map ([`LocalizedText`]). It is
//! persisted as rows in `deploy_app_capability_text`, keyed by the document node's
//! stable business key, so adding a locale never requires a schema change.
//! `I18N_SPEC.md` requires a real locale tag; `zh` alone is normalized to
//! `zh-CN` on write because the platform's default locale pair is `zh-CN`/`en-US`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::problem::{DeployServiceError, DeployServiceResult};

/// Locale → text. Ordered so the serialized document is stable.
pub type LocalizedText = BTreeMap<String, String>;

/// Schema version written into every capability revision snapshot.
///
/// The snapshot is a JSONB document, so a future field addition is a read-time
/// upgrade rather than a migration; this version is what makes that possible.
pub const APP_CAPABILITY_SCHEMA_VERSION: i32 = 1;

/// Lifecycle of the capability document.
///
/// The vocabulary is the platform's (`APPLICATION_IDENTITY_SPEC.md` §5):
/// `draft → pending_review → approved → published`, plus `deprecated` and
/// `retired`. A rejected review returns the document to `draft`; the rejection
/// itself lives on the review row, not on the document, so the document never
/// claims a state the platform vocabulary does not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Draft,
    PendingReview,
    Approved,
    Published,
    Deprecated,
    Retired,
}

impl CapabilityStatus {
    pub const ALL: [Self; 6] = [
        Self::Draft,
        Self::PendingReview,
        Self::Approved,
        Self::Published,
        Self::Deprecated,
        Self::Retired,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::PendingReview => "pending_review",
            Self::Approved => "approved",
            Self::Published => "published",
            Self::Deprecated => "deprecated",
            Self::Retired => "retired",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == value)
    }

    /// Whether the document may be replaced in this state.
    ///
    /// A published document is editable (WorkBuddy: "已发布应用如需变更，修改后需重新
    /// 提交审核"); editing it moves the status back to `draft` and keeps
    /// `published_revision_id`, so the live configuration stays live while the next
    /// version is prepared. `retired` is terminal.
    pub const fn allows_edit(self) -> bool {
        !matches!(self, Self::Retired)
    }
}

/// Who may discover and open the application.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionScope {
    /// Creator only.
    Private,
    /// Members of the narrowing organization.
    Organization,
    /// Every member of the owning tenant.
    Tenant,
    /// Only enterprises holding an active enterprise order (owned by sdkwork-iam).
    Enterprise,
    /// Publicly discoverable.
    Public,
}

impl DistributionScope {
    pub const ALL: [Self; 5] = [
        Self::Private,
        Self::Organization,
        Self::Tenant,
        Self::Enterprise,
        Self::Public,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Organization => "organization",
            Self::Tenant => "tenant",
            Self::Enterprise => "enterprise",
            Self::Public => "public",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == value)
    }
}

/// What kind of ecosystem asset a scene or a market shelf entry references.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityAssetKind {
    Expert,
    ExpertTeam,
    Skill,
    Connector,
    /// Inspiration case / best-practice showcase.
    Case,
}

impl CapabilityAssetKind {
    pub const ALL: [Self; 5] = [
        Self::Expert,
        Self::ExpertTeam,
        Self::Skill,
        Self::Connector,
        Self::Case,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expert => "EXPERT",
            Self::ExpertTeam => "EXPERT_TEAM",
            Self::Skill => "SKILL",
            Self::Connector => "CONNECTOR",
            Self::Case => "CASE",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == value)
    }

    /// The module that owns this asset kind. The pairing is closed and the
    /// database enforces it (`chk_deploy_app_scene_asset_module`); deriving it
    /// here means the wire never carries a module the caller could get wrong.
    pub const fn source_module(self) -> &'static str {
        match self {
            Self::Expert | Self::ExpertTeam => "agents",
            Self::Skill => "skills",
            Self::Connector => "mcp",
            Self::Case => "appstore",
        }
    }
}

/// In-app market shelf.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityMarketSection {
    Experts,
    Skills,
    Connectors,
    FeaturedScenes,
    Cases,
}

impl CapabilityMarketSection {
    pub const ALL: [Self; 5] = [
        Self::Experts,
        Self::Skills,
        Self::Connectors,
        Self::FeaturedScenes,
        Self::Cases,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Experts => "experts",
            Self::Skills => "skills",
            Self::Connectors => "connectors",
            Self::FeaturedScenes => "featured_scenes",
            Self::Cases => "cases",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == value)
    }

    /// Which asset kinds this shelf may list.
    ///
    /// The featured-scene slot is a *featured expert or expert team*, matching
    /// WorkBuddy's "精选场景（关联专家或专家团）"; it is not a scene capsule.
    pub const fn accepts(self, kind: CapabilityAssetKind) -> bool {
        match self {
            Self::Experts | Self::FeaturedScenes => {
                matches!(
                    kind,
                    CapabilityAssetKind::Expert | CapabilityAssetKind::ExpertTeam
                )
            }
            Self::Skills => matches!(kind, CapabilityAssetKind::Skill),
            Self::Connectors => matches!(kind, CapabilityAssetKind::Connector),
            Self::Cases => matches!(kind, CapabilityAssetKind::Case),
        }
    }
}

/// Tool kind pre-bound to a work mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityToolKind {
    /// A tool the host runtime provides.
    Builtin,
    /// An MCP tool, referenced by `server_key`/`connector_key`.
    Mcp,
}

impl CapabilityToolKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "BUILTIN",
            Self::Mcp => "MCP",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "BUILTIN" => Some(Self::Builtin),
            "MCP" => Some(Self::Mcp),
            _ => None,
        }
    }

    pub const fn source_module(self) -> &'static str {
        match self {
            Self::Builtin => "deploy",
            Self::Mcp => "mcp",
        }
    }
}

/// Whether a configurable row is in service.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRowStatus {
    #[default]
    Active,
    Disabled,
}

impl CapabilityRowStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

/// Embedded existing system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityEmbedKind {
    /// An MCP application the workspace can call.
    McpApp,
    /// An iframe surface it can show.
    Iframe,
}

impl CapabilityEmbedKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::McpApp => "MCP_APP",
            Self::Iframe => "IFRAME",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "MCP_APP" => Some(Self::McpApp),
            "IFRAME" => Some(Self::Iframe),
            _ => None,
        }
    }
}

/// MCP transport of an embedded server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityEmbedTransport {
    Stdio,
    Http,
    Sse,
}

impl CapabilityEmbedTransport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stdio" => Some(Self::Stdio),
            "http" => Some(Self::Http),
            "sse" => Some(Self::Sse),
            _ => None,
        }
    }
}

/// How the embed authenticates to the embedded system. Credentials themselves
/// belong to sdkwork-iam; this records only the mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityEmbedAuthMode {
    #[default]
    None,
    Oauth,
    ApiKey,
}

impl CapabilityEmbedAuthMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Oauth => "oauth",
            Self::ApiKey => "api_key",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "oauth" => Some(Self::Oauth),
            "api_key" => Some(Self::ApiKey),
            _ => None,
        }
    }
}

/// Review state of one submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityReviewStatus {
    Pending,
    Approved,
    Rejected,
    Withdrawn,
}

impl CapabilityReviewStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Withdrawn => "withdrawn",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "withdrawn" => Some(Self::Withdrawn),
            _ => None,
        }
    }
}

/// Why a submission was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDecisionCode {
    ContentPolicy,
    Security,
    Design,
    Functional,
    Other,
}

impl CapabilityDecisionCode {
    pub const ALL: [Self; 5] = [
        Self::ContentPolicy,
        Self::Security,
        Self::Design,
        Self::Functional,
        Self::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContentPolicy => "content_policy",
            Self::Security => "security",
            Self::Design => "design",
            Self::Functional => "functional",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == value)
    }
}

/// Review decision command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityReviewDecision {
    Approve,
    Reject,
}

// ---------------------------------------------------------------------------
// Document nodes
// ---------------------------------------------------------------------------

/// A tool pre-bound to a work mode.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkModeTool {
    pub kind: CapabilityToolKind,
    #[serde(rename = "ref")]
    pub tool_ref: String,
    #[serde(default = "empty_object")]
    pub policy: Value,
    #[serde(default)]
    pub sort_order: i32,
}

/// One working mode: the industry prompt plus its tools.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkModeDefinition {
    pub key: String,
    #[serde(default)]
    pub name: LocalizedText,
    #[serde(default)]
    pub description: LocalizedText,
    pub system_prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_ref: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub status: CapabilityRowStatus,
    #[serde(default)]
    pub tools: Vec<WorkModeTool>,
}

/// An ecosystem asset a scene starts.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneAsset {
    pub kind: CapabilityAssetKind,
    #[serde(rename = "ref")]
    pub asset_ref: String,
    #[serde(default = "empty_object")]
    pub snapshot: Value,
    #[serde(default)]
    pub sort_order: i32,
}

/// A scene capsule above the composer.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneDefinition {
    pub key: String,
    #[serde(default)]
    pub title: LocalizedText,
    #[serde(default)]
    pub subtitle: LocalizedText,
    pub prompt_template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_ref: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub is_featured: bool,
    #[serde(default)]
    pub status: CapabilityRowStatus,
    #[serde(default)]
    pub assets: Vec<SceneAsset>,
}

/// One selectable model.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelBindingDefinition {
    pub provider_code: String,
    pub model_key: String,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub status: CapabilityRowStatus,
}

/// One discoverable asset on the in-app market shelf.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketAssetDefinition {
    pub section: CapabilityMarketSection,
    pub kind: CapabilityAssetKind,
    #[serde(rename = "ref")]
    pub asset_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_code: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub is_featured: bool,
    #[serde(default)]
    pub status: CapabilityRowStatus,
    #[serde(default = "empty_object")]
    pub snapshot: Value,
}

/// An embedded existing system.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedDefinition {
    pub key: String,
    pub kind: CapabilityEmbedKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<CapabilityEmbedTransport>,
    #[serde(default)]
    pub auth_mode: CapabilityEmbedAuthMode,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub status: CapabilityRowStatus,
}

/// The root-level localized strings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRootTexts {
    #[serde(default)]
    pub slogan: LocalizedText,
    #[serde(default)]
    pub input_placeholder: LocalizedText,
    #[serde(default)]
    pub summary: LocalizedText,
}

/// The capability document as the API returns it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCapabilityResponse {
    pub app_id: String,
    pub status: CapabilityStatus,
    pub distribution_scope: DistributionScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// The IAM OAuth client (client_id) that serves this application.
    ///
    /// A string reference, never a foreign key: the client lives in
    /// iam_application_installation_credential, whose lifecycle this module does
    /// not own. It is how the capability document and the authorization server
    /// name the same client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_work_mode_key: Option<String>,
    pub config_schema_version: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_revision_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_revision_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// `None` when the application has never been configured; the caller may
    /// treat that as an empty draft document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texts: Option<CapabilityRootTexts>,
    #[serde(default)]
    pub work_modes: Vec<WorkModeDefinition>,
    #[serde(default)]
    pub scenes: Vec<SceneDefinition>,
    #[serde(default)]
    pub models: Vec<ModelBindingDefinition>,
    #[serde(default)]
    pub market: Vec<MarketAssetDefinition>,
    #[serde(default)]
    pub embeds: Vec<EmbedDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Optimistic-concurrency token (int64 on the wire per API_SPEC §13.6).
    pub version: String,
}

/// Whole-document replacement.
///
/// Every collection is replaced, not merged: an omitted collection means "this
/// dimension becomes empty". That is the semantics of a `PUT` and the only one
/// that lets an editor delete the last scene.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceAppCapabilityRequest {
    #[serde(default)]
    pub distribution_scope: Option<DistributionScope>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub oauth_client_id: Option<String>,
    #[serde(default)]
    pub default_work_mode_key: Option<String>,
    #[serde(default)]
    pub texts: CapabilityRootTexts,
    #[serde(default)]
    pub work_modes: Vec<WorkModeDefinition>,
    #[serde(default)]
    pub scenes: Vec<SceneDefinition>,
    #[serde(default)]
    pub models: Vec<ModelBindingDefinition>,
    #[serde(default)]
    pub market: Vec<MarketAssetDefinition>,
    #[serde(default)]
    pub embeds: Vec<EmbedDefinition>,
    /// Optional optimistic concurrency: the `version` the editor last read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<String>,
    /// Move the document to `pending_review` in the same transaction, producing a
    /// `REVIEW` revision. Equivalent to a `PUT` followed by `POST /reviews`, but
    /// atomic — an editor's "save and submit" must not leave a saved draft behind
    /// when the submission is refused.
    #[serde(default)]
    pub submit_for_review: bool,
}

/// One revision of the document.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRevisionResponse {
    pub id: String,
    pub app_id: String,
    pub revision_no: i32,
    /// `DRAFT` | `REVIEW` | `PUBLISH`.
    pub source: String,
    pub sha256: String,
    pub schema_version: i32,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Present only on the single-revision read; absent in lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Value>,
}

/// A page of revisions.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CapabilityRevisionPage {
    pub items: Vec<CapabilityRevisionResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

/// One review submission.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityReviewResponse {
    pub id: String,
    pub app_id: String,
    pub revision_id: String,
    pub attempt_no: i32,
    pub status: CapabilityReviewStatus,
    pub submitted_by: String,
    pub submitted_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_code: Option<CapabilityDecisionCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub version: String,
}

/// A page of reviews.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CapabilityReviewPage {
    pub items: Vec<CapabilityReviewResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

/// Submit the current revision for review.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitCapabilityReviewRequest {
    /// Optional note for the reviewer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Decide a pending review.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideCapabilityReviewRequest {
    pub decision: CapabilityReviewDecision,
    /// Required when rejecting: a rejection without a reason family is not
    /// actionable for the author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_code: Option<CapabilityDecisionCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_note: Option<String>,
}

impl DecideCapabilityReviewRequest {
    pub fn validate(&self) -> DeployServiceResult<()> {
        if self.decision == CapabilityReviewDecision::Reject && self.decision_code.is_none() {
            return Err(DeployServiceError::validation(
                "decisionCode is required when rejecting a capability review",
            ));
        }
        if let Some(note) = self.decision_note.as_deref() {
            if note.chars().count() > 2000 {
                return Err(DeployServiceError::validation(
                    "decisionNote must be at most 2000 characters",
                ));
            }
        }
        Ok(())
    }
}

/// List filters for reviews.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListCapabilityReviewsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<CapabilityReviewStatus>,
}

/// List filters for revisions.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListCapabilityRevisionsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// A localized string that passed normalization and is ready to be persisted as
/// rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedText {
    pub scope_kind: &'static str,
    pub scope_key: String,
    pub field_name: &'static str,
    pub locale: String,
    pub text_value: String,
}

/// The whole document, validated and flattened for the repository.
///
/// The repository receives exactly this: no re-validation, no second opinion
/// about what a valid key looks like.
#[derive(Clone, Debug, Default)]
pub struct NormalizedAppCapability {
    pub distribution_scope: &'static str,
    pub avatar_url: Option<String>,
    pub oauth_client_id: Option<String>,
    pub default_work_mode_key: Option<String>,
    pub texts: Vec<NormalizedText>,
    pub work_modes: Vec<NormalizedWorkMode>,
    pub scenes: Vec<NormalizedScene>,
    pub models: Vec<NormalizedModelBinding>,
    pub market: Vec<NormalizedMarketAsset>,
    pub embeds: Vec<NormalizedEmbed>,
    pub submit_for_review: bool,
    pub note: Option<String>,
    pub expected_version: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct NormalizedWorkMode {
    pub mode_key: String,
    pub system_prompt: String,
    pub icon_ref: Option<String>,
    pub sort_order: i32,
    pub is_default: bool,
    pub status: &'static str,
    pub texts: Vec<NormalizedText>,
    pub tools: Vec<NormalizedWorkModeTool>,
}

#[derive(Clone, Debug)]
pub struct NormalizedWorkModeTool {
    pub tool_kind: &'static str,
    pub tool_ref: String,
    pub source_module: &'static str,
    pub policy: Value,
    pub sort_order: i32,
}

#[derive(Clone, Debug)]
pub struct NormalizedScene {
    pub scene_key: String,
    pub prompt_template: String,
    pub icon_ref: Option<String>,
    pub sort_order: i32,
    pub is_featured: bool,
    pub status: &'static str,
    pub texts: Vec<NormalizedText>,
    pub assets: Vec<NormalizedSceneAsset>,
}

#[derive(Clone, Debug)]
pub struct NormalizedSceneAsset {
    pub asset_kind: &'static str,
    pub source_module: &'static str,
    pub asset_ref: String,
    pub snapshot: Value,
    pub sort_order: i32,
}

#[derive(Clone, Debug)]
pub struct NormalizedModelBinding {
    pub provider_code: String,
    pub model_key: String,
    pub sort_order: i32,
    pub is_default: bool,
    pub status: &'static str,
}

#[derive(Clone, Debug)]
pub struct NormalizedMarketAsset {
    pub section: &'static str,
    pub asset_kind: &'static str,
    pub source_module: &'static str,
    pub asset_ref: String,
    pub category_code: Option<String>,
    pub sort_order: i32,
    pub is_featured: bool,
    pub status: &'static str,
    pub snapshot: Value,
}

#[derive(Clone, Debug)]
pub struct NormalizedEmbed {
    pub embed_key: String,
    pub embed_kind: &'static str,
    pub server_ref: Option<String>,
    pub endpoint_url: Option<String>,
    pub transport: Option<&'static str>,
    pub auth_mode: &'static str,
    pub scopes: Vec<String>,
    pub sort_order: i32,
    pub status: &'static str,
}

const MAX_LOCALE_TEXT: usize = 4000;
const MAX_IDENTIFIER: usize = 64;
const MAX_PROMPT: usize = 8000;
const MAX_SYSTEM_PROMPT: usize = 32000;

/// Normalize a locale tag.
///
/// `I18N_SPEC.md` requires a real BCP 47 tag, and the platform's default pair is
/// `zh-CN` / `en-US`, so a bare `zh` or `en` is widened instead of rejected — an
/// editor typing `en` means English, not a malformed document.
fn normalize_locale(raw: &str) -> Option<String> {
    let trimmed = raw.trim().replace('_', "-");
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = trimmed.split('-');
    let language = parts.next()?.to_ascii_lowercase();
    if language.len() < 2
        || language.len() > 3
        || !language.chars().all(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    let mut normalized = language;
    for part in parts {
        if part.is_empty() || part.len() > 8 || !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        normalized.push('-');
        normalized.push_str(part);
    }
    // Widen a bare language to the platform's default region for that language.
    match normalized.as_str() {
        "zh" => Some("zh-CN".to_owned()),
        "en" => Some("en-US".to_owned()),
        _ => Some(normalized),
    }
}

fn normalize_text_map(
    map: &LocalizedText,
    scope_kind: &'static str,
    scope_key: &str,
    field_name: &'static str,
    errors: &mut Vec<String>,
) -> Vec<NormalizedText> {
    let mut out = Vec::new();
    for (locale, value) in map {
        let Some(locale) = normalize_locale(locale) else {
            errors.push(format!(
                "{field_name}: locale `{locale}` is not a BCP 47 tag"
            ));
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            // An empty translation is "not translated yet", not an empty string
            // to render. Dropped rather than stored so a missing key falls back
            // to the default locale at read time.
            continue;
        }
        if value.chars().count() > MAX_LOCALE_TEXT {
            errors.push(format!(
                "{field_name}: `{locale}` exceeds {MAX_LOCALE_TEXT} characters"
            ));
            continue;
        }
        out.push(NormalizedText {
            scope_kind,
            scope_key: scope_key.to_owned(),
            field_name,
            locale,
            text_value: value.to_owned(),
        });
    }
    out
}

fn validate_key(raw: &str, field: &str, errors: &mut Vec<String>) -> Option<String> {
    let key = raw.trim();
    if key.is_empty() || key.chars().count() > MAX_IDENTIFIER {
        errors.push(format!(
            "{field}: key must be 1..={MAX_IDENTIFIER} characters"
        ));
        return None;
    }
    let mut chars = key.chars();
    let first = chars.next()?;
    if !first.is_ascii_lowercase() {
        errors.push(format!("{field}: key must start with a lowercase letter"));
        return None;
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        errors.push(format!(
            "{field}: key may only contain lowercase letters, digits, `_` and `-`"
        ));
        return None;
    }
    Some(key.to_owned())
}

fn validate_asset_ref(raw: &str, field: &str, errors: &mut Vec<String>) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() || value.chars().count() > 191 {
        errors.push(format!("{field}: ref must be 1..=191 characters"));
        return None;
    }
    let mut chars = value.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphanumeric() {
        errors.push(format!(
            "{field}: ref must start with an alphanumeric character"
        ));
        return None;
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '@' | '-'))
    {
        errors.push(format!(
            "{field}: ref may only contain letters, digits, `.`, `_`, `:`, `/`, `@` and `-`"
        ));
        return None;
    }
    Some(value.to_owned())
}

impl ReplaceAppCapabilityRequest {
    /// Validate and flatten the document.
    ///
    /// The function collects *all* problems before returning, so an editor fixes
    /// a document in one round instead of one field per request.
    pub fn normalize(&self) -> DeployServiceResult<NormalizedAppCapability> {
        let mut errors: Vec<String> = Vec::new();

        let distribution_scope = self
            .distribution_scope
            .unwrap_or(DistributionScope::Private)
            .as_str();

        let avatar_url = self
            .avatar_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if let Some(url) = avatar_url.as_deref() {
            if url.chars().count() > 2048 {
                errors.push("avatarUrl must be at most 2048 characters".to_owned());
            }
            if !url.starts_with("https://") {
                errors.push("avatarUrl must be an absolute https URL".to_owned());
            }
        }

        // The OAuth client this document points at. Validated as an identifier
        // rather than as a URL: it is the IAM credential's `client_id`
        // (`iamc_<hex>` or an operator-supplied id), and this module never
        // resolves it — a dangling reference is caught by the identity
        // provisioning flow, not by the capability write.
        let oauth_client_id = self
            .oauth_client_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if let Some(client_id) = oauth_client_id.as_deref() {
            if client_id.chars().count() > 128 {
                errors.push("oauthClientId must be at most 128 characters".to_owned());
            } else if !client_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
            {
                errors.push(
                    "oauthClientId may only contain letters, digits, `.`, `_`, `:` and `-`"
                        .to_owned(),
                );
            }
        }

        let mut texts = Vec::new();
        texts.extend(normalize_text_map(
            &self.texts.slogan,
            "CAPABILITY",
            "",
            "slogan",
            &mut errors,
        ));
        texts.extend(normalize_text_map(
            &self.texts.input_placeholder,
            "CAPABILITY",
            "",
            "input_placeholder",
            &mut errors,
        ));
        texts.extend(normalize_text_map(
            &self.texts.summary,
            "CAPABILITY",
            "",
            "summary",
            &mut errors,
        ));

        // Work modes -------------------------------------------------------
        let mut work_modes = Vec::new();
        let mut seen_mode_keys = std::collections::BTreeSet::new();
        let mut defaults = 0usize;
        for (index, mode) in self.work_modes.iter().enumerate() {
            let path = format!("workModes[{index}]");
            let Some(mode_key) = validate_key(&mode.key, &format!("{path}.key"), &mut errors)
            else {
                continue;
            };
            if !seen_mode_keys.insert(mode_key.clone()) {
                errors.push(format!("{path}.key: duplicate work mode key `{mode_key}`"));
                continue;
            }
            let system_prompt = mode.system_prompt.trim();
            if system_prompt.is_empty() || system_prompt.chars().count() > MAX_SYSTEM_PROMPT {
                errors.push(format!(
                    "{path}.systemPrompt: must be 1..={MAX_SYSTEM_PROMPT} characters"
                ));
                continue;
            }
            if mode.is_default {
                defaults += 1;
            }
            let mut mode_texts = Vec::new();
            mode_texts.extend(normalize_text_map(
                &mode.name,
                "WORK_MODE",
                &mode_key,
                "name",
                &mut errors,
            ));
            mode_texts.extend(normalize_text_map(
                &mode.description,
                "WORK_MODE",
                &mode_key,
                "description",
                &mut errors,
            ));

            let mut tools = Vec::new();
            let mut seen_tools = std::collections::BTreeSet::new();
            for (tool_index, tool) in mode.tools.iter().enumerate() {
                let tool_path = format!("{path}.tools[{tool_index}]");
                let Some(tool_ref) =
                    validate_asset_ref(&tool.tool_ref, &format!("{tool_path}.ref"), &mut errors)
                else {
                    continue;
                };
                if !seen_tools.insert((tool.kind.as_str(), tool_ref.clone())) {
                    errors.push(format!(
                        "{tool_path}: duplicate tool `{tool_ref}` of kind `{}`",
                        tool.kind.as_str()
                    ));
                    continue;
                }
                if !tool.policy.is_null() && !tool.policy.is_object() {
                    errors.push(format!("{tool_path}.policy: must be a JSON object"));
                    continue;
                }
                tools.push(NormalizedWorkModeTool {
                    tool_kind: tool.kind.as_str(),
                    source_module: tool.kind.source_module(),
                    tool_ref,
                    policy: if tool.policy.is_null() {
                        empty_object()
                    } else {
                        tool.policy.clone()
                    },
                    sort_order: tool.sort_order.max(0),
                });
            }

            work_modes.push(NormalizedWorkMode {
                mode_key,
                system_prompt: system_prompt.to_owned(),
                icon_ref: mode
                    .icon_ref
                    .clone()
                    .filter(|value| !value.trim().is_empty()),
                sort_order: mode.sort_order.max(0),
                is_default: mode.is_default,
                status: mode.status.as_str(),
                texts: mode_texts,
                tools,
            });
        }
        if defaults > 1 {
            errors.push("workModes: at most one mode may set isDefault".to_owned());
        }

        // Scenes -----------------------------------------------------------
        let mut scenes = Vec::new();
        let mut seen_scene_keys = std::collections::BTreeSet::new();
        for (index, scene) in self.scenes.iter().enumerate() {
            let path = format!("scenes[{index}]");
            let Some(scene_key) = validate_key(&scene.key, &format!("{path}.key"), &mut errors)
            else {
                continue;
            };
            if !seen_scene_keys.insert(scene_key.clone()) {
                errors.push(format!("{path}.key: duplicate scene key `{scene_key}`"));
                continue;
            }
            let prompt = scene.prompt_template.trim();
            if prompt.is_empty() || prompt.chars().count() > MAX_PROMPT {
                errors.push(format!(
                    "{path}.promptTemplate: must be 1..={MAX_PROMPT} characters"
                ));
                continue;
            }
            let mut scene_texts = Vec::new();
            scene_texts.extend(normalize_text_map(
                &scene.title,
                "SCENE",
                &scene_key,
                "title",
                &mut errors,
            ));
            scene_texts.extend(normalize_text_map(
                &scene.subtitle,
                "SCENE",
                &scene_key,
                "subtitle",
                &mut errors,
            ));

            let mut assets = Vec::new();
            let mut seen_assets = std::collections::BTreeSet::new();
            for (asset_index, asset) in scene.assets.iter().enumerate() {
                let asset_path = format!("{path}.assets[{asset_index}]");
                let Some(asset_ref) =
                    validate_asset_ref(&asset.asset_ref, &format!("{asset_path}.ref"), &mut errors)
                else {
                    continue;
                };
                if !seen_assets.insert((asset.kind.as_str(), asset_ref.clone())) {
                    errors.push(format!(
                        "{asset_path}: duplicate asset `{asset_ref}` of kind `{}`",
                        asset.kind.as_str()
                    ));
                    continue;
                }
                if !asset.snapshot.is_null() && !asset.snapshot.is_object() {
                    errors.push(format!("{asset_path}.snapshot: must be a JSON object"));
                    continue;
                }
                assets.push(NormalizedSceneAsset {
                    asset_kind: asset.kind.as_str(),
                    source_module: asset.kind.source_module(),
                    asset_ref,
                    snapshot: if asset.snapshot.is_null() {
                        empty_object()
                    } else {
                        asset.snapshot.clone()
                    },
                    sort_order: asset.sort_order.max(0),
                });
            }

            scenes.push(NormalizedScene {
                scene_key,
                prompt_template: prompt.to_owned(),
                icon_ref: scene
                    .icon_ref
                    .clone()
                    .filter(|value| !value.trim().is_empty()),
                sort_order: scene.sort_order.max(0),
                is_featured: scene.is_featured,
                status: scene.status.as_str(),
                texts: scene_texts,
                assets,
            });
        }

        // Models -----------------------------------------------------------
        let mut models = Vec::new();
        let mut seen_models = std::collections::BTreeSet::new();
        let mut model_defaults = 0usize;
        for (index, model) in self.models.iter().enumerate() {
            let path = format!("models[{index}]");
            let provider_code = model.provider_code.trim().to_ascii_lowercase();
            if provider_code.is_empty()
                || provider_code.len() > 32
                || !provider_code
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
            {
                errors.push(format!(
                    "{path}.providerCode: must be 1..=32 characters of lowercase letters, digits, `_` or `-`"
                ));
                continue;
            }
            let Some(model_key) =
                validate_asset_ref(&model.model_key, &format!("{path}.modelKey"), &mut errors)
            else {
                continue;
            };
            if !seen_models.insert((provider_code.clone(), model_key.clone())) {
                errors.push(format!(
                    "{path}: duplicate model `{provider_code}/{model_key}`"
                ));
                continue;
            }
            if model.is_default {
                model_defaults += 1;
            }
            models.push(NormalizedModelBinding {
                provider_code,
                model_key,
                sort_order: model.sort_order.max(0),
                is_default: model.is_default,
                status: model.status.as_str(),
            });
        }
        if model_defaults > 1 {
            errors.push("models: at most one model may set isDefault".to_owned());
        }

        // Market -----------------------------------------------------------
        let mut market = Vec::new();
        let mut seen_market = std::collections::BTreeSet::new();
        for (index, entry) in self.market.iter().enumerate() {
            let path = format!("market[{index}]");
            if !entry.section.accepts(entry.kind) {
                errors.push(format!(
                    "{path}: shelf `{}` cannot list asset kind `{}`",
                    entry.section.as_str(),
                    entry.kind.as_str()
                ));
                continue;
            }
            let Some(asset_ref) =
                validate_asset_ref(&entry.asset_ref, &format!("{path}.ref"), &mut errors)
            else {
                continue;
            };
            if !seen_market.insert((
                entry.section.as_str(),
                entry.kind.as_str(),
                asset_ref.clone(),
            )) {
                errors.push(format!(
                    "{path}: duplicate market asset `{asset_ref}` on shelf `{}`",
                    entry.section.as_str()
                ));
                continue;
            }
            if let Some(category) = entry.category_code.as_deref() {
                let category = category.trim();
                if !category.is_empty()
                    && !validate_key(category, &format!("{path}.categoryCode"), &mut errors)
                        .is_some()
                {
                    continue;
                }
            }
            if !entry.snapshot.is_null() && !entry.snapshot.is_object() {
                errors.push(format!("{path}.snapshot: must be a JSON object"));
                continue;
            }
            market.push(NormalizedMarketAsset {
                section: entry.section.as_str(),
                asset_kind: entry.kind.as_str(),
                source_module: entry.kind.source_module(),
                asset_ref,
                category_code: entry
                    .category_code
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_owned()),
                sort_order: entry.sort_order.max(0),
                is_featured: entry.is_featured,
                status: entry.status.as_str(),
                snapshot: if entry.snapshot.is_null() {
                    empty_object()
                } else {
                    entry.snapshot.clone()
                },
            });
        }

        // Embeds -----------------------------------------------------------
        let mut embeds = Vec::new();
        let mut seen_embeds = std::collections::BTreeSet::new();
        for (index, embed) in self.embeds.iter().enumerate() {
            let path = format!("embeds[{index}]");
            let Some(embed_key) = validate_key(&embed.key, &format!("{path}.key"), &mut errors)
            else {
                continue;
            };
            if !seen_embeds.insert(embed_key.clone()) {
                errors.push(format!("{path}.key: duplicate embed key `{embed_key}`"));
                continue;
            }
            let mut normalized = NormalizedEmbed {
                embed_key,
                embed_kind: embed.kind.as_str(),
                server_ref: None,
                endpoint_url: None,
                transport: embed.transport.map(|value| value.as_str()),
                auth_mode: embed.auth_mode.as_str(),
                scopes: Vec::new(),
                sort_order: embed.sort_order.max(0),
                status: embed.status.as_str(),
            };
            match embed.kind {
                CapabilityEmbedKind::McpApp => {
                    let Some(server_ref) = embed.server_ref.as_deref().and_then(|value| {
                        validate_asset_ref(value, &format!("{path}.serverRef"), &mut errors)
                    }) else {
                        errors.push(format!(
                            "{path}.serverRef: an MCP_APP embed requires the MCP server key"
                        ));
                        continue;
                    };
                    if embed.transport.is_none() {
                        errors.push(format!(
                            "{path}.transport: an MCP_APP embed requires a transport"
                        ));
                        continue;
                    }
                    if embed.endpoint_url.is_some() {
                        errors.push(format!(
                            "{path}: an MCP_APP embed must not carry endpointUrl"
                        ));
                        continue;
                    }
                    normalized.server_ref = Some(server_ref);
                }
                CapabilityEmbedKind::Iframe => {
                    let Some(url) = embed
                        .endpoint_url
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    else {
                        errors.push(format!(
                            "{path}.endpointUrl: an IFRAME embed requires an https URL"
                        ));
                        continue;
                    };
                    if !url.starts_with("https://") || url.chars().count() > 2048 {
                        errors.push(format!(
                            "{path}.endpointUrl: must be an absolute https URL of at most 2048 characters"
                        ));
                        continue;
                    }
                    if embed.server_ref.is_some() {
                        errors.push(format!("{path}: an IFRAME embed must not carry serverRef"));
                        continue;
                    }
                    normalized.endpoint_url = Some(url.to_owned());
                    normalized.transport = None;
                }
            }
            let mut scopes = Vec::new();
            for scope in &embed.scopes {
                let scope = scope.trim();
                if scope.is_empty() {
                    continue;
                }
                if scope.len() > 128
                    || !scope
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
                {
                    errors.push(format!("{path}.scopes: `{scope}` is not a valid scope"));
                    scopes.clear();
                    break;
                }
                scopes.push(scope.to_owned());
            }
            if scopes.len() > 64 {
                errors.push(format!("{path}.scopes: at most 64 scopes"));
                continue;
            }
            normalized.scopes = scopes;
            embeds.push(normalized);
        }

        // The default pointer must name an existing, enabled mode.
        let default_work_mode_key = self
            .default_work_mode_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if let Some(key) = default_work_mode_key.as_deref() {
            match work_modes
                .iter()
                .find(|mode| mode.mode_key == key && mode.status == "active")
            {
                Some(_) => {}
                None => errors.push(format!(
                    "defaultWorkModeKey: `{key}` does not name an active work mode in this document"
                )),
            }
        }

        let expected_version = match self.expected_version.as_deref() {
            None => None,
            Some(raw) => match raw.trim().parse::<i64>() {
                Ok(value) if value >= 0 => Some(value),
                _ => {
                    errors.push("expectedVersion must be a non-negative integer string".to_owned());
                    None
                }
            },
        };

        if !errors.is_empty() {
            return Err(DeployServiceError::validation(errors.join("; ")));
        }

        Ok(NormalizedAppCapability {
            distribution_scope,
            avatar_url,
            oauth_client_id,
            default_work_mode_key,
            texts,
            work_modes,
            scenes,
            models,
            market,
            embeds,
            submit_for_review: self.submit_for_review,
            note: None,
            expected_version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> ReplaceAppCapabilityRequest {
        ReplaceAppCapabilityRequest {
            distribution_scope: Some(DistributionScope::Tenant),
            default_work_mode_key: Some("research".to_owned()),
            work_modes: vec![WorkModeDefinition {
                key: "research".to_owned(),
                name: [("zh".to_owned(), "投研模式".to_owned())]
                    .into_iter()
                    .collect(),
                description: LocalizedText::new(),
                system_prompt: "You are an equity research analyst.".to_owned(),
                icon_ref: None,
                sort_order: 0,
                is_default: true,
                status: CapabilityRowStatus::Active,
                tools: vec![WorkModeTool {
                    kind: CapabilityToolKind::Mcp,
                    tool_ref: "market-data".to_owned(),
                    policy: empty_object(),
                    sort_order: 0,
                }],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_minimal_document_normalizes_and_widens_the_locale() {
        let normalized = base_request().normalize().expect("valid document");
        assert_eq!(normalized.distribution_scope, "tenant");
        assert_eq!(normalized.work_modes.len(), 1);
        let mode = &normalized.work_modes[0];
        assert_eq!(mode.mode_key, "research");
        assert_eq!(mode.tools[0].source_module, "mcp");
        assert_eq!(mode.texts[0].locale, "zh-CN");
        assert_eq!(mode.texts[0].field_name, "name");
    }

    #[test]
    fn two_default_work_modes_are_refused() {
        let mut request = base_request();
        let mut second = request.work_modes[0].clone();
        second.key = "legal".to_owned();
        second.name = LocalizedText::new();
        request.work_modes.push(second);
        let error = request.normalize().expect_err("two defaults");
        assert!(error.to_string().contains("at most one mode"));
    }

    #[test]
    fn a_dangling_default_pointer_is_refused() {
        let mut request = base_request();
        request.default_work_mode_key = Some("missing".to_owned());
        let error = request.normalize().expect_err("dangling pointer");
        assert!(error.to_string().contains("defaultWorkModeKey"));
    }

    #[test]
    fn an_expert_cannot_be_listed_on_the_connector_shelf() {
        let mut request = base_request();
        request.market = vec![MarketAssetDefinition {
            section: CapabilityMarketSection::Connectors,
            kind: CapabilityAssetKind::Expert,
            asset_ref: "agent.expert.legal".to_owned(),
            category_code: None,
            sort_order: 0,
            is_featured: false,
            status: CapabilityRowStatus::Active,
            snapshot: empty_object(),
        }];
        let error = request.normalize().expect_err("wrong shelf");
        assert!(error.to_string().contains("cannot list asset kind"));
    }

    #[test]
    fn an_iframe_embed_requires_https_and_no_server_ref() {
        let mut request = base_request();
        request.embeds = vec![EmbedDefinition {
            key: "finance".to_owned(),
            kind: CapabilityEmbedKind::Iframe,
            server_ref: None,
            endpoint_url: Some("http://insecure.example.com".to_owned()),
            transport: None,
            auth_mode: CapabilityEmbedAuthMode::None,
            scopes: Vec::new(),
            sort_order: 0,
            status: CapabilityRowStatus::Active,
        }];
        let error = request.normalize().expect_err("http iframe");
        assert!(error.to_string().contains("https"));
    }

    #[test]
    fn a_rejection_requires_a_reason_family() {
        let request = DecideCapabilityReviewRequest {
            decision: CapabilityReviewDecision::Reject,
            decision_code: None,
            decision_note: None,
        };
        assert!(request.validate().is_err());
    }
}
