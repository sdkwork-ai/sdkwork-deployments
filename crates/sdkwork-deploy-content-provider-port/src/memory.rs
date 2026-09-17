use async_trait::async_trait;
use sdkwork_deploy_contract::{ContentProviderResourceSource, DeployServiceError};
use sdkwork_deploy_runtime_compiler::{
    RuntimeProviderType, RuntimeResourceCapabilities, DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION,
    KNOWLEDGEBASE_WIKI_PUBLICATION_PROVIDER_CONTRACT_VERSION,
};

use crate::{
    ContentProviderPort, ProviderRequestCredentials, ValidateContentProviderResourceCommand,
    ValidatedContentProviderResource,
};

#[derive(Clone, Debug, Default)]
pub struct MemoryContentProviderPort;

#[async_trait]
impl ContentProviderPort for MemoryContentProviderPort {
    async fn validate_resource(
        &self,
        _credentials: &ProviderRequestCredentials,
        command: ValidateContentProviderResourceCommand,
    ) -> Result<ValidatedContentProviderResource, DeployServiceError> {
        let key = command.resource.key;
        let source = command.resource.source;
        let (provider_type, provider_resource_uuid, provider_contract_version, capabilities) =
            match &source {
                ContentProviderResourceSource::DriveDirectory { .. } => (
                    RuntimeProviderType::Drive,
                    stable_memory_id("drive", command.tenant_id, &command.app_uuid, &key),
                    DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION.to_owned(),
                    RuntimeResourceCapabilities {
                        static_content: true,
                        wiki_routes: false,
                        wiki_search: false,
                        range_requests: true,
                    },
                ),
                ContentProviderResourceSource::KnowledgebaseWiki { publication_uuid } => (
                    RuntimeProviderType::Knowledgebase,
                    publication_uuid.clone(),
                    KNOWLEDGEBASE_WIKI_PUBLICATION_PROVIDER_CONTRACT_VERSION.to_owned(),
                    RuntimeResourceCapabilities {
                        static_content: false,
                        wiki_routes: true,
                        wiki_search: true,
                        range_requests: false,
                    },
                ),
            };
        Ok(ValidatedContentProviderResource {
            key,
            source,
            provider_type,
            provider_resource_uuid,
            provider_contract_version,
            capabilities,
        })
    }
}

fn stable_memory_id(provider: &str, tenant_id: i64, app_uuid: &str, key: &str) -> String {
    let digest = sdkwork_utils_rust::sha256_hash(
        format!("{provider}:{tenant_id}:{app_uuid}:{key}").as_bytes(),
    );
    format!("memory-{provider}-{}", &digest[..32])
}
