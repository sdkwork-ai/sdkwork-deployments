//! Download catalog and per-installer update configuration contracts.
//!
//! Deploy owns the download catalog that used to be a free-form JSON document in
//! the Cloud Router site settings: one catalog per product, cards for the
//! download page groups, one item per platform/architecture installer, and at
//! most one update configuration per item. The rendering site keeps only a
//! `catalog_key`; the desktop updater reads the public projection of the same
//! row, so the package a visitor downloads and the feed an installed client
//! follows can never disagree.
//!
//! Two traits split the surface:
//!
//! - [`DeployDownloadsApi`] is the operator-facing backend surface (write), used
//!   by the Deploy console.
//! - [`DeployDownloadsReadApi`] is the anonymous public surface (read), used by
//!   the rendering site and by desktop clients checking for updates. It only
//!   ever sees `PUBLISHED` catalogs, and it never exposes tenant identifiers.
//!
//! Installers reuse the release domain (`deploy_package`, `deploy_artifact`,
//! `deploy_release_channel`) rather than duplicating artifacts or channels; this
//! module only carries the catalog's own presentation and update facts.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DeployBackendRequestContext, DeployServiceResult};

/// `deploy_download_catalog.status`: only `PUBLISHED` is readable publicly.
pub const DOWNLOAD_CATALOG_STATUS_DRAFT: &str = "DRAFT";
pub const DOWNLOAD_CATALOG_STATUS_PUBLISHED: &str = "PUBLISHED";
pub const DOWNLOAD_CATALOG_STATUS_ARCHIVED: &str = "ARCHIVED";

/// `deploy_download_update.provider`.
///
/// `DEPLOYMENTS` means Deploy generates the electron-updater feed itself from
/// the linked release channel; `GENERIC`/`GITHUB` point at an external feed that
/// already exists (the migration path for deployments that ship one today);
/// `NONE` pauses updates for this installer without removing it from the page.
pub const DOWNLOAD_UPDATE_PROVIDER_DEPLOYMENTS: &str = "deployments";
pub const DOWNLOAD_UPDATE_PROVIDER_GENERIC: &str = "generic";
pub const DOWNLOAD_UPDATE_PROVIDER_GITHUB: &str = "github";
pub const DOWNLOAD_UPDATE_PROVIDER_NONE: &str = "none";

/// Release channel keys accepted by the updater contract.
pub const DOWNLOAD_CHANNEL_NIGHTLY: &str = "nightly";
pub const DOWNLOAD_CHANNEL_STABLE: &str = "stable";

/// Installer platforms the catalog can publish.
pub const DOWNLOAD_PLATFORMS: [&str; 9] = [
    "windows",
    "macos",
    "linux",
    "android",
    "ios",
    "harmonyos",
    "docker",
    "helm",
    "generic",
];

/// Installer architectures the catalog can publish. `universal` installers run
/// on both architectures of a platform.
pub const DOWNLOAD_ARCHITECTURES: [&str; 3] = ["x64", "arm64", "universal"];

/// Version token of the public update-configuration document. Bumped whenever a
/// field changes meaning; clients refuse a document they do not understand and
/// fall back to their packaged feed.
pub const DOWNLOAD_UPDATE_SCHEMA_VERSION: &str = "2026-11-10.sdkwork-download-update-config.v1";

/// Version token of the public download catalog document. Kept identical to the
/// portal runtime's reader (`2026-05-18.sdkwork-download-catalog.v1`) so the
/// existing download section renders this server's projection unchanged.
pub const PUBLIC_DOWNLOAD_CATALOG_SCHEMA_VERSION: &str = "2026-05-18.sdkwork-download-catalog.v1";

/// One installer's auto-update configuration (operator-facing projection).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadUpdateDto {
    #[serde(default)]
    pub enabled: bool,
    pub provider: String,
    #[serde(rename = "feedUrl", default, skip_serializing_if = "String::is_empty")]
    pub feed_url: String,
    #[serde(
        rename = "githubOwner",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub github_owner: String,
    #[serde(
        rename = "githubRepo",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub github_repo: String,
    #[serde(rename = "channelId", default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(rename = "channelKey")]
    pub channel_key: String,
    #[serde(
        rename = "channelFile",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub channel_file: String,
    #[serde(rename = "allowedChannels", default)]
    pub allowed_channels: Vec<String>,
    #[serde(rename = "allowPrerelease")]
    pub allow_prerelease: bool,
    #[serde(rename = "allowDowngrade")]
    pub allow_downgrade: bool,
    #[serde(
        rename = "minimumSupportedVersion",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub minimum_supported_version: String,
    #[serde(rename = "checkIntervalMinutes")]
    pub check_interval_minutes: i32,
    #[serde(rename = "rolloutPercentage")]
    pub rollout_percentage: i32,
    #[serde(
        rename = "releaseNotesUrl",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub release_notes_url: String,
}

