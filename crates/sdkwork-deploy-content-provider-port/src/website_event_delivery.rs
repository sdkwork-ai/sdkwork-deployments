//! Drive WebsiteRoot provider-event delivery, in-process.
//!
//! The runtime publication pass used to `PUT` every Drive website root's delivery
//! channel to the Drive Internal API. MODULE_SPEC.md §4.2: that registration is
//! drive state in the shared PostgreSQL database, so it is performed here through
//! drive's own `DriveWebsiteRootEventDeliveryService` on the host pool.
//!
//! Preserved: channel-id derivation, verification-token derivation from the node
//! derivation secret, the per-node renewal cache (expiration + secret rotation),
//! the registration concurrency bound, and the receipt identity check.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sdkwork_drive_contract::drive::events::{
    derive_website_event_channel_id, derive_website_event_verification_token,
    WEBSITE_PROVIDER_EVENT_SUBSCRIPTION_ID,
};
use sdkwork_drive_workspace_service::application::website_root_event_delivery_service::{
    DriveWebsiteRootEventDeliveryService, EnsureWebsiteRootEventDeliveryCommand,
};
use sdkwork_drive_workspace_service::domain::provider_event_delivery::DriveProviderEventDelivery;
use sdkwork_drive_workspace_service::infrastructure::sql::provider_event_delivery_store::SqlProviderEventDeliveryStore;
use sdkwork_drive_workspace_service::DriveServiceError;
use sdkwork_utils_rust::sha256_hash;
use serde_json::Value;
use sqlx::PgPool;
use tokio::task::JoinSet;
use url::Url;
use zeroize::{Zeroize, Zeroizing};

use crate::support::{provider_unavailable, required_env, required_value};

pub const WEBSITE_PROVIDER_EVENT_CALLBACK_BASE_URL_ENV: &str =
    "SDKWORK_DEPLOY_WEBSITE_PROVIDER_EVENT_CALLBACK_BASE_URL";
pub const WEBSITE_PROVIDER_EVENT_SECRET_DIRECTORY_ENV: &str =
    "SDKWORK_DEPLOY_WEBSITE_PROVIDER_EVENT_SECRET_DIRECTORY";
pub const WEBSITE_PROVIDER_EVENT_EXPIRATION_SECONDS_ENV: &str =
    "SDKWORK_DEPLOY_WEBSITE_PROVIDER_EVENT_EXPIRATION_SECONDS";
pub const WEBSITE_PROVIDER_EVENT_RENEW_BEFORE_SECONDS_ENV: &str =
    "SDKWORK_DEPLOY_WEBSITE_PROVIDER_EVENT_RENEW_BEFORE_SECONDS";

const DEFAULT_EXPIRATION_SECONDS: u64 = 86_400;
const DEFAULT_RENEW_BEFORE_SECONDS: u64 = 3_600;
const MINIMUM_EXPIRATION_SECONDS: u64 = 300;
const MAXIMUM_EXPIRATION_SECONDS: u64 = 604_800;
const MINIMUM_RENEW_BEFORE_SECONDS: u64 = 60;
const MAXIMUM_RUNTIME_DESCRIPTORS: usize = 10_000;
const MAXIMUM_RESOURCES_PER_DESCRIPTOR: usize = 512;
const MAXIMUM_DRIVE_ROOTS_PER_RUNTIME_SET: usize = 65_536;
const MAXIMUM_RENEWAL_CACHE_ENTRIES: usize = 262_144;
const MAXIMUM_CONCURRENT_REGISTRATIONS: usize = 16;
const MINIMUM_NODE_SECRET_BYTES: usize = 32;
const MAXIMUM_NODE_SECRET_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WebsiteProviderEventDeliveryResult {
    pub ensured: usize,
    pub skipped: usize,
}

#[async_trait]
pub trait WebsiteProviderEventDeliveryPort: Send + Sync {
    /// `tenant_id` is the runtime assignment's tenant: the drive rows this
    /// registers belong to it, and it must come from the assignment rather
    /// than from an environment default.
    async fn ensure_runtime_set(
        &self,
        tenant_id: i64,
        node_uuid: &str,
        runtime_set: &Value,
    ) -> DeployServiceResult<WebsiteProviderEventDeliveryResult>;
}

