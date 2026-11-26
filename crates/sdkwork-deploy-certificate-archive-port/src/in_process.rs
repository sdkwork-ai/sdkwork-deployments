//! Production: Drive's own services on the host PostgreSQL pool.
//!
//! The archive runs through the same three Drive primitives the verification-file
//! store uses (`SqlDriveSpaceService`, `DriveNodeService`, `DriveUploaderService`),
//! so space ownership validation, the writer ACL, content policy, part accounting
//! and node-version activation all stay inside Drive. This module only chooses the
//! space, the two folder levels and the file list.
//!
//! # What Drive has to accept, and why the constants look odd
//!
//! * The space type is `certificate`, a first-class Drive space type
//!   (`sdkwork-drive` `baa3c6c`, `ck_dr_drive_space_space_type` and
//!   `ck_dr_drive_node_space_type`). A dedicated type is what gives the folder
//!   tree its own ACL surface and storage binding instead of mixing certificate
//!   bundles into the generic app-upload space.
//! * The space is **app-owned**, with owner id `app:sdkwork-deploy:system`. Drive
//!   derives that owner from `UploaderActor::System` (`resolve_actor`), and
//!   `ensure_target_space_upload_allowed` refuses a write whose actor owner does
//!   not match the space's owner. The upload is the control plane's own act — it
//!   happens inside a worker, after a conversation with the CA, with no human to
//!   attribute it to — so the app identity is the honest one.
//! * The per-certificate and per-version folders are created through
//!   `DriveNodeService::create_node`; a `Conflict` means a previous attempt (or a
//!   concurrent one) already created it, which is success, not an error. Reusing
//!   the deterministic id keeps that idempotent.
//!
//! # The private key is not uploaded here
//!
//! [`crate::bundle_policy::public_bundle_files`] builds the file list and refuses
//! a secret kind. This module would happily write whatever it is handed, which is
//! exactly why it is never handed the key.

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sdkwork_drive_object_runtime::{DriveObjectStoreRuntime, ProviderAccessIntent};
use sdkwork_drive_storage_contract::{DriveObjectLocator, PutObjectRequest};
use sdkwork_drive_workspace_service::application::node_service::{
    CreateNodeCommand, DriveNodeService,
};
use sdkwork_drive_workspace_service::application::space_service::{
    CreateSpaceCommand, SqlDriveSpaceService,
};
use sdkwork_drive_workspace_service::application::uploader_service::{
    CompleteStoredUploaderUploadCommand, DriveUploaderService, MarkUploaderPartUploadedCommand,
    PrepareUploaderUploadCommand, UploaderActor, UploaderRetention, UploaderTarget,
};
use sdkwork_drive_workspace_service::domain::node::DriveNodeType;
use sdkwork_drive_workspace_service::domain::space::DriveSpaceType;
use sdkwork_drive_workspace_service::infrastructure::sql::node_store::SqlNodeStore;
use sdkwork_drive_workspace_service::infrastructure::sql::uploader_store::SqlUploaderStore;
use sdkwork_drive_workspace_service::ports::uploader_store::DriveUploaderStore;
use sdkwork_drive_workspace_service::DriveServiceError;
use sdkwork_utils_rust::to_unix_millis;

use crate::layout::{
    certificate_folder_name, sha256_hex, version_folder_name, ArchiveBundleKind,
    ArchiveName, CERTIFICATE_SPACE_NAME, MANIFEST_FILE_NAME,
};
use crate::{
    ArchivedCertificateVersion, CertificateArchiveScope, CertificateArchiveStore,
    StoreCertificateVersionArchive,
};

/// The Drive space type a tenant's certificate library uses.
const CERTIFICATE_SPACE_TYPE: &str = "certificate";

/// The Drive app identity this control plane uploads as.
///
/// Must equal the `app_id` handed to `prepare_upload`, because Drive derives the
/// space owner from the actor *and* the app id (`app:<app_id>:system`).
const CERTIFICATE_UPLOAD_APP_ID: &str = "sdkwork-deploy";

