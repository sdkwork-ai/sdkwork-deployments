use std::collections::HashSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    ContentProviderResourceSource, DeployServiceError, DeployServiceResult,
    DriveWebsiteRootSelector,
};
use sdkwork_deploy_runtime_compiler::{
    RuntimeProviderType, RuntimeResourceCapabilities, DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION,
    KNOWLEDGEBASE_WIKI_PUBLICATION_PROVIDER_CONTRACT_VERSION,
};
use sdkwork_drive_app_sdk_generated_rust::{
    CreateWebsiteRootRequest, SdkworkAppClient, WebsiteRootFolderSelector, WebsiteRootSelector,
    WebsiteRootSpaceSelector,
};
use sdkwork_drive_internal_sdk_generated_rust::SdkworkCustomClient as DriveInternalClient;
use sdkwork_knowledgebase_internal_sdk::SdkworkCustomClient as KnowledgebaseInternalClient;
use sdkwork_utils_rust::string::trim;

use crate::{
    ContentProviderPort, ProviderRequestCredentials, ValidateContentProviderResourceCommand,
    ValidatedContentProviderResource,
};

pub const DRIVE_APP_API_URL_ENV: &str = "SDKWORK_DRIVE_FACADE_URL";
pub const DRIVE_INTERNAL_API_URL_ENV: &str = "SDKWORK_DEPLOY_DRIVE_INTERNAL_API_URL";
pub const DRIVE_INTERNAL_TOKEN_FILE_ENV: &str =
    "SDKWORK_DEPLOY_DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE";
pub const KNOWLEDGEBASE_INTERNAL_API_URL_ENV: &str =
    "SDKWORK_DEPLOY_KNOWLEDGEBASE_INTERNAL_API_URL";
pub const KNOWLEDGEBASE_INTERNAL_TOKEN_FILE_ENV: &str =
    "SDKWORK_DEPLOY_KNOWLEDGEBASE_INTERNAL_API_INGRESS_TOKEN_FILE";

#[derive(Clone, Debug)]
pub struct SdkContentProviderPort {
    drive_app_url: String,
    drive_internal_url: String,
    drive_internal_token_file: PathBuf,
    knowledgebase_internal_url: String,
    knowledgebase_internal_token_file: PathBuf,
}

impl SdkContentProviderPort {
    pub fn new(
        drive_app_url: String,
        drive_internal_url: String,
        drive_internal_token_file: PathBuf,
        knowledgebase_internal_url: String,
        knowledgebase_internal_token_file: PathBuf,
    ) -> Result<Self, String> {
        let drive_app_url = required_value(&drive_app_url, DRIVE_APP_API_URL_ENV)?;
        let drive_internal_url = required_value(&drive_internal_url, DRIVE_INTERNAL_API_URL_ENV)?;
        let knowledgebase_internal_url = required_value(
            &knowledgebase_internal_url,
            KNOWLEDGEBASE_INTERNAL_API_URL_ENV,
        )?;
        if drive_internal_token_file.as_os_str().is_empty()
            || knowledgebase_internal_token_file.as_os_str().is_empty()
        {
            return Err("content provider ingress token file paths must not be blank".to_owned());
        }
        Ok(Self {
            drive_app_url,
            drive_internal_url,
            drive_internal_token_file,
            knowledgebase_internal_url,
            knowledgebase_internal_token_file,
        })
    }

    pub fn from_env() -> Result<Self, String> {
        Self::new(
            required_env(DRIVE_APP_API_URL_ENV)?,
            required_env(DRIVE_INTERNAL_API_URL_ENV)?,
            PathBuf::from(required_env(DRIVE_INTERNAL_TOKEN_FILE_ENV)?),
            required_env(KNOWLEDGEBASE_INTERNAL_API_URL_ENV)?,
            PathBuf::from(required_env(KNOWLEDGEBASE_INTERNAL_TOKEN_FILE_ENV)?),
        )
    }

