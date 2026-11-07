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
use sdkwork_drive_internal_sdk_generated_rust::{
    EnsureWebsiteRootEventDeliveryRequest, SdkworkCustomClient as DriveInternalClient,
    WebsiteRootEventDelivery,
};
use sdkwork_utils_rust::sha256_hash;
use sdkwork_utils_rust::service_base_url::registry_base_url_or;
use serde_json::Value;
use tokio::task::JoinSet;
use url::Url;
use zeroize::{Zeroize, Zeroizing};

use crate::sdk::{
    map_provider_error, provider_unavailable, read_secret_file, required_env, required_value,
    valid_opaque_id, DRIVE_INTERNAL_API_URL_ENV, DRIVE_INTERNAL_TOKEN_FILE_ENV,
};

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
    async fn ensure_runtime_set(
        &self,
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
        _node_uuid: &str,
        _runtime_set: &Value,
    ) -> DeployServiceResult<WebsiteProviderEventDeliveryResult> {
        Ok(WebsiteProviderEventDeliveryResult::default())
    }
}

#[derive(Clone, Debug)]
pub struct WebsiteProviderEventDeliveryConfig {
    drive_internal_url: String,
    drive_internal_token_file: PathBuf,
    callback_base_url: Url,
    node_secret_directory: PathBuf,
    expiration_seconds: u64,
    renew_before_seconds: u64,
}