#[derive(Clone, Debug, Default)]
pub struct NoopWebsiteProviderEventDeliveryPort;

#[async_trait]
impl WebsiteProviderEventDeliveryPort for NoopWebsiteProviderEventDeliveryPort {
    async fn ensure_runtime_set(
        &self,
        _tenant_id: i64,
        _node_uuid: &str,
        _runtime_set: &Value,
    ) -> DeployServiceResult<WebsiteProviderEventDeliveryResult> {
        Ok(WebsiteProviderEventDeliveryResult::default())
    }
}

#[derive(Clone, Debug)]
pub struct WebsiteProviderEventDeliveryConfig {
    callback_base_url: Url,
    node_secret_directory: PathBuf,
    expiration_seconds: u64,
    renew_before_seconds: u64,
}

impl WebsiteProviderEventDeliveryConfig {
    pub fn new(
        callback_base_url: String,
        node_secret_directory: PathBuf,
        expiration_seconds: u64,
        renew_before_seconds: u64,
        production_like: bool,
    ) -> Result<Self, String> {
        if node_secret_directory.as_os_str().is_empty() {
            return Err(format!(
                "{WEBSITE_PROVIDER_EVENT_SECRET_DIRECTORY_ENV} must not be blank"
            ));
        }
        if production_like && !node_secret_directory.is_absolute() {
            return Err(
                "website provider event secret paths must be absolute in production-like environments"
                    .to_owned(),
            );
        }
        let callback_base_url = validate_callback_base_url(&callback_base_url)?;
        if !(MINIMUM_EXPIRATION_SECONDS..=MAXIMUM_EXPIRATION_SECONDS).contains(&expiration_seconds)
        {
            return Err(format!(
                "{WEBSITE_PROVIDER_EVENT_EXPIRATION_SECONDS_ENV} must be between {MINIMUM_EXPIRATION_SECONDS} and {MAXIMUM_EXPIRATION_SECONDS}"
            ));
        }
        if renew_before_seconds < MINIMUM_RENEW_BEFORE_SECONDS
            || renew_before_seconds >= expiration_seconds
        {
            return Err(format!(
                "{WEBSITE_PROVIDER_EVENT_RENEW_BEFORE_SECONDS_ENV} must be at least {MINIMUM_RENEW_BEFORE_SECONDS} and less than the expiration"
            ));
        }
        Ok(Self {
            callback_base_url,
            node_secret_directory,
            expiration_seconds,
            renew_before_seconds,
        })
    }

