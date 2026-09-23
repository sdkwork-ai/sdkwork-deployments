//! Unified application delivery DTOs: apps, platform targets, source
//! repositories, build templates, builds, packages, releases, channels,
//! rollouts, deployments, and signing identities (REQ-2026-0002).

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Enums (canonical string vocabulary; no ad hoc integer meanings)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppKind {
    StaticWeb,
    SpaWeb,
    ApiService,
    WechatMiniprogram,
    DouyinMiniprogram,
    IosApp,
    AndroidApp,
    HarmonyosApp,
    DesktopApp,
}

impl AppKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StaticWeb => "STATIC_WEB",
            Self::SpaWeb => "SPA_WEB",
            Self::ApiService => "API_SERVICE",
            Self::WechatMiniprogram => "WECHAT_MINIPROGRAM",
            Self::DouyinMiniprogram => "DOUYIN_MINIPROGRAM",
            Self::IosApp => "IOS_APP",
            Self::AndroidApp => "ANDROID_APP",
            Self::HarmonyosApp => "HARMONYOS_APP",
            Self::DesktopApp => "DESKTOP_APP",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "STATIC_WEB" => Some(Self::StaticWeb),
            "SPA_WEB" => Some(Self::SpaWeb),
            "API_SERVICE" => Some(Self::ApiService),
            "WECHAT_MINIPROGRAM" => Some(Self::WechatMiniprogram),
            "DOUYIN_MINIPROGRAM" => Some(Self::DouyinMiniprogram),
            "IOS_APP" => Some(Self::IosApp),
            "ANDROID_APP" => Some(Self::AndroidApp),
            "HARMONYOS_APP" => Some(Self::HarmonyosApp),
            "DESKTOP_APP" => Some(Self::DesktopApp),
            _ => None,
        }
    }

    pub fn is_web(self) -> bool {
        matches!(self, Self::StaticWeb | Self::SpaWeb)
    }
}

/// Which level owns an application, and therefore who may reach it.
///
/// `apps.list` has always been tenant-wide, so a reader could not tell a
/// platform-operated app from a tenant's shared app from one person's personal
/// app. This is the same question [`crate::dto::ZoneScope`] answers for
/// `deploy_dns_zone`, and it is modelled the same way: an explicit level rather
/// than a flag derived from a nullable column, because every pre-existing
/// `deploy_app` row carries `user_id IS NULL` and a derived flag would classify
/// the whole inventory as platform-owned.
///
/// The vocabulary mirrors the cloud-account scopes this repository already ships
/// (`ACCOUNT_SCOPE_*` in `sdkwork-deploy-cloud-account-port`), so the account
/// centre and the app inventory name and order the levels alike. `DATABASE_SPEC.md`
/// §6.7 requires `owner_type` values to come from a documented enum.
///
/// Stored in `deploy_app.owner_type` and constrained there by
/// `chk_deploy_app_owner_pointer`, which keeps the level and its pointer in
/// agreement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppOwnerType {
    /// Platform-operated or built-in application. No user owner, reachable from
    /// every tenant.
    Platform,
    /// Shared across the whole tenant. No user owner; `tenant_id` is the scope.
    /// This is the default, and the honest reading of the ledger's own history.
    #[default]
    Tenant,
    /// Shared inside one organization. Owner pointer is `organization_id`.
    Organization,
    /// One person's application. Owner pointer is `user_id`; reachable only by
    /// that user, and by platform operators.
    User,
}

impl AppOwnerType {
    pub const ALL: [Self; 4] = [Self::Platform, Self::Tenant, Self::Organization, Self::User];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Platform => "PLATFORM",
            Self::Tenant => "TENANT",
            Self::Organization => "ORGANIZATION",
            Self::User => "USER",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "PLATFORM" => Some(Self::Platform),
            "TENANT" => Some(Self::Tenant),
            "ORGANIZATION" => Some(Self::Organization),
            "USER" => Some(Self::User),
            _ => None,
        }
    }

    /// Widest-first rank, matching `scope_rank` in
    /// `sdkwork-deploy-cloud-account-port` so list ordering and precedence agree
    /// with the account centre. Lowest rank is the narrowest reach.
    pub const fn rank(self) -> u8 {
        match self {
            Self::User => 0,
            Self::Organization => 1,
            Self::Tenant => 2,
            Self::Platform => 3,
        }
    }

    /// Whether an app at this level is reachable by a subject that is not its
    /// owner. `PLATFORM` and `TENANT` are shared; `ORGANIZATION` is shared only
    /// inside its organization and `USER` not at all, so both need the caller's
    /// own scope to be checked — see the predicate in the repository.
    pub const fn is_shared(self) -> bool {
        matches!(self, Self::Platform | Self::Tenant)
    }
}

impl std::fmt::Display for AppOwnerType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Platform {
    Web,
    Api,
    Wechat,
    Douyin,
    Ios,
    Android,
    Harmonyos,
    Windows,
    Macos,
    Linux,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Web => "WEB",
            Self::Api => "API",
            Self::Wechat => "WECHAT",
            Self::Douyin => "DOUYIN",
            Self::Ios => "IOS",
            Self::Android => "ANDROID",
            Self::Harmonyos => "HARMONYOS",
            Self::Windows => "WINDOWS",
            Self::Macos => "MACOS",
            Self::Linux => "LINUX",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TechStack {
    Flutter,
    Native,
    UniApp,
    Node,
    Rust,
    Go,
    Java,
    Electron,
    Tauri,
    Other,
}

impl TechStack {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flutter => "FLUTTER",
            Self::Native => "NATIVE",
            Self::UniApp => "UNI_APP",
            Self::Node => "NODE",
            Self::Rust => "RUST",
            Self::Go => "GO",
            Self::Java => "JAVA",
            Self::Electron => "ELECTRON",
            Self::Tauri => "TAURI",
            Self::Other => "OTHER",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BuildStatus {
    Queued,
    Preparing,
    Compiling,
    Testing,
    Packaging,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

impl BuildStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Preparing => "PREPARING",
            Self::Compiling => "COMPILING",
            Self::Testing => "TESTING",
            Self::Packaging => "PACKAGING",
            Self::Succeeded => "SUCCEEDED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
            Self::TimedOut => "TIMED_OUT",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }

    pub fn is_active(self) -> bool {
        !self.is_terminal()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageFormat {
    DistDir,
    Zip,
    Apk,
    Aab,
    Ipa,
    Xcarchive,
    Hap,
    App,
    OciImage,
    ProcessBundle,
    TarGz,
    Msi,
    Nsis,
    Msix,
    Exe,
    Dmg,
    Pkg,
    Deb,
    Rpm,
    AppImage,
    Jar,
    War,
}

impl PackageFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DistDir => "DIST_DIR",
            Self::Zip => "ZIP",
            Self::Apk => "APK",
            Self::Aab => "AAB",
            Self::Ipa => "IPA",
            Self::Xcarchive => "XCARCHIVE",
            Self::Hap => "HAP",
            Self::App => "APP",
            Self::OciImage => "OCI_IMAGE",
            Self::ProcessBundle => "PROCESS_BUNDLE",
            Self::TarGz => "TAR_GZ",
            Self::Msi => "MSI",
            Self::Nsis => "NSIS",
            Self::Msix => "MSIX",
            Self::Exe => "EXE",
            Self::Dmg => "DMG",
            Self::Pkg => "PKG",
            Self::Deb => "DEB",
            Self::Rpm => "RPM",
            Self::AppImage => "APPIMAGE",
            Self::Jar => "JAR",
            Self::War => "WAR",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "DIST_DIR" => Some(Self::DistDir),
            "ZIP" => Some(Self::Zip),
            "APK" => Some(Self::Apk),
            "AAB" => Some(Self::Aab),
            "IPA" => Some(Self::Ipa),
            "XCARCHIVE" => Some(Self::Xcarchive),
            "HAP" => Some(Self::Hap),
            "APP" => Some(Self::App),
            "OCI_IMAGE" => Some(Self::OciImage),
            "PROCESS_BUNDLE" => Some(Self::ProcessBundle),
            "TAR_GZ" => Some(Self::TarGz),
            "MSI" => Some(Self::Msi),
            "NSIS" => Some(Self::Nsis),
            "MSIX" => Some(Self::Msix),
            "EXE" => Some(Self::Exe),
            "DMG" => Some(Self::Dmg),
            "PKG" => Some(Self::Pkg),
            "DEB" => Some(Self::Deb),
            "RPM" => Some(Self::Rpm),
            "APPIMAGE" => Some(Self::AppImage),
            "JAR" => Some(Self::Jar),
            "WAR" => Some(Self::War),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageStatus {
    Draft,
    Validated,
    Ready,
    Superseded,
    Retired,
    Archived,
}

impl PackageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Validated => "VALIDATED",
            Self::Ready => "READY",
            Self::Superseded => "SUPERSEDED",
            Self::Retired => "RETIRED",
            Self::Archived => "ARCHIVED",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReleaseStatus {
    Draft,
    Active,
    Superseded,
    Deprecated,
    Retired,
    Archived,
}

/// Release status transition request (`PATCH /apps/{appId}/releases/{releaseId}`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateAppReleaseStatusRequest {
    #[serde(rename = "releaseStatus")]
    pub release_status: ReleaseStatus,
}

impl ReleaseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Active => "ACTIVE",
            Self::Superseded => "SUPERSEDED",
            Self::Deprecated => "DEPRECATED",
            Self::Retired => "RETIRED",
            Self::Archived => "ARCHIVED",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChannelKey {
    Stable,
    Beta,
    Alpha,
    Qa,
}

impl ChannelKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
            Self::Qa => "qa",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            "alpha" => Some(Self::Alpha),
            "qa" => Some(Self::Qa),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RolloutStrategy {
    Immediate,
    Percentage,
    ManualApproval,
}

impl RolloutStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "IMMEDIATE",
            Self::Percentage => "PERCENTAGE",
            Self::ManualApproval => "MANUAL_APPROVAL",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RolloutStatus {
    Pending,
    Rolling,
    Completed,
    RolledBack,
    Failed,
    Cancelled,
}

impl RolloutStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Rolling => "ROLLING",
            Self::Completed => "COMPLETED",
            Self::RolledBack => "ROLLED_BACK",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeploymentKind {
    ArtifactRelease,
    AppPublishConfig,
    TlsConfig,
    MiniprogramReview,
    StoreSubmission,
    OtaDistribution,
    EnterpriseDistribution,
    ContainerRollout,
}

impl DeploymentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ArtifactRelease => "ARTIFACT_RELEASE",
            Self::AppPublishConfig => "SITE_CONFIG",
            Self::TlsConfig => "TLS_CONFIG",
            Self::MiniprogramReview => "MINIPROGRAM_REVIEW",
            Self::StoreSubmission => "STORE_SUBMISSION",
            Self::OtaDistribution => "OTA_DISTRIBUTION",
            Self::EnterpriseDistribution => "ENTERPRISE_DISTRIBUTION",
            Self::ContainerRollout => "CONTAINER_ROLLOUT",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeploymentTarget {
    WebNode,
    Container,
    WechatReview,
    DouyinReview,
    AppStoreConnect,
    Testflight,
    Ota,
    Enterprise,
    HarmonyosStore,
    MicrosoftStore,
    MacAppStore,
}

impl DeploymentTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WebNode => "WEB_NODE",
            Self::Container => "CONTAINER",
            Self::WechatReview => "WECHAT_REVIEW",
            Self::DouyinReview => "DOUYIN_REVIEW",
            Self::AppStoreConnect => "APP_STORE_CONNECT",
            Self::Testflight => "TESTFLIGHT",
            Self::Ota => "OTA",
            Self::Enterprise => "ENTERPRISE",
            Self::HarmonyosStore => "HARMONYOS_STORE",
            Self::MicrosoftStore => "MICROSOFT_STORE",
            Self::MacAppStore => "MAC_APP_STORE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeploymentStatus {
    Pending,
    Submitting,
    PendingReview,
    InReview,
    Rejected,
    Approved,
    Live,
    Active,
    Degraded,
    Failed,
    RolledBack,
    Cancelled,
}

impl DeploymentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Submitting => "SUBMITTING",
            Self::PendingReview => "PENDING_REVIEW",
            Self::InReview => "IN_REVIEW",
            Self::Rejected => "REJECTED",
            Self::Approved => "APPROVED",
            Self::Live => "LIVE",
            Self::Active => "ACTIVE",
            Self::Degraded => "DEGRADED",
            Self::Failed => "FAILED",
            Self::RolledBack => "ROLLED_BACK",
            Self::Cancelled => "CANCELLED",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SigningKind {
    IosSigning,
    AndroidKeystore,
    HarmonyosCertProfile,
    MiniprogramUploadKey,
    ApiRepoToken,
    WindowsAuthenticode,
    MacosDeveloperId,
}

impl SigningKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IosSigning => "IOS_SIGNING",
            Self::AndroidKeystore => "ANDROID_KEYSTORE",
            Self::HarmonyosCertProfile => "HARMONYOS_CERT_PROFILE",
            Self::MiniprogramUploadKey => "MINIPROGRAM_UPLOAD_KEY",
            Self::ApiRepoToken => "API_REPO_TOKEN",
            Self::WindowsAuthenticode => "WINDOWS_AUTHENTICODE",
            Self::MacosDeveloperId => "MACOS_DEVELOPER_ID",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppStatus {
    Draft,
    Ready,
    Active,
    Paused,
    Archived,
    Failed,
}

impl AppStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Ready => "READY",
            Self::Active => "ACTIVE",
            Self::Paused => "PAUSED",
            Self::Archived => "ARCHIVED",
            Self::Failed => "FAILED",
        }
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateAppRequest {
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(rename = "appKind")]
    pub app_kind: AppKind,
    /// Ownership level for the new application.
    ///
    /// Absent means the repository derives it from the request principal:
    /// [`AppOwnerType::User`] when a user subject is present — the normal case,
    /// since a console create is always somebody's app — and
    /// [`AppOwnerType::Tenant`] otherwise. `PLATFORM` and `ORGANIZATION` are
    /// never inferred; a caller that wants them must say so, because a tenant
    /// member cannot be allowed to promote their own app to platform visibility.
    ///
    /// The level and the owner pointer are kept in agreement by
    /// `chk_deploy_app_owner_pointer`.
    #[serde(rename = "ownerType", default)]
    pub owner_type: Option<AppOwnerType>,
    #[serde(default)]
    pub description: Option<String>,
    /// Web publishing type (1..6, was `deploy_app.type`).
    ///
    /// Optional on the wire: the published contract (`apps.create`) does not
    /// declare it, so callers normally omit it and the DB column default
    /// applies. The repository must never bind a NULL here — see
    /// `DEFAULT_APP_TYPE` in `sdkwork-intelligence-deploy-repository-sqlx`.
    #[serde(rename = "type", default)]
    pub app_type: Option<i32>,
    #[serde(rename = "runtimeConfig", default)]
    pub runtime_config: Option<Value>,
    /// Free-form JSONB persisted into `deploy_app.metadata` (category, media,
    /// version, releaseNotes). Declared by the published contract and typed by
    /// the generated SDK as `metadata?: Record<string, unknown>`; without this
    /// field serde silently discarded it, so the console's app category and
    /// store media never reached the database.
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(rename = "defaultEnvironment", default)]
    pub default_environment: Option<String>,
    /// Explicit `<appId>` label of the app's default publishing hostnames
    /// (`<appDomainLabel>.app[-<env>].<suffix>`). One lowercase DNS label;
    /// absent means the slug is used, which is why a Chinese-named app used to
    /// publish on a degenerate hostname. See `sdkwork-deploy-core::app_domains`.
    #[serde(rename = "appDomainLabel", default)]
    pub app_domain_label: Option<String>,
    /// Per-app override of the platform app-domain suffix catalog
    /// (`PLATFORM_APP_DOMAIN_SUFFIXES`). Absent means the catalog applies.
    #[serde(rename = "appDomainSuffixes", default)]
    pub app_domain_suffixes: Option<Vec<String>>,
    #[serde(rename = "idempotencyKey", default)]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateAppRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub app_type: Option<i32>,
    #[serde(rename = "runtimeConfig", default)]
    pub runtime_config: Option<Value>,
    /// Free-form JSONB merged into `deploy_app.metadata`. Declared by the
    /// published contract (`UpdateAppRequest.metadata`) and used by the console
    /// to write back `metadata.media` after the Drive upload completes.
    #[serde(default)]
    pub metadata: Option<Value>,
    /// Moves the app to another ownership level. Absent leaves it alone.
    ///
    /// A plain `Option` rather than the double-`Option` used by `appDomainLabel`:
    /// that one needs "present but null" because clearing the override is a real
    /// operation, whereas `deploy_app.owner_type` is `NOT NULL` and every app
    /// always has exactly one level, so there is no clear to express.
    ///
    /// Changing the level rewrites the owner pointer in the same statement,
    /// because `chk_deploy_app_owner_pointer` requires the two to agree: `USER`
    /// takes `ownerUserId`, `ORGANIZATION` uses the app's organization, and
    /// `PLATFORM` / `TENANT` clear `user_id`.
    #[serde(rename = "ownerType", default)]
    pub owner_type: Option<AppOwnerType>,
    /// The user that should own the app when moving it to `USER`. Absent means
    /// "keep the current owner"; ignored at every other level.
    #[serde(rename = "ownerUserId", default)]
    pub owner_user_id: Option<String>,
    #[serde(rename = "appStatus", default)]
    pub app_status: Option<AppStatus>,
    #[serde(rename = "defaultEnvironment", default)]
    pub default_environment: Option<String>,
    /// Replaces the app's default-hostname label. The outer `Option` is "was
    /// the field present on the wire"; the inner one is the declared `null`.
    /// `Some(None)` therefore clears the override so the slug applies again,
    /// while `None` leaves the stored override untouched — a plain
    /// `Option<String>` would collapse those two into the same value and make
    /// `apps.update` unable to distinguish "leave alone" from "remove".
    #[serde(
        rename = "appDomainLabel",
        default,
        deserialize_with = "deserialize_double_option"
    )]
    pub app_domain_label: Option<Option<String>>,
    /// Same double-`Option` shape as `app_domain_label`: `null` restores the
    /// platform suffix catalog.
    #[serde(
        rename = "appDomainSuffixes",
        default,
        deserialize_with = "deserialize_double_option"
    )]
    pub app_domain_suffixes: Option<Option<Vec<String>>>,
}

