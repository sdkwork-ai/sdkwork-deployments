//! Content provider ports for live Drive websites and Knowledgebase Wiki
//! publications.
//!
//! MODULE_SPEC.md §4.2 governs the transport: the default implementation is
//! in-process on the host PostgreSQL pool ([`InProcessContentProviderPort`]);
//! the HTTP adapter exists only for an explicitly selected split deployment.

mod content_provider_http_adapter;
mod in_process;
mod knowledgebase_publication;
mod memory;
mod selection;
mod support;
mod website_event_delivery;

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    AppResourceDefinition, ContentProviderResourceSource, DeployServiceResult,
};
use sdkwork_deploy_runtime_compiler::{RuntimeProviderType, RuntimeResourceCapabilities};

pub use content_provider_http_adapter::{
    HttpContentProviderAdapter, DRIVE_APP_API_URL_ENV, DRIVE_INTERNAL_API_URL_ENV,
    DRIVE_INTERNAL_TOKEN_FILE_ENV, KNOWLEDGEBASE_INTERNAL_API_URL_ENV,
    KNOWLEDGEBASE_INTERNAL_TOKEN_FILE_ENV,
};
pub use in_process::InProcessContentProviderPort;
pub use memory::MemoryContentProviderPort;
pub use selection::{
    content_provider_port_from_env, content_provider_port_from_pool,
    website_provider_event_delivery_port_from_env, ContentProviderPortSelection,
    CONTENT_PROVIDER_MODE_ENV, USE_MEMORY_CONTENT_PROVIDER_ENV,
};
pub use website_event_delivery::{
    InProcessWebsiteProviderEventDeliveryPort, NoopWebsiteProviderEventDeliveryPort,
    WebsiteProviderEventDeliveryConfig, WebsiteProviderEventDeliveryPort,
    WebsiteProviderEventDeliveryResult, WEBSITE_PROVIDER_EVENT_CALLBACK_BASE_URL_ENV,
    WEBSITE_PROVIDER_EVENT_EXPIRATION_SECONDS_ENV, WEBSITE_PROVIDER_EVENT_RENEW_BEFORE_SECONDS_ENV,
    WEBSITE_PROVIDER_EVENT_SECRET_DIRECTORY_ENV,
};

/// Per-request caller credentials (forwarded to the split-deployment HTTP
/// adapter; the in-process port needs no credentials).
#[derive(Clone, Debug, Default)]
pub struct ProviderRequestCredentials {
    pub auth_token: Option<String>,
    pub access_token: Option<String>,
}

/// The acting subject the sibling module's own ACL must resolve.
///
/// The Drive App API route derives this from the authenticated request; the
/// in-process port has to be handed the same identity, so it is part of the
/// command instead of an environment default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentProviderActor {
    pub subject_type: String,
    pub subject_id: String,
}

impl ContentProviderActor {
    pub fn user(user_id: impl Into<String>) -> Self {
        Self {
            subject_type: "user".to_owned(),
            subject_id: user_id.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ValidateContentProviderResourceCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub app_uuid: String,
    /// Calling subject for the sibling module's own ACL. `None` is accepted by
    /// the split-deployment HTTP adapter (which forwards caller credentials) and
    /// rejected by the in-process port, which has no other identity to resolve.
    pub operator: Option<ContentProviderActor>,
    pub resource: AppResourceDefinition,
}

#[derive(Clone, Debug)]
pub struct ValidatedContentProviderResource {
    pub key: String,
    pub source: ContentProviderResourceSource,
    pub provider_type: RuntimeProviderType,
    pub provider_resource_uuid: String,
    pub provider_contract_version: String,
    pub capabilities: RuntimeResourceCapabilities,
}

#[async_trait]
pub trait ContentProviderPort: Send + Sync {
    async fn validate_resource(
        &self,
        credentials: &ProviderRequestCredentials,
        command: ValidateContentProviderResourceCommand,
    ) -> DeployServiceResult<ValidatedContentProviderResource>;
}
