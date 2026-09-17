//! Deterministic producer for Web Server website runtime descriptors and runtime sets.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const WEBSITE_RUNTIME_SCHEMA_VERSION: &str = "sdkwork.website-runtime.v1";
pub const WEBSITE_RUNTIME_DESCRIPTOR_KIND: &str = "sdkwork.website-runtime.descriptor";
pub const WEBSITE_RUNTIME_SET_SCHEMA_VERSION: &str = "sdkwork.website-runtime-set.v1";
pub const MAXIMUM_RUNTIME_SET_BYTES: usize = 64 * 1024 * 1024;
pub const WEBSITE_RUNTIME_SET_KIND: &str = "sdkwork.website-runtime-set.snapshot";
pub const DESCRIPTOR_COMPILER_VERSION: &str = "sdkwork-deploy-runtime-compiler/1";
pub const RUNTIME_SET_COMPILER_VERSION: &str = "sdkwork-deploy-runtime-set-compiler/1";

/// Canonical provider contract versions carried by compiled descriptors.
///
/// These values are the *consumer* contract: the Web Server provider
/// adapters (`sdkwork-webserver-drive-provider`,
/// `sdkwork-webserver-knowledgebase-provider`) reject any other value with a
/// `CONTRACT_MISMATCH`, so a producer that emits a different string ships a
/// descriptor that can only fail at serve time. The compiler therefore
/// validates every resource against this catalog at compile time, which turns
/// that runtime failure into a build failure.
///
/// Keep byte-identical with `DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION`
/// and `KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION` in sdkwork-webserver
/// (`crates/sdkwork-webserver-drive-provider/src/adapter.rs`,
/// `crates/sdkwork-webserver-knowledgebase-provider/src/adapter.rs`); the Web
/// Server also has a cross-repository contract test that reads this file.
pub const DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION: &str = "drive.website-root.v1";
pub const KNOWLEDGEBASE_WIKI_PUBLICATION_PROVIDER_CONTRACT_VERSION: &str =
    "knowledgebase.wiki-publication.v1";

/// Legacy producer-side spellings accepted on the compile path only so that
/// descriptors authored against the pre-unification names do not hard-fail.
/// They are normalized to the canonical catalog above before validation and
/// are never emitted.
const LEGACY_PROVIDER_CONTRACT_VERSIONS: [(&str, &str); 2] = [
    (
        "sdkwork.drive.website-root.v1",
        DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION,
    ),
    (
        "sdkwork.knowledgebase.wiki-publication.v1",
        KNOWLEDGEBASE_WIKI_PUBLICATION_PROVIDER_CONTRACT_VERSION,
    ),
];

/// The canonical contract version for a provider type.
pub fn canonical_provider_contract_version(provider_type: RuntimeProviderType) -> &'static str {
    match provider_type {
        RuntimeProviderType::Drive => DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION,
        RuntimeProviderType::Knowledgebase => {
            KNOWLEDGEBASE_WIKI_PUBLICATION_PROVIDER_CONTRACT_VERSION
        }
    }
}

/// Normalize a producer-supplied contract version to the canonical catalog
/// value, accepting the legacy spellings. Returns `Err` with the reason when
/// the value is unknown.
pub fn normalize_provider_contract_version(
    provider_type: RuntimeProviderType,
    value: &str,
) -> Result<&'static str, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("providerContractVersion must not be blank".into());
    }
    if value.len() > 64 {
        return Err(format!(
            "providerContractVersion is longer than 64 bytes: {value}"
        ));
    }
    let canonical = canonical_provider_contract_version(provider_type);
    if value == canonical {
        return Ok(canonical);
    }
    if let Some((_, target)) = LEGACY_PROVIDER_CONTRACT_VERSIONS
        .iter()
        .find(|(legacy, _)| *legacy == value)
    {
        return Ok(target);
    }
    Err(format!(
        "providerContractVersion {value} is not supported for {provider_type:?}; expected {canonical}"
    ))
}

