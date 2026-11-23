//! Verification-file bytes in sdkwork-drive.
//!
//! A domain-ownership verification file is small (a token, a short HTML page, an
//! association document) but it is still a *file the tenant published*: it
//! belongs in Drive next to everything else the tenant owns, so quota, audit,
//! retention and deletion are Drive's job rather than four kilobytes of text in
//! a control-plane table.
//!
//! # Layout
//!
//! One space per tenant, named [`VERIFICATION_SPACE_NAME`], holding one folder
//! per root domain (`apex_hostname`) and one node per file
//! (`file_name`). The fixed-name platforms replace the node wholesale on
//! re-upload, which is what "the platform reissued its file" means.
//!
//! # Two implementations, one contract
//!
//! [`InProcessDeployVerificationObjectStore`] runs Drive's own uploader service
//! on the host PostgreSQL pool (MODULE_SPEC.md §4.2); [`MemoryDeployVerificationObjectStore`]
//! is the test double. Both answer the same [`DeployVerificationObjectStore`]
//! contract, so the deploy service never learns which one it holds.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sdkwork_utils_rust::sha256_hash;

use crate::DeployDriveScope;

/// Display name of the tenant's verification-file space.
pub const VERIFICATION_SPACE_NAME: &str = "域名验证";

/// Where one stored verification file lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriveVerificationFileRef {
    /// Drive node that names the file; the control-plane row records it.
    pub node_id: String,
    /// Drive space the node lives in.
    pub space_id: String,
    /// SHA-256 of the stored bytes, so the row can prove what was written.
    pub content_sha256: String,
    /// Byte length the edge reports in `Content-Length`.
    pub content_length: i64,
}

/// One verification file to store.
#[derive(Clone, Debug)]
pub struct StoreVerificationFileCommand {
    pub scope: DeployDriveScope,
    /// Root domain the file is published under; becomes the folder name.
    pub apex_hostname: String,
    /// File name the platform fetches (`MP_verify_x.txt`).
    pub file_name: String,
    /// `Content-Type` the edge must answer with.
    pub content_type: String,
    /// Exact bytes the platform issued.
    pub content: String,
}

/// Durable home for verification-file bytes.
#[async_trait]
pub trait DeployVerificationObjectStore: Send + Sync {
    /// Stores (or replaces) one verification file and answers its Drive
    /// reference. Replacing the same `(apex, file_name)` must not create a
    /// second node.
    async fn store_verification_file(
        &self,
        command: StoreVerificationFileCommand,
    ) -> DeployServiceResult<DriveVerificationFileRef>;

    /// Reads the bytes back, for the control plane's own self-check.
    async fn read_verification_file(
        &self,
        scope: &DeployDriveScope,
        node_id: &str,
    ) -> DeployServiceResult<Option<String>>;
}

/// Test double: the same contract, in process memory.
///
/// State is process-global on purpose. The double is constructed as a unit
/// value in dozens of tests, and giving it a field would have changed every one
/// of those call sites; keys are random node ids, so instances never observe
/// each other's rows.
#[derive(Clone, Default)]
pub struct MemoryDeployVerificationObjectStore;

fn memory_files() -> &'static Mutex<HashMap<String, (DriveVerificationFileRef, String)>> {
    static FILES: OnceLock<Mutex<HashMap<String, (DriveVerificationFileRef, String)>>> =
        OnceLock::new();
    FILES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[async_trait]
impl DeployVerificationObjectStore for MemoryDeployVerificationObjectStore {
    async fn store_verification_file(
        &self,
        command: StoreVerificationFileCommand,
    ) -> DeployServiceResult<DriveVerificationFileRef> {
        let reference = DriveVerificationFileRef {
            node_id: format!("mem-verification-{}", sdkwork_database_id::uuid_v4()),
            space_id: format!("mem-space-{}", command.scope.tenant_id),
            content_sha256: sha256_hash(command.content.as_bytes()),
            content_length: command.content.len() as i64,
        };
        memory_files()
            .lock()
            .map_err(|_| DeployServiceError::Internal("verification store poisoned".to_owned()))?
            .insert(
                reference.node_id.clone(),
                (reference.clone(), command.content),
            );
        Ok(reference)
    }