impl WebsiteProviderEventDeliveryConfig {
    pub fn new(
        drive_internal_url: String,
        drive_internal_token_file: PathBuf,
        callback_base_url: String,
        node_secret_directory: PathBuf,
        expiration_seconds: u64,
        renew_before_seconds: u64,
        production_like: bool,
    ) -> Result<Self, String> {
        let drive_internal_url = required_value(&drive_internal_url, DRIVE_INTERNAL_API_URL_ENV)?;
        if drive_internal_token_file.as_os_str().is_empty() {
            return Err(format!("{DRIVE_INTERNAL_TOKEN_FILE_ENV} must not be blank"));
        }
        if node_secret_directory.as_os_str().is_empty() {
            return Err(format!(
                "{WEBSITE_PROVIDER_EVENT_SECRET_DIRECTORY_ENV} must not be blank"
            ));
        }
        if production_like
            && (!drive_internal_token_file.is_absolute() || !node_secret_directory.is_absolute())
        {
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
            drive_internal_url,
            drive_internal_token_file,
            callback_base_url,
            node_secret_directory,
            expiration_seconds,
            renew_before_seconds,
        })
    }

    pub fn from_env(production_like: bool) -> Result<Self, String> {
        Self::new(
            required_env(DRIVE_INTERNAL_API_URL_ENV)?,
            PathBuf::from(required_env(DRIVE_INTERNAL_TOKEN_FILE_ENV)?),
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

#[derive(Clone, Debug)]
pub struct SdkWebsiteProviderEventDeliveryPort {
    config: WebsiteProviderEventDeliveryConfig,
    renewal_cache: Arc<Mutex<RenewalCache>>,
}

impl SdkWebsiteProviderEventDeliveryPort {
    pub fn new(config: WebsiteProviderEventDeliveryConfig) -> Self {
        Self {
            config,
            renewal_cache: Arc::new(Mutex::new(RenewalCache::default())),
        }
    }

    pub fn from_env(production_like: bool) -> Result<Self, String> {
        Ok(Self::new(WebsiteProviderEventDeliveryConfig::from_env(
            production_like,
        )?))
    }

    fn drive_internal_client(&self) -> DeployServiceResult<DriveInternalClient> {
        // DISCOVERY_SPEC §12.1: registry-first, configured URL as degradation.
        let base_url = registry_base_url_or("sdkwork-drive", &self.config.drive_internal_url);
        let client = DriveInternalClient::new_with_base_url(&base_url)
            .map_err(|_| provider_unavailable("Drive Internal API"))?;
        client.set_api_key(read_secret_file(
            &self.config.drive_internal_token_file,
            "Drive",
        )?);
        Ok(client)
    }
}

#[async_trait]
impl WebsiteProviderEventDeliveryPort for SdkWebsiteProviderEventDeliveryPort {
    async fn ensure_runtime_set(
        &self,
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
        let client = self.drive_internal_client()?;
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
                    client.clone(),
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
                    client.clone(),
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
    client: DriveInternalClient,
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
        let verification_token =
            derive_website_event_verification_token(&channel_id, secret.as_slice());
        let mut request = EnsureWebsiteRootEventDeliveryRequest {
            address: address.clone(),
            verification_token,
            expiration_epoch_ms: expiration_epoch_ms.to_string(),
        };
        let response = client
            .drive_internal_publishing()
            .website_root_event_deliveries_replace(&website_root_uuid, &channel_id, &request)
            .await;
        request.verification_token.zeroize();
        let response = response.map_err(|error| map_provider_error("Drive", &error.to_string()))?;
        validate_delivery_receipt(
            &response,
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

fn drive_website_root_uuids(
    expected_node_uuid: &str,
    runtime_set: &Value,
) -> DeployServiceResult<BTreeSet<String>> {
    if !valid_opaque_id(expected_node_uuid) {
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
                "KNOWLEDGEBASE" if valid_opaque_id(provider_resource_uuid) => {}
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
    delivery: &WebsiteRootEventDelivery,
    channel_id: &str,
    website_root_uuid: &str,
    address: &str,
    expiration_epoch_ms: i64,
) -> DeployServiceResult<()> {
    let valid_version = delivery
        .version
        .parse::<u64>()
        .is_ok_and(|version| version > 0);
    if delivery.channel_id != channel_id
        || delivery.website_root_uuid != website_root_uuid
        || delivery.address != address
        || delivery.expiration_epoch_ms != expiration_epoch_ms.to_string()
        || delivery.lifecycle_status != "ACTIVE"
        || !valid_version
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::{
        extract::{Path as AxumPath, State},
        http::{HeaderMap, StatusCode},
        routing::put,
        Json, Router,
    };

    #[derive(Clone)]
    struct MockDriveState {
        calls: Arc<AtomicUsize>,
        api_key: String,
        node_uuid: String,
        node_secret: Arc<Mutex<Vec<u8>>>,
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "sdkwork-deploy-provider-events-{}",
                uuid::Uuid::new_v4()
            ));
            fs::create_dir_all(&path).expect("create provider event test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    async fn ensure_delivery(
        State(state): State<MockDriveState>,
        AxumPath((website_root_uuid, channel_id)): AxumPath<(String, String)>,
        headers: HeaderMap,
        Json(request): Json<EnsureWebsiteRootEventDeliveryRequest>,
    ) -> Result<Json<Value>, StatusCode> {
        if headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            != Some(state.api_key.as_str())
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let expected_channel =
            derive_website_event_channel_id(&state.node_uuid, &website_root_uuid);
        let expected_token = derive_website_event_verification_token(
            &channel_id,
            &state.node_secret.lock().unwrap(),
        );
        let expected_address = format!(
            "https://provider-events.sdkwork.test/nodes/{}/provider-events/drive-website-events",
            state.node_uuid
        );
        if channel_id != expected_channel
            || request.verification_token != expected_token
            || request.address != expected_address
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        state.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Json(serde_json::json!({
            "code": 0,
            "traceId": "trace-provider-event-1",
            "data": {
                "item": {
                    "channelId": channel_id,
                    "websiteRootUuid": website_root_uuid,
                    "address": request.address,
                    "expirationEpochMs": request.expiration_epoch_ms,
                    "lifecycleStatus": "ACTIVE",
                    "version": "1",
                    "createdAt": "2026-07-23T00:00:00Z",
                    "updatedAt": "2026-07-23T00:00:00Z"
                }
            }
        })))
    }

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
        let delivery = WebsiteRootEventDelivery {
            channel_id: "web:channel".to_owned(),
            website_root_uuid: "01900000-0000-7000-8000-000000000001".to_owned(),
            address: "https://events.sdkwork.com/nodes/node-1/provider-events/drive-website-events"
                .to_owned(),
            expiration_epoch_ms: "1000".to_owned(),
            lifecycle_status: "ACTIVE".to_owned(),
            version: "1".to_owned(),
            ..Default::default()
        };
        assert!(validate_delivery_receipt(
            &delivery,
            "web:channel",
            "01900000-0000-7000-8000-000000000001",
            &delivery.address,
            1000,
        )
        .is_ok());
        assert!(validate_delivery_receipt(
            &WebsiteRootEventDelivery {
                lifecycle_status: "STOPPED".to_owned(),
                ..delivery.clone()
            },
            "web:channel",
            "01900000-0000-7000-8000-000000000001",
            &delivery.address,
            1000,
        )
        .is_err());
    }

    #[tokio::test]
    async fn generated_sdk_registration_is_node_bound_and_cached() {
        let node_uuid = "node-1";
        let website_root_uuid = "01900000-0000-7000-8000-000000000001";
        let api_key = "drive-internal-test-token";
        let node_secret = b"node-1-derivation-secret-with-at-least-32-bytes".to_vec();
        let server_node_secret = Arc::new(Mutex::new(node_secret.clone()));
        let calls = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route(
                "/internal/v3/api/drive/website_roots/{website_root_uuid}/event_deliveries/{channel_id}",
                put(ensure_delivery),
            )
            .with_state(MockDriveState {
                calls: calls.clone(),
                api_key: api_key.to_owned(),
                node_uuid: node_uuid.to_owned(),
                node_secret: server_node_secret.clone(),
            });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock Drive Internal API");
        let address = listener.local_addr().expect("mock server address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve mock Drive Internal API");
        });

        let directory = TestDirectory::new();
        let ingress_token_file = directory.0.join("drive-internal-token");
        fs::write(&ingress_token_file, api_key).expect("write Drive ingress token");
        fs::write(node_secret_path(&directory.0, node_uuid), &node_secret)
            .expect("write node derivation secret");
        let port = SdkWebsiteProviderEventDeliveryPort::new(
            WebsiteProviderEventDeliveryConfig::new(
                format!("http://{address}"),
                ingress_token_file,
                "https://provider-events.sdkwork.test".to_owned(),
                directory.0.clone(),
                3_600,
                600,
                false,
            )
            .expect("valid provider event config"),
        );
        let runtime_set = serde_json::json!({
            "nodeUuid": node_uuid,
            "descriptors": [{
                "resources": [
                    {"provider": {"providerType": "DRIVE", "providerResourceUuid": website_root_uuid}},
                    {"provider": {"providerType": "DRIVE", "providerResourceUuid": website_root_uuid}}
                ]
            }]
        });

        let first = port
            .ensure_runtime_set(node_uuid, &runtime_set)
            .await
            .expect("register WebsiteRoot delivery");
        assert_eq!(first.ensured, 1);
        assert_eq!(first.skipped, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let replay = port
            .ensure_runtime_set(node_uuid, &runtime_set)
            .await
            .expect("reuse fresh WebsiteRoot delivery");
        assert_eq!(replay.ensured, 0);
        assert_eq!(replay.skipped, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let rotated_secret = b"node-1-rotated-derivation-secret-at-least-32-bytes".to_vec();
        fs::write(node_secret_path(&directory.0, node_uuid), &rotated_secret)
            .expect("rotate node derivation secret");
        *server_node_secret.lock().unwrap() = rotated_secret;
        let rotation = port
            .ensure_runtime_set(node_uuid, &runtime_set)
            .await
            .expect("replace delivery after node secret rotation");
        assert_eq!(rotation.ensured, 1);
        assert_eq!(rotation.skipped, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        let other_node_runtime_set = serde_json::json!({
            "nodeUuid": "node-2",
            "descriptors": [{
                "resources": [{"provider": {
                    "providerType": "DRIVE",
                    "providerResourceUuid": website_root_uuid
                }}]
            }]
        });
        assert!(port
            .ensure_runtime_set("node-2", &other_node_runtime_set)
            .await
            .is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        server.abort();
    }
}