const MAXIMUM_BINDINGS: usize = 1_024;
const MAXIMUM_VARIANTS: usize = 64;
const MAXIMUM_VARIANT_RULES: usize = 1_024;
const MAXIMUM_RESOURCES: usize = 512;
const MAXIMUM_MOUNTS: usize = 2_048;
const MAXIMUM_SITES: usize = 10_000;
const MAXIMUM_GENERATION: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeEnvironment {
    Development,
    Test,
    Staging,
    Demo,
    Production,
}

impl RuntimeEnvironment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Test => "test",
            Self::Staging => "staging",
            Self::Demo => "demo",
            Self::Production => "production",
        }
    }

    /// Parse a lifecycle environment key. Mirrors
    /// `sdkwork-deploy-contract::AppPublishEnvironment` and the Web Server
    /// `WebsiteRuntimeEnvironment` catalogs (five environments).
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "development" => Ok(Self::Development),
            "test" => Ok(Self::Test),
            "staging" => Ok(Self::Staging),
            "demo" => Ok(Self::Demo),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "environment must be development, test, staging, demo, or production: {other}"
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRuntimeCompilationInput {
    pub revision_uuid: String,
    pub app_uuid: String,
    pub tenant_scope_hash: String,
    pub environment: RuntimeEnvironment,
    pub generated_at: String,
    pub app_default_variant_uuid: String,
    pub bindings: Vec<RuntimeBinding>,
    pub variants: Vec<RuntimeVariant>,
    pub variant_rules: Vec<RuntimeVariantRule>,
    pub resources: Vec<RuntimeResource>,
    pub mounts: Vec<RuntimeMount>,
    pub delivery_policy: RuntimeDeliveryPolicy,
    pub security_policy: RuntimeSecurityPolicy,
    pub limits: RuntimeLimits,
    pub observability_policy: RuntimeObservabilityPolicy,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBinding {
    pub binding_uuid: String,
    pub hostname: String,
    pub path_prefix: String,
    pub action: RuntimeBindingAction,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeBindingAction {
    Serve {
        #[serde(rename = "defaultVariantUuid", skip_serializing_if = "Option::is_none")]
        default_variant_uuid: Option<String>,
        #[serde(rename = "forcedVariantUuid", skip_serializing_if = "Option::is_none")]
        forced_variant_uuid: Option<String>,
    },
    Redirect {
        #[serde(rename = "statusCode")]
        status_code: u16,
        scheme: RuntimeRedirectScheme,
        hostname: String,
        #[serde(rename = "pathPrefix")]
        path_prefix: String,
        #[serde(rename = "preservePath")]
        preserve_path: bool,
        #[serde(rename = "preserveQuery")]
        preserve_query: bool,
    },
}

impl RuntimeBindingAction {
    pub fn serve(
        default_variant_uuid: Option<String>,
        forced_variant_uuid: Option<String>,
    ) -> Self {
        Self::Serve {
            default_variant_uuid,
            forced_variant_uuid,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeRedirectScheme {
    Http,
    Https,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeVariant {
    pub variant_uuid: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeVariantRule {
    pub rule_uuid: String,
    pub variant_uuid: String,
    pub priority: u16,
    #[serde(rename = "match")]
    pub matcher: RuntimeVariantRuleMatcher,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeVariantRuleMatcher {
    PathPrefix {
        #[serde(rename = "pathPrefix")]
        path_prefix: String,
    },
    ClientClass {
        #[serde(rename = "clientClass")]
        client_class: RuntimeClientClass,
    },
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeClientClass {
    Desktop,
    Mobile,
    Tablet,
    Tv,
    Bot,
    Other,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeResource {
    pub resource_uuid: String,
    pub provider: RuntimeProviderReference,
    pub capabilities: RuntimeResourceCapabilities,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProviderReference {
    pub provider_type: RuntimeProviderType,
    pub provider_resource_uuid: String,
    pub provider_contract_version: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeProviderType {
    Drive,
    Knowledgebase,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeResourceCapabilities {
    pub static_content: bool,
    pub wiki_routes: bool,
    pub wiki_search: bool,
    pub range_requests: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMount {
    pub mount_uuid: String,
    pub variant_uuid: String,
    pub path_prefix: String,
    pub resource_uuid: String,
    pub handler: RuntimeHandler,
    pub translation: RuntimeMountTranslation,
    pub index_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spa_fallback: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeHandler {
    Static,
    Spa,
    Wiki,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMountTranslation {
    pub mode: RuntimeMountMode,
    pub resource_subpath: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeMountMode {
    Root,
    Alias,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDeliveryPolicy {
    pub provider_timeout_ms: u64,
    pub metadata_cache_ttl_seconds: u32,
    pub negative_cache_ttl_seconds: u32,
    pub stale_while_revalidate_seconds: u32,
    pub maximum_object_bytes: u64,
}

impl Default for RuntimeDeliveryPolicy {
    fn default() -> Self {
        Self {
            provider_timeout_ms: 5_000,
            metadata_cache_ttl_seconds: 60,
            negative_cache_ttl_seconds: 5,
            stale_while_revalidate_seconds: 30,
            maximum_object_bytes: 1_073_741_824,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSecurityPolicy {
    pub force_https: bool,
    pub deny_dot_files: bool,
    pub denied_path_prefixes: Vec<String>,
}

impl Default for RuntimeSecurityPolicy {
    fn default() -> Self {
        Self {
            force_https: true,
            deny_dot_files: true,
            denied_path_prefixes: vec!["/.git".to_owned(), "/.sdkwork".to_owned()],
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeLimits {
    pub maximum_bindings: usize,
    pub maximum_variants: usize,
    pub maximum_variant_rules: usize,
    pub maximum_resources: usize,
    pub maximum_mounts: usize,
    pub maximum_index_files_per_mount: usize,
    pub maximum_path_bytes: usize,
    pub maximum_path_segments: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            maximum_bindings: 64,
            maximum_variants: 16,
            maximum_variant_rules: 64,
            maximum_resources: 64,
            maximum_mounts: 256,
            maximum_index_files_per_mount: 16,
            maximum_path_bytes: 4_096,
            maximum_path_segments: 128,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeObservabilityPolicy {
    pub access_log_enabled: bool,
    pub usage_metering_enabled: bool,
    pub trace_sample_rate_per_mille: u16,
}

impl Default for RuntimeObservabilityPolicy {
    fn default() -> Self {
        Self {
            access_log_enabled: true,
            usage_metering_enabled: true,
            trace_sample_rate_per_mille: 10,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeSetCompilationInput {
    pub snapshot_uuid: String,
    pub node_uuid: String,
    pub environment: RuntimeEnvironment,
    pub generation: u64,
    pub generated_at: String,
    pub maximum_sites: usize,
    pub descriptors: Vec<Value>,
}

#[derive(Clone, Debug)]
pub struct CompiledSiteRevision {
    pub descriptor: Value,
    pub descriptor_sha256: String,
}

#[derive(Clone, Debug)]
pub struct CompiledRuntimeSet {
    pub snapshot: Value,
    pub snapshot_sha256: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuntimeCompilationError {
    #[error("invalid website runtime input: {0}")]
    Validation(String),
    #[error("serialize website runtime input: {0}")]
    Serialization(String),
}

pub fn compile_app_revision(
    mut input: AppRuntimeCompilationInput,
) -> Result<CompiledSiteRevision, RuntimeCompilationError> {
    normalize_app_input(&mut input);
    validate_app_input(&input)?;

    let mut descriptor = serde_json::to_value(&input)
        .map_err(|error| RuntimeCompilationError::Serialization(error.to_string()))?;
    {
        let object = descriptor.as_object_mut().ok_or_else(|| {
            RuntimeCompilationError::Serialization("descriptor is not an object".into())
        })?;
        object.insert(
            "schemaVersion".to_owned(),
            Value::String(WEBSITE_RUNTIME_SCHEMA_VERSION.to_owned()),
        );
        object.insert(
            "kind".to_owned(),
            Value::String(WEBSITE_RUNTIME_DESCRIPTOR_KIND.to_owned()),
        );
        object.insert(
            "compilerVersion".to_owned(),
            Value::String(DESCRIPTOR_COMPILER_VERSION.to_owned()),
        );
        object.insert("descriptorSha256".to_owned(), Value::String(String::new()));
    }
    let descriptor_sha256 = canonical_sha256_excluding_field(&descriptor, "descriptorSha256")?;
    descriptor
        .as_object_mut()
        .expect("descriptor object")
        .insert(
            "descriptorSha256".to_owned(),
            Value::String(descriptor_sha256.clone()),
        );
    Ok(CompiledSiteRevision {
        descriptor,
        descriptor_sha256,
    })
}

pub fn compile_runtime_set(
    mut input: RuntimeSetCompilationInput,
) -> Result<CompiledRuntimeSet, RuntimeCompilationError> {
    validate_opaque_id(&input.snapshot_uuid, "snapshotUuid")?;
    validate_opaque_id(&input.node_uuid, "nodeUuid")?;
    validate_canonical_timestamp(&input.generated_at, "generatedAt")?;
    if input.generation == 0 || input.generation > MAXIMUM_GENERATION {
        return Err(RuntimeCompilationError::Validation(
            "generation must be a positive JSON-safe integer".into(),
        ));
    }
    if input.maximum_sites == 0 || input.maximum_sites > MAXIMUM_SITES {
        return Err(RuntimeCompilationError::Validation(
            "maximumSites is outside the supported range".into(),
        ));
    }
    if input.descriptors.len() > input.maximum_sites {
        return Err(RuntimeCompilationError::Validation(
            "descriptor count exceeds maximumSites".into(),
        ));
    }
    normalize_runtime_descriptors(&mut input.descriptors);
    let mut app_ids = HashSet::new();
    for descriptor in &input.descriptors {
        let app_uuid = descriptor_app_uuid(descriptor);
        if app_uuid.is_empty() || !app_ids.insert(app_uuid) {
            return Err(RuntimeCompilationError::Validation(
                "descriptors require unique non-empty appUuid values".into(),
            ));
        }
        let environment = descriptor
            .get("environment")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if environment != input.environment.as_str() {
            return Err(RuntimeCompilationError::Validation(
                "descriptor environment must match runtime-set environment".into(),
            ));
        }
        verify_embedded_descriptor_hash(descriptor)?;
    }

    let mut snapshot = serde_json::json!({
        "schemaVersion": WEBSITE_RUNTIME_SET_SCHEMA_VERSION,
        "kind": WEBSITE_RUNTIME_SET_KIND,
        "snapshotUuid": input.snapshot_uuid,
        "nodeUuid": input.node_uuid,
        "environment": input.environment,
        "generation": input.generation,
        "generatedAt": input.generated_at,
        "compilerVersion": RUNTIME_SET_COMPILER_VERSION,
        "snapshotSha256": "",
        "maximumSites": input.maximum_sites,
        "descriptors": input.descriptors,
    });
    let snapshot_sha256 = canonical_sha256_excluding_field(&snapshot, "snapshotSha256")?;
    snapshot["snapshotSha256"] = Value::String(snapshot_sha256.clone());
    runtime_set_size_bytes(&snapshot)?;
    Ok(CompiledRuntimeSet {
        snapshot,
        snapshot_sha256,
    })
}

pub fn runtime_set_size_bytes(snapshot: &Value) -> Result<usize, RuntimeCompilationError> {
    runtime_set_size_bytes_with_limit(snapshot, MAXIMUM_RUNTIME_SET_BYTES)
}

fn runtime_set_size_bytes_with_limit(
    snapshot: &Value,
    maximum_bytes: usize,
) -> Result<usize, RuntimeCompilationError> {
    let bytes = serde_json::to_vec(snapshot)
        .map_err(|error| RuntimeCompilationError::Serialization(error.to_string()))?
        .len();
    if bytes == 0 || bytes > maximum_bytes {
        return Err(RuntimeCompilationError::Validation(format!(
            "serialized runtime set exceeds the {maximum_bytes}-byte limit"
        )));
    }
    Ok(bytes)
}

/// Orders runtime descriptors exactly as they are represented in a compiled runtime set.
pub fn normalize_runtime_descriptors(descriptors: &mut [Value]) {
    descriptors.sort_by(|left, right| descriptor_app_uuid(left).cmp(descriptor_app_uuid(right)));
}

pub fn canonical_sha256_excluding_field(
    value: &Value,
    excluded_field: &str,
) -> Result<String, RuntimeCompilationError> {
    let mut value = value.clone();
    value
        .as_object_mut()
        .ok_or_else(|| {
            RuntimeCompilationError::Serialization("runtime document is not an object".into())
        })?
        .remove(excluded_field);
    let mut canonical = String::new();
    write_canonical_json(&value, &mut canonical)?;
    Ok(sdkwork_utils_rust::sha256_hash(canonical.as_bytes()))
}

fn normalize_app_input(input: &mut AppRuntimeCompilationInput) {
    input
        .bindings
        .sort_by(|left, right| left.binding_uuid.cmp(&right.binding_uuid));
    input
        .variants
        .sort_by(|left, right| left.variant_uuid.cmp(&right.variant_uuid));
    input
        .variant_rules
        .sort_by(|left, right| left.rule_uuid.cmp(&right.rule_uuid));
    input
        .resources
        .sort_by(|left, right| left.resource_uuid.cmp(&right.resource_uuid));
    input
        .mounts
        .sort_by(|left, right| left.mount_uuid.cmp(&right.mount_uuid));
    input.security_policy.denied_path_prefixes.sort();
    // Canonicalize provider contract versions so the emitted descriptor
    // always carries the value the Web Server providers accept. Unknown
    // values are preserved here and rejected by `validate_app_input`, which
    // keeps the "empty or too long" and "unsupported" diagnostics distinct.
    for resource in &mut input.resources {
        let canonical = canonical_provider_contract_version(resource.provider.provider_type);
        let current = resource.provider.provider_contract_version.trim();
        if current == canonical {
            continue;
        }
        resource.provider.provider_contract_version = LEGACY_PROVIDER_CONTRACT_VERSIONS
            .iter()
            .find(|(legacy, _)| *legacy == current)
            .map(|(_, canonical)| (*canonical).to_owned())
            .unwrap_or_else(|| current.to_owned());
    }
}

fn validate_app_input(input: &AppRuntimeCompilationInput) -> Result<(), RuntimeCompilationError> {
    validate_opaque_id(&input.revision_uuid, "revisionUuid")?;
    validate_opaque_id(&input.app_uuid, "appUuid")?;
    validate_sha256(&input.tenant_scope_hash, "tenantScopeHash")?;
    validate_canonical_timestamp(&input.generated_at, "generatedAt")?;
    validate_collection_size(input.bindings.len(), 1, MAXIMUM_BINDINGS, "bindings")?;
    validate_collection_size(input.variants.len(), 1, MAXIMUM_VARIANTS, "variants")?;
    validate_collection_size(
        input.variant_rules.len(),
        0,
        MAXIMUM_VARIANT_RULES,
        "variantRules",
    )?;
    validate_collection_size(input.resources.len(), 1, MAXIMUM_RESOURCES, "resources")?;
    validate_collection_size(input.mounts.len(), 1, MAXIMUM_MOUNTS, "mounts")?;
    validate_limits(&input.limits)?;

    let variants = unique_ids(
        input.variants.iter().map(|item| item.variant_uuid.as_str()),
        "variants",
    )?;
    let resources = unique_ids(
        input
            .resources
            .iter()
            .map(|item| item.resource_uuid.as_str()),
        "resources",
    )?;
    unique_ids(
        input.bindings.iter().map(|item| item.binding_uuid.as_str()),
        "bindings",
    )?;
    unique_ids(
        input
            .variant_rules
            .iter()
            .map(|item| item.rule_uuid.as_str()),
        "variantRules",
    )?;
    unique_ids(
        input.mounts.iter().map(|item| item.mount_uuid.as_str()),
        "mounts",
    )?;
    if !variants.contains(input.app_default_variant_uuid.as_str()) {
        return Err(RuntimeCompilationError::Validation(
            "appDefaultVariantUuid does not reference a Variant".into(),
        ));
    }
    for binding in &input.bindings {
        validate_hostname(&binding.hostname)?;
        validate_path(&binding.path_prefix, "binding.pathPrefix")?;
        if let RuntimeBindingAction::Serve {
            default_variant_uuid,
            forced_variant_uuid,
        } = &binding.action
        {
            for variant_uuid in [default_variant_uuid, forced_variant_uuid]
                .into_iter()
                .flatten()
            {
                if !variants.contains(variant_uuid.as_str()) {
                    return Err(RuntimeCompilationError::Validation(
                        "Binding action references an unknown Variant".into(),
                    ));
                }
            }
        }
    }
    for rule in &input.variant_rules {
        if !variants.contains(rule.variant_uuid.as_str()) {
            return Err(RuntimeCompilationError::Validation(
                "VariantRule references an unknown Variant".into(),
            ));
        }
    }
    for resource in &input.resources {
        validate_opaque_id(
            &resource.provider.provider_resource_uuid,
            "providerResourceUuid",
        )?;
        normalize_provider_contract_version(
            resource.provider.provider_type,
            &resource.provider.provider_contract_version,
        )
        .map_err(RuntimeCompilationError::Validation)?;
    }
    for mount in &input.mounts {
        if !variants.contains(mount.variant_uuid.as_str())
            || !resources.contains(mount.resource_uuid.as_str())
        {
            return Err(RuntimeCompilationError::Validation(
                "Mount references an unknown Variant or Resource".into(),
            ));
        }
        validate_path(&mount.path_prefix, "mount.pathPrefix")?;
        validate_path(
            &mount.translation.resource_subpath,
            "mount.translation.resourceSubpath",
        )?;
        if let Some(path) = &mount.spa_fallback {
            validate_path(path, "mount.spaFallback")?;
        }
        let resource = input
            .resources
            .iter()
            .find(|resource| resource.resource_uuid == mount.resource_uuid)
            .expect("resource reference checked");
        match mount.handler {
            RuntimeHandler::Static | RuntimeHandler::Spa
                if !resource.capabilities.static_content =>
            {
                return Err(RuntimeCompilationError::Validation(
                    "STATIC and SPA Mounts require staticContent capability".into(),
                ));
            }
            RuntimeHandler::Wiki if !resource.capabilities.wiki_routes => {
                return Err(RuntimeCompilationError::Validation(
                    "WIKI Mounts require wikiRoutes capability".into(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_limits(limits: &RuntimeLimits) -> Result<(), RuntimeCompilationError> {
    for (name, value, hard_maximum) in [
        ("maximumBindings", limits.maximum_bindings, MAXIMUM_BINDINGS),
        ("maximumVariants", limits.maximum_variants, MAXIMUM_VARIANTS),
        (
            "maximumVariantRules",
            limits.maximum_variant_rules,
            MAXIMUM_VARIANT_RULES,
        ),
        (
            "maximumResources",
            limits.maximum_resources,
            MAXIMUM_RESOURCES,
        ),
        ("maximumMounts", limits.maximum_mounts, MAXIMUM_MOUNTS),
        (
            "maximumIndexFilesPerMount",
            limits.maximum_index_files_per_mount,
            16,
        ),
        ("maximumPathBytes", limits.maximum_path_bytes, 4_096),
        ("maximumPathSegments", limits.maximum_path_segments, 128),
    ] {
        if value == 0 || value > hard_maximum {
            return Err(RuntimeCompilationError::Validation(format!(
                "{name} is outside the supported range"
            )));
        }
    }
    Ok(())
}

fn validate_collection_size(
    actual: usize,
    minimum: usize,
    maximum: usize,
    field: &str,
) -> Result<(), RuntimeCompilationError> {
    if actual < minimum || actual > maximum {
        return Err(RuntimeCompilationError::Validation(format!(
            "{field} contains {actual} entries; expected {minimum}..={maximum}"
        )));
    }
    Ok(())
}

fn unique_ids<'a>(
    values: impl Iterator<Item = &'a str>,
    field: &str,
) -> Result<HashSet<&'a str>, RuntimeCompilationError> {
    let mut unique = HashSet::new();
    for value in values {
        validate_opaque_id(value, field)?;
        if !unique.insert(value) {
            return Err(RuntimeCompilationError::Validation(format!(
                "{field} contains duplicate identifier {value}"
            )));
        }
    }
    Ok(unique)
}

fn validate_opaque_id(value: &str, field: &str) -> Result<(), RuntimeCompilationError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphanumeric() || (index > 0 && matches!(byte, b'-' | b'_' | b'.' | b':'))
        })
    {
        return Err(RuntimeCompilationError::Validation(format!(
            "{field} is not a bounded opaque identifier"
        )));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &str) -> Result<(), RuntimeCompilationError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(RuntimeCompilationError::Validation(format!(
            "{field} must be a lowercase SHA-256 digest"
        )));
    }
    Ok(())
}

fn validate_canonical_timestamp(value: &str, field: &str) -> Result<(), RuntimeCompilationError> {
    let canonical = chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| {
            parsed
                .with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        });
    if canonical.as_deref() != Some(value) {
        return Err(RuntimeCompilationError::Validation(format!(
            "{field} must be canonical UTC RFC 3339 seconds"
        )));
    }
    Ok(())
}

fn validate_hostname(value: &str) -> Result<(), RuntimeCompilationError> {
    if value.is_empty()
        || value.len() > 255
        || value != value.to_ascii_lowercase()
        || value.ends_with('.')
        || value.bytes().any(|byte| {
            !(byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'-' | b'*'))
        })
    {
        return Err(RuntimeCompilationError::Validation(
            "hostname must be normalized lowercase ASCII without a trailing dot".into(),
        ));
    }
    Ok(())
}

fn validate_path(value: &str, field: &str) -> Result<(), RuntimeCompilationError> {
    if !value.starts_with('/')
        || (value.len() > 1 && value.ends_with('/'))
        || value.contains("//")
        || value
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
    {
        return Err(RuntimeCompilationError::Validation(format!(
            "{field} must be a canonical absolute path"
        )));
    }
    Ok(())
}

fn descriptor_app_uuid(value: &Value) -> &str {
    value
        .get("appUuid")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn verify_embedded_descriptor_hash(descriptor: &Value) -> Result<(), RuntimeCompilationError> {
    let expected = descriptor
        .get("descriptorSha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    validate_sha256(expected, "descriptorSha256")?;
    let calculated = canonical_sha256_excluding_field(descriptor, "descriptorSha256")?;
    if expected != calculated {
        return Err(RuntimeCompilationError::Validation(
            "embedded descriptor hash does not match canonical content".into(),
        ));
    }
    Ok(())
}

fn write_canonical_json(value: &Value, output: &mut String) -> Result<(), RuntimeCompilationError> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => output.push_str(&value.to_string()),
        Value::String(value) => output.push_str(
            &serde_json::to_string(value)
                .map_err(|error| RuntimeCompilationError::Serialization(error.to_string()))?,
        ),
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical_json(value, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key).map_err(|error| {
                        RuntimeCompilationError::Serialization(error.to_string())
                    })?,
                );
                output.push(':');
                write_canonical_json(value, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sdkwork_webserver_core::website_runtime::{
        compile_website_runtime_descriptor, compile_website_runtime_set_snapshot,
    };

    use super::*;

    fn app_input(app_uuid: &str) -> AppRuntimeCompilationInput {
        AppRuntimeCompilationInput {
            revision_uuid: format!("revision-{app_uuid}"),
            app_uuid: app_uuid.to_owned(),
            tenant_scope_hash: "1".repeat(64),
            environment: RuntimeEnvironment::Production,
            generated_at: "2026-07-22T00:00:00Z".to_owned(),
            app_default_variant_uuid: "variant-default".to_owned(),
            bindings: vec![RuntimeBinding {
                binding_uuid: format!("binding-{app_uuid}"),
                hostname: format!("{app_uuid}.example.com"),
                path_prefix: "/".to_owned(),
                action: RuntimeBindingAction::serve(None, None),
            }],
            variants: vec![RuntimeVariant {
                variant_uuid: "variant-default".to_owned(),
                label: "Default".to_owned(),
            }],
            variant_rules: vec![],
            resources: vec![RuntimeResource {
                resource_uuid: "resource-drive".to_owned(),
                provider: RuntimeProviderReference {
                    provider_type: RuntimeProviderType::Drive,
                    provider_resource_uuid: "website-root-1".to_owned(),
                    provider_contract_version: DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION
                        .to_owned(),
                },
                capabilities: RuntimeResourceCapabilities {
                    static_content: true,
                    wiki_routes: false,
                    wiki_search: false,
                    range_requests: true,
                },
            }],
            mounts: vec![RuntimeMount {
                mount_uuid: "mount-root".to_owned(),
                variant_uuid: "variant-default".to_owned(),
                path_prefix: "/".to_owned(),
                resource_uuid: "resource-drive".to_owned(),
                handler: RuntimeHandler::Spa,
                translation: RuntimeMountTranslation {
                    mode: RuntimeMountMode::Root,
                    resource_subpath: "/".to_owned(),
                },
                index_files: vec!["index.html".to_owned()],
                spa_fallback: Some("/index.html".to_owned()),
            }],
            delivery_policy: RuntimeDeliveryPolicy::default(),
            security_policy: RuntimeSecurityPolicy::default(),
            limits: RuntimeLimits::default(),
            observability_policy: RuntimeObservabilityPolicy::default(),
        }
    }

    #[test]
    fn descriptor_is_accepted_by_the_web_server_consumer() {
        let compiled = compile_app_revision(app_input("site-a")).expect("compile descriptor");
        let bytes = serde_json::to_vec(&compiled.descriptor).expect("encode descriptor");
        let web = compile_website_runtime_descriptor(&bytes).expect("Web consumer accepts output");
        assert_eq!(web.descriptor().app_uuid, "site-a");
        assert_eq!(web.descriptor_sha256(), compiled.descriptor_sha256);
    }

    #[test]
    fn tv_client_class_is_serialized_for_the_web_server_consumer() {
        let mut input = app_input("site-tv");
        input.variant_rules.push(RuntimeVariantRule {
            rule_uuid: "rule-tv-client".to_owned(),
            variant_uuid: "variant-default".to_owned(),
            priority: 100,
            matcher: RuntimeVariantRuleMatcher::ClientClass {
                client_class: RuntimeClientClass::Tv,
            },
        });

        let compiled = compile_app_revision(input).expect("compile TV descriptor");
        assert_eq!(
            compiled.descriptor["variantRules"][0]["match"]["clientClass"],
            "TV"
        );
        let bytes = serde_json::to_vec(&compiled.descriptor).expect("encode TV descriptor");
        compile_website_runtime_descriptor(&bytes).expect("Web consumer accepts TV client class");
    }

    #[test]
    fn runtime_set_is_stably_sorted_and_accepted_by_the_web_server_consumer() {
        let site_b = compile_app_revision(app_input("site-b")).unwrap();
        let site_a = compile_app_revision(app_input("site-a")).unwrap();
        let compiled = compile_runtime_set(RuntimeSetCompilationInput {
            snapshot_uuid: "snapshot-1".to_owned(),
            node_uuid: "node-1".to_owned(),
            environment: RuntimeEnvironment::Production,
            generation: 1,
            generated_at: "2026-07-22T00:00:00Z".to_owned(),
            maximum_sites: 100,
            descriptors: vec![site_b.descriptor, site_a.descriptor],
        })
        .expect("compile runtime set");
        assert_eq!(compiled.snapshot["descriptors"][0]["appUuid"], "site-a");
        let bytes = serde_json::to_vec(&compiled.snapshot).expect("encode runtime set");
        let web =
            compile_website_runtime_set_snapshot(&bytes).expect("Web consumer accepts output");
        assert_eq!(web.generation(), 1);
        assert_eq!(web.snapshot_sha256(), compiled.snapshot_sha256);
    }

    #[test]
    fn runtime_set_size_is_bounded_before_persistence() {
        let snapshot = serde_json::json!({"payload": "0123456789"});
        let exact_size = serde_json::to_vec(&snapshot).unwrap().len();
        assert_eq!(
            runtime_set_size_bytes_with_limit(&snapshot, exact_size).unwrap(),
            exact_size
        );
        assert!(matches!(
            runtime_set_size_bytes_with_limit(&snapshot, exact_size - 1),
            Err(RuntimeCompilationError::Validation(_))
        ));
    }

    #[test]
    fn content_changes_do_not_enter_the_compiler_contract() {
        let value = serde_json::to_value(app_input("site-a")).unwrap();
        let encoded = serde_json::to_string(&value).unwrap();
        for forbidden in [
            "objectKey",
            "presigned",
            "contentVersion",
            "releaseId",
            "releaseUuid",
            "deploymentId",
            "deploymentUuid",
            "siteRevisionId",
            "siteRevisionUuid",
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }
}