    pub fn from_env(production_like: bool) -> Result<Self, String> {
        Self::new(
            required_env(WEBSITE_PROVIDER_EVENT_CALLBACK_BASE_URL_ENV)?,
            PathBuf::from(required_env(WEBSITE_PROVIDER_EVENT_SECRET_DIRECTORY_ENV)?),
            parse_u64_env(
                WEBSITE_PROVIDER_EVENT_EXPIRATION_SECONDS_ENV,
                DEFAULT_EXPIRATION_SECONDS,
            )?,
            parse_u64_env(
                WEBSITE_PROVIDER_EVENT_RENEW_BEFORE_SECONDS_ENV,
                DEFAULT_RENEW_BEFORE_SECONDS,
            )?,
            production_like,
        )
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct DeliveryCacheKey {
    node_uuid: String,
    website_root_uuid: String,
}

#[derive(Debug, Default)]
struct RenewalCache {
    entries: BTreeMap<DeliveryCacheKey, RenewalCacheEntry>,
}

#[derive(Debug)]
struct RenewalCacheEntry {
    renew_at_epoch_ms: i64,
    node_secret_sha256: String,
}

impl RenewalCache {
    fn partition_due(
        &self,
        node_uuid: &str,
        roots: BTreeSet<String>,
        now_epoch_ms: i64,
        node_secret_sha256: &str,
    ) -> (Vec<String>, usize) {
        let mut due = Vec::new();
        let mut skipped = 0;
        for website_root_uuid in roots {
            let key = DeliveryCacheKey {
                node_uuid: node_uuid.to_owned(),
                website_root_uuid: website_root_uuid.clone(),
            };
            if self.entries.get(&key).is_some_and(|entry| {
                entry.renew_at_epoch_ms > now_epoch_ms
                    && entry.node_secret_sha256 == node_secret_sha256
            }) {
                skipped += 1;
            } else {
                due.push(website_root_uuid);
            }
        }
        (due, skipped)
    }

    fn record(
        &mut self,
        key: DeliveryCacheKey,
        renew_at_epoch_ms: i64,
        node_secret_sha256: String,
    ) {
        if self.entries.len() >= MAXIMUM_RENEWAL_CACHE_ENTRIES && !self.entries.contains_key(&key) {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.renew_at_epoch_ms)
                .map(|(key, _)| key.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            key,
            RenewalCacheEntry {
                renew_at_epoch_ms,
                node_secret_sha256,
            },
        );
    }
}

/// Registers website-root event deliveries in-process on the host pool.
#[derive(Clone, Debug)]
pub struct InProcessWebsiteProviderEventDeliveryPort {
    pool: PgPool,
    config: WebsiteProviderEventDeliveryConfig,
    renewal_cache: Arc<Mutex<RenewalCache>>,
}

impl InProcessWebsiteProviderEventDeliveryPort {
    pub fn new(pool: PgPool, config: WebsiteProviderEventDeliveryConfig) -> Self {
        Self {
            pool,
            config,
            renewal_cache: Arc::new(Mutex::new(RenewalCache::default())),
        }
    }

    pub fn from_env(pool: PgPool, production_like: bool) -> Result<Self, String> {
        Ok(Self::new(
            pool,
            WebsiteProviderEventDeliveryConfig::from_env(production_like)?,
        ))
    }
}

#[async_trait]
impl WebsiteProviderEventDeliveryPort for InProcessWebsiteProviderEventDeliveryPort {
    async fn ensure_runtime_set(
        &self,
        tenant_id: i64,
        node_uuid: &str,
        runtime_set: &Value,
    ) -> DeployServiceResult<WebsiteProviderEventDeliveryResult> {
        let roots = drive_website_root_uuids(node_uuid, runtime_set)?;
        if roots.is_empty() {
            return Ok(WebsiteProviderEventDeliveryResult::default());
        }
        let now_epoch_ms = epoch_millis()?;
        let secret = Arc::new(read_node_derivation_secret(
            &self.config.node_secret_directory,
            node_uuid,
        )?);
        let node_secret_sha256 = sha256_hash(secret.as_slice());
        let (due, skipped) = self
            .renewal_cache
            .lock()
            .map_err(|_| provider_unavailable("Drive WebsiteRoot event delivery"))?
            .partition_due(node_uuid, roots, now_epoch_ms, &node_secret_sha256);
        if due.is_empty() {
            return Ok(WebsiteProviderEventDeliveryResult {
                ensured: 0,
                skipped,
            });
        }
        let address = callback_address(&self.config.callback_base_url, node_uuid)?;
        let expiration_epoch_ms = now_epoch_ms
            .checked_add(
                i64::try_from(self.config.expiration_seconds)
                    .map_err(|_| provider_unavailable("Drive WebsiteRoot event delivery"))?
                    .saturating_mul(1_000),
            )
            .ok_or_else(|| provider_unavailable("Drive WebsiteRoot event delivery"))?;
        let renew_at_epoch_ms = expiration_epoch_ms
            - i64::try_from(self.config.renew_before_seconds)
                .map_err(|_| provider_unavailable("Drive WebsiteRoot event delivery"))?
                * 1_000;

        let mut pending = due.into_iter();
        let mut tasks = JoinSet::new();
        for _ in 0..MAXIMUM_CONCURRENT_REGISTRATIONS {
            if let Some(root) = pending.next() {
                spawn_registration(
                    &mut tasks,
                    self.pool.clone(),
                    tenant_id,
                    node_uuid.to_owned(),
                    root,
                    address.clone(),
                    expiration_epoch_ms,
                    renew_at_epoch_ms,
                    secret.clone(),
                    node_secret_sha256.clone(),
                );
            }
        }

        let mut ensured = 0;
        while let Some(joined) = tasks.join_next().await {
            let (key, renew_at_epoch_ms, completed_secret_sha256) =
                joined.map_err(|_| provider_unavailable("Drive WebsiteRoot event delivery"))??;
            self.renewal_cache
                .lock()
                .map_err(|_| provider_unavailable("Drive WebsiteRoot event delivery"))?
                .record(key, renew_at_epoch_ms, completed_secret_sha256);
            ensured += 1;
            if let Some(root) = pending.next() {
                spawn_registration(
                    &mut tasks,
                    self.pool.clone(),
                    tenant_id,
                    node_uuid.to_owned(),
                    root,
                    address.clone(),
                    expiration_epoch_ms,
                    renew_at_epoch_ms,
                    secret.clone(),
                    node_secret_sha256.clone(),
                );
            }
        }
        Ok(WebsiteProviderEventDeliveryResult { ensured, skipped })
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_registration(
    tasks: &mut JoinSet<DeployServiceResult<(DeliveryCacheKey, i64, String)>>,
    pool: PgPool,
    tenant_id: i64,
    node_uuid: String,
    website_root_uuid: String,
    address: String,
    expiration_epoch_ms: i64,
    renew_at_epoch_ms: i64,
    secret: Arc<Zeroizing<Vec<u8>>>,
    node_secret_sha256: String,
) {
    tasks.spawn(async move {
        let channel_id = derive_website_event_channel_id(&node_uuid, &website_root_uuid);
        let mut verification_token =
            derive_website_event_verification_token(&channel_id, secret.as_slice());
        let command = EnsureWebsiteRootEventDeliveryCommand {
            tenant_id: tenant_id.to_string(),
            website_root_uuid: website_root_uuid.clone(),
            channel_id: channel_id.clone(),
            address: address.clone(),
            verification_token: verification_token.clone(),
            expiration_epoch_ms,
            operator_id: "sdkwork-deploy".to_owned(),
        };
        verification_token.zeroize();
        let ensured =
            DriveWebsiteRootEventDeliveryService::new(SqlProviderEventDeliveryStore::new(pool))
                .ensure_delivery(command)
                .await
                .map_err(map_drive_event_error)?;
        validate_delivery_receipt(
            &ensured.delivery,
            &channel_id,
            &website_root_uuid,
            &address,
            expiration_epoch_ms,
        )?;
        Ok((
            DeliveryCacheKey {
                node_uuid,
                website_root_uuid,
            },
            renew_at_epoch_ms,
            node_secret_sha256,
        ))
    });
}

fn map_drive_event_error(error: DriveServiceError) -> DeployServiceError {
    match error {
        DriveServiceError::Validation(_) | DriveServiceError::NotFound(_) => {
            DeployServiceError::validation("Drive WebsiteRoot event delivery was rejected")
        }
        DriveServiceError::Conflict(_) | DriveServiceError::PermissionDenied(_) => {
            DeployServiceError::forbidden("Drive WebsiteRoot event delivery was rejected")
        }
        DriveServiceError::Internal(_) => provider_unavailable("Drive WebsiteRoot event delivery"),
    }
}

fn drive_website_root_uuids(
    expected_node_uuid: &str,
    runtime_set: &Value,
) -> DeployServiceResult<BTreeSet<String>> {
    if !crate::support::valid_opaque_id(expected_node_uuid) {
        return Err(DeployServiceError::validation(
            "runtime assignment nodeUuid is invalid",
        ));
    }
    if runtime_set.get("nodeUuid").and_then(Value::as_str) != Some(expected_node_uuid) {
        return Err(DeployServiceError::conflict(
            "runtime assignment nodeUuid does not match its runtime set",
        ));
    }
    let descriptors = runtime_set
        .get("descriptors")
        .and_then(Value::as_array)
        .ok_or_else(|| DeployServiceError::validation("runtime set descriptors are invalid"))?;
    if descriptors.len() > MAXIMUM_RUNTIME_DESCRIPTORS {
        return Err(DeployServiceError::validation(
            "runtime set descriptor count exceeds the supported bound",
        ));
    }
    let mut roots = BTreeSet::new();
    for descriptor in descriptors {
        let resources = descriptor
            .get("resources")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                DeployServiceError::validation("runtime descriptor resources are invalid")
            })?;
        if resources.len() > MAXIMUM_RESOURCES_PER_DESCRIPTOR {
            return Err(DeployServiceError::validation(
                "runtime descriptor resource count exceeds the supported bound",
            ));
        }
        for resource in resources {
            let provider = resource
                .get("provider")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    DeployServiceError::validation("runtime resource provider is invalid")
                })?;
            let provider_type = provider
                .get("providerType")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    DeployServiceError::validation("runtime resource providerType is invalid")
                })?;
            let provider_resource_uuid = provider
                .get("providerResourceUuid")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    DeployServiceError::validation(
                        "runtime resource providerResourceUuid is invalid",
                    )
                })?;
            match provider_type {
                "DRIVE" => {
                    if uuid::Uuid::parse_str(provider_resource_uuid).is_err() {
                        return Err(DeployServiceError::validation(
                            "Drive providerResourceUuid must be a UUID",
                        ));
                    }
                    roots.insert(provider_resource_uuid.to_owned());
                    if roots.len() > MAXIMUM_DRIVE_ROOTS_PER_RUNTIME_SET {
                        return Err(DeployServiceError::validation(
                            "runtime set Drive WebsiteRoot count exceeds the supported bound",
                        ));
                    }
                }
                "KNOWLEDGEBASE" if crate::support::valid_opaque_id(provider_resource_uuid) => {}
                "KNOWLEDGEBASE" => {
                    return Err(DeployServiceError::validation(
                        "Knowledgebase providerResourceUuid is invalid",
                    ));
                }
                _ => {
                    return Err(DeployServiceError::validation(
                        "runtime resource providerType is unsupported",
                    ));
                }
            }
        }
    }
    Ok(roots)
}