/// One downloadable installer (operator-facing projection).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadItemDto {
    pub id: String,
    #[serde(rename = "itemKey")]
    pub item_key: String,
    pub platform: String,
    pub architecture: String,
    pub label: String,
    #[serde(rename = "fileName", default, skip_serializing_if = "String::is_empty")]
    pub file_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(
        rename = "releaseTag",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub release_tag: String,
    #[serde(rename = "packageId", default, skip_serializing_if = "Option::is_none")]
    pub package_id: Option<String>,
    #[serde(
        rename = "artifactId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub artifact_id: Option<String>,
    /// `{"href": "https://..."}` or `{"drive": {...}}`; passed through verbatim.
    #[serde(rename = "downloadSource", default)]
    pub download_source: Value,
    #[serde(rename = "sizeBytes", default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    #[serde(
        rename = "checksumSha256",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub checksum_sha256: Option<String>,
    #[serde(rename = "primaryAction")]
    pub primary_action: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(
        rename = "unavailableLabel",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub unavailable_label: String,
    #[serde(rename = "sortOrder")]
    pub sort_order: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<DownloadUpdateDto>,
}

/// One download page group (operator-facing projection).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadCardDto {
    pub id: String,
    #[serde(rename = "cardKey")]
    pub card_key: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub badge: String,
    #[serde(rename = "primaryStrategy")]
    pub primary_strategy: String,
    pub hidden: bool,
    #[serde(rename = "sortOrder")]
    pub sort_order: i32,
    #[serde(default)]
    pub items: Vec<DownloadItemDto>,
}

/// One catalog with its cards and items (operator-facing aggregate).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadCatalogDto {
    pub id: String,
    #[serde(rename = "catalogKey")]
    pub catalog_key: String,
    #[serde(rename = "appId", default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(rename = "productKey")]
    pub product_key: String,
    #[serde(rename = "productName")]
    pub product_name: String,
    #[serde(rename = "productVersion")]
    pub product_version: String,
    #[serde(rename = "defaultChannelKey")]
    pub default_channel_key: String,
    pub status: String,
    pub revision: i64,
    #[serde(
        rename = "publishedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub published_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
    #[serde(default)]
    pub cards: Vec<DownloadCardDto>,
}

/// Catalog list row without the nested documents.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadCatalogSummaryDto {
    pub id: String,
    #[serde(rename = "catalogKey")]
    pub catalog_key: String,
    #[serde(rename = "productKey")]
    pub product_key: String,
    #[serde(rename = "productName")]
    pub product_name: String,
    #[serde(rename = "productVersion")]
    pub product_version: String,
    pub status: String,
    pub revision: i64,
    #[serde(
        rename = "publishedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub published_at: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

/// Create or update one catalog. Absent fields keep their stored value.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpsertDownloadCatalogRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_channel_key: Option<String>,
    #[serde(rename = "appId", default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
}

/// Create or update one card. Absent fields keep their stored value.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpsertDownloadCardRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_strategy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
}

/// Create or update one installer. Absent fields keep their stored value.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpsertDownloadItemRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_source: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_action: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
}

/// Create or update one installer's update configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpsertDownloadUpdateRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_channels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_prerelease: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_downgrade: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_supported_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_interval_minutes: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_percentage: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_notes_url: Option<String>,
}

/// Public catalog lookup: the rendering site asks for its own catalog key.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublicDownloadCatalogQuery {
    pub catalog_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// Absolute origin the caller reached this API through. Filled by the route
    /// from the request (never by the client) so artifact links stay on the
    /// deployment's own public origin instead of leaking an internal address.
    #[serde(skip)]
    pub base_url: Option<String>,
}

/// Public update-config lookup: a desktop client asks for its own installer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublicDownloadUpdateQuery {
    pub product_key: String,
    pub platform: String,
    pub architecture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// Absolute origin for the generated feed URL; filled by the route.
    #[serde(skip)]
    pub base_url: Option<String>,
}

/// Public feed lookup: the channel file the updater fetches.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublicDownloadFeedQuery {
    pub product_key: String,
    pub platform: String,
    pub architecture: String,
    pub channel: String,
    /// Absolute origin for the artifact link inside the feed; filled by the route.
    #[serde(skip)]
    pub base_url: Option<String>,
}

/// Public artifact lookup: stable redirect target for one installer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublicDownloadArtifactQuery {
    pub item_uuid: String,
}

/// Resolved download target the route turns into an HTTP redirect.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadArtifactTarget {
    pub url: String,
    pub file_name: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
}

