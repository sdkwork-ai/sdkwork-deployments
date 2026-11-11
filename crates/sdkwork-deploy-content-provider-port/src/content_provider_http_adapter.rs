//! Opt-in split-deployment HTTP adapter for the content provider port.
//!
//! MODULE_SPEC.md §4.2.3 allows a deliberate split deployment to keep calling a
//! sibling module over its HTTP surface, provided HTTP is **never** the default,
//! requires an explicit opt-in, and fails at startup when the opt-in lacks its
//! configuration. That is this module:
//!
//! * It is only constructed when `SDKWORK_DEPLOY_CONTENT_PROVIDER_MODE=http`.
//! * `from_env` builds every client eagerly and returns an error for a missing
//!   endpoint or ingress token file, so a half-configured split deployment
//!   aborts the boot instead of failing the first website publication.
//!
//! The default deployment never loads it. The in-process ports in
//! [`crate::in_process`] are the default path.

use std::collections::HashSet;
use std::path::PathBuf;

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
use sdkwork_utils_rust::service_base_url::registry_base_url_or;

use crate::support::{
    map_provider_error, non_blank, provider_unavailable, read_secret_file, required_env,
    required_value, stable_root_key, valid_opaque_id,
};
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

/// Split-deployment content provider: Drive App API + Drive/Knowledgebase
/// Internal APIs over HTTP. Never the default.
#[derive(Clone, Debug)]
pub struct HttpContentProviderAdapter {
    drive_app_url: String,
    drive_internal_url: String,
    drive_internal_token_file: PathBuf,
    knowledgebase_internal_url: String,
    knowledgebase_internal_token_file: PathBuf,
}

impl HttpContentProviderAdapter {
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
        if !drive_internal_token_file.is_file() {
            return Err(format!(
                "{DRIVE_INTERNAL_TOKEN_FILE_ENV} does not name a readable file"
            ));
        }
        if !knowledgebase_internal_token_file.is_file() {
            return Err(format!(
                "{KNOWLEDGEBASE_INTERNAL_TOKEN_FILE_ENV} does not name a readable file"
            ));
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
        // DISCOVERY_SPEC §12.1: registry-first, configured URL as degradation.
        let base_url = registry_base_url_or("sdkwork-drive", &self.drive_app_url);
        let client = SdkworkAppClient::new_with_base_url(&base_url)
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
        let base_url = registry_base_url_or("sdkwork-drive", &self.drive_internal_url);
        let client = DriveInternalClient::new_with_base_url(&base_url)
            .map_err(|_| provider_unavailable("Drive Internal API"))?;
        client.set_api_key(read_secret_file(&self.drive_internal_token_file, "Drive")?);
        Ok(client)
    }

    fn knowledgebase_internal_client(&self) -> DeployServiceResult<KnowledgebaseInternalClient> {
        let base_url =
            registry_base_url_or("sdkwork-knowledgebase", &self.knowledgebase_internal_url);
        let client = KnowledgebaseInternalClient::new_with_base_url(&base_url)
            .map_err(|_| provider_unavailable("Knowledgebase Internal API"))?;
        client.set_api_key(read_secret_file(
            &self.knowledgebase_internal_token_file,
            "Knowledgebase",
        )?);
        Ok(client)
    }
}

#[async_trait]
impl ContentProviderPort for HttpContentProviderAdapter {
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

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_deploy_contract::DriveWebsiteRootSelector;

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