fn validate_callback_base_url(value: &str) -> Result<Url, String> {
    let value = required_value(value, WEBSITE_PROVIDER_EVENT_CALLBACK_BASE_URL_ENV)?;
    let parsed = Url::parse(&value).map_err(|_| {
        format!("{WEBSITE_PROVIDER_EVENT_CALLBACK_BASE_URL_ENV} must be an absolute HTTPS URL")
    })?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(format!(
            "{WEBSITE_PROVIDER_EVENT_CALLBACK_BASE_URL_ENV} must be an absolute HTTPS URL without credentials, query, or fragment"
        ));
    }
    Ok(parsed)
}

fn callback_address(base_url: &Url, node_uuid: &str) -> DeployServiceResult<String> {
    let mut address = base_url.clone();
    address
        .path_segments_mut()
        .map_err(|_| provider_unavailable("Drive WebsiteRoot event delivery"))?
        .pop_if_empty()
        .extend([
            "nodes",
            node_uuid,
            "provider-events",
            WEBSITE_PROVIDER_EVENT_SUBSCRIPTION_ID,
        ]);
    Ok(address.to_string())
}

fn node_secret_path(directory: &Path, node_uuid: &str) -> PathBuf {
    let node_hash = sha256_hash(node_uuid.as_bytes());
    directory.join(format!("drive-website-node-{node_hash}.derivation-secret"))
}

