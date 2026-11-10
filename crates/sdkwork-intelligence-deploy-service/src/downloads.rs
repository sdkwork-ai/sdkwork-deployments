//! Download catalog service: validation, public projection and feed assembly.
//!
//! The store owns persistence; this module owns the rules that decide whether a
//! catalog may be published and what a public reader is allowed to see:
//!
//! - Every enum written by an operator is checked against the contract
//!   vocabulary before it reaches SQL.
//! - One installer carries one update configuration, and that configuration is
//!   only valid when the provider it names has the facts it needs (a release
//!   channel for `deployments`, a feed URL for `generic`, an owner/repo pair for
//!   `github`).
//! - Public reads never see `DRAFT` catalogs, hidden cards, disabled installers
//!   or tenant identifiers. A catalog key published by more than one tenant is
//!   rejected as ambiguous instead of silently serving one tenant's catalog to
//!   another tenant's visitors.
//! - A feed is generated only for `provider = deployments`; the other providers
//!   exist so an operator can keep an external feed during migration.

use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use sdkwork_deploy_contract::{
    DeployBackendRequestContext, DeployDownloadsApi, DeployDownloadsReadApi, DeployServiceError,
    DeployServiceResult, DownloadArtifactTarget, DownloadCardDto, DownloadCatalogDto,
    DownloadCatalogSummaryDto, DownloadFeedDocument, DownloadItemDto, DownloadUpdateConfigDocument,
    DownloadUpdateDto, PublicDownloadAction, PublicDownloadCard, PublicDownloadCatalogDocument,
    PublicDownloadCatalogQuery, PublicDownloadFeedQuery, PublicDownloadProduct,
    PublicDownloadUpdateConfig, PublicDownloadUpdateQuery, UpsertDownloadCardRequest,
    UpsertDownloadCatalogRequest, UpsertDownloadItemRequest, UpsertDownloadUpdateRequest,
    DOWNLOAD_ARCHITECTURES, DOWNLOAD_CATALOG_STATUS_PUBLISHED, DOWNLOAD_PLATFORMS,
    DOWNLOAD_UPDATE_PROVIDER_DEPLOYMENTS, DOWNLOAD_UPDATE_PROVIDER_GENERIC,
    DOWNLOAD_UPDATE_PROVIDER_GITHUB, DOWNLOAD_UPDATE_PROVIDER_NONE, DOWNLOAD_UPDATE_SCHEMA_VERSION,
    PUBLIC_DOWNLOAD_CATALOG_SCHEMA_VERSION,
};
use sdkwork_deploy_feed::{build_feed, channel_filename, FeedError, FeedInput};

use crate::repository::PublishedDownloadRecord;
use crate::DeployService;

const ARTIFACT_PATH: &str = "/app/v3/api/downloads/artifact";
const FEED_PATH: &str = "/app/v3/api/downloads/feed";

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn trim(value: &str) -> &str {
    value.trim()
}

fn require_tenant(context: &DeployBackendRequestContext) -> DeployServiceResult<i64> {
    context
        .tenant_id
        .filter(|tenant_id| *tenant_id > 0)
        .ok_or_else(|| {
            DeployServiceError::validation("tenant context is required for download catalog writes")
        })
}