    fn drive_app_client(
        &self,
        credentials: &ProviderRequestCredentials,
    ) -> DeployServiceResult<SdkworkAppClient> {
        let client = SdkworkAppClient::new_with_base_url(&self.drive_app_url)
            .map_err(|_| provider_unavailable("Drive App API"))?;
        if let Some(token) = non_blank(credentials.auth_token.as_deref()) {
            client.set_auth_token(token);
        }
        if let Some(token) = non_blank(credentials.access_token.as_deref()) {
            client.set_access_token(token);
        }
        Ok(client)
    }

    fn drive_internal_client(&self) -> DeployServiceResult<DriveInternalClient> {
        let client = DriveInternalClient::new_with_base_url(&self.drive_internal_url)
            .map_err(|_| provider_unavailable("Drive Internal API"))?;
        client.set_api_key(read_secret_file(&self.drive_internal_token_file, "Drive")?);
        Ok(client)
    }

    fn knowledgebase_internal_client(&self) -> DeployServiceResult<KnowledgebaseInternalClient> {
        let client =
            KnowledgebaseInternalClient::new_with_base_url(&self.knowledgebase_internal_url)
                .map_err(|_| provider_unavailable("Knowledgebase Internal API"))?;
        client.set_api_key(read_secret_file(
            &self.knowledgebase_internal_token_file,
            "Knowledgebase",
        )?);
        Ok(client)
    }
}

#[async_trait]
impl ContentProviderPort for SdkContentProviderPort {
    async fn validate_resource(
        &self,
        credentials: &ProviderRequestCredentials,
        command: ValidateContentProviderResourceCommand,
    ) -> DeployServiceResult<ValidatedContentProviderResource> {
        let key = command.resource.key;
        let source = command.resource.source;
        match &source {
            ContentProviderResourceSource::DriveDirectory {
                website_space_id,
                root,
                content_mode,
            } => {
                let root_key = stable_root_key(command.tenant_id, &command.app_uuid, &key);
                let created = self
                    .drive_app_client(credentials)?
                    .drive()
                    .website_roots_create(
                        website_space_id,
                        &CreateWebsiteRootRequest {
                            root_key: root_key.clone(),
                            display_name: format!("SDKWork Deploy {key}"),
                            source_root: drive_selector(root)?,
                            content_mode: content_mode.as_str().to_owned(),
                        },
                        &root_key,
                    )
                    .await
                    .map_err(|error| map_provider_error("Drive", &error.to_string()))?;
                validate_created_drive_root(
                    &created,
                    website_space_id,
                    root,
                    content_mode.as_str(),
                )?;
                let observed = self
                    .drive_internal_client()?
                    .drive_internal_publishing()
                    .website_roots_retrieve(&created.uuid)
                    .await
                    .map_err(|error| map_provider_error("Drive", &error.to_string()))?;
                validate_observed_drive_root(
                    &observed,
                    website_space_id,
                    root,
                    content_mode.as_str(),
                )?;
                Ok(ValidatedContentProviderResource {
                    key,
                    source,
                    provider_type: RuntimeProviderType::Drive,
                    provider_resource_uuid: observed.uuid,
                    provider_contract_version: DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION
                        .to_owned(),
                    capabilities: RuntimeResourceCapabilities {
                        static_content: true,
                        wiki_routes: false,
                        wiki_search: false,
                        range_requests: true,
                    },
                })
            }
            ContentProviderResourceSource::KnowledgebaseWiki { publication_uuid } => {
                let publication_uuid = publication_uuid.clone();
                if !valid_opaque_id(&publication_uuid) {
                    return Err(DeployServiceError::validation(
                        "Knowledgebase publicationUuid is invalid",
                    ));
                }
                let publication = self
                    .knowledgebase_internal_client()?
                    .knowledgebase_internal_wiki()
                    .wiki_publications_retrieve(&publication_uuid)
                    .await
                    .map_err(|error| map_provider_error("Knowledgebase", &error.to_string()))?;
                validated_knowledgebase_resource(key, source, &publication_uuid, publication)
            }
        }
    }
}