/// The subject type Drive accepts for an app-owned space.
const CERTIFICATE_SPACE_OWNER_TYPE: &str = "app";

/// The `operator_id` recorded on every row this adapter creates.
///
/// Drive requires a non-empty operator for audit; there is no human here, so the
/// control plane names itself. It is intentionally not an end-user id: attributing
/// an archive to whoever happened to trigger the renewal would be a false record.
const CERTIFICATE_UPLOAD_OPERATOR_ID: &str = "app:sdkwork-deploy:certificates";

/// Drive scene label for the archive uploads.
const CERTIFICATE_UPLOAD_SCENE: &str = "certificate_archive";

fn certificate_space_owner_id() -> String {
    format!("app:{CERTIFICATE_UPLOAD_APP_ID}:system")
}

fn map_drive_error(operation: &str, error: DriveServiceError) -> DeployServiceError {
    DeployServiceError::Internal(format!("drive {operation} failed: {error:?}"))
}

/// A Drive space, and the two folders below it, addressed by deterministic ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateArchiveFolders {
    pub space_id: String,
    pub certificate_folder_node_id: String,
    pub certificate_folder_name: ArchiveName,
    pub version_folder_node_id: String,
    pub version_folder_name: ArchiveName,
}

/// Resolves where one certificate version belongs.
///
/// Pure: the same version resolves the same folders in any process, which is what
/// makes a retry land on the existing folder instead of creating a second copy.
///
/// # Errors
///
/// [`DeployServiceError::Validation`] when the certificate id or the validity
/// window is not something this control plane stored.
pub fn resolve_archive_folders(
    tenant_id: i64,
    certificate_id: &str,
    cert_name: &str,
    version_no: i64,
    not_after: &str,
) -> DeployServiceResult<(ArchiveName, ArchiveName, String, String)> {
    let certificate_folder = certificate_folder_name(cert_name, certificate_id)
        .map_err(|error| DeployServiceError::validation(error.to_string()))?;
    let version_folder = version_folder_name(version_no, not_after)
        .map_err(|error| DeployServiceError::validation(error.to_string()))?;
    let certificate_node = format!(
        "cert-segment-{tenant_id}-{}",
        certificate_folder.as_str()
    );
    let version_node = format!(
        "cert-version-{tenant_id}-{}-{version_no}",
        certificate_folder.as_str()
    );
    Ok((
        certificate_folder,
        version_folder,
        certificate_node,
        version_node,
    ))
}

/// The in-process certificate archive.
#[derive(Clone)]
pub struct InProcessCertificateArchiveStore {
    pool: sqlx::PgPool,
}

impl InProcessCertificateArchiveStore {
    #[must_use]
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Resolves the tenant's certificate space, creating it on first use.
    ///
    /// The space service — not a raw insert — validates the owner pair against
    /// Drive's own contract. That is the lesson the verification-file store
    /// learned the hard way: writing `dr_drive_space` directly shipped an owner
    /// type the table's check constraint has never admitted, and every write was
    /// denied at runtime.
    async fn ensure_space(&self, tenant: &str) -> DeployServiceResult<String> {
        let store = SqlUploaderStore::new(self.pool.clone());
        let owner_type = CERTIFICATE_SPACE_OWNER_TYPE;
        let owner_id = certificate_space_owner_id();
        if let Some(space_id) = store
            .find_upload_space(tenant, owner_type, &owner_id, CERTIFICATE_SPACE_TYPE)
            .await
            .map_err(|error| map_drive_error("find certificate space", error))?
        {
            return Ok(space_id);
        }
        let id = format!("deploy-certificate-space-{tenant}");
        let created = SqlDriveSpaceService::new(self.pool.clone())
            .create_space(CreateSpaceCommand {
                id: id.clone(),
                tenant_id: tenant.to_owned(),
                owner_subject_type: owner_type.to_owned(),
                owner_subject_id: owner_id.clone(),
                display_name: CERTIFICATE_SPACE_NAME.to_owned(),
                space_type: DriveSpaceType::Certificate,
                presentation_icon: None,
                presentation_color: None,
                description: None,
                operator_id: CERTIFICATE_UPLOAD_OPERATOR_ID.to_owned(),
            })
            .await;
        match created {
            Ok(_) => Ok(id),
            // A concurrent archive created the same tenant/owner/type space
            // first: reuse it instead of failing the write.
            Err(DriveServiceError::Conflict(_)) => store
                .find_upload_space(tenant, owner_type, &owner_id, CERTIFICATE_SPACE_TYPE)
                .await
                .map_err(|error| map_drive_error("find certificate space", error))?
                .ok_or_else(|| {
                    DeployServiceError::Internal(
                        "drive reported the certificate space as existing but it is not readable"
                            .to_owned(),
                    )
                }),
            Err(error) => Err(map_drive_error("create certificate space", error)),
        }
    }

