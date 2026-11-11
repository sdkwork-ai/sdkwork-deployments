//! In-process content-provider ports on the host PostgreSQL pool.
//!
//! MODULE_SPEC.md §4.2: a packaged deployment runs one backend process against
//! one PostgreSQL, so deploy reaches the drive and knowledgebase modules through
//! their own service/repository ports instead of their HTTP surfaces.
//!
//! What the removed calls did, and what replaces them:
//!
//! * `POST /app/v3/api/drive/spaces/{spaceId}/website_roots` (Drive App API) —
//!   `acl::ensure_space_owner` then `DriveWebsiteRootService::create_root`
//!   (`create_or_get`, so the same `root_key`/selector replay is a 200, not a
//!   duplicate). Both are called here: drive's own permission service for the
//!   ACL and drive's own website-root service for the write.
//! * `GET /internal/v3/api/drive/website_roots/{uuid}` (Drive Internal API) —
//!   `DriveWebsiteRootService::get_root` plus the constant capability set the
//!   internal route returns for a website root.
//! * `GET /internal/v3/api/knowledgebase/wiki_publications/{uuid}` —
//!   [`crate::knowledgebase_publication`] reads the same active-publication row.
//!
//! Tenant scope comes from the request context, never from configured defaults,
//! and error kinds are mapped to the same `DeployServiceError` variants the HTTP
//! path produced, so HTTP status codes do not change.

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    ContentProviderResourceSource, DeployServiceError, DeployServiceResult,
    DriveWebsiteRootSelector,
};
use sdkwork_deploy_runtime_compiler::{
    RuntimeProviderType, RuntimeResourceCapabilities, DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION,
    KNOWLEDGEBASE_WIKI_PUBLICATION_PROVIDER_CONTRACT_VERSION,
};
use sdkwork_drive_workspace_service::application::permission_service::SqlDrivePermissionService;
use sdkwork_drive_workspace_service::application::website_root_service::{
    CreateWebsiteRootCommand, DriveWebsiteRootService, GetWebsiteRootCommand,
};
use sdkwork_drive_workspace_service::domain::website_root::{
    DriveWebsiteContentMode, DriveWebsiteRoot, DriveWebsiteSourceRootMode,
};
use sdkwork_drive_workspace_service::infrastructure::sql::website_root_store::SqlWebsiteRootStore;
use sdkwork_drive_workspace_service::ports::permission_store::ResolveEffectiveNodeAccessCommand;
use sdkwork_drive_workspace_service::DriveServiceError;
use sqlx::PgPool;

use crate::knowledgebase_publication::find_active_wiki_publication;
use crate::support::{provider_unavailable, stable_root_key, valid_opaque_id};
use crate::{
    ContentProviderActor, ContentProviderPort, ProviderRequestCredentials,
    ValidateContentProviderResourceCommand, ValidatedContentProviderResource,
};

/// Drive and Knowledgebase content validated in-process on the host pool.
#[derive(Clone, Debug)]
pub struct InProcessContentProviderPort {
    pool: PgPool,
}

impl InProcessContentProviderPort {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Host pool this port is bound to.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    fn website_roots(&self) -> DriveWebsiteRootService<SqlWebsiteRootStore> {
        DriveWebsiteRootService::new(SqlWebsiteRootStore::new(self.pool.clone()))
    }

    /// Same authorization the Drive App API route performs before it creates a
    /// website root: space owner, or the `owner` role on the space's permission
    /// anchor node. The check is drive's own permission service — never a
    /// re-implementation of its ACL.
    async fn ensure_space_owner(
        &self,
        tenant_id: &str,
        space_id: &str,
        actor: &ContentProviderActor,
    ) -> DeployServiceResult<()> {
        let service = SqlDrivePermissionService::new(self.pool.clone());
        if service
            .is_space_owner(tenant_id, space_id, &actor.subject_type, &actor.subject_id)
            .await
            .map_err(map_drive_error)?
        {
            return Ok(());
        }
        let anchor_node_id = match service
            .resolve_space_permission_anchor_node(tenant_id, space_id)
            .await
        {
            Ok(anchor_node_id) => anchor_node_id,
            Err(DriveServiceError::NotFound(_)) => return Err(provider_content_forbidden()),
            Err(error) => return Err(map_drive_error(error)),
        };
        let access = service
            .resolve_effective_node_access(ResolveEffectiveNodeAccessCommand {
                tenant_id: tenant_id.to_string(),
                space_id: space_id.to_string(),
                node_id: anchor_node_id,
                subject_type: actor.subject_type.clone(),
                subject_id: actor.subject_id.clone(),
            })
            .await
            .map_err(map_drive_error)?;
        if !access.allows_role("owner") {
            return Err(provider_content_forbidden());
        }
        Ok(())
    }