fn validated_knowledgebase_resource(
    key: String,
    source: ContentProviderResourceSource,
    expected_publication_uuid: &str,
    publication: sdkwork_knowledgebase_internal_sdk::WikiPublication,
) -> DeployServiceResult<ValidatedContentProviderResource> {
    if publication.publication_uuid != expected_publication_uuid {
        return Err(provider_unavailable("Knowledgebase Internal API"));
    }
    Ok(ValidatedContentProviderResource {
        key,
        source,
        provider_type: RuntimeProviderType::Knowledgebase,
        provider_resource_uuid: publication.publication_uuid,
        provider_contract_version: KNOWLEDGEBASE_WIKI_PUBLICATION_PROVIDER_CONTRACT_VERSION
            .to_owned(),
        capabilities: RuntimeResourceCapabilities {
            static_content: false,
            wiki_routes: true,
            wiki_search: publication.search_enabled,
            range_requests: false,
        },
    })
}

pub(crate) fn required_env(key: &str) -> Result<String, String> {
    std::env::var(key).map_err(|_| format!("{key} is required"))
}

pub(crate) fn required_value(value: &str, key: &str) -> Result<String, String> {
    let value = trim(value);
    if value.is_empty() {
        return Err(format!("{key} must not be blank"));
    }
    Ok(value.to_owned())
}

fn non_blank(value: Option<&str>) -> Option<String> {
    value.map(trim).filter(|value| !value.is_empty())
}

pub(crate) fn read_secret_file(path: &Path, provider: &str) -> DeployServiceResult<String> {
    let value = std::fs::read_to_string(path)
        .map_err(|_| provider_unavailable(&format!("{provider} Internal API")))?;
    let value = trim(&value);
    if !(16..=4_096).contains(&value.len()) || value.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(provider_unavailable(&format!("{provider} Internal API")));
    }
    Ok(value.to_owned())
}

fn drive_selector(root: &DriveWebsiteRootSelector) -> DeployServiceResult<WebsiteRootSelector> {
    match root {
        DriveWebsiteRootSelector::SpaceRoot => Ok(WebsiteRootSelector::WebsiteRootSpaceSelector(
            WebsiteRootSpaceSelector {
                mode: "SPACE_ROOT".to_owned(),
            },
        )),
        DriveWebsiteRootSelector::Folder { folder_node_id } if valid_opaque_id(folder_node_id) => {
            Ok(WebsiteRootSelector::WebsiteRootFolderSelector(
                WebsiteRootFolderSelector {
                    mode: "FOLDER".to_owned(),
                    folder_node_id: folder_node_id.clone(),
                },
            ))
        }
        DriveWebsiteRootSelector::Folder { .. } => Err(DeployServiceError::validation(
            "Drive folderNodeId is invalid",
        )),
    }
}

fn expected_root_mode(root: &DriveWebsiteRootSelector) -> &'static str {
    match root {
        DriveWebsiteRootSelector::SpaceRoot => "SPACE_ROOT",
        DriveWebsiteRootSelector::Folder { .. } => "FOLDER",
    }
}

fn validate_created_drive_root(
    root: &sdkwork_drive_app_sdk_generated_rust::WebsiteRoot,
    space_id: &str,
    selector: &DriveWebsiteRootSelector,
    content_mode: &str,
) -> DeployServiceResult<()> {
    let folder_matches = match selector {
        DriveWebsiteRootSelector::SpaceRoot => root.selected_folder_node_id.is_empty(),
        DriveWebsiteRootSelector::Folder { folder_node_id } => {
            root.selected_folder_node_id == *folder_node_id
        }
    };
    if root.space_id != space_id
        || root.source_root_mode != expected_root_mode(selector)
        || root.content_mode != content_mode
        || root.root_status != "ACTIVE"
        || !folder_matches
        || !valid_opaque_id(&root.uuid)
    {
        return Err(provider_unavailable("Drive App API"));
    }
    Ok(())
}