/// Published catalog projection rendered by the download page.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublicDownloadCatalogDocument {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    pub product: PublicDownloadProduct,
    #[serde(default)]
    pub cards: Vec<PublicDownloadCard>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublicDownloadProduct {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub channel: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublicDownloadCard {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub badge: String,
    #[serde(rename = "primaryActionStrategy", default)]
    pub primary_action_strategy: String,
    pub actions: Vec<PublicDownloadAction>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublicDownloadAction {
    pub id: String,
    pub label: String,
    pub platform: String,
    pub architecture: String,
    pub href: String,
    #[serde(rename = "fileName", default, skip_serializing_if = "String::is_empty")]
    pub file_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(
        rename = "releaseTag",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub release_tag: String,
    #[serde(rename = "sizeBytes", default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub primary: bool,
    pub disabled: bool,
    #[serde(
        rename = "unavailableLabel",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub unavailable_label: String,
}

/// The document a desktop client reads before checking for updates.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadUpdateConfigDocument {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "catalogKey")]
    pub catalog_key: String,
    #[serde(rename = "itemKey")]
    pub item_key: String,
    #[serde(rename = "productKey")]
    pub product_key: String,
    pub platform: String,
    pub architecture: String,
    #[serde(default)]
    pub version: String,
    pub revision: String,
    pub update: PublicDownloadUpdateConfig,
}

/// Public update facts; the client mirrors this shape exactly.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublicDownloadUpdateConfig {
    pub enabled: bool,
    pub provider: String,
    #[serde(rename = "feedUrl", default, skip_serializing_if = "String::is_empty")]
    pub feed_url: String,
    #[serde(
        rename = "githubOwner",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub github_owner: String,
    #[serde(
        rename = "githubRepo",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub github_repo: String,
    pub channel: String,
    #[serde(
        rename = "channelFile",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub channel_file: String,
    #[serde(rename = "allowedChannels", default)]
    pub allowed_channels: Vec<String>,
    #[serde(rename = "allowPrerelease")]
    pub allow_prerelease: bool,
    #[serde(rename = "allowDowngrade")]
    pub allow_downgrade: bool,
    #[serde(
        rename = "minimumSupportedVersion",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub minimum_supported_version: String,
    #[serde(rename = "checkIntervalMinutes")]
    pub check_interval_minutes: i32,
    #[serde(rename = "rolloutPercentage")]
    pub rollout_percentage: i32,
    #[serde(
        rename = "releaseNotesUrl",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub release_notes_url: String,
}

/// electron-updater compatible feed document served for `provider=deployments`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadFeedDocument {
    pub version: String,
    pub files: Vec<DownloadFeedFile>,
    pub path: String,
    #[serde(rename = "releaseDate")]
    pub release_date: String,
    #[serde(
        rename = "stagingPercentage",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub staging_percentage: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadFeedFile {
    pub url: String,
    pub sha512: String,
    pub size: i64,
}

/// Operator-facing catalog and update configuration surface.
#[async_trait]
pub trait DeployDownloadsApi: Send + Sync {
    /// Lists every catalog of the caller's tenant (cross-tenant for operators).
    async fn list_download_catalogs(
        &self,
        context: &DeployBackendRequestContext,
    ) -> DeployServiceResult<Vec<DownloadCatalogSummaryDto>>;

    /// Reads one catalog with its cards, items and update configurations.
    async fn retrieve_download_catalog(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
    ) -> DeployServiceResult<DownloadCatalogDto>;

    /// Creates the catalog when absent and patches the supplied fields otherwise.
    async fn upsert_download_catalog(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        request: &UpsertDownloadCatalogRequest,
    ) -> DeployServiceResult<DownloadCatalogDto>;

    /// Publishes the catalog: bumps the revision and makes it publicly readable.
    async fn publish_download_catalog(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
    ) -> DeployServiceResult<DownloadCatalogDto>;

    /// Creates or patches one card of a catalog.
    async fn upsert_download_card(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        card_key: &str,
        request: &UpsertDownloadCardRequest,
    ) -> DeployServiceResult<DownloadCardDto>;

    /// Deletes one card and its items.
    async fn delete_download_card(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        card_key: &str,
    ) -> DeployServiceResult<()>;

    /// Creates or patches one installer of a card.
    async fn upsert_download_item(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
        request: &UpsertDownloadItemRequest,
    ) -> DeployServiceResult<DownloadItemDto>;

    /// Deletes one installer and its update configuration.
    async fn delete_download_item(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
    ) -> DeployServiceResult<()>;

    /// Creates or replaces one installer's update configuration.
    async fn upsert_download_update(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
        request: &UpsertDownloadUpdateRequest,
    ) -> DeployServiceResult<DownloadUpdateDto>;
}

/// Anonymous public surface consumed by the rendering site and desktop clients.
#[async_trait]
pub trait DeployDownloadsReadApi: Send + Sync {
    /// Published catalog rendered by the download page.
    async fn public_download_catalog(
        &self,
        query: &PublicDownloadCatalogQuery,
    ) -> DeployServiceResult<PublicDownloadCatalogDocument>;

    /// The update configuration a desktop client applies before checking.
    async fn public_download_update_config(
        &self,
        query: &PublicDownloadUpdateQuery,
    ) -> DeployServiceResult<DownloadUpdateConfigDocument>;

    /// The electron-updater feed for one installer channel.
    async fn public_download_feed(
        &self,
        query: &PublicDownloadFeedQuery,
    ) -> DeployServiceResult<DownloadFeedDocument>;

    /// Resolves the stable redirect target for one installer.
    async fn public_download_artifact(
        &self,
        query: &PublicDownloadArtifactQuery,
    ) -> DeployServiceResult<DownloadArtifactTarget>;
}
