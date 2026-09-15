use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppPublishEnvironment {
    Development,
    Test,
    Staging,
    Demo,
    Production,
}

/// The five lifecycle environments of the SDKWork fleet, in promotion order.
pub const APP_PUBLISH_ENVIRONMENTS: [AppPublishEnvironment; 5] = [
    AppPublishEnvironment::Development,
    AppPublishEnvironment::Test,
    AppPublishEnvironment::Staging,
    AppPublishEnvironment::Demo,
    AppPublishEnvironment::Production,
];

impl AppPublishEnvironment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Test => "test",
            Self::Staging => "staging",
            Self::Demo => "demo",
            Self::Production => "production",
        }
    }

    /// Parse a lifecycle environment key (case-insensitive).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "development" => Some(Self::Development),
            "test" => Some(Self::Test),
            "staging" => Some(Self::Staging),
            "demo" => Some(Self::Demo),
            "production" => Some(Self::Production),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppCompositionRequest {
    pub environment: AppPublishEnvironment,
    pub default_variant_key: String,
    pub resources: Vec<AppResourceDefinition>,
    pub variants: Vec<AppVariantDefinition>,
    #[serde(default)]
    pub variant_rules: Vec<AppVariantRuleDefinition>,
    pub mounts: Vec<AppMountDefinition>,
    pub bindings: Vec<AppBindingDefinition>,
    #[serde(default)]
    pub delivery_policy: AppDeliveryPolicy,
    #[serde(default)]
    pub security_policy: AppSecurityPolicy,
    #[serde(default)]
    pub limits: AppRuntimeLimits,
    #[serde(default)]
    pub observability_policy: AppObservabilityPolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppResourceDefinition {
    pub key: String,
    pub source: ContentProviderResourceSource,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContentProviderResourceSource {
    DriveDirectory {
        #[serde(rename = "websiteSpaceId")]
        website_space_id: String,
        root: DriveWebsiteRootSelector,
        #[serde(rename = "contentMode")]
        content_mode: DriveWebsiteContentMode,
    },
    KnowledgebaseWiki {
        #[serde(rename = "publicationUuid")]
        publication_uuid: String,
    },
}

impl ContentProviderResourceSource {
    pub fn drive_directory(
        website_space_id: String,
        root: DriveWebsiteRootSelector,
        content_mode: DriveWebsiteContentMode,
    ) -> Self {
        Self::DriveDirectory {
            website_space_id,
            root,
            content_mode,
        }
    }

    pub fn knowledgebase_wiki(publication_uuid: String) -> Self {
        Self::KnowledgebaseWiki { publication_uuid }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DriveWebsiteRootSelector {
    SpaceRoot,
    Folder {
        #[serde(rename = "folderNodeId")]
        folder_node_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DriveWebsiteContentMode {
    LiveTree,
    AtomicGeneration,
}

impl DriveWebsiteContentMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiveTree => "LIVE_TREE",
            Self::AtomicGeneration => "ATOMIC_GENERATION",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppVariantDefinition {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub client_class: AppClientClass,
    #[serde(default)]
    pub priority: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppClientClass {
    Desktop,
    Mobile,
    Tablet,
    Tv,
    Bot,
    #[default]
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppVariantRuleDefinition {
    pub key: String,
    pub target_variant_key: String,
    pub priority: u16,
    #[serde(rename = "match")]
    pub matcher: AppVariantRuleMatcher,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppVariantRuleMatcher {
    PathPrefix {
        #[serde(rename = "pathPrefix")]
        path_prefix: String,
    },
    ClientClass {
        #[serde(rename = "clientClass")]
        client_class: AppClientClass,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMountDefinition {
    pub key: String,
    pub variant_key: String,
    pub resource_key: String,
    pub path_prefix: String,
    pub resource_subpath: String,
    pub mode: AppMountMode,
    pub handler: AppMountHandler,
    #[serde(default)]
    pub index_files: Vec<String>,
    #[serde(default)]
    pub spa_fallback: Option<String>,
    #[serde(default)]
    pub priority: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppMountMode {
    Root,
    Alias,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppMountHandler {
    Static,
    Spa,
    Wiki,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBindingDefinition {
    pub key: String,
    pub domain_id: String,
    #[serde(default = "root_path")]
    pub path_prefix: String,
    pub action: AppBindingAction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppBindingAction {
    Serve {
        #[serde(default, rename = "defaultVariantKey")]
        default_variant_key: Option<String>,
        #[serde(default, rename = "forcedVariantKey")]
        forced_variant_key: Option<String>,
    },
    Redirect {
        #[serde(rename = "statusCode")]
        status_code: u16,
        scheme: AppRedirectScheme,
        hostname: String,
        #[serde(rename = "pathPrefix")]
        path_prefix: String,
        #[serde(default = "default_true", rename = "preservePath")]
        preserve_path: bool,
        #[serde(default = "default_true", rename = "preserveQuery")]
        preserve_query: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppRedirectScheme {
    Http,
    Https,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDeliveryPolicy {
    pub provider_timeout_ms: u64,
    pub metadata_cache_ttl_seconds: u32,
    pub negative_cache_ttl_seconds: u32,
    pub stale_while_revalidate_seconds: u32,
    pub maximum_object_bytes: u64,
}

impl Default for AppDeliveryPolicy {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSecurityPolicy {
    pub force_https: bool,
    pub deny_dot_files: bool,
    pub denied_path_prefixes: Vec<String>,
}

impl Default for AppSecurityPolicy {
    fn default() -> Self {
        Self {
            force_https: true,
            deny_dot_files: true,
            denied_path_prefixes: vec!["/.git".to_owned(), "/.sdkwork".to_owned()],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRuntimeLimits {
    pub maximum_bindings: usize,
    pub maximum_variants: usize,
    pub maximum_variant_rules: usize,
    pub maximum_resources: usize,
    pub maximum_mounts: usize,
    pub maximum_index_files_per_mount: usize,
    pub maximum_path_bytes: usize,
    pub maximum_path_segments: usize,
}

impl Default for AppRuntimeLimits {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppObservabilityPolicy {
    pub access_log_enabled: bool,
    pub usage_metering_enabled: bool,
    pub trace_sample_rate_per_mille: u16,
}

impl Default for AppObservabilityPolicy {
    fn default() -> Self {
        Self {
            access_log_enabled: true,
            usage_metering_enabled: true,
            trace_sample_rate_per_mille: 10,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCompositionResponse {
    pub app_id: String,
    pub app_version: String,
    pub revision: AppRevisionResponse,
    pub runtime_assignments: Vec<AppRuntimeAssignmentResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRevisionResponse {
    pub id: String,
    pub number: String,
    pub descriptor_sha256: String,
    pub validation_status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRuntimeAssignmentResponse {
    pub target_id: String,
    pub assignment_id: String,
    pub generation: String,
    pub status: String,
}

fn root_path() -> String {
    "/".to_owned()
}

fn default_true() -> bool {
    true
}