    async fn validate_drive_directory(
        &self,
        tenant_id: i64,
        app_uuid: &str,
        key: String,
        source: ContentProviderResourceSource,
        website_space_id: &str,
        root: &DriveWebsiteRootSelector,
        content_mode: &sdkwork_deploy_contract::DriveWebsiteContentMode,
        actor: &ContentProviderActor,
    ) -> DeployServiceResult<ValidatedContentProviderResource> {
        let tenant_scope = tenant_id.to_string();
        self.ensure_space_owner(&tenant_scope, website_space_id, actor)
            .await?;
        let (source_root_mode, selected_folder_node_id) = match root {
            DriveWebsiteRootSelector::SpaceRoot => (DriveWebsiteSourceRootMode::SpaceRoot, None),
            DriveWebsiteRootSelector::Folder { folder_node_id }
                if valid_opaque_id(folder_node_id) =>
            {
                (
                    DriveWebsiteSourceRootMode::Folder,
                    Some(folder_node_id.clone()),
                )
            }
            DriveWebsiteRootSelector::Folder { .. } => {
                return Err(DeployServiceError::validation(
                    "Drive folderNodeId is invalid",
                ))
            }
        };
        let drive_content_mode = match content_mode {
            sdkwork_deploy_contract::DriveWebsiteContentMode::LiveTree => {
                DriveWebsiteContentMode::LiveTree
            }
            sdkwork_deploy_contract::DriveWebsiteContentMode::AtomicGeneration => {
                DriveWebsiteContentMode::AtomicGeneration
            }
        };
        let root_key = stable_root_key(tenant_id, app_uuid, &key);
        let created = self
            .website_roots()
            .create_root(CreateWebsiteRootCommand {
                tenant_id: tenant_scope.clone(),
                space_id: website_space_id.to_string(),
                root_key: root_key.clone(),
                display_name: format!("SDKWork Deploy {key}"),
                source_root_mode,
                selected_folder_node_id,
                content_mode: drive_content_mode,
                operator_id: actor.subject_id.clone(),
            })
            .await
            .map_err(map_drive_error)?;
        // Re-read through the same service the Drive Internal API route uses, so
        // the row is validated in the tenant scope the caller asked for rather
        // than trusting the write that just happened.
        let observed = self
            .website_roots()
            .get_root(GetWebsiteRootCommand {
                tenant_id: tenant_scope,
                root_uuid: created.root.uuid.clone(),
            })
            .await
            .map_err(map_drive_error)?;
        validate_website_root(&created.root, website_space_id, root, content_mode.as_str())?;
        validate_website_root(&observed, website_space_id, root, content_mode.as_str())?;
        Ok(ValidatedContentProviderResource {
            key,
            source,
            provider_type: RuntimeProviderType::Drive,
            provider_resource_uuid: observed.uuid,
            provider_contract_version: DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION.to_owned(),
            capabilities: RuntimeResourceCapabilities {
                static_content: true,
                wiki_routes: false,
                wiki_search: false,
                range_requests: true,
            },
        })
    }