    async fn read_verification_file(
        &self,
        scope: &DeployDriveScope,
        node_id: &str,
    ) -> DeployServiceResult<Option<String>> {
        Ok(memory_files()
            .lock()
            .map_err(|_| DeployServiceError::Internal("verification store poisoned".to_owned()))?
            .get(node_id)
            .map(|(_, content)| content.clone()))
    }
}

/// Production: Drive's own uploader service on the host pool.
///
/// The upload runs through `DriveUploaderService` (prepare → single-part put →
/// complete), so tenant scoping, the writer ACL, content policy, part accounting
/// and the node-version activation all stay inside Drive. This module only picks
/// the space and the folder and translates the answer.
#[derive(Clone)]
pub struct InProcessDeployVerificationObjectStore {
    pool: sqlx::PgPool,
}

impl InProcessDeployVerificationObjectStore {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

/// The space a tenant's verification files live in.
///
/// One per tenant, named [`VERIFICATION_SPACE_NAME`]; `app_upload` is the Drive
/// space type the platform's own uploads already use, so the space is not a new
/// storage class.
const VERIFICATION_SPACE_TYPE: &str = "app_upload";

mod in_process_store {
    use super::*;
    use sdkwork_drive_object_runtime::{DriveObjectStoreRuntime, ProviderAccessIntent};
    use sdkwork_drive_storage_contract::{DriveObjectLocator, PutObjectRequest};
    use sdkwork_drive_workspace_service::application::node_service::{
        CreateNodeCommand, DriveNodeService,
    };
    use sdkwork_drive_workspace_service::application::uploader_service::{
        CompleteStoredUploaderUploadCommand, DriveUploaderService, MarkUploaderPartUploadedCommand,
        PrepareUploaderUploadCommand, UploaderActor, UploaderRetention, UploaderTarget,
    };
    use sdkwork_drive_workspace_service::domain::node::DriveNodeType;
    use sdkwork_drive_workspace_service::infrastructure::sql::node_store::SqlNodeStore;
    use sdkwork_drive_workspace_service::infrastructure::sql::uploader_store::SqlUploaderStore;
    use sdkwork_drive_workspace_service::ports::uploader_store::{
        DriveUploaderStore, NewDriveUploaderSpace,
    };
    use sdkwork_drive_workspace_service::DriveServiceError;
    use sdkwork_utils_rust::to_unix_millis;

    fn map_drive_error(error: DriveServiceError) -> DeployServiceError {
        DeployServiceError::Internal(format!("drive verification storage failed: {error:?}"))
    }

    fn require_operator(scope: &DeployDriveScope) -> DeployServiceResult<String> {
        scope
            .operator
            .as_ref()
            .map(|operator| operator.subject_id.clone())
            .ok_or_else(|| {
                DeployServiceError::validation(
                    "storing a verification file requires an acting subject",
                )
            })
    }

    /// Deterministic ids keep re-uploads idempotent: the same tenant/apex/file
    /// resolves the same folder and node instead of accumulating copies.
    fn space_id(tenant_id: i64) -> String {
        format!("deploy-domain-verification-space-{tenant_id}")
    }

    fn folder_id(tenant_id: i64, apex: &str) -> String {
        format!("deploy-domain-verification-folder-{tenant_id}-{apex}")
    }

    fn node_id(tenant_id: i64, apex: &str, file_name: &str) -> String {
        format!("deploy-domain-verification-node-{tenant_id}-{apex}-{file_name}")
    }

    impl InProcessDeployVerificationObjectStore {
        async fn ensure_space(&self, tenant: &str, operator: &str) -> DeployServiceResult<String> {
            let store = SqlUploaderStore::new(self.pool.clone());
            if let Some(space_id) = store
                .find_upload_space(tenant, "tenant", tenant, VERIFICATION_SPACE_TYPE)
                .await
                .map_err(map_drive_error)?
            {
                return Ok(space_id);
            }
            let id = space_id(tenant.parse().unwrap_or_default());
            store
                .insert_upload_space(&NewDriveUploaderSpace {
                    id: id.clone(),
                    tenant_id: tenant.to_owned(),
                    owner_subject_type: "tenant".to_owned(),
                    owner_subject_id: tenant.to_owned(),
                    space_type: VERIFICATION_SPACE_TYPE.to_owned(),
                    display_name: VERIFICATION_SPACE_NAME.to_owned(),
                    operator_id: operator.to_owned(),
                })
                .await
                .map_err(map_drive_error)?;
            Ok(id)
        }