fn read_node_derivation_secret(
    directory: &Path,
    node_uuid: &str,
) -> DeployServiceResult<Zeroizing<Vec<u8>>> {
    let path = node_secret_path(directory, node_uuid);
    let metadata = fs::metadata(&path)
        .map_err(|_| provider_unavailable("Drive WebsiteRoot event delivery"))?;
    if !metadata.is_file()
        || metadata.len() < MINIMUM_NODE_SECRET_BYTES as u64
        || metadata.len() > (MAXIMUM_NODE_SECRET_BYTES + 2) as u64
    {
        return Err(provider_unavailable("Drive WebsiteRoot event delivery"));
    }
    let mut secret = Zeroizing::new(
        fs::read(path).map_err(|_| provider_unavailable("Drive WebsiteRoot event delivery"))?,
    );
    while matches!(secret.last(), Some(b'\r' | b'\n')) {
        secret.pop();
    }
    if !(MINIMUM_NODE_SECRET_BYTES..=MAXIMUM_NODE_SECRET_BYTES).contains(&secret.len()) {
        return Err(provider_unavailable("Drive WebsiteRoot event delivery"));
    }
    Ok(secret)
}

fn validate_delivery_receipt(
    delivery: &DriveProviderEventDelivery,
    channel_id: &str,
    website_root_uuid: &str,
    address: &str,
    expiration_epoch_ms: i64,
) -> DeployServiceResult<()> {
    if delivery.channel_id != channel_id
        || delivery.provider_resource_uuid != website_root_uuid
        || delivery.address != address
        || delivery.expiration_epoch_ms != expiration_epoch_ms
        || delivery.lifecycle_status != "ACTIVE"
        || delivery.version <= 0
    {
        return Err(provider_unavailable("Drive WebsiteRoot event delivery"));
    }
    Ok(())
}