/// Distinguishes "field absent" (`None`) from "field present and `null`"
/// (`Some(None)`) for PATCH-style optional clears.
fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(rename = "appKind")]
    pub app_kind: String,
    #[serde(rename = "appStatus")]
    pub app_status: String,
    #[serde(rename = "type")]
    pub app_type: i32,
    #[serde(rename = "description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "runtimeConfig", skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<Value>,
    /// Echo of `deploy_app.metadata`. The contract and the generated SDK both
    /// expose this (`AppResponse.metadata`), and the console reads it back to
    /// merge `metadata.media` after uploading store assets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(rename = "currentRevisionId", skip_serializing_if = "Option::is_none")]
    pub current_revision_id: Option<String>,
    #[serde(rename = "desiredRevisionId", skip_serializing_if = "Option::is_none")]
    pub desired_revision_id: Option<String>,
    #[serde(rename = "defaultEnvironment")]
    pub default_environment: String,
    #[serde(rename = "platformTargetCount")]
    pub platform_target_count: i64,
    /// The **effective** label: the explicit override when set, otherwise the
    /// slug. Computed in the service, never in the client, so the console's
    /// hostname preview and the provisioned DNS record always agree.
    #[serde(rename = "appDomainLabel")]
    pub app_domain_label: String,
    /// The **effective** suffix catalog: the per-app override when set,
    /// otherwise the platform catalog.
    #[serde(rename = "appDomainSuffixes")]
    pub app_domain_suffixes: Vec<String>,
    #[serde(rename = "latestReleaseTag", skip_serializing_if = "Option::is_none")]
    pub latest_release_tag: Option<String>,
    /// Which level owns the application. Drives both the ledger's ownership
    /// column and the reachability predicate in `list_apps_repo`; see
    /// [`AppOwnerType`].
    #[serde(rename = "ownerType")]
    pub owner_type: AppOwnerType,
    /// The owning user, present only for [`AppOwnerType::User`].
    ///
    /// Deliberately an id and not a display name: this database owns no user
    /// table (`deploy_app.user_id` has no FK), and a stored name snapshot drifts
    /// on rename. `DATABASE_SPEC.md` permits a denormalized snapshot only as a
    /// read-model projection. Matches how the provider-account DTO already
    /// exposes `ownerUserId`.
    #[serde(rename = "ownerUserId", skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    /// The resolved owner subject: `user_id`, `organization_id`, or absent for
    /// [`AppOwnerType::Platform`] / [`AppOwnerType::Tenant`], whose scope is the
    /// tenant itself. Saved as a string per the int64 wire contract.
    #[serde(rename = "ownerId", skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    /// Tenant scope. Exposed so a platform operator can locate an app across
    /// tenants; a tenant member only ever sees their own.
    #[serde(rename = "tenantId")]
    pub tenant_id: String,
    /// Organization scope. `0` in the column means "none" and reads as absent,
    /// never as an organization whose id happens to be zero.
    #[serde(rename = "organizationId", skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(rename = "createdBy", skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(rename = "updatedBy", skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
    /// Lifecycle observations written by the `apps.activate` / `apps.pause` /
    /// archive transitions. The status badge shows the current state; these say
    /// how long it has held it.
    #[serde(rename = "activatedAt", skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<String>,
    #[serde(rename = "pausedAt", skip_serializing_if = "Option::is_none")]
    pub paused_at: Option<String>,
    #[serde(rename = "archivedAt", skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// Whether an operator has overridden the app-level nginx-compatible
    /// configuration (`deploy_app.nginx_conf_sha256 IS NOT NULL`). The generated
    /// sidecar is the norm; a hand-written override is what an operator needs to
    /// know about during an incident.
    #[serde(rename = "nginxConfigOverridden")]
    pub nginx_config_overridden: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppPage {
    pub items: Vec<AppResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// App domain (default publishing hostnames + bound custom hostnames)
// ---------------------------------------------------------------------------

/// One hostname the app answers on, with the state needed to render a domain
/// column and a CNAME instruction. `DEFAULT` rows come from
/// `provision_app_default_domains*` (`<label>.app[-<env>].<suffix>`, already
/// verified because the platform owns the zone); `CUSTOM` rows are the
/// app's own `deploy_app_binding` rows pointing at user-owned zones.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppDomainResponse {
    pub hostname: String,
    /// `DEFAULT` | `CUSTOM`.
    pub kind: String,
    pub environment: String,
    #[serde(rename = "bindingStatus")]
    pub binding_status: String,
    #[serde(rename = "verificationStatus")]
    pub verification_status: String,
    #[serde(rename = "isCanonical", skip_serializing_if = "Option::is_none")]
    pub is_canonical: Option<bool>,
    #[serde(rename = "pathPrefix", skip_serializing_if = "Option::is_none")]
    pub path_prefix: Option<String>,
    #[serde(rename = "domainId", skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<String>,
    /// The record the user must create for a `CUSTOM` hostname.
    #[serde(rename = "dnsRecordName", skip_serializing_if = "Option::is_none")]
    pub dns_record_name: Option<String>,
    #[serde(rename = "dnsRecordValue", skip_serializing_if = "Option::is_none")]
    pub dns_record_value: Option<String>,
    /// Populated for `DEFAULT` hostnames: what the user should CNAME their own
    /// domain at to alias this app.
    #[serde(rename = "cnameTarget", skip_serializing_if = "Option::is_none")]
    pub cname_target: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppDomainPage {
    pub items: Vec<AppDomainResponse>,
    pub total: i64,
}

// ---------------------------------------------------------------------------
// Platform target
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatePlatformTargetRequest {
    #[serde(rename = "targetKey")]
    pub target_key: String,
    pub platform: Platform,
    #[serde(rename = "techStack", default)]
    pub tech_stack: Option<TechStack>,
    #[serde(rename = "bundleId", default)]
    pub bundle_id: Option<String>,
    #[serde(rename = "packageName", default)]
    pub package_name: Option<String>,
    #[serde(rename = "appId", default)]
    pub app_id: Option<String>,
    #[serde(rename = "bundleName", default)]
    pub bundle_name: Option<String>,
    #[serde(rename = "buildTemplateId", default)]
    pub build_template_id: Option<String>,
    #[serde(rename = "allowedChannels", default)]
    pub allowed_channels: Option<Vec<String>>,
    #[serde(rename = "idempotencyKey", default)]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlatformTargetResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "targetKey")]
    pub target_key: String,
    pub platform: String,
    #[serde(rename = "techStack")]
    pub tech_stack: String,
    #[serde(rename = "bundleId", skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(rename = "packageName", skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    #[serde(rename = "appIdValue", skip_serializing_if = "Option::is_none")]
    pub app_id_value: Option<String>,
    #[serde(rename = "bundleName", skip_serializing_if = "Option::is_none")]
    pub bundle_name: Option<String>,
    #[serde(rename = "buildTemplateId", skip_serializing_if = "Option::is_none")]
    pub build_template_id: Option<String>,
    #[serde(rename = "allowedChannels")]
    pub allowed_channels: Vec<String>,
    #[serde(rename = "targetStatus")]
    pub target_status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlatformTargetPage {
    pub items: Vec<PlatformTargetResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Source repository
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateSourceRepositoryRequest {
    #[serde(rename = "repoKey")]
    pub repo_key: String,
    #[serde(rename = "repoProvider")]
    pub repo_provider: String,
    #[serde(rename = "repoUrl")]
    pub repo_url: String,
    #[serde(rename = "defaultBranch", default)]
    pub default_branch: Option<String>,
    #[serde(rename = "cloneMode", default)]
    pub clone_mode: Option<String>,
    #[serde(rename = "credentialSecretRef", default)]
    pub credential_secret_ref: Option<String>,
    #[serde(rename = "idempotencyKey", default)]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceRepositoryResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "repoKey")]
    pub repo_key: String,
    #[serde(rename = "repoProvider")]
    pub repo_provider: String,
    #[serde(rename = "repoUrl")]
    pub repo_url: String,
    #[serde(rename = "defaultBranch")]
    pub default_branch: String,
    #[serde(rename = "cloneMode")]
    pub clone_mode: String,
    #[serde(
        rename = "credentialSecretRef",
        skip_serializing_if = "Option::is_none"
    )]
    pub credential_secret_ref: Option<String>,
    #[serde(rename = "repoStatus")]
    pub repo_status: String,
    #[serde(rename = "lastErrorCode", skip_serializing_if = "Option::is_none")]
    pub last_error_code: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceRepositoryPage {
    pub items: Vec<SourceRepositoryResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Build template
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateBuildTemplateRequest {
    #[serde(rename = "templateName")]
    pub template_name: String,
    #[serde(rename = "templateVersion")]
    pub template_version: String,
    pub platform: Platform,
    #[serde(rename = "techStack", default)]
    pub tech_stack: Option<TechStack>,
    #[serde(rename = "toolchain", default)]
    pub toolchain: Option<Value>,
    #[serde(rename = "commands", default)]
    pub commands: Option<Vec<String>>,
    #[serde(rename = "artifactOutputs", default)]
    pub artifact_outputs: Option<Vec<String>>,
    #[serde(rename = "qualityGates", default)]
    pub quality_gates: Option<Value>,
    #[serde(rename = "idempotencyKey", default)]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BuildTemplateResponse {
    pub id: String,
    #[serde(rename = "templateName")]
    pub template_name: String,
    #[serde(rename = "templateVersion")]
    pub template_version: String,
    pub platform: String,
    #[serde(rename = "techStack")]
    pub tech_stack: String,
    #[serde(rename = "toolchain", skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<Value>,
    #[serde(rename = "commands", skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<String>>,
    #[serde(rename = "artifactOutputs", skip_serializing_if = "Option::is_none")]
    pub artifact_outputs: Option<Vec<String>>,
    #[serde(rename = "qualityGates", skip_serializing_if = "Option::is_none")]
    pub quality_gates: Option<Value>,
    #[serde(rename = "templateStatus")]
    pub template_status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BuildTemplatePage {
    pub items: Vec<BuildTemplateResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateBuildRequest {
    #[serde(rename = "platformTargetId")]
    pub platform_target_id: String,
    #[serde(rename = "sourceRepositoryId", default)]
    pub source_repository_id: Option<String>,
    #[serde(rename = "sourceRef", default)]
    pub source_ref: Option<String>,
    #[serde(rename = "templateId", default)]
    pub template_id: Option<String>,
    #[serde(rename = "semanticVersion", default)]
    pub semantic_version: Option<String>,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BuildResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "platformTargetId")]
    pub platform_target_id: String,
    #[serde(rename = "templateId", skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    #[serde(rename = "buildNumber")]
    pub build_number: i64,
    #[serde(rename = "sourceRepositoryId", skip_serializing_if = "Option::is_none")]
    pub source_repository_id: Option<String>,
    #[serde(rename = "sourceRef", skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(rename = "sourceSnapshot", skip_serializing_if = "Option::is_none")]
    pub source_snapshot: Option<Value>,
    #[serde(rename = "buildStatus")]
    pub build_status: String,
    #[serde(rename = "logRef", skip_serializing_if = "Option::is_none")]
    pub log_ref: Option<String>,
    #[serde(rename = "producedPackageId", skip_serializing_if = "Option::is_none")]
    pub produced_package_id: Option<String>,
    #[serde(rename = "qualityGate", skip_serializing_if = "Option::is_none")]
    pub quality_gate: Option<Value>,
    #[serde(rename = "runnerNodeUuid", skip_serializing_if = "Option::is_none")]
    pub runner_node_uuid: Option<String>,
    #[serde(rename = "errorCode", skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(rename = "startedAt", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(rename = "finishedAt", skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(rename = "durationMs", skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BuildPage {
    pub items: Vec<BuildResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

/// Runner-reported build state transition (typed executor contract).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateBuildStateRequest {
    #[serde(rename = "buildStatus")]
    pub build_status: BuildStatus,
    #[serde(rename = "runnerNodeUuid")]
    pub runner_node_uuid: String,
    #[serde(rename = "runnerVersion", default)]
    pub runner_version: Option<String>,
    #[serde(rename = "logRef", default)]
    pub log_ref: Option<String>,
    #[serde(rename = "sourceSnapshot", default)]
    pub source_snapshot: Option<Value>,
    #[serde(rename = "qualityGate", default)]
    pub quality_gate: Option<Value>,
    #[serde(rename = "errorCode", default)]
    pub error_code: Option<String>,
    #[serde(rename = "startedAt", default)]
    pub started_at: Option<String>,
    #[serde(rename = "finishedAt", default)]
    pub finished_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Package
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterPackageRequest {
    #[serde(rename = "platformTargetId")]
    pub platform_target_id: String,
    #[serde(rename = "buildId")]
    pub build_id: String,
    #[serde(rename = "packageFormat")]
    pub package_format: PackageFormat,
    #[serde(rename = "semanticVersion")]
    pub semantic_version: String,
    #[serde(rename = "packageSizeBytes")]
    pub package_size_bytes: i64,
    #[serde(rename = "checksumSha256")]
    pub checksum_sha256: String,
    #[serde(rename = "manifestSha256")]
    pub manifest_sha256: String,
    #[serde(rename = "driveNodeId")]
    pub drive_node_id: String,
    #[serde(rename = "driveSpaceId", default)]
    pub drive_space_id: Option<String>,
    #[serde(rename = "signingIdentityId", default)]
    pub signing_identity_id: Option<String>,
    #[serde(rename = "minPlatformVersion", default)]
    pub min_platform_version: Option<String>,
    #[serde(rename = "architectures", default)]
    pub architectures: Option<Vec<String>>,
    #[serde(rename = "bundleIdentity", default)]
    pub bundle_identity: Option<Value>,
    #[serde(rename = "validationReport", default)]
    pub validation_report: Option<Value>,
    #[serde(rename = "idempotencyKey", default)]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PackageResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "platformTargetId")]
    pub platform_target_id: String,
    #[serde(rename = "buildId")]
    pub build_id: String,
    #[serde(rename = "packageFormat")]
    pub package_format: String,
    #[serde(rename = "semanticVersion")]
    pub semantic_version: String,
    #[serde(rename = "packageSizeBytes")]
    pub package_size_bytes: i64,
    #[serde(rename = "checksumSha256")]
    pub checksum_sha256: String,
    #[serde(rename = "manifestSha256")]
    pub manifest_sha256: String,
    #[serde(rename = "driveNodeId", skip_serializing_if = "Option::is_none")]
    pub drive_node_id: Option<String>,
    #[serde(rename = "signingIdentityId", skip_serializing_if = "Option::is_none")]
    pub signing_identity_id: Option<String>,
    #[serde(rename = "minPlatformVersion", skip_serializing_if = "Option::is_none")]
    pub min_platform_version: Option<String>,
    #[serde(rename = "architectures", skip_serializing_if = "Option::is_none")]
    pub architectures: Option<Vec<String>>,
    #[serde(rename = "packageStatus")]
    pub package_status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PackagePage {
    pub items: Vec<PackageResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Release
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateAppReleaseRequest {
    #[serde(rename = "platformTargetId")]
    pub platform_target_id: String,
    #[serde(rename = "packageId")]
    pub package_id: String,
    #[serde(rename = "semanticVersion")]
    pub semantic_version: String,
    #[serde(rename = "releaseNotes", default)]
    pub release_notes: Option<Value>,
    #[serde(rename = "releaseStatus", default)]
    pub release_status: Option<ReleaseStatus>,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppReleaseResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "platformTargetId")]
    pub platform_target_id: String,
    #[serde(rename = "packageId")]
    pub package_id: String,
    #[serde(rename = "semanticVersion")]
    pub semantic_version: String,
    #[serde(rename = "buildNumber")]
    pub build_number: i64,
    #[serde(rename = "releaseStatus")]
    pub release_status: String,
    #[serde(rename = "releaseNotes", skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<Value>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppReleasePage {
    pub items: Vec<AppReleaseResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Channel and rollout
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromoteChannelRequest {
    #[serde(rename = "releaseId")]
    pub release_id: String,
    #[serde(rename = "strategy", default)]
    pub strategy: Option<RolloutStrategy>,
    #[serde(default)]
    pub percentage: Option<u32>,
    #[serde(rename = "idempotencyKey", default)]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "platformTargetId")]
    pub platform_target_id: String,
    #[serde(rename = "channelKey")]
    pub channel_key: String,
    #[serde(rename = "currentReleaseId", skip_serializing_if = "Option::is_none")]
    pub current_release_id: Option<String>,
    #[serde(
        rename = "currentReleaseVersion",
        skip_serializing_if = "Option::is_none"
    )]
    pub current_release_version: Option<String>,
    #[serde(rename = "channelStatus")]
    pub channel_status: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelPage {
    pub items: Vec<ChannelResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelRolloutResponse {
    pub id: String,
    #[serde(rename = "channelId")]
    pub channel_id: String,
    #[serde(rename = "releaseId")]
    pub release_id: String,
    #[serde(rename = "releaseVersion")]
    pub release_version: String,
    pub strategy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<u32>,
    #[serde(rename = "rolloutStatus")]
    pub rollout_status: String,
    #[serde(
        rename = "supersedesRolloutId",
        skip_serializing_if = "Option::is_none"
    )]
    pub supersedes_rollout_id: Option<String>,
    #[serde(rename = "requestedAt")]
    pub requested_at: String,
    #[serde(rename = "completedAt", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelRolloutPage {
    pub items: Vec<ChannelRolloutResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Deployment
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateAppDeploymentRequest {
    #[serde(rename = "platformTargetId")]
    pub platform_target_id: String,
    #[serde(rename = "releaseId")]
    pub release_id: String,
    #[serde(rename = "deploymentKind")]
    pub deployment_kind: DeploymentKind,
    #[serde(rename = "deploymentTarget")]
    pub deployment_target: DeploymentTarget,
    #[serde(rename = "environment", default)]
    pub environment: Option<String>,
    #[serde(rename = "strategy", default)]
    pub strategy: Option<RolloutStrategy>,
    #[serde(default)]
    pub percentage: Option<u32>,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppDeploymentResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "platformTargetId", skip_serializing_if = "Option::is_none")]
    pub platform_target_id: Option<String>,
    #[serde(rename = "releaseId", skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(rename = "deploymentKind", skip_serializing_if = "Option::is_none")]
    pub deployment_kind: Option<String>,
    #[serde(rename = "deploymentTarget", skip_serializing_if = "Option::is_none")]
    pub deployment_target: Option<String>,
    #[serde(rename = "environment")]
    pub environment: String,
    #[serde(rename = "strategy", skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<u32>,
    #[serde(rename = "platformReviewRef", skip_serializing_if = "Option::is_none")]
    pub platform_review_ref: Option<String>,
    #[serde(rename = "deploymentStatus")]
    pub deployment_status: String,
    #[serde(
        rename = "rollbackFromDeploymentId",
        skip_serializing_if = "Option::is_none"
    )]
    pub rollback_from_deployment_id: Option<String>,
    #[serde(rename = "startedAt", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(rename = "completedAt", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppDeploymentPage {
    pub items: Vec<AppDeploymentResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Signing identity
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateSigningIdentityRequest {
    #[serde(rename = "identityName")]
    pub identity_name: String,
    #[serde(rename = "signingKind")]
    pub signing_kind: SigningKind,
    #[serde(rename = "platformTargetId", default)]
    pub platform_target_id: Option<String>,
    #[serde(rename = "fingerprintSha256", default)]
    pub fingerprint_sha256: Option<String>,
    #[serde(rename = "expiresAt", default)]
    pub expires_at: Option<String>,
    #[serde(rename = "secretRef", default)]
    pub secret_ref: Option<String>,
    #[serde(rename = "idempotencyKey", default)]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SigningIdentityResponse {
    pub id: String,
    #[serde(rename = "identityName")]
    pub identity_name: String,
    #[serde(rename = "signingKind")]
    pub signing_kind: String,
    #[serde(rename = "platformTargetId", skip_serializing_if = "Option::is_none")]
    pub platform_target_id: Option<String>,
    #[serde(rename = "fingerprintSha256", skip_serializing_if = "Option::is_none")]
    pub fingerprint_sha256: Option<String>,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(rename = "secretRef", skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<String>,
    #[serde(rename = "identityStatus")]
    pub identity_status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SigningIdentityPage {
    pub items: Vec<SigningIdentityResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Usage metering
// ---------------------------------------------------------------------------

/// Canonical usage dimensions emitted by Deploy.
pub const USAGE_DIMENSION_BUILD_MINUTES: &str = "build_minutes";
pub const USAGE_DIMENSION_PACKAGE_STORAGE_BYTES: &str = "package_storage_bytes";
pub const USAGE_DIMENSION_DEPLOYMENT_COUNT: &str = "deployment_count";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UsageEventResponse {
    pub id: String,
    #[serde(rename = "tenantId")]
    pub tenant_id: i64,
    #[serde(rename = "appId", skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(rename = "bindingId", skip_serializing_if = "Option::is_none")]
    pub binding_id: Option<String>,
    #[serde(rename = "periodStart")]
    pub period_start: String,
    pub dimension: String,
    pub quantity: i64,
    pub unit: String,
    #[serde(rename = "sourceTargetUuid", skip_serializing_if = "Option::is_none")]
    pub source_target_uuid: Option<String>,
    #[serde(rename = "sourceWindowId", skip_serializing_if = "Option::is_none")]
    pub source_window_id: Option<String>,
    #[serde(rename = "deduplicationKey")]
    pub deduplication_key: String,
    #[serde(
        rename = "attribution",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub attribution: Option<crate::usage::UsageEventAttribution>,
    #[serde(rename = "observedAt")]
    pub observed_at: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UsageEventPage {
    pub items: Vec<UsageEventResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Application database structure contract (REQ-2026-0002 extension)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DatabaseEngine {
    Postgres,
    Mysql,
    Sqlite,
}

impl DatabaseEngine {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "POSTGRES",
            Self::Mysql => "MYSQL",
            Self::Sqlite => "SQLITE",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "POSTGRES" => Some(Self::Postgres),
            "MYSQL" => Some(Self::Mysql),
            "SQLITE" => Some(Self::Sqlite),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DatabaseProfileStatus {
    Draft,
    Ready,
    Active,
    Superseded,
    Archived,
}

impl DatabaseProfileStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Ready => "READY",
            Self::Active => "ACTIVE",
            Self::Superseded => "SUPERSEDED",
            Self::Archived => "ARCHIVED",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DatabaseMigrationStatus {
    Pending,
    Applied,
    Failed,
    Superseded,
}

impl DatabaseMigrationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Applied => "APPLIED",
            Self::Failed => "FAILED",
            Self::Superseded => "SUPERSEDED",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateAppDatabaseProfileRequest {
    pub profile_key: String,
    pub db_engine: String,
    pub catalog_name: String,
    #[serde(rename = "schemaVersion", skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
    #[serde(rename = "baselineVersion", skip_serializing_if = "Option::is_none")]
    pub baseline_version: Option<String>,
    #[serde(rename = "migrationStrategy", skip_serializing_if = "Option::is_none")]
    pub migration_strategy: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateAppDatabaseProfileRequest {
    #[serde(rename = "schemaVersion", skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
    #[serde(rename = "baselineVersion", skip_serializing_if = "Option::is_none")]
    pub baseline_version: Option<String>,
    #[serde(rename = "migrationStrategy", skip_serializing_if = "Option::is_none")]
    pub migration_strategy: Option<String>,
    #[serde(rename = "profileStatus", skip_serializing_if = "Option::is_none")]
    pub profile_status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateAppDatabaseMigrationRequest {
    #[serde(rename = "migrationVersion")]
    pub migration_version: String,
    #[serde(rename = "migrationName")]
    pub migration_name: String,
    #[serde(rename = "checksumSha256")]
    pub checksum_sha256: String,
    #[serde(rename = "scriptRef", skip_serializing_if = "Option::is_none")]
    pub script_ref: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppDatabaseProfileResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "profileKey")]
    pub profile_key: String,
    #[serde(rename = "dbEngine")]
    pub db_engine: String,
    #[serde(rename = "catalogName")]
    pub catalog_name: String,
    #[serde(rename = "schemaVersion", skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
    #[serde(rename = "baselineVersion", skip_serializing_if = "Option::is_none")]
    pub baseline_version: Option<String>,
    #[serde(rename = "migrationStrategy")]
    pub migration_strategy: String,
    #[serde(rename = "profileStatus")]
    pub profile_status: String,
    #[serde(rename = "migrationCount")]
    pub migration_count: i64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppDatabaseProfilePage {
    pub items: Vec<AppDatabaseProfileResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppDatabaseMigrationResponse {
    pub id: String,
    #[serde(rename = "profileId")]
    pub profile_id: String,
    #[serde(rename = "migrationVersion")]
    pub migration_version: String,
    #[serde(rename = "migrationName")]
    pub migration_name: String,
    #[serde(rename = "checksumSha256")]
    pub checksum_sha256: String,
    #[serde(rename = "scriptRef", skip_serializing_if = "Option::is_none")]
    pub script_ref: Option<String>,
    #[serde(rename = "migrationStatus")]
    pub migration_status: String,
    #[serde(rename = "appliedAt", skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppDatabaseMigrationPage {
    pub items: Vec<AppDatabaseMigrationResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Entitlement consumption enforcement (TECH §4.6, migration 0008)
// ---------------------------------------------------------------------------

pub const ENTITLEMENT_DIMENSION_ACTIVE_APPS: &str = "active_apps";
pub const ENTITLEMENT_DIMENSION_PLATFORM_TARGETS: &str = "platform_targets";
pub const ENTITLEMENT_DIMENSION_BUILD_CONCURRENCY: &str = "build_concurrency";
pub const ENTITLEMENT_DIMENSION_PACKAGE_STORAGE_BYTES: &str = "package_storage_bytes";
pub const ENTITLEMENT_DIMENSION_RELEASE_COUNT: &str = "release_count";
pub const ENTITLEMENT_DIMENSION_DEPLOYMENT_COUNT: &str = "deployment_count";
pub const ENTITLEMENT_DIMENSION_CHANNEL_COUNT: &str = "channel_count";
pub const ENTITLEMENT_DIMENSION_TRAFFIC_REQUESTS: &str = "traffic.requests";
pub const ENTITLEMENT_DIMENSION_TRAFFIC_INGRESS_BYTES: &str = "traffic.ingress_bytes";
pub const ENTITLEMENT_DIMENSION_TRAFFIC_EGRESS_BYTES: &str = "traffic.egress_bytes";

/// Commerce-backed entitlement projection read model (backend read surface).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitlementProjectionResponse {
    pub id: String,
    #[serde(rename = "tenantId")]
    pub tenant_id: i64,
    #[serde(rename = "sourceSystem")]
    pub source_system: String,
    #[serde(rename = "sourceSubscriptionUuid")]
    pub source_subscription_uuid: String,
    #[serde(rename = "sourceRevision", skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
    #[serde(rename = "planKey", skip_serializing_if = "Option::is_none")]
    pub plan_key: Option<String>,
    #[serde(rename = "entitlements")]
    pub entitlements: serde_json::Value,
    #[serde(rename = "effectiveAt")]
    pub effective_at: String,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(rename = "projectionStatus")]
    pub projection_status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EntitlementProjectionPage {
    pub items: Vec<EntitlementProjectionResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// Backend build fleet administration (TECH §8)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildQueueItemResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "platformTargetId")]
    pub platform_target_id: String,
    #[serde(rename = "buildNumber")]
    pub build_number: i64,
    #[serde(rename = "buildStatus")]
    pub build_status: String,
    #[serde(rename = "runnerNodeUuid", skip_serializing_if = "Option::is_none")]
    pub runner_node_uuid: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BuildQueuePage {
    pub items: Vec<BuildQueueItemResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunnerHealthResponse {
    #[serde(rename = "runnerNodeUuid")]
    pub runner_node_uuid: String,
    #[serde(rename = "runnerVersion", skip_serializing_if = "Option::is_none")]
    pub runner_version: Option<String>,
    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: String,
    #[serde(rename = "buildsCompleted")]
    pub builds_completed: i64,
    #[serde(rename = "buildsFailed")]
    pub builds_failed: i64,
    #[serde(rename = "activeBuilds")]
    pub active_builds: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunnerHealthPage {
    pub items: Vec<RunnerHealthResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// TLS control plane (TECH-cloud-app-publishing §4.5): ACME account, order,
// challenge, and certificate version orchestration read models
// ---------------------------------------------------------------------------

pub const ACME_CA_LETS_ENCRYPT_STAGING: &str = "LETS_ENCRYPT_STAGING";
pub const ACME_CA_LETS_ENCRYPT_PRODUCTION: &str = "LETS_ENCRYPT_PRODUCTION";
pub const ORDER_STATUS_REQUESTED: &str = "REQUESTED";
pub const ORDER_STATUS_VERSION_STORED: &str = "VERSION_STORED";
pub const ORDER_STATUS_FAILED: &str = "FAILED";
pub const ORDER_STATUS_CANCELLED: &str = "CANCELLED";
pub const CHALLENGE_TYPE_HTTP_01: &str = "HTTP_01";
pub const CHALLENGE_TYPE_DNS_01: &str = "DNS_01";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateAcmeAccountRequest {
    pub ca_profile: String,
    #[serde(rename = "directoryUrl")]
    pub directory_url: String,
    #[serde(rename = "contactEmail")]
    pub contact_email: String,
    #[serde(
        rename = "externalAccountDigest",
        skip_serializing_if = "Option::is_none"
    )]
    pub external_account_digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AcmeAccountResponse {
    pub id: String,
    #[serde(rename = "tenantId")]
    pub tenant_id: i64,
    #[serde(rename = "caProfile")]
    pub ca_profile: String,
    #[serde(rename = "directoryUrl")]
    pub directory_url: String,
    #[serde(rename = "contactEmail")]
    pub contact_email: String,
    #[serde(
        rename = "externalAccountDigest",
        skip_serializing_if = "Option::is_none"
    )]
    pub external_account_digest: Option<String>,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AcmeAccountPage {
    pub items: Vec<AcmeAccountResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestCertificateOrderRequest {
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
    #[serde(rename = "challengeType", skip_serializing_if = "Option::is_none")]
    pub challenge_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertificateOrderResponse {
    pub id: String,
    #[serde(rename = "tenantId")]
    pub tenant_id: i64,
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    #[serde(rename = "acmeAccountId")]
    pub acme_account_id: String,
    #[serde(rename = "requestedVersionNo")]
    pub requested_version_no: i64,
    #[serde(rename = "requestSha256")]
    pub request_sha256: String,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
    #[serde(
        rename = "externalOrderDigest",
        skip_serializing_if = "Option::is_none"
    )]
    pub external_order_digest: Option<String>,
    pub status: String,
    #[serde(rename = "attemptCount")]
    pub attempt_count: i32,
    #[serde(rename = "lastErrorCode", skip_serializing_if = "Option::is_none")]
    pub last_error_code: Option<String>,
    /// CAA pre-issuance decision, recorded on the order so a refusal is
    /// auditable without re-querying DNS.
    #[serde(rename = "caaDecision", skip_serializing_if = "Option::is_none")]
    pub caa_decision: Option<String>,
    #[serde(rename = "caaCheckedAt", skip_serializing_if = "Option::is_none")]
    pub caa_checked_at: Option<String>,
    #[serde(rename = "deadlineAt")]
    pub deadline_at: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateOrderPage {
    pub items: Vec<CertificateOrderResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertificateChallengeResponse {
    pub id: String,
    #[serde(rename = "tenantId")]
    pub tenant_id: i64,
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(rename = "identifierId")]
    pub identifier_id: String,
    #[serde(rename = "hostname")]
    pub hostname: String,
    #[serde(rename = "challengeType")]
    pub challenge_type: String,
    #[serde(rename = "proofSha256")]
    pub proof_sha256: String,
    #[serde(rename = "presentationRef", skip_serializing_if = "Option::is_none")]
    pub presentation_ref: Option<String>,
    /// Manual DNS-01 presentation. Present only while the challenge is still
    /// presentable; the orchestrator clears these once the authorization is
    /// validated, fails, or is cleaned up.
    #[serde(rename = "dnsRecordName", skip_serializing_if = "Option::is_none")]
    pub dns_record_name: Option<String>,
    #[serde(rename = "dnsRecordType", skip_serializing_if = "Option::is_none")]
    pub dns_record_type: Option<String>,
    #[serde(rename = "dnsRecordValue", skip_serializing_if = "Option::is_none")]
    pub dns_record_value: Option<String>,
    #[serde(
        rename = "presentationExpiresAt",
        skip_serializing_if = "Option::is_none"
    )]
    pub presentation_expires_at: Option<String>,
    pub status: String,
    #[serde(rename = "attemptCount")]
    pub attempt_count: i32,
    #[serde(rename = "checkedAt", skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
    #[serde(rename = "validatedAt", skip_serializing_if = "Option::is_none")]
    pub validated_at: Option<String>,
    #[serde(rename = "lastErrorCode", skip_serializing_if = "Option::is_none")]
    pub last_error_code: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateChallengePage {
    pub items: Vec<CertificateChallengeResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

pub const ORDER_STATUS_ACCOUNT_READY: &str = "ACCOUNT_READY";
pub const ORDER_STATUS_ORDER_PENDING: &str = "ORDER_PENDING";
pub const ORDER_STATUS_CHALLENGE_PRESENTING: &str = "CHALLENGE_PRESENTING";
pub const ORDER_STATUS_CHALLENGE_VALIDATING: &str = "CHALLENGE_VALIDATING";
pub const ORDER_STATUS_FINALIZING: &str = "FINALIZING";

/// Reason codes recorded on `deploy_certificate_order.last_error_code`.
///
/// Codes, not sentences: an operator filters on them and the console renders them,
/// so a message that varies per attempt would make either impossible. Each code
/// names *who* stopped the order — the request was rejected, the CA refused, or the
/// control plane simply ran out of time — because that is what decides whether
/// retrying is useful.
pub const ORDER_ERROR_VALIDATION_FAILED: &str = "ORDER_VALIDATION_FAILED";
pub const ORDER_ERROR_CONFLICT: &str = "ORDER_CONFLICT";
pub const ORDER_ERROR_NOT_FOUND: &str = "ORDER_NOT_FOUND";
pub const ORDER_ERROR_FORBIDDEN: &str = "ORDER_FORBIDDEN";
pub const ORDER_ERROR_QUOTA_EXCEEDED: &str = "ORDER_QUOTA_EXCEEDED";
pub const ORDER_ERROR_DATABASE_UNAVAILABLE: &str = "ORDER_DATABASE_UNAVAILABLE";
pub const ORDER_ERROR_INTERNAL: &str = "ORDER_INTERNAL_ERROR";
/// The order outlived its deadline without reaching a stored version.
pub const ORDER_ERROR_DEADLINE_EXCEEDED: &str = "ORDER_DEADLINE_EXCEEDED";

/// CAA decisions recorded on an order before the ACME order is created
/// (RFC 8659). Any value other than `PERMITTED` blocks issuance.
pub const CAA_DECISION_PERMITTED: &str = "PERMITTED";
pub const CAA_DECISION_UNAUTHORIZED_CA: &str = "UNAUTHORIZED_CA";
pub const CAA_DECISION_LOOKUP_FAILED: &str = "LOOKUP_FAILED";

/// DNS record type a DNS-01 challenge is published as.
pub const DNS01_RECORD_TYPE: &str = "TXT";

/// One renewal attempt, as returned by the certificate renewal history endpoint.
///
/// The previous/new window pair is the reason this record exists rather than
/// being folded into the certificate row: it is what lets "which certificate
/// covered this name between these two instants" be answered after the version it
/// describes has been superseded and its own row no longer says anything about
/// the current state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertificateRenewalResponse {
    pub id: String,
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    #[serde(rename = "triggerKind")]
    pub trigger_kind: String,
    pub status: String,
    #[serde(rename = "attemptNo")]
    pub attempt_no: i32,
    #[serde(rename = "previousVersionId", skip_serializing_if = "Option::is_none")]
    pub previous_version_id: Option<String>,
    #[serde(rename = "resultingVersionId", skip_serializing_if = "Option::is_none")]
    pub resulting_version_id: Option<String>,
    /// Validity window of the version this attempt replaced, captured when the
    /// attempt was claimed.
    #[serde(rename = "previousNotBefore", skip_serializing_if = "Option::is_none")]
    pub previous_not_before: Option<String>,
    #[serde(rename = "previousNotAfter", skip_serializing_if = "Option::is_none")]
    pub previous_not_after: Option<String>,
    /// Validity window of the version this attempt produced; present only once
    /// the attempt succeeded.
    #[serde(rename = "newNotBefore", skip_serializing_if = "Option::is_none")]
    pub new_not_before: Option<String>,
    #[serde(rename = "newNotAfter", skip_serializing_if = "Option::is_none")]
    pub new_not_after: Option<String>,
    #[serde(rename = "scheduledAt")]
    pub scheduled_at: String,
    #[serde(rename = "startedAt", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(rename = "finishedAt", skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(rename = "lastErrorCode", skip_serializing_if = "Option::is_none")]
    pub last_error_code: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateRenewalPage {
    pub items: Vec<CertificateRenewalResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

/// Where a renewal attempt came from.
///
/// The distinction is recorded rather than inferred because it changes how an
/// operator reads a failure: a `SCHEDULED` failure is a broken automation that
/// will keep retrying on its own, while a `MANUAL` one was a deliberate act
/// whose result the operator is already watching.
pub const RENEWAL_TRIGGER_SCHEDULED: &str = "SCHEDULED";
pub const RENEWAL_TRIGGER_MANUAL: &str = "MANUAL";

/// Renewal attempt lifecycle.
///
/// `PLANNED` and `ORDERED` are open, and at most one attempt per certificate may
/// be open at a time; every other value is terminal and carries `finishedAt`.
/// `PLANNED` exists separately from `ORDERED` because claiming a certificate and
/// opening its order are two steps, and a worker that dies between them must be
/// distinguishable from one that never claimed the certificate at all.
pub const RENEWAL_STATUS_PLANNED: &str = "PLANNED";
pub const RENEWAL_STATUS_ORDERED: &str = "ORDERED";
pub const RENEWAL_STATUS_SUCCEEDED: &str = "SUCCEEDED";
pub const RENEWAL_STATUS_FAILED: &str = "FAILED";
pub const RENEWAL_STATUS_SKIPPED: &str = "SKIPPED";
pub const RENEWAL_STATUS_CANCELLED: &str = "CANCELLED";

/// Which part of a certificate's validity window it currently sits in.
///
/// Computed on read from the X.509 window and the configured lead time, never
/// stored: a persisted phase would be wrong from the instant the clock crossed a
/// boundary, and nothing would be around to correct it.
pub const CERTIFICATE_VALIDITY_PHASE_NOT_YET_VALID: &str = "NOT_YET_VALID";
pub const CERTIFICATE_VALIDITY_PHASE_VALID: &str = "VALID";
pub const CERTIFICATE_VALIDITY_PHASE_EXPIRING_SOON: &str = "EXPIRING_SOON";
pub const CERTIFICATE_VALIDITY_PHASE_EXPIRED: &str = "EXPIRED";

/// `_acme-challenge` record name an operator (or a provider adapter) must
/// publish for a DNS-01 challenge.
///
/// A wildcard identifier proves control of the base domain it expands from, so
/// `*.example.com` and `example.com` share one TXT name. Collapsing them is
/// deliberate: publishing two records under the same name with different values
/// makes the ACME validation result order-dependent, which is a well-known
/// source of intermittent DNS-01 failures.
pub fn dns01_challenge_record_name(hostname: &str) -> String {
    let base = crate::dto::wildcard_apex(hostname).unwrap_or(hostname);
    format!("_acme-challenge.{base}")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreCertificateVersionRequest {
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(rename = "versionNo")]
    pub version_no: i64,
    #[serde(rename = "serialSha256")]
    pub serial_sha256: String,
    #[serde(rename = "fingerprintSha256")]
    pub fingerprint_sha256: String,
    #[serde(rename = "spkiSha256")]
    pub spki_sha256: String,
    #[serde(rename = "chainSha256")]
    pub chain_sha256: String,
    pub issuer: String,
    pub subject: String,
    #[serde(rename = "keyAlgorithm")]
    pub key_algorithm: String,
    #[serde(rename = "notBefore")]
    pub not_before: String,
    #[serde(rename = "notAfter")]
    pub not_after: String,
    #[serde(rename = "secretBundleRef")]
    pub secret_bundle_ref: String,
    /// The PEM material itself.
    ///
    /// Required rather than optional: this endpoint is the only path by which a
    /// managed certificate enters the control plane, and a version whose material
    /// was never stored cannot be delivered to a node or audited afterwards. The
    /// four digests above are re-derived from these bytes and a disagreement
    /// refuses the whole request, so the recorded metadata and the stored
    /// material can never drift apart.
    pub material: CertificateMaterialPayload,
}

/// The PEM files of one issued certificate, as handed over by the issuance
/// worker.
///
/// The worker holds no custody: it transports the material and the control plane
/// seals it. Keeping the sealing on this side is what lets the master key exist
/// in exactly one place.
///
/// The chain is one field rather than a leaf plus a separate intermediate list
/// because that is what an ACME client actually receives, and because
/// `chainSha256` hashes the blob verbatim — asking the worker to split it first
/// would make the digest depend on how it chose to join the halves back
/// together.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertificateMaterialPayload {
    /// The certificate chain the CA returned, leaf first, then its
    /// intermediates — byte for byte what the client stores as `fullchain.pem`.
    ///
    /// The control plane splits this into `cert.pem` (first block), `chain.pem`
    /// (the rest) and `fullchain.pem` (the whole blob), and hashes it as given
    /// for the `chainSha256` cross-check.
    #[serde(rename = "certificateChainPem")]
    pub certificate_chain_pem: String,
    /// The private key, in PKCS#8 (`PRIVATE KEY`) or SEC1/PKCS#1 form.
    #[serde(rename = "privateKeyPem")]
    pub private_key_pem: String,
    /// The root certificate the chain terminates at.
    ///
    /// Optional, because most CAs do not send it — the anchor belongs in the
    /// client's trust store, so a chain normally stops at an intermediate. When
    /// this is empty the control plane establishes the anchor from the chain
    /// itself (if it ends at a self-signed certificate) or from its configured
    /// trust anchor bundle, and refuses the request if neither applies. It never
    /// stores an unanchored bundle.
    #[serde(rename = "rootPem", default)]
    pub root_pem: String,
}

/// Canonical certificate order state machine transitions (migration 0004).
pub const ORDER_TRANSITIONS: &[(&str, &str)] = &[
    ("REQUESTED", "ACCOUNT_READY"),
    ("ACCOUNT_READY", "ORDER_PENDING"),
    ("ORDER_PENDING", "CHALLENGE_PRESENTING"),
    ("CHALLENGE_PRESENTING", "CHALLENGE_VALIDATING"),
    ("CHALLENGE_VALIDATING", "FINALIZING"),
];

/// Returns the next order state, or `None` at a terminal state.
pub fn next_order_transition(status: &str) -> Option<&'static str> {
    ORDER_TRANSITIONS
        .iter()
        .find(|(from, _)| *from == status)
        .map(|(_, to)| *to)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailCertificateOrderRequest {
    #[serde(rename = "errorCode")]
    pub error_code: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChallengeResultRequest {
    #[serde(rename = "challengeId", skip_serializing_if = "Option::is_none")]
    pub challenge_id: Option<String>,
    pub valid: bool,
}

// ---------------------------------------------------------------------------
// Retention, reconciliation, and signing identity health (TECH §8, PRD §5.8)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetentionRunRequest {
    #[serde(rename = "dryRun", default = "default_true")]
    pub dry_run: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetentionRunResponse {
    #[serde(rename = "dryRun")]
    pub dry_run: bool,
    #[serde(rename = "packagesRetired")]
    pub packages_retired: i64,
    #[serde(rename = "releasesRetired")]
    pub releases_retired: i64,
    #[serde(rename = "buildLogsPurged")]
    pub build_logs_purged: i64,
    #[serde(rename = "packageRetentionDays")]
    pub package_retention_days: i64,
    #[serde(rename = "releaseRetentionDays")]
    pub release_retention_days: i64,
    #[serde(rename = "buildLogRetentionDays")]
    pub build_log_retention_days: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SigningIdentityHealthResponse {
    pub id: String,
    #[serde(rename = "tenantId")]
    pub tenant_id: i64,
    #[serde(rename = "identityName")]
    pub identity_name: String,
    #[serde(rename = "signingKind")]
    pub signing_kind: String,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(rename = "daysUntilExpiry", skip_serializing_if = "Option::is_none")]
    pub days_until_expiry: Option<i64>,
    #[serde(rename = "identityStatus")]
    pub identity_status: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SigningIdentityHealthPage {
    pub items: Vec<SigningIdentityHealthResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageReconciliationRequest {
    #[serde(rename = "windowStart", skip_serializing_if = "Option::is_none")]
    pub window_start: Option<String>,
    #[serde(rename = "windowEnd", skip_serializing_if = "Option::is_none")]
    pub window_end: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageReconciliationResponse {
    #[serde(rename = "rebuiltRows")]
    pub rebuilt_rows: i64,
    #[serde(rename = "windowStart")]
    pub window_start: String,
    #[serde(rename = "windowEnd")]
    pub window_end: String,
}

// ---------------------------------------------------------------------------
// Application environments and promotion chain (P0 product gap)
// ---------------------------------------------------------------------------

pub const ENVIRONMENT_LEVEL_DEVELOPMENT: &str = "DEVELOPMENT";
pub const ENVIRONMENT_LEVEL_STAGING: &str = "STAGING";
pub const ENVIRONMENT_LEVEL_PRODUCTION: &str = "PRODUCTION";
pub const ENVIRONMENT_STATUS_ACTIVE: &str = "ACTIVE";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateAppEnvironmentRequest {
    #[serde(rename = "envKey")]
    pub env_key: String,
    #[serde(rename = "envName")]
    pub env_name: String,
    #[serde(rename = "envLevel")]
    pub env_level: String,
    #[serde(rename = "approvalRequired", default)]
    pub approval_required: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateAppEnvironmentRequest {
    #[serde(rename = "envName", skip_serializing_if = "Option::is_none")]
    pub env_name: Option<String>,
    #[serde(rename = "approvalRequired", skip_serializing_if = "Option::is_none")]
    pub approval_required: Option<bool>,
    #[serde(rename = "envStatus", skip_serializing_if = "Option::is_none")]
    pub env_status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppEnvironmentResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "envKey")]
    pub env_key: String,
    #[serde(rename = "envName")]
    pub env_name: String,
    #[serde(rename = "envLevel")]
    pub env_level: String,
    #[serde(rename = "approvalRequired")]
    pub approval_required: bool,
    #[serde(rename = "currentReleaseId", skip_serializing_if = "Option::is_none")]
    pub current_release_id: Option<String>,
    #[serde(
        rename = "currentReleaseVersion",
        skip_serializing_if = "Option::is_none"
    )]
    pub current_release_version: Option<String>,
    #[serde(rename = "envStatus")]
    pub env_status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppEnvironmentPage {
    pub items: Vec<AppEnvironmentResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromoteEnvironmentRequest {
    #[serde(rename = "releaseId")]
    pub release_id: String,
    #[serde(rename = "fromEnvironmentId", skip_serializing_if = "Option::is_none")]
    pub from_environment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnvironmentPromotionResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "environmentId")]
    pub environment_id: String,
    #[serde(rename = "environmentKey")]
    pub environment_key: String,
    #[serde(rename = "releaseId")]
    pub release_id: String,
    #[serde(rename = "releaseVersion")]
    pub release_version: String,
    #[serde(rename = "fromEnvironmentId", skip_serializing_if = "Option::is_none")]
    pub from_environment_id: Option<String>,
    #[serde(rename = "fromEnvironmentKey", skip_serializing_if = "Option::is_none")]
    pub from_environment_key: Option<String>,
    #[serde(rename = "promotedBy", skip_serializing_if = "Option::is_none")]
    pub promoted_by: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EnvironmentPromotionPage {
    pub items: Vec<EnvironmentPromotionResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

// ---------------------------------------------------------------------------
// CI source events (P0 product gap)
// ---------------------------------------------------------------------------

pub const SOURCE_EVENT_KIND_PUSH: &str = "PUSH";
pub const SOURCE_EVENT_STATUS_PROCESSED: &str = "PROCESSED";
pub const SOURCE_EVENT_STATUS_SKIPPED: &str = "SKIPPED";
pub const SOURCE_EVENT_STATUS_FAILED: &str = "FAILED";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceEventResponse {
    pub id: String,
    #[serde(rename = "tenantId")]
    pub tenant_id: i64,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "sourceRepositoryId")]
    pub source_repository_id: String,
    #[serde(rename = "eventKind")]
    pub event_kind: String,
    #[serde(rename = "sourceRef")]
    pub source_ref: String,
    #[serde(rename = "sourceCommit")]
    pub source_commit: String,
    #[serde(rename = "commitMessage", skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
    #[serde(rename = "payloadSha256")]
    pub payload_sha256: String,
    #[serde(rename = "eventStatus")]
    pub event_status: String,
    #[serde(rename = "buildsTriggered")]
    pub builds_triggered: i32,
    #[serde(rename = "errorCode", skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(rename = "processedAt", skip_serializing_if = "Option::is_none")]
    pub processed_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceEventPage {
    pub items: Vec<SourceEventResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceEventIngestResponse {
    #[serde(rename = "eventId")]
    pub event_id: String,
    #[serde(rename = "eventStatus")]
    pub event_status: String,
    #[serde(rename = "buildsTriggered")]
    pub builds_triggered: i32,
    #[serde(rename = "duplicate")]
    pub duplicate: bool,
}