fn validate_observed_drive_root(
    root: &sdkwork_drive_internal_sdk_generated_rust::WebsiteRoot,
    space_id: &str,
    selector: &DriveWebsiteRootSelector,
    content_mode: &str,
) -> DeployServiceResult<()> {
    let capabilities = root
        .capabilities
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if root.space_id != space_id
        || root.source_root_mode != expected_root_mode(selector)
        || root.content_mode != content_mode
        || root.root_status != "ACTIVE"
        || !capabilities.contains("STATIC_CONTENT")
        || !capabilities.contains("BYTE_RANGE")
        || !capabilities.contains("CONDITIONAL_REQUESTS")
    {
        return Err(DeployServiceError::validation(
            "Drive WebsiteRoot is not eligible for website delivery",
        ));
    }
    Ok(())
}

fn stable_root_key(tenant_id: i64, app_uuid: &str, resource_key: &str) -> String {
    let digest = sdkwork_utils_rust::sha256_hash(
        format!("{tenant_id}:{app_uuid}:{resource_key}").as_bytes(),
    );
    format!("deploy-{}", &digest[..32])
}

pub(crate) fn valid_opaque_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

pub(crate) fn map_provider_error(provider: &str, message: &str) -> DeployServiceError {
    let lowercase = message.to_ascii_lowercase();
    if message.contains("404") || lowercase.contains("not found") {
        DeployServiceError::not_found(format!("{provider} publication resource was not found"))
    } else if message.contains("401") || message.contains("403") {
        DeployServiceError::forbidden("provider content access forbidden")
    } else if message.contains("409") {
        DeployServiceError::conflict(format!("{provider} publication resource conflicts"))
    } else if message.contains("400") || message.contains("422") {
        DeployServiceError::validation(format!("{provider} publication resource is invalid"))
    } else {
        provider_unavailable(&format!("{provider} API"))
    }
}

pub(crate) fn provider_unavailable(provider: &str) -> DeployServiceError {
    DeployServiceError::Internal(format!("{provider} is unavailable"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_deploy_contract::DriveWebsiteRootSelector;

    #[test]
    fn provider_errors_are_redacted() {
        let error = map_provider_error("Drive", "500 token=do-not-leak");
        assert_eq!(
            error.to_string(),
            "internal error: Drive API is unavailable"
        );
    }

    #[test]
    fn root_keys_are_stable_and_bounded() {
        let left = stable_root_key(7, "site", "desktop");
        let right = stable_root_key(7, "site", "desktop");
        assert_eq!(left, right);
        assert!(left.len() <= 64);
    }

    #[test]
    fn drive_folder_selector_requires_a_valid_folder_node_id() {
        let error = drive_selector(&DriveWebsiteRootSelector::Folder {
            folder_node_id: String::new(),
        })
        .expect_err("blank folder id must be rejected");
        assert!(error.to_string().contains("folderNodeId"));
    }

    #[test]
    fn created_drive_root_must_match_the_exact_folder_selector() {
        let root = sdkwork_drive_app_sdk_generated_rust::WebsiteRoot {
            uuid: "root-1".to_owned(),
            space_id: "space-1".to_owned(),
            source_root_mode: "FOLDER".to_owned(),
            selected_folder_node_id: "folder-other".to_owned(),
            content_mode: "LIVE_TREE".to_owned(),
            root_status: "ACTIVE".to_owned(),
            ..Default::default()
        };
        assert!(validate_created_drive_root(
            &root,
            "space-1",
            &DriveWebsiteRootSelector::Folder {
                folder_node_id: "folder-1".to_owned(),
            },
            "LIVE_TREE",
        )
        .is_err());
    }

    #[test]
    fn knowledgebase_search_capability_matches_the_owner_publication() {
        for search_enabled in [false, true] {
            let publication = sdkwork_knowledgebase_internal_sdk::WikiPublication {
                publication_uuid: "publication-1".to_owned(),
                search_enabled,
                ..Default::default()
            };
            let resource = validated_knowledgebase_resource(
                "wiki".to_owned(),
                ContentProviderResourceSource::KnowledgebaseWiki {
                    publication_uuid: "publication-1".to_owned(),
                },
                "publication-1",
                publication,
            )
            .expect("valid owner publication");
            assert_eq!(resource.capabilities.wiki_search, search_enabled);
            assert!(resource.capabilities.wiki_routes);
            assert!(!resource.capabilities.static_content);
        }
    }
}