        async fn ensure_folder(
            &self,
            tenant: &str,
            space_id: &str,
            apex: &str,
            operator: &str,
        ) -> DeployServiceResult<String> {
            let nodes = DriveNodeService::new(SqlNodeStore::new(self.pool.clone()));
            let id = folder_id(tenant.parse().unwrap_or_default(), apex);
            match nodes
                .create_node(CreateNodeCommand {
                    id: id.clone(),
                    tenant_id: tenant.to_owned(),
                    space_id: space_id.to_owned(),
                    parent_node_id: None,
                    node_type: DriveNodeType::Folder,
                    node_name: apex.to_owned(),
                    operator_id: operator.to_owned(),
                })
                .await
            {
                Ok(node) => Ok(node.id),
                // The folder already exists (a previous upload, or a concurrent
                // one that won the create): reuse it.
                Err(DriveServiceError::Conflict(_)) => Ok(id),
                Err(error) => Err(map_drive_error(error)),
            }
        }
    }

    #[async_trait]
    impl DeployVerificationObjectStore for InProcessDeployVerificationObjectStore {
        async fn store_verification_file(
            &self,
            command: StoreVerificationFileCommand,
        ) -> DeployServiceResult<DriveVerificationFileRef> {
            let operator = require_operator(&command.scope)?;
            let tenant = command.scope.tenant_id.to_string();
            let space_id = self.ensure_space(&tenant, &operator).await?;
            let folder_id = self
                .ensure_folder(&tenant, &space_id, &command.apex_hostname, &operator)
                .await?;
            let node_id = node_id(
                command.scope.tenant_id,
                &command.apex_hostname,
                &command.file_name,
            );
            let checksum = sha256_hash(command.content.as_bytes());
            let content_length = command.content.len() as i64;
            let service = DriveUploaderService::new(SqlUploaderStore::new(self.pool.clone()));
            let prepared = service
                .prepare_upload(PrepareUploaderUploadCommand {
                    id: sdkwork_database_id::uuid_v4(),
                    task_id: format!(
                        "domain-verification-{node_id}-{}",
                        &checksum[..checksum.len().min(16)]
                    ),
                    tenant_id: tenant.clone(),
                    organization_id: None,
                    actor: UploaderActor::System {
                        operator_id: operator.clone(),
                    },
                    app_id: "sdkwork-deploy".to_owned(),
                    app_resource_type: "domain_verification_file".to_owned(),
                    app_resource_id: command.apex_hostname.clone(),
                    scene: Some("domain_verification".to_owned()),
                    source: Some("deploy.console".to_owned()),
                    upload_profile_code: "text".to_owned(),
                    file_fingerprint: checksum.clone(),
                    original_file_name: command.file_name.clone(),
                    content_type: command.content_type.clone(),
                    content_length,
                    chunk_size_bytes: content_length.max(1),
                    target: UploaderTarget::Space {
                        space_id: space_id.clone(),
                        parent_node_id: Some(folder_id),
                        share_token: None,
                    },
                    retention: UploaderRetention::LongTerm,
                    operator_id: operator.clone(),
                    now_epoch_ms: to_unix_millis(sdkwork_utils_rust::now()),
                })
                .await
                .map_err(map_drive_error)?;
            let upload_session_id = prepared.upload_session_id.clone().ok_or_else(|| {
                DeployServiceError::Internal("drive prepared upload has no session".to_owned())
            })?;
            let provider_id = prepared.storage_provider_id.clone().ok_or_else(|| {
                DeployServiceError::Internal("drive prepared upload names no provider".to_owned())
            })?;
            let bucket = prepared.object_bucket.clone().ok_or_else(|| {
                DeployServiceError::Internal("drive prepared upload names no bucket".to_owned())
            })?;
            let object_key = prepared.object_key.clone().ok_or_else(|| {
                DeployServiceError::Internal("drive prepared upload names no object key".to_owned())
            })?;
            let store = DriveObjectStoreRuntime::new(self.pool.clone())
                .resolve(&provider_id, ProviderAccessIntent::Write)
                .await
                .map_err(|error| {
                    DeployServiceError::Internal(format!(
                        "resolve drive object store failed: {error}"
                    ))
                })?;
            store
                .put_object(PutObjectRequest {
                    locator: DriveObjectLocator { bucket, object_key },
                    content_type: Some(command.content_type.clone()),
                    metadata: Default::default(),
                    body: command.content.as_bytes().to_vec(),
                    checksum_sha256_hex: Some(checksum.clone()),
                })
                .await
                .map_err(|error| {
                    DeployServiceError::Internal(format!(
                        "write verification object failed: {error}"
                    ))
                })?;
            service
                .mark_part_uploaded(MarkUploaderPartUploadedCommand {
                    id: format!("part-{}-1", prepared.id),
                    tenant_id: tenant.clone(),
                    upload_item_id: prepared.id.clone(),
                    upload_session_id: upload_session_id.clone(),
                    part_no: 1,
                    offset_bytes: 0,
                    size_bytes: content_length,
                    etag: checksum.clone(),
                    checksum_sha256_hex: Some(checksum.clone()),
                    uploaded_at_epoch_ms: to_unix_millis(sdkwork_utils_rust::now()),
                })
                .await
                .map_err(map_drive_error)?;
            let completed = service
                .complete_stored_upload(CompleteStoredUploaderUploadCommand {
                    tenant_id: tenant,
                    upload_item_id: prepared.id,
                    upload_session_id,
                    content_type: command.content_type,
                    content_length,
                    checksum_sha256_hex: checksum.clone(),
                    uploaded_parts_count: 1,
                    operator_id: operator,
                })
                .await
                .map_err(map_drive_error)?;
            Ok(DriveVerificationFileRef {
                node_id: completed.node_id,
                space_id: completed.space_id,
                content_sha256: checksum,
                content_length,
            })
        }