fn parse_u64_env(key: &str, default: u64) -> Result<u64, String> {
    match std::env::var(key) {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|error| format!("invalid {key}: {error}")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(format!("read {key} failed: {error}")),
    }
}

fn epoch_millis() -> DeployServiceResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| provider_unavailable("Drive WebsiteRoot event delivery"))?;
    i64::try_from(duration.as_millis())
        .map_err(|_| provider_unavailable("Drive WebsiteRoot event delivery"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_roots_are_deduplicated_and_node_bound() {
        let root = "01900000-0000-7000-8000-000000000001";
        let runtime_set = serde_json::json!({
            "nodeUuid": "node-1",
            "descriptors": [
                {"resources": [{"provider": {"providerType": "DRIVE", "providerResourceUuid": root}}]},
                {"resources": [
                    {"provider": {"providerType": "DRIVE", "providerResourceUuid": root}},
                    {"provider": {"providerType": "KNOWLEDGEBASE", "providerResourceUuid": "wiki-1"}}
                ]}
            ]
        });
        let roots = drive_website_root_uuids("node-1", &runtime_set).unwrap();
        assert_eq!(roots.into_iter().collect::<Vec<_>>(), [root]);
        assert!(drive_website_root_uuids("node-2", &runtime_set).is_err());
    }

    #[test]
    fn node_secret_path_never_contains_the_node_identity() {
        let directory = Path::new("secrets");
        let path = node_secret_path(directory, "../../node:1");
        let file_name = path.file_name().unwrap().to_string_lossy();
        assert!(file_name.starts_with("drive-website-node-"));
        assert!(file_name.ends_with(".derivation-secret"));
        assert!(!file_name.contains(".."));
        assert!(!file_name.contains(':'));
    }

    #[test]
    fn callback_base_requires_https_and_no_embedded_credentials() {
        assert!(validate_callback_base_url("https://events.sdkwork.com/base").is_ok());
        assert!(validate_callback_base_url("http://events.sdkwork.com").is_err());
        assert!(validate_callback_base_url("https://user:secret@events.sdkwork.com").is_err());
        assert!(validate_callback_base_url("https://events.sdkwork.com?token=secret").is_err());
    }

    #[test]
    fn delivery_receipt_requires_exact_identity() {
        let delivery = DriveProviderEventDelivery {
            channel_id: "web:channel".to_owned(),
            provider_resource_uuid: "01900000-0000-7000-8000-000000000001".to_owned(),
            address: "https://events.sdkwork.com/nodes/node-1/provider-events/drive-website-events"
                .to_owned(),
            expiration_epoch_ms: 1_000,
            lifecycle_status: "ACTIVE".to_owned(),
            version: 1,
            created_at: "2026-07-23T00:00:00Z".to_owned(),
            updated_at: "2026-07-23T00:00:00Z".to_owned(),
        };
        assert!(validate_delivery_receipt(
            &delivery,
            "web:channel",
            "01900000-0000-7000-8000-000000000001",
            &delivery.address,
            1_000,
        )
        .is_ok());
        assert!(validate_delivery_receipt(
            &DriveProviderEventDelivery {
                lifecycle_status: "STOPPED".to_owned(),
                ..delivery.clone()
            },
            "web:channel",
            "01900000-0000-7000-8000-000000000001",
            &delivery.address,
            1_000,
        )
        .is_err());
    }
}