    async fn ensure_folder(
        &self,
        tenant: &str,
        space_id: &str,
        parent_node_id: Option<&str>,
        node_id: &str,
        name: &ArchiveName,
    ) -> DeployServiceResult<String> {
        let nodes = DriveNodeService::new(SqlNodeStore::new(self.pool.clone()));
        match nodes
            .create_node(CreateNodeCommand {
                id: node_id.to_owned(),
                tenant_id: tenant.to_owned(),
                space_id: space_id.to_owned(),
                parent_node_id: parent_node_id.map(str::to_owned),
                node_type: DriveNodeType::Folder,
                node_name: name.as_str().to_owned(),
                operator_id: CERTIFICATE_UPLOAD_OPERATOR_ID.to_owned(),
            })
            .await
        {
            Ok(node) => Ok(node.id),
            // The folder already exists (a previous attempt, or a concurrent one
            // that won the create): reuse the deterministic id.
            Err(DriveServiceError::Conflict(_)) => Ok(node_id.to_owned()),
            Err(error) => Err(map_drive_error("create certificate archive folder", error)),
        }
    }

    /// Writes one file through Drive's uploader: prepare → put → part → complete.
    ///
    /// `task_id` is the idempotency key. Drive returns the existing upload item for
    /// a task id it has already seen (`find_upload_item_by_task`), so a retried
    /// file reuses its node instead of creating a numbered sibling.
    async fn store_file(
        &self,
        tenant: &str,
        request: &StoreCertificateVersionArchive,
        space_id: &str,
        folder_node_id: &str,
        file: &crate::bundle_policy::ArchiveFileContent,
    ) -> DeployServiceResult<String> {
        let checksum = sha256_hex(&file.content);
        let content_length = i64::try_from(file.content.len()).map_err(|_| {
            DeployServiceError::validation("certificate file is too large to archive")
        })?;
        let task_id = format!(
            "cert-archive-{}-{}",
            request.certificate_version_id, file.file_name
        );
        let service = DriveUploaderService::new(SqlUploaderStore::new(self.pool.clone()));
        let prepared = service
            .prepare_upload(PrepareUploaderUploadCommand {
                id: sdkwork_database_id::uuid_v4(),
                task_id,
                tenant_id: tenant.to_owned(),
                organization_id: None,
                actor: UploaderActor::System {
                    operator_id: CERTIFICATE_UPLOAD_OPERATOR_ID.to_owned(),
                },
                app_id: CERTIFICATE_UPLOAD_APP_ID.to_owned(),
                app_resource_type: "certificate_version".to_owned(),
                app_resource_id: request.certificate_version_id.clone(),
                scene: Some(CERTIFICATE_UPLOAD_SCENE.to_owned()),
                source: Some("deploy.certificate".to_owned()),
                upload_profile_code: "document".to_owned(),
                file_fingerprint: checksum.clone(),
                original_file_name: file.file_name.clone(),
                content_type: file.media_type.clone(),
                content_length,
                chunk_size_bytes: content_length.max(1),
                target: UploaderTarget::Space {
                    space_id: space_id.to_owned(),
                    parent_node_id: Some(folder_node_id.to_owned()),
                    share_token: None,
                },
                retention: UploaderRetention::LongTerm,
                operator_id: CERTIFICATE_UPLOAD_OPERATOR_ID.to_owned(),
                now_epoch_ms: to_unix_millis(sdkwork_utils_rust::now()),
            })
            .await
            .map_err(|error| map_drive_error("prepare certificate archive upload", error))?;
        // An upload item Drive already has for this task id is either complete
        // (a previous attempt finished this exact file) or still prepared. In
        // both cases the node exists and the bytes are the ones this call would
        // write, so the node is the answer.
        if prepared.status != "prepared" {
            return Ok(prepared.node_id);
        }
        let upload_session_id = prepared.upload_session_id.clone().ok_or_else(|| {
            DeployServiceError::Internal(
                "drive prepared upload has no session for the certificate archive".to_owned(),
            )
        })?;
        let provider_id = prepared.storage_provider_id.clone().ok_or_else(|| {
            DeployServiceError::Internal(
                "drive prepared upload names no provider for the certificate archive".to_owned(),
            )
        })?;
        let bucket = prepared.object_bucket.clone().ok_or_else(|| {
            DeployServiceError::Internal(
                "drive prepared upload names no bucket for the certificate archive".to_owned(),
            )
        })?;
        let object_key = prepared.object_key.clone().ok_or_else(|| {
            DeployServiceError::Internal(
                "drive prepared upload names no object key for the certificate archive".to_owned(),
            )
        })?;
        let store = DriveObjectStoreRuntime::new(self.pool.clone())
            .resolve(&provider_id, ProviderAccessIntent::Write)
            .await
            .map_err(|error| {
                DeployServiceError::Internal(format!(
                    "resolve drive object store for the certificate archive failed: {error}"
                ))
            })?;
        store
            .put_object(PutObjectRequest {
                locator: DriveObjectLocator { bucket, object_key },
                content_type: Some(file.media_type.clone()),
                metadata: Default::default(),
                body: file.content.clone(),
                checksum_sha256_hex: Some(checksum.clone()),
            })
            .await
            .map_err(|error| {
                DeployServiceError::Internal(format!(
                    "write certificate archive object failed: {error}"
                ))
            })?;
        service
            .mark_part_uploaded(MarkUploaderPartUploadedCommand {
                id: format!("part-{}-1", prepared.id),
                tenant_id: tenant.to_owned(),
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
            .map_err(|error| map_drive_error("mark certificate archive part uploaded", error))?;
        let completed = service
            .complete_stored_upload(CompleteStoredUploaderUploadCommand {
                tenant_id: tenant.to_owned(),
                upload_item_id: prepared.id,
                upload_session_id,
                content_type: file.media_type.clone(),
                content_length,
                checksum_sha256_hex: checksum,
                uploaded_parts_count: 1,
                operator_id: CERTIFICATE_UPLOAD_OPERATOR_ID.to_owned(),
            })
            .await
            .map_err(|error| map_drive_error("complete certificate archive upload", error))?;
        Ok(completed.node_id)
    }
}

#[async_trait]
impl CertificateArchiveStore for InProcessCertificateArchiveStore {
    fn is_configured(&self) -> bool {
        true
    }

    async fn store_version(
        &self,
        request: StoreCertificateVersionArchive,
    ) -> DeployServiceResult<ArchivedCertificateVersion> {
        if request.bundle_kind != ArchiveBundleKind::Public {
            return Err(DeployServiceError::validation(
                crate::bundle_policy::private_key_refusal_detail(),
            ));
        }
        let CertificateArchiveScope { tenant_id } = request.scope;
        let tenant = tenant_id.to_string();
        let (certificate_folder, version_folder, certificate_node, version_node) =
            resolve_archive_folders(
                tenant_id,
                &request.certificate_id,
                &request.cert_name,
                request.version_no,
                &request.facts.not_after,
            )?;
        let space_id = self.ensure_space(&tenant).await?;
        let certificate_folder_node_id = self
            .ensure_folder(
                &tenant,
                &space_id,
                None,
                &certificate_node,
                &certificate_folder,
            )
            .await?;
        let version_folder_node_id = self
            .ensure_folder(
                &tenant,
                &space_id,
                Some(&certificate_folder_node_id),
                &version_node,
                &version_folder,
            )
            .await?;

        let mut manifest_node_id = String::new();
        let mut manifest_sha256 = String::new();
        for file in &request.files {
            let node_id = self
                .store_file(&tenant, &request, &space_id, &version_folder_node_id, file)
                .await?;
            if file.file_name == MANIFEST_FILE_NAME {
                manifest_node_id = node_id;
                manifest_sha256 = sha256_hex(&file.content);
            }
        }
        if manifest_node_id.is_empty() {
            return Err(DeployServiceError::Internal(
                "certificate archive plan has no manifest to write".to_owned(),
            ));
        }
        Ok(ArchivedCertificateVersion {
            space_id,
            certificate_folder_node_id,
            version_folder_node_id,
            manifest_node_id,
            manifest_sha256,
            certificate_folder_name: certificate_folder,
            version_folder_name: version_folder,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_space_owner_pair_matches_what_drive_derives_for_a_system_upload() {
        // Drive's `resolve_actor` builds `app:<app_id>:system` for
        // `UploaderActor::System`, and `ensure_target_space_upload_allowed`
        // compares it to the space's owner. Both halves are pinned here because a
        // mismatch is a permission denial that only appears at runtime.
        assert_eq!(certificate_space_owner_id(), "app:sdkwork-deploy:system");
        assert_eq!(
            certificate_space_owner_id(),
            format!("app:{CERTIFICATE_UPLOAD_APP_ID}:system")
        );
        assert_eq!(CERTIFICATE_SPACE_OWNER_TYPE, "app");
    }

    #[test]
    fn the_space_type_is_the_drive_enum_value() {
        assert_eq!(
            DriveSpaceType::Certificate.as_str(),
            CERTIFICATE_SPACE_TYPE,
            "the constant must name a space type Drive's check constraint admits"
        );
    }

    #[test]
    fn folders_resolve_deterministically_for_a_retry() {
        let first = resolve_archive_folders(
            7,
            "3f7c1a9b-1111-4222-8333-444455556666",
            "example.com",
            2,
            "2027-02-22T09:00:00Z",
        )
        .expect("first");
        let second = resolve_archive_folders(
            7,
            "3f7c1a9b-1111-4222-8333-444455556666",
            "example.com",
            2,
            "2027-02-22T09:00:00Z",
        )
        .expect("second");
        assert_eq!(first, second);
        assert_eq!(first.0.as_str(), "example.com-3f7c1a9b");
        assert_eq!(first.1.as_str(), "v2-2027-02-22T09-00-00Z");
        assert_eq!(first.2, "cert-segment-7-example.com-3f7c1a9b");
        assert_eq!(first.3, "cert-version-7-example.com-3f7c1a9b-2");
    }

    #[test]
    fn a_different_version_of_one_certificate_gets_its_own_folder() {
        let v1 = resolve_archive_folders(
            7,
            "3f7c1a9b-1111-4222-8333-444455556666",
            "example.com",
            1,
            "2027-02-22T09:00:00Z",
        )
        .expect("v1");
        let v2 = resolve_archive_folders(
            7,
            "3f7c1a9b-1111-4222-8333-444455556666",
            "example.com",
            2,
            "2027-05-22T09:00:00Z",
        )
        .expect("v2");
        assert_eq!(v1.2, v2.2, "one certificate folder per certificate");
        assert_ne!(v1.3, v2.3, "one version folder per version");
        assert_ne!(v1.1, v2.1);
    }

    #[test]
    fn an_unusable_identity_is_a_validation_error_not_a_folder_invented() {
        let error = resolve_archive_folders(7, "abc", "example.com", 1, "2027-02-22T09:00:00Z")
            .expect_err("must refuse");
        assert!(matches!(error, DeployServiceError::Validation(_)));
        let error = resolve_archive_folders(
            7,
            "3f7c1a9b-1111-4222-8333-444455556666",
            "example.com",
            1,
            "later",
        )
        .expect_err("must refuse");
        assert!(matches!(error, DeployServiceError::Validation(_)));
    }
}