        async fn read_verification_file(
            &self,
            _scope: &DeployDriveScope,
            _node_id: &str,
        ) -> DeployServiceResult<Option<String>> {
            // Drive's storage contract has no plain object read (downloads go
            // through a presigned URL), and the edge already keeps the bytes in
            // its own row-backed source. Nothing in the control plane needs this
            // direction yet, so it fails loudly instead of pretending.
            Err(DeployServiceError::Internal(
                "reading verification content back from drive is not supported in-process"
                    .to_owned(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeployDriveActor, DeployDriveScope};

    fn command(content: &str) -> StoreVerificationFileCommand {
        StoreVerificationFileCommand {
            scope: DeployDriveScope {
                tenant_id: 7,
                operator: Some(DeployDriveActor::user("11")),
            },
            apex_hostname: "example.com".to_owned(),
            file_name: "MP_verify_abc.txt".to_owned(),
            content_type: "text/plain; charset=utf-8".to_owned(),
            content: content.to_owned(),
        }
    }

    #[tokio::test]
    async fn a_stored_file_can_be_read_back_byte_for_byte() {
        let store = MemoryDeployVerificationObjectStore;
        let reference = store
            .store_verification_file(command("verification-token"))
            .await
            .expect("store");
        assert_eq!(reference.content_length, "verification-token".len() as i64);
        assert_eq!(reference.content_sha256, sha256_hash(b"verification-token"));
        let scope = DeployDriveScope {
            tenant_id: 7,
            operator: Some(DeployDriveActor::user("11")),
        };
        assert_eq!(
            store
                .read_verification_file(&scope, &reference.node_id)
                .await
                .expect("read")
                .as_deref(),
            Some("verification-token")
        );
    }

    #[tokio::test]
    async fn a_reupload_replaces_the_content_under_a_new_reference() {
        let store = MemoryDeployVerificationObjectStore;
        let first = store
            .store_verification_file(command("token-one"))
            .await
            .expect("first");
        let second = store
            .store_verification_file(command("token-two"))
            .await
            .expect("second");
        assert_ne!(first.node_id, second.node_id);
        assert_ne!(first.content_sha256, second.content_sha256);
        let scope = DeployDriveScope {
            tenant_id: 7,
            operator: Some(DeployDriveActor::user("11")),
        };
        assert_eq!(
            store
                .read_verification_file(&scope, &second.node_id)
                .await
                .expect("read")
                .as_deref(),
            Some("token-two")
        );
    }

    #[tokio::test]
    async fn an_unknown_node_reads_as_absent() {
        let store = MemoryDeployVerificationObjectStore;
        let scope = DeployDriveScope {
            tenant_id: 7,
            operator: Some(DeployDriveActor::user("11")),
        };
        assert!(store
            .read_verification_file(&scope, "no-such-node")
            .await
            .expect("read")
            .is_none());
    }
}