    async fn validate_knowledgebase_wiki(
        &self,
        tenant_id: i64,
        organization_id: i64,
        key: String,
        source: ContentProviderResourceSource,
        publication_uuid: &str,
    ) -> DeployServiceResult<ValidatedContentProviderResource> {
        if !valid_opaque_id(publication_uuid) {
            return Err(DeployServiceError::validation(
                "Knowledgebase publicationUuid is invalid",
            ));
        }
        let publication =
            find_active_wiki_publication(&self.pool, tenant_id, organization_id, publication_uuid)
                .await?
                .ok_or_else(|| {
                    DeployServiceError::not_found(
                        "Knowledgebase publication resource was not found",
                    )
                })?;
        if publication.publication_uuid != publication_uuid {
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
}

#[async_trait]
impl ContentProviderPort for InProcessContentProviderPort {
    async fn validate_resource(
        &self,
        _credentials: &ProviderRequestCredentials,
        command: ValidateContentProviderResourceCommand,
    ) -> DeployServiceResult<ValidatedContentProviderResource> {
        let key = command.resource.key;
        let source = command.resource.source;
        let actor = command
            .operator
            .as_ref()
            .ok_or_else(|| DeployServiceError::forbidden("provider content access forbidden"))?;
        match source.clone() {
            ContentProviderResourceSource::DriveDirectory {
                ref website_space_id,
                ref root,
                ref content_mode,
            } => {
                self.validate_drive_directory(
                    command.tenant_id,
                    &command.app_uuid,
                    key,
                    source,
                    website_space_id,
                    root,
                    content_mode,
                    actor,
                )
                .await
            }
            ContentProviderResourceSource::KnowledgebaseWiki { publication_uuid } => {
                self.validate_knowledgebase_wiki(
                    command.tenant_id,
                    command.organization_id,
                    key,
                    source,
                    &publication_uuid,
                )
                .await
            }
        }
    }
}

/// Validates the website root the way the two removed routes did: the App API
/// response had to match the requested space/selector/content mode and be
/// `ACTIVE`, and the Internal API response additionally had to advertise the
/// static-delivery capability set.
fn validate_website_root(
    root: &DriveWebsiteRoot,
    space_id: &str,
    selector: &DriveWebsiteRootSelector,
    content_mode: &str,
) -> DeployServiceResult<()> {
    let folder_matches = match selector {
        DriveWebsiteRootSelector::SpaceRoot => root.selected_folder_node_id.is_none(),
        DriveWebsiteRootSelector::Folder { folder_node_id } => {
            root.selected_folder_node_id.as_deref() == Some(folder_node_id.as_str())
        }
    };
    let expected_mode = match selector {
        DriveWebsiteRootSelector::SpaceRoot => DriveWebsiteSourceRootMode::SpaceRoot,
        DriveWebsiteRootSelector::Folder { .. } => DriveWebsiteSourceRootMode::Folder,
    };
    if root.space_id != space_id
        || root.source_root_mode != expected_mode
        || !root
            .content_mode
            .as_str()
            .eq_ignore_ascii_case(content_mode)
        || !root.root_status.eq_ignore_ascii_case("active")
        || !folder_matches
        || !valid_opaque_id(&root.uuid)
    {
        return Err(DeployServiceError::validation(
            "Drive WebsiteRoot is not eligible for website delivery",
        ));
    }
    Ok(())
}

fn provider_content_forbidden() -> DeployServiceError {
    DeployServiceError::forbidden("provider content access forbidden")
}

/// Maps drive's typed service error onto the same `DeployServiceError` kind the
/// HTTP adapter produced from a status code. Messages stay redacted.
fn map_drive_error(error: DriveServiceError) -> DeployServiceError {
    match error {
        DriveServiceError::Validation(_) => {
            DeployServiceError::validation("Drive publication resource is invalid")
        }
        DriveServiceError::Conflict(_) => {
            DeployServiceError::conflict("Drive publication resource conflicts")
        }
        DriveServiceError::NotFound(_) => {
            DeployServiceError::not_found("Drive publication resource was not found")
        }
        DriveServiceError::PermissionDenied(_) => provider_content_forbidden(),
        DriveServiceError::Internal(_) => provider_unavailable("Drive API"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn website_root(
        space_id: &str,
        folder: Option<&str>,
        content_mode: DriveWebsiteContentMode,
    ) -> DriveWebsiteRoot {
        DriveWebsiteRoot {
            id: "1".to_owned(),
            uuid: "01900000-0000-7000-8000-000000000001".to_owned(),
            tenant_id: "7".to_owned(),
            space_id: space_id.to_owned(),
            root_key: "deploy-1".to_owned(),
            display_name: "SDKWork Deploy site".to_owned(),
            source_root_mode: if folder.is_some() {
                DriveWebsiteSourceRootMode::Folder
            } else {
                DriveWebsiteSourceRootMode::SpaceRoot
            },
            selected_folder_node_id: folder.map(str::to_owned),
            content_mode,
            active_node_id: "node-1".to_owned(),
            active_generation: 1,
            root_status: "active".to_owned(),
            version: 1,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn space_root_must_not_carry_a_selected_folder() {
        let root = website_root(
            "space-1",
            Some("folder-1"),
            DriveWebsiteContentMode::LiveTree,
        );
        assert!(validate_website_root(
            &root,
            "space-1",
            &DriveWebsiteRootSelector::SpaceRoot,
            "LIVE_TREE"
        )
        .is_err());
    }

    #[test]
    fn folder_root_must_match_the_exact_requested_folder() {
        let root = website_root(
            "space-1",
            Some("folder-other"),
            DriveWebsiteContentMode::LiveTree,
        );
        assert!(validate_website_root(
            &root,
            "space-1",
            &DriveWebsiteRootSelector::Folder {
                folder_node_id: "folder-1".to_owned(),
            },
            "LIVE_TREE",
        )
        .is_err());
        let root = website_root(
            "space-1",
            Some("folder-1"),
            DriveWebsiteContentMode::LiveTree,
        );
        assert!(validate_website_root(
            &root,
            "space-1",
            &DriveWebsiteRootSelector::Folder {
                folder_node_id: "folder-1".to_owned(),
            },
            "LIVE_TREE",
        )
        .is_ok());
    }

    #[test]
    fn inactive_roots_are_not_eligible() {
        let mut root = website_root("space-1", None, DriveWebsiteContentMode::AtomicGeneration);
        root.root_status = "suspended".to_owned();
        assert!(validate_website_root(
            &root,
            "space-1",
            &DriveWebsiteRootSelector::SpaceRoot,
            "ATOMIC_GENERATION"
        )
        .is_err());
    }

    #[test]
    fn drive_errors_keep_the_http_error_kinds() {
        assert_eq!(
            map_drive_error(DriveServiceError::NotFound("x".to_owned())).kind(),
            DeployServiceError::not_found("x").kind()
        );
        assert_eq!(
            map_drive_error(DriveServiceError::Conflict("x".to_owned())).kind(),
            DeployServiceError::conflict("x").kind()
        );
        assert_eq!(
            map_drive_error(DriveServiceError::Validation("x".to_owned())).kind(),
            DeployServiceError::validation("x").kind()
        );
        assert_eq!(
            map_drive_error(DriveServiceError::PermissionDenied("x".to_owned())).kind(),
            DeployServiceError::forbidden("x").kind()
        );
    }
}