fn require_text(value: &str, field: &str) -> DeployServiceResult<()> {
    if trim(value).is_empty() {
        return Err(DeployServiceError::validation(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

fn validate_platform(value: &str) -> DeployServiceResult<()> {
    if DOWNLOAD_PLATFORMS.contains(&trim(value)) {
        return Ok(());
    }
    Err(DeployServiceError::validation(format!(
        "unsupported download platform: {value}"
    )))
}

fn validate_architecture(value: &str) -> DeployServiceResult<()> {
    if DOWNLOAD_ARCHITECTURES.contains(&trim(value)) {
        return Ok(());
    }
    Err(DeployServiceError::validation(format!(
        "unsupported download architecture: {value}"
    )))
}

fn validate_channel(value: &str) -> DeployServiceResult<()> {
    if sdkwork_deploy_feed::is_supported_channel(trim(value)) {
        return Ok(());
    }
    Err(DeployServiceError::validation(format!(
        "unsupported update channel: {value}"
    )))
}

fn validate_provider(value: &str) -> DeployServiceResult<()> {
    let value = trim(value);
    if [
        DOWNLOAD_UPDATE_PROVIDER_DEPLOYMENTS,
        DOWNLOAD_UPDATE_PROVIDER_GENERIC,
        DOWNLOAD_UPDATE_PROVIDER_GITHUB,
        DOWNLOAD_UPDATE_PROVIDER_NONE,
    ]
    .contains(&value)
    {
        return Ok(());
    }
    Err(DeployServiceError::validation(format!(
        "unsupported update provider: {value}"
    )))
}

fn validate_https_url(value: &str, field: &str) -> DeployServiceResult<()> {
    let value = trim(value);
    if value.is_empty() {
        return Ok(());
    }
    if !value.starts_with("https://") || value.contains(char::is_whitespace) {
        return Err(DeployServiceError::validation(format!(
            "{field} must be an absolute https URL"
        )));
    }
    Ok(())
}

/// Structural semver check: the service does not need ordering, only that the
/// operator typed a version and not an arbitrary string.
fn validate_version_shape(value: &str, field: &str) -> DeployServiceResult<()> {
    let value = trim(value);
    if value.is_empty() {
        return Ok(());
    }
    let core = value.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|part| part.parse::<u64>().is_err()) {
        return Err(DeployServiceError::validation(format!(
            "{field} must be a semantic version"
        )));
    }
    Ok(())
}

/// Compares two dotted versions; prerelease suffixes are ignored, which is the
/// only ordering the minimum-supported-version rule needs.
fn version_at_most(left: &str, right: &str) -> bool {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split(['-', '+'])
            .next()
            .unwrap_or("")
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let left = parse(left);
    let right = parse(right);
    for index in 0..3 {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        if a != b {
            return a < b;
        }
    }
    true
}

fn join_base(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}{path}")
}

fn resolve_base(explicit: &Option<String>) -> String {
    explicit
        .as_deref()
        .map(trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            std::env::var("SDKWORK_DEPLOY_PUBLIC_BASE_URL")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_default()
}

fn content_type_for(file_name: &str) -> String {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".exe") {
        "application/vnd.microsoft.portable-executable".to_owned()
    } else if lower.ends_with(".msi") {
        "application/x-msi".to_owned()
    } else if lower.ends_with(".dmg") {
        "application/x-apple-diskimage".to_owned()
    } else if lower.ends_with(".zip") {
        "application/zip".to_owned()
    } else if lower.ends_with(".pkg") {
        "application/octet-stream".to_owned()
    } else {
        "application/octet-stream".to_owned()
    }
}

fn file_name_of(item: &DownloadItemDto, url: &str) -> String {
    if !item.file_name.trim().is_empty() {
        return item.file_name.clone();
    }
    url.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("installer")
        .to_owned()
}

fn direct_href(item: &DownloadItemDto) -> Option<String> {
    item.download_source
        .get("href")
        .and_then(|value| value.as_str())
        .map(trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

impl DeployService {
    fn public_update_document(
        record: &PublishedDownloadRecord,
        base: &str,
    ) -> DeployServiceResult<DownloadUpdateConfigDocument> {
        let Some(update) = record.update.as_ref() else {
            return Err(DeployServiceError::not_found(
                "installer has no update configuration",
            ));
        };
        if !update.enabled || update.provider == DOWNLOAD_UPDATE_PROVIDER_NONE {
            return Ok(DownloadUpdateConfigDocument {
                schema_version: DOWNLOAD_UPDATE_SCHEMA_VERSION.to_owned(),
                catalog_key: record.catalog.catalog_key.clone(),
                item_key: record.item.item_key.clone(),
                product_key: record.catalog.product_key.clone(),
                platform: record.item.platform.clone(),
                architecture: record.item.architecture.clone(),
                version: record.item.version.clone(),
                revision: format!("{}:{}", record.catalog.id, record.catalog.revision),
                update: PublicDownloadUpdateConfig {
                    enabled: false,
                    provider: DOWNLOAD_UPDATE_PROVIDER_NONE.to_owned(),
                    channel: update.channel_key.clone(),
                    channel_file: channel_filename(&record.item.platform, &update.channel_key),
                    allowed_channels: update.allowed_channels.clone(),
                    allow_prerelease: update.allow_prerelease,
                    allow_downgrade: update.allow_downgrade,
                    minimum_supported_version: update.minimum_supported_version.clone(),
                    check_interval_minutes: update.check_interval_minutes,
                    rollout_percentage: update.rollout_percentage,
                    release_notes_url: update.release_notes_url.clone(),
                    feed_url: String::new(),
                    github_owner: String::new(),
                    github_repo: String::new(),
                },
            });
        }
        let channel = if record.item.platform == "macos" {
            channel_filename("macos", &update.channel_key)
        } else {
            channel_filename(&record.item.platform, &update.channel_key)
        };
        let feed_url = if update.provider == DOWNLOAD_UPDATE_PROVIDER_DEPLOYMENTS {
            format!(
                "{}{}/{}/{}/{}/{}",
                join_base(base, FEED_PATH),
                record.catalog.product_key,
                record.item.platform,
                record.item.architecture,
                update.channel_key,
                channel
            )
        } else {
            update.feed_url.clone()
        };
        Ok(DownloadUpdateConfigDocument {
            schema_version: DOWNLOAD_UPDATE_SCHEMA_VERSION.to_owned(),
            catalog_key: record.catalog.catalog_key.clone(),
            item_key: record.item.item_key.clone(),
            product_key: record.catalog.product_key.clone(),
            platform: record.item.platform.clone(),
            architecture: record.item.architecture.clone(),
            version: record.item.version.clone(),
            revision: format!("{}:{}", record.catalog.id, record.catalog.revision),
            update: PublicDownloadUpdateConfig {
                enabled: true,
                provider: update.provider.clone(),
                feed_url,
                github_owner: update.github_owner.clone(),
                github_repo: update.github_repo.clone(),
                channel: update.channel_key.clone(),
                channel_file: channel,
                allowed_channels: update.allowed_channels.clone(),
                allow_prerelease: update.allow_prerelease,
                allow_downgrade: update.allow_downgrade,
                minimum_supported_version: update.minimum_supported_version.clone(),
                check_interval_minutes: update.check_interval_minutes,
                rollout_percentage: update.rollout_percentage,
                release_notes_url: update.release_notes_url.clone(),
            },
        })
    }

    fn public_catalog_document(
        catalog: &DownloadCatalogDto,
        base: &str,
    ) -> PublicDownloadCatalogDocument {
        PublicDownloadCatalogDocument {
            schema_version: PUBLIC_DOWNLOAD_CATALOG_SCHEMA_VERSION.to_owned(),
            generated_at: now_rfc3339(),
            product: PublicDownloadProduct {
                id: catalog.product_key.clone(),
                name: catalog.product_name.clone(),
                version: catalog.product_version.clone(),
                channel: catalog.default_channel_key.clone(),
            },
            cards: catalog
                .cards
                .iter()
                .filter(|card| !card.hidden)
                .map(|card| PublicDownloadCard {
                    id: card.card_key.clone(),
                    title: card.title.clone(),
                    description: card.description.clone(),
                    icon: card.icon.clone(),
                    tone: card.tone.clone(),
                    kind: card.kind.clone(),
                    badge: card.badge.clone(),
                    primary_action_strategy: card.primary_strategy.clone(),
                    actions: card
                        .items
                        .iter()
                        .filter(|item| !item.disabled)
                        .map(|item| {
                            let href = direct_href(item).unwrap_or_else(|| {
                                format!("{}/{}", join_base(base, ARTIFACT_PATH), item.id)
                            });
                            PublicDownloadAction {
                                id: item.item_key.clone(),
                                label: item.label.clone(),
                                platform: item.platform.clone(),
                                architecture: item.architecture.clone(),
                                href,
                                file_name: item.file_name.clone(),
                                version: item.version.clone(),
                                release_tag: item.release_tag.clone(),
                                size_bytes: item.size_bytes,
                                sha256: item.checksum_sha256.clone(),
                                primary: item.primary_action,
                                disabled: item.disabled,
                                unavailable_label: item.unavailable_label.clone(),
                            }
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn artifact_target(
        record: &PublishedDownloadRecord,
        base: &str,
    ) -> DeployServiceResult<DownloadArtifactTarget> {
        if let Some(href) = direct_href(&record.item) {
            let file_name = file_name_of(&record.item, &href);
            let content_type = content_type_for(&file_name);
            return Ok(DownloadArtifactTarget {
                url: href,
                file_name,
                content_type,
            });
        }
        // A Drive-referenced installer needs a presigned URL from the Drive port,
        // which the deploy drive port does not expose yet. Until it does, the
        // installer is reported as not downloadable instead of handing the
        // client an inert `drive://` reference.
        let _ = base;
        Err(DeployServiceError::not_found(
            "installer download source is not configured",
        ))
    }
}

#[async_trait]
impl DeployDownloadsApi for DeployService {
    async fn list_download_catalogs(
        &self,
        context: &DeployBackendRequestContext,
    ) -> DeployServiceResult<Vec<DownloadCatalogSummaryDto>> {
        let tenant_id = context.tenant_id.filter(|tenant_id| *tenant_id > 0);
        self.repository.list_download_catalogs(tenant_id).await
    }

    async fn retrieve_download_catalog(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
    ) -> DeployServiceResult<DownloadCatalogDto> {
        let tenant_id = context.tenant_id.filter(|tenant_id| *tenant_id > 0);
        self.repository
            .retrieve_download_catalog(tenant_id, trim(catalog_key))
            .await
    }

    async fn upsert_download_catalog(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        request: &UpsertDownloadCatalogRequest,
    ) -> DeployServiceResult<DownloadCatalogDto> {
        let tenant_id = require_tenant(context)?;
        require_text(catalog_key, "catalogKey")?;
        if let Some(product_key) = request.product_key.as_deref() {
            require_text(product_key, "productKey")?;
        }
        if let Some(channel) = request.default_channel_key.as_deref() {
            validate_channel(channel)?;
        }
        self.repository
            .upsert_download_catalog(tenant_id, context.operator_id, trim(catalog_key), request)
            .await
    }

    async fn publish_download_catalog(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
    ) -> DeployServiceResult<DownloadCatalogDto> {
        let tenant_id = require_tenant(context)?;
        let catalog = self
            .repository
            .retrieve_download_catalog(Some(tenant_id), trim(catalog_key))
            .await?;
        if catalog.cards.is_empty() {
            return Err(DeployServiceError::validation(
                "a catalog needs at least one card before it can be published",
            ));
        }
        if catalog.status == DOWNLOAD_CATALOG_STATUS_PUBLISHED {
            return Ok(catalog);
        }
        self.repository
            .publish_download_catalog(tenant_id, context.operator_id, trim(catalog_key))
            .await
    }

    async fn upsert_download_card(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        card_key: &str,
        request: &UpsertDownloadCardRequest,
    ) -> DeployServiceResult<DownloadCardDto> {
        let tenant_id = require_tenant(context)?;
        require_text(card_key, "cardKey")?;
        if let Some(title) = request.title.as_deref() {
            require_text(title, "title")?;
        }
        if let Some(strategy) = request.primary_strategy.as_deref() {
            if !["detected-platform", "first-available"].contains(&trim(strategy)) {
                return Err(DeployServiceError::validation(
                    "primaryStrategy must be detected-platform or first-available",
                ));
            }
        }
        if let Some(sort_order) = request.sort_order {
            if sort_order < 0 {
                return Err(DeployServiceError::validation(
                    "sortOrder must not be negative",
                ));
            }
        }
        self.repository
            .upsert_download_card(
                tenant_id,
                context.operator_id,
                trim(catalog_key),
                trim(card_key),
                request,
            )
            .await
    }

    async fn delete_download_card(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        card_key: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = require_tenant(context)?;
        self.repository
            .delete_download_card(tenant_id, trim(catalog_key), trim(card_key))
            .await
    }

    async fn upsert_download_item(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
        request: &UpsertDownloadItemRequest,
    ) -> DeployServiceResult<DownloadItemDto> {
        let tenant_id = require_tenant(context)?;
        require_text(item_key, "itemKey")?;
        if let Some(platform) = request.platform.as_deref() {
            validate_platform(platform)?;
        }
        if let Some(architecture) = request.architecture.as_deref() {
            validate_architecture(architecture)?;
        }
        if let Some(label) = request.label.as_deref() {
            require_text(label, "label")?;
        }
        if let Some(version) = request.version.as_deref() {
            validate_version_shape(version, "version")?;
        }
        if let Some(size) = request.size_bytes {
            if size < 0 {
                return Err(DeployServiceError::validation(
                    "sizeBytes must not be negative",
                ));
            }
        }
        self.repository
            .upsert_download_item(
                tenant_id,
                context.operator_id,
                trim(catalog_key),
                trim(card_key),
                trim(item_key),
                request,
            )
            .await
    }

    async fn delete_download_item(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = require_tenant(context)?;
        self.repository
            .delete_download_item(tenant_id, trim(catalog_key), trim(card_key), trim(item_key))
            .await
    }

    async fn upsert_download_update(
        &self,
        context: &DeployBackendRequestContext,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
        request: &UpsertDownloadUpdateRequest,
    ) -> DeployServiceResult<DownloadUpdateDto> {
        let tenant_id = require_tenant(context)?;
        if let Some(provider) = request.provider.as_deref() {
            validate_provider(provider)?;
        }
        if let Some(channel) = request.channel_key.as_deref() {
            validate_channel(channel)?;
        }
        if let Some(channels) = request.allowed_channels.as_ref() {
            for channel in channels {
                validate_channel(channel)?;
            }
            if let Some(channel) = request.channel_key.as_deref() {
                if !channels.iter().any(|value| trim(value) == trim(channel)) {
                    return Err(DeployServiceError::validation(
                        "allowedChannels must contain the selected channel",
                    ));
                }
            }
        }
        if let Some(feed_url) = request.feed_url.as_deref() {
            validate_https_url(feed_url, "feedUrl")?;
        }
        if let Some(notes) = request.release_notes_url.as_deref() {
            validate_https_url(notes, "releaseNotesUrl")?;
        }
        if let Some(minimum) = request.minimum_supported_version.as_deref() {
            validate_version_shape(minimum, "minimumSupportedVersion")?;
            let catalog = self
                .repository
                .retrieve_download_catalog(Some(tenant_id), trim(catalog_key))
                .await?;
            let item_version = catalog
                .cards
                .iter()
                .filter(|card| card.card_key == trim(card_key))
                .flat_map(|card| card.items.iter())
                .find(|item| item.item_key == trim(item_key))
                .map(|item| item.version.clone())
                .unwrap_or_default();
            if !item_version.is_empty() && !version_at_most(minimum, &item_version) {
                return Err(DeployServiceError::validation(
                    "minimumSupportedVersion must not be greater than the installer version",
                ));
            }
        }
        if let Some(interval) = request.check_interval_minutes {
            if !(1..=1440).contains(&interval) {
                return Err(DeployServiceError::validation(
                    "checkIntervalMinutes must be between 1 and 1440",
                ));
            }
        }
        if let Some(percentage) = request.rollout_percentage {
            if !(0..=100).contains(&percentage) {
                return Err(DeployServiceError::validation(
                    "rolloutPercentage must be between 0 and 100",
                ));
            }
        }
        let provider = request
            .provider
            .as_deref()
            .map(trim)
            .unwrap_or(DOWNLOAD_UPDATE_PROVIDER_DEPLOYMENTS);
        match provider {
            DOWNLOAD_UPDATE_PROVIDER_DEPLOYMENTS => {
                let has_channel = request
                    .channel_id
                    .as_deref()
                    .map(trim)
                    .is_some_and(|value| !value.is_empty());
                if !has_channel {
                    // The repository upserts an existing row without requiring the
                    // field again; only a brand-new configuration must carry it.
                    let existing = self
                        .repository
                        .retrieve_download_catalog(Some(tenant_id), trim(catalog_key))
                        .await?;
                    let already_configured = existing
                        .cards
                        .iter()
                        .flat_map(|card| card.items.iter())
                        .any(|item| item.item_key == trim(item_key) && item.update.is_some());
                    if !already_configured {
                        return Err(DeployServiceError::validation(
                            "provider deployments requires channelId",
                        ));
                    }
                }
            }
            DOWNLOAD_UPDATE_PROVIDER_GENERIC => {
                if request
                    .feed_url
                    .as_deref()
                    .map(trim)
                    .is_none_or(str::is_empty)
                {
                    return Err(DeployServiceError::validation(
                        "provider generic requires feedUrl",
                    ));
                }
            }
            DOWNLOAD_UPDATE_PROVIDER_GITHUB => {
                if request
                    .github_owner
                    .as_deref()
                    .map(trim)
                    .is_none_or(str::is_empty)
                    || request
                        .github_repo
                        .as_deref()
                        .map(trim)
                        .is_none_or(str::is_empty)
                {
                    return Err(DeployServiceError::validation(
                        "provider github requires githubOwner and githubRepo",
                    ));
                }
            }
            _ => {}
        }
        self.repository
            .upsert_download_update(
                tenant_id,
                context.operator_id,
                trim(catalog_key),
                trim(card_key),
                trim(item_key),
                request,
            )
            .await
    }
}

#[async_trait]
impl DeployDownloadsReadApi for DeployService {
    async fn public_download_catalog(
        &self,
        query: &PublicDownloadCatalogQuery,
    ) -> DeployServiceResult<PublicDownloadCatalogDocument> {
        require_text(&query.catalog_key, "catalogKey")?;
        let catalogs = self
            .repository
            .published_download_catalogs(trim(&query.catalog_key))
            .await?;
        match catalogs.as_slice() {
            [] => Err(DeployServiceError::not_found("download catalog not found")),
            [catalog] => Ok(Self::public_catalog_document(
                catalog,
                &resolve_base(&query.base_url),
            )),
            _ => Err(DeployServiceError::not_found(
                "download catalog key is ambiguous across tenants",
            )),
        }
    }

    async fn public_download_update_config(
        &self,
        query: &PublicDownloadUpdateQuery,
    ) -> DeployServiceResult<DownloadUpdateConfigDocument> {
        require_text(&query.product_key, "productKey")?;
        validate_platform(&query.platform)?;
        validate_architecture(&query.architecture)?;
        let record = self
            .repository
            .published_download_record(
                trim(&query.product_key),
                trim(&query.platform),
                trim(&query.architecture),
            )
            .await?
            .ok_or_else(|| DeployServiceError::not_found("installer not found"))?;
        Self::public_update_document(&record, &resolve_base(&query.base_url))
    }

    async fn public_download_feed(
        &self,
        query: &PublicDownloadFeedQuery,
    ) -> DeployServiceResult<DownloadFeedDocument> {
        require_text(&query.product_key, "productKey")?;
        validate_platform(&query.platform)?;
        validate_architecture(&query.architecture)?;
        validate_channel(&query.channel)?;
        let record = self
            .repository
            .published_download_record(
                trim(&query.product_key),
                trim(&query.platform),
                trim(&query.architecture),
            )
            .await?
            .ok_or_else(|| DeployServiceError::not_found("installer not found"))?;
        let update = record.update.as_ref().ok_or_else(|| {
            DeployServiceError::not_found("installer has no update configuration")
        })?;
        if update.provider != DOWNLOAD_UPDATE_PROVIDER_DEPLOYMENTS {
            return Err(DeployServiceError::not_found(
                "this installer uses an external update feed",
            ));
        }
        if trim(&update.channel_key) != trim(&query.channel) {
            return Err(DeployServiceError::not_found(
                "installer is not published on this channel",
            ));
        }
        let base = resolve_base(&query.base_url);
        let package = record.package.as_ref();
        let sha512 = package
            .and_then(|package| package.sha512.clone())
            .unwrap_or_default();
        let file_url = direct_href(&record.item)
            .unwrap_or_else(|| format!("{}/{}", join_base(&base, ARTIFACT_PATH), record.item.id));
        let version = if record.item.version.trim().is_empty() {
            package
                .map(|package| package.version.clone())
                .unwrap_or_default()
        } else {
            record.item.version.clone()
        };
        let size = record
            .item
            .size_bytes
            .or_else(|| package.map(|package| package.size_bytes))
            .unwrap_or(0);
        build_feed(&FeedInput {
            version,
            file_url,
            sha512,
            size,
            release_date: now_rfc3339(),
            staging_percentage: Some(update.rollout_percentage),
        })
        .map_err(|error| match error {
            FeedError::MissingSha512 => DeployServiceError::validation(
                "package has no sha512 digest; it cannot be published as a feed",
            ),
            other => DeployServiceError::Internal(other.to_string()),
        })
    }

    async fn public_download_artifact(
        &self,
        query: &sdkwork_deploy_contract::PublicDownloadArtifactQuery,
    ) -> DeployServiceResult<DownloadArtifactTarget> {
        require_text(&query.item_uuid, "itemUuid")?;
        let record = self
            .repository
            .published_download_record_by_item(trim(&query.item_uuid))
            .await?
            .ok_or_else(|| DeployServiceError::not_found("installer not found"))?;
        Self::artifact_target(&record, "")
    }
}
