//! In-process Drive upload-session port on the host PostgreSQL pool.
//!
//! MODULE_SPEC.md §4.2: a packaged deployment runs one backend process against
//! one PostgreSQL, so Deploy reaches Drive through Drive's own service port
//! instead of Drive's App API. What used to be `POST /app/v3/api/drive/uploader/
//! uploads` plus the upload-session retrieve/complete/abort routes is now:
//!
//! * **target** — the same Drive upload target the uploader route resolves for
//!   an app upload: the acting owner's active `app_upload` space, plus one
//!   active file node per upload intent. Both are created through Drive's own
//!   `DriveUploaderStore` and `DriveNodeService`, never by writing Drive tables
//!   from here.
//! * **lifecycle** — [`DriveUploadSessionService`] over the host pool and
//!   [`DriveObjectRuntimeUploadStorage`]. Tenant scoping, the `writer`/`reader`
//!   ACL (resolved by Drive's permission service), part accounting, the
//!   idempotency key, storage-target resolution, multipart create/complete/
//!   abort and node-version activation all stay inside Drive. This module only
//!   translates commands and maps errors.
//!
//! Every method takes the tenant and the acting subject explicitly; neither is
//! ever read from configuration or an environment default, so the shared pool
//! cannot leak one tenant's session to another.

use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_database_id::uuid_v4;
use sdkwork_deploy_contract::{
    CompleteDeployUploadSessionRequest, CreateDeployUploadSessionRequest, DeployServiceError,
    DeployServiceResult, DeployUploadSessionResponse,
};
use sdkwork_drive_object_runtime::DriveObjectRuntimeUploadStorage;
use sdkwork_drive_workspace_service::application::node_service::{
    CreateNodeCommand, DriveNodeService,
};
use sdkwork_drive_workspace_service::application::upload_session_service::{
    CancelDriveUploadSessionCommand, CompleteDriveUploadSessionCommand, DriveUploadSessionService,
    PrepareDriveUploadSessionCommand,
};
use sdkwork_drive_workspace_service::domain::node::DriveNodeType;
use sdkwork_drive_workspace_service::domain::upload::{
    DriveUploadSession, DriveUploadSessionState,
};
use sdkwork_drive_workspace_service::infrastructure::sql::node_store::SqlNodeStore;
use sdkwork_drive_workspace_service::infrastructure::sql::uploader_store::SqlUploaderStore;
use sdkwork_drive_workspace_service::ports::upload_multipart_storage::DriveMultipartPart;
use sdkwork_drive_workspace_service::ports::uploader_store::{
    DriveUploaderStore, NewDriveUploaderSpace,
};
use sdkwork_drive_workspace_service::DriveServiceError;
use sdkwork_utils_rust::{format_datetime, now, to_unix_millis};
use sqlx::PgPool;

use crate::{
    DeployDriveActor, DeployDrivePort, DeployDriveScope, DriveRequestCredentials,
    PrepareDeployUploadCommand,
};

const STATUS_PENDING: i32 = 0;
const STATUS_COMPLETED: i32 = 1;
const STATUS_CANCELLED: i32 = 2;

/// Drive space type the uploader route uses for an app upload without an
/// explicit space (`scene = deploy_artifact` falls back to this profile).
const DEPLOY_UPLOAD_SPACE_TYPE: &str = "app_upload";
const DEPLOY_UPLOAD_SPACE_NAME: &str = "Upload";
/// Session deadline the in-process port requests. The uploader route uses the
/// same 24 h window.
const DEPLOY_UPLOAD_SESSION_TTL_MS: i64 = 86_400_000;
/// Longest node name the port derives from the caller's file name; the Drive
/// node name column is narrower than the caller-supplied `fileName` bound.
const UPLOAD_NODE_NAME_LIMIT: usize = 200;
/// Attempts to find a free node name in the upload space before giving up.
const UPLOAD_NODE_NAME_ATTEMPTS: usize = 5;

/// The Drive space/node an upload session will write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriveUploadTarget {
    pub space_id: String,
    pub node_id: String,
}

/// The Drive-side operations the in-process port needs, as one injectable seam.
///
/// Production is [`InProcessDriveSessionGateway`]; a test can install a
/// recording implementation to assert the exact commands the port sends.
#[async_trait]
pub trait DriveUploadSessionGateway: Send + Sync {
    /// Resolve (or create) the Drive upload target for one session.
    ///
    /// The node id is derived from the caller's idempotency key, so replaying
    /// the same upload intent resolves the same node and Drive's own
    /// `find_by_idempotency` returns the already-open session instead of
    /// creating a second one.
    async fn resolve_upload_target(
        &self,
        tenant_id: &str,
        operator_id: &str,
        file_name: &str,
        idempotency_key: &str,
    ) -> Result<DriveUploadTarget, DriveServiceError>;

    async fn prepare(
        &self,
        command: PrepareDriveUploadSessionCommand,
    ) -> Result<DriveUploadSession, DriveServiceError>;

    async fn retrieve(
        &self,
        tenant_id: &str,
        upload_session_id: &str,
        subject_type: &str,
        subject_id: &str,
    ) -> Result<DriveUploadSession, DriveServiceError>;

    async fn complete(
        &self,
        command: CompleteDriveUploadSessionCommand,
    ) -> Result<DriveUploadSession, DriveServiceError>;

    async fn cancel(
        &self,
        command: CancelDriveUploadSessionCommand,
    ) -> Result<DriveUploadSession, DriveServiceError>;
}

/// Drive's own services over the host pool.
#[derive(Clone, Debug)]
pub struct InProcessDriveSessionGateway {
    pool: PgPool,
}

impl InProcessDriveSessionGateway {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Host pool this gateway is bound to.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    fn sessions(&self) -> DriveUploadSessionService<DriveObjectRuntimeUploadStorage> {
        DriveUploadSessionService::new(
            self.pool.clone(),
            DriveObjectRuntimeUploadStorage::new(self.pool.clone()),
        )
    }
}

#[async_trait]
impl DriveUploadSessionGateway for InProcessDriveSessionGateway {
    async fn resolve_upload_target(
        &self,
        tenant_id: &str,
        operator_id: &str,
        file_name: &str,
        idempotency_key: &str,
    ) -> Result<DriveUploadTarget, DriveServiceError> {
        let owner_subject_type = "user";
        let uploader_store = SqlUploaderStore::new(self.pool.clone());
        let node_id = deploy_upload_node_id(tenant_id, operator_id, idempotency_key);
        if let Some(node) = uploader_store.find_active_node(tenant_id, &node_id).await? {
            return Ok(DriveUploadTarget {
                space_id: node.space_id,
                node_id: node.id,
            });
        }
        let space_id = match uploader_store
            .find_upload_space(
                tenant_id,
                owner_subject_type,
                operator_id,
                DEPLOY_UPLOAD_SPACE_TYPE,
            )
            .await?
        {
            Some(space_id) => space_id,
            None => {
                uploader_store
                    .insert_upload_space(&NewDriveUploaderSpace {
                        id: deploy_upload_space_id(tenant_id, owner_subject_type, operator_id),
                        tenant_id: tenant_id.to_string(),
                        owner_subject_type: owner_subject_type.to_string(),
                        owner_subject_id: operator_id.to_string(),
                        space_type: DEPLOY_UPLOAD_SPACE_TYPE.to_string(),
                        display_name: DEPLOY_UPLOAD_SPACE_NAME.to_string(),
                        operator_id: operator_id.to_string(),
                    })
                    .await?
            }
        };

        let nodes = DriveNodeService::new(SqlNodeStore::new(self.pool.clone()));
        let base_name = upload_node_name(file_name);
        for attempt in 0..UPLOAD_NODE_NAME_ATTEMPTS {
            let node_name = if attempt == 0 {
                base_name.clone()
            } else {
                format!("{base_name} ({attempt})")
            };
            match nodes
                .create_node(CreateNodeCommand {
                    id: node_id.clone(),
                    tenant_id: tenant_id.to_string(),
                    space_id: space_id.clone(),
                    parent_node_id: None,
                    node_type: DriveNodeType::File,
                    node_name,
                    operator_id: operator_id.to_string(),
                })
                .await
            {
                Ok(node) => {
                    return Ok(DriveUploadTarget {
                        space_id: node.space_id,
                        node_id: node.id,
                    })
                }
                // The id is deterministic: a concurrent request for the same
                // intent may have created it first, so adopt that node instead
                // of failing.
                Err(DriveServiceError::Conflict(_)) if attempt + 1 < UPLOAD_NODE_NAME_ATTEMPTS => {
                    if let Some(node) = uploader_store.find_active_node(tenant_id, &node_id).await?
                    {
                        return Ok(DriveUploadTarget {
                            space_id: node.space_id,
                            node_id: node.id,
                        });
                    }
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        Err(DriveServiceError::Conflict(
            "deploy upload node name is not available".to_string(),
        ))
    }

    async fn prepare(
        &self,
        command: PrepareDriveUploadSessionCommand,
    ) -> Result<DriveUploadSession, DriveServiceError> {
        self.sessions().prepare(command).await
    }

    async fn retrieve(
        &self,
        tenant_id: &str,
        upload_session_id: &str,
        subject_type: &str,
        subject_id: &str,
    ) -> Result<DriveUploadSession, DriveServiceError> {
        self.sessions()
            .retrieve(tenant_id, upload_session_id, subject_type, subject_id)
            .await
    }

    async fn complete(
        &self,
        command: CompleteDriveUploadSessionCommand,
    ) -> Result<DriveUploadSession, DriveServiceError> {
        self.sessions().complete(command).await
    }

    async fn cancel(
        &self,
        command: CancelDriveUploadSessionCommand,
    ) -> Result<DriveUploadSession, DriveServiceError> {
        self.sessions().cancel(command).await
    }
}

/// The default Drive upload port: Drive's upload-session lifecycle in-process.
#[derive(Clone)]
pub struct InProcessDeployDrivePort {
    gateway: Arc<dyn DriveUploadSessionGateway>,
}

impl InProcessDeployDrivePort {
    pub fn new(gateway: Arc<dyn DriveUploadSessionGateway>) -> Self {
        Self { gateway }
    }

    /// Bind the port to the host pool through Drive's own services.
    pub fn with_pool(pool: PgPool) -> Self {
        Self::new(Arc::new(InProcessDriveSessionGateway::new(pool)))
    }
}

#[async_trait]
impl DeployDrivePort for InProcessDeployDrivePort {
    async fn prepare_package_upload(
        &self,
        _credentials: &DriveRequestCredentials,
        command: PrepareDeployUploadCommand,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let actor = require_actor(&command.scope)?;
        let tenant_id = command.scope.tenant_id.to_string();
        let request = &command.request;
        let target = self
            .gateway
            .resolve_upload_target(
                &tenant_id,
                &actor.subject_id,
                &request.file_name,
                &request.idempotency_key,
            )
            .await
            .map_err(map_drive_error)?;
        let session = self
            .gateway
            .prepare(PrepareDriveUploadSessionCommand {
                session_id: uuid_v4(),
                tenant_id,
                space_id: target.space_id,
                node_id: target.node_id,
                requested_storage_provider_id: None,
                requested_bucket: None,
                idempotency_key: request.idempotency_key.clone(),
                operator_id: actor.subject_id.clone(),
                subject_type: actor.subject_type.clone(),
                subject_id: actor.subject_id.clone(),
                expires_at_epoch_ms: to_unix_millis(now()) + DEPLOY_UPLOAD_SESSION_TTL_MS,
            })
            .await
            .map_err(map_drive_error)?;
        Ok(response_from_prepare(request, &session))
    }

    async fn retrieve_upload_session(
        &self,
        _credentials: &DriveRequestCredentials,
        scope: &DeployDriveScope,
        drive_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let actor = require_actor(scope)?;
        let session = self
            .gateway
            .retrieve(
                &scope.tenant_id.to_string(),
                drive_session_id,
                &actor.subject_type,
                &actor.subject_id,
            )
            .await
            .map_err(map_drive_error)?;
        Ok(response_from_session(&session))
    }

    async fn complete_upload_session(
        &self,
        _credentials: &DriveRequestCredentials,
        scope: &DeployDriveScope,
        drive_session_id: &str,
        request: &CompleteDeployUploadSessionRequest,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let actor = require_actor(scope)?;
        let parts = request
            .parts
            .iter()
            .map(|part| {
                let part_no = u16::try_from(part.part_no)
                    .ok()
                    .filter(|part_no| (1..=10_000).contains(part_no))
                    .ok_or_else(|| {
                        DeployServiceError::validation("partNo must be between 1 and 10000")
                    })?;
                Ok(DriveMultipartPart {
                    part_no,
                    etag: part.etag.clone(),
                })
            })
            .collect::<DeployServiceResult<Vec<_>>>()?;
        let session = self
            .gateway
            .complete(CompleteDriveUploadSessionCommand {
                tenant_id: scope.tenant_id.to_string(),
                upload_session_id: drive_session_id.to_string(),
                upload_id: None,
                content_type: request
                    .content_type
                    .clone()
                    .unwrap_or_else(|| "application/octet-stream".to_string()),
                content_length: request.content_length.unwrap_or(0),
                checksum_sha256_hex: request.checksum_sha256_hex.clone(),
                parts,
                operator_id: actor.subject_id.clone(),
                subject_type: actor.subject_type.clone(),
                subject_id: actor.subject_id.clone(),
            })
            .await
            .map_err(map_drive_error)?;
        Ok(response_from_session(&session))
    }

    async fn cancel_upload_session(
        &self,
        _credentials: &DriveRequestCredentials,
        scope: &DeployDriveScope,
        drive_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let actor = require_actor(scope)?;
        let session = self
            .gateway
            .cancel(CancelDriveUploadSessionCommand {
                tenant_id: scope.tenant_id.to_string(),
                upload_session_id: drive_session_id.to_string(),
                operator_id: actor.subject_id.clone(),
                subject_type: actor.subject_type.clone(),
                subject_id: actor.subject_id.clone(),
            })
            .await
            .map_err(map_drive_error)?;
        Ok(response_from_session(&session))
    }
}

/// The acting subject the in-process port hands to Drive's ACL.
///
/// Drive's own permission service resolves it; the port never asserts access.
fn require_actor(scope: &DeployDriveScope) -> DeployServiceResult<&DeployDriveActor> {
    scope
        .operator
        .as_ref()
        .filter(|actor| {
            !actor.subject_type.trim().is_empty() && !actor.subject_id.trim().is_empty()
        })
        .ok_or_else(|| {
            DeployServiceError::forbidden("app upload sessions require an authorized actor")
        })
}

fn response_from_prepare(
    request: &CreateDeployUploadSessionRequest,
    session: &DriveUploadSession,
) -> DeployUploadSessionResponse {
    let timestamp = format_datetime(now(), None);
    DeployUploadSessionResponse {
        id: session.id.clone(),
        app_id: request.app_id.clone(),
        package_type: request.package_type,
        file_name: request.file_name.clone(),
        content_type: request.content_type.clone(),
        content_length: request.content_length,
        checksum: request.checksum.clone(),
        status: session_status(&session.state),
        drive_upload_session_id: session.id.clone(),
        // In-process sessions write through the workspace service, which keys
        // part accounting off the session itself; there is no uploader item.
        drive_upload_item_id: None,
        drive_space_id: Some(session.space_id.clone()),
        drive_node_id: Some(session.node_id.clone()),
        created_at: timestamp.clone(),
        updated_at: timestamp,
    }
}

fn response_from_session(session: &DriveUploadSession) -> DeployUploadSessionResponse {
    let timestamp = format_datetime(now(), None);
    DeployUploadSessionResponse {
        id: session.id.clone(),
        app_id: None,
        package_type: 1,
        file_name: String::new(),
        content_type: "application/octet-stream".to_string(),
        content_length: 0,
        checksum: None,
        status: session_status(&session.state),
        drive_upload_session_id: session.id.clone(),
        drive_upload_item_id: None,
        drive_space_id: Some(session.space_id.clone()),
        drive_node_id: Some(session.node_id.clone()),
        created_at: timestamp.clone(),
        updated_at: timestamp,
    }
}

fn session_status(state: &DriveUploadSessionState) -> i32 {
    match state {
        DriveUploadSessionState::Completed => STATUS_COMPLETED,
        DriveUploadSessionState::Aborted => STATUS_CANCELLED,
        _ => STATUS_PENDING,
    }
}

/// Maps Drive's typed error onto the same `DeployServiceError` kind the removed
/// HTTP adapter produced from an SDK status code, so HTTP statuses do not move.
fn map_drive_error(error: DriveServiceError) -> DeployServiceError {
    match error {
        DriveServiceError::NotFound(message) => DeployServiceError::not_found(message),
        DriveServiceError::Conflict(message) => DeployServiceError::conflict(message),
        DriveServiceError::Validation(message) => DeployServiceError::validation(message),
        DriveServiceError::PermissionDenied(_) => {
            DeployServiceError::forbidden("drive access forbidden")
        }
        DriveServiceError::Internal(message) => DeployServiceError::Internal(message),
    }
}

fn upload_node_name(file_name: &str) -> String {
    let trimmed = file_name.trim();
    let fallback = "package";
    let name = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    name.chars().take(UPLOAD_NODE_NAME_LIMIT).collect()
}

/// Deterministic id so two concurrent sessions of one owner converge on one
/// space instead of racing two inserts (`insert_upload_space` treats the
/// uniqueness violation as "already created").
fn deploy_upload_space_id(
    tenant_id: &str,
    owner_subject_type: &str,
    owner_subject_id: &str,
) -> String {
    let digest = sdkwork_utils_rust::sha256_hash(
        format!("{tenant_id}:{owner_subject_type}:{owner_subject_id}").as_bytes(),
    );
    format!(
        "space-app-upload-{owner_subject_type}-{}",
        digest.chars().take(16).collect::<String>()
    )
}

/// Deterministic node id keyed on the upload intent.
///
/// A replay resolves the same node, which lets Drive's own
/// `find_by_idempotency(tenant, space, node, key)` return the session that was
/// already opened instead of creating a second node and a second session.
fn deploy_upload_node_id(tenant_id: &str, owner_subject_id: &str, idempotency_key: &str) -> String {
    let digest = sdkwork_utils_rust::sha256_hash(
        format!("{tenant_id}:{owner_subject_id}:{idempotency_key}").as_bytes(),
    );
    format!(
        "node-deploy-{}",
        digest.chars().take(32).collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use sdkwork_deploy_contract::{
        CompletedUploadPartInput, CreateDeployUploadSessionRequest, DeployServiceErrorKind,
    };

    use super::*;

    #[derive(Default)]
    struct RecordingGateway {
        targets: Mutex<Vec<(String, String, String, String)>>,
        prepares: Mutex<Vec<PrepareDriveUploadSessionCommand>>,
        completes: Mutex<Vec<CompleteDriveUploadSessionCommand>>,
        cancels: Mutex<Vec<CancelDriveUploadSessionCommand>>,
        retrieves: Mutex<Vec<(String, String, String, String)>>,
    }

    impl RecordingGateway {
        fn session(&self, id: &str, state: DriveUploadSessionState) -> DriveUploadSession {
            DriveUploadSession {
                id: id.to_string(),
                tenant_id: "42".to_string(),
                space_id: "space-recording".to_string(),
                node_id: "node-recording".to_string(),
                bucket: "bucket".to_string(),
                object_key: "object-key".to_string(),
                idempotency_key: "idem".to_string(),
                storage_provider_id: "provider".to_string(),
                storage_upload_id: "upload".to_string(),
                state,
                expires_at_epoch_ms: i64::MAX,
                version: 1,
            }
        }
    }

    #[async_trait]
    impl DriveUploadSessionGateway for RecordingGateway {
        async fn resolve_upload_target(
            &self,
            tenant_id: &str,
            operator_id: &str,
            file_name: &str,
            idempotency_key: &str,
        ) -> Result<DriveUploadTarget, DriveServiceError> {
            self.targets.lock().unwrap().push((
                tenant_id.to_string(),
                operator_id.to_string(),
                file_name.to_string(),
                idempotency_key.to_string(),
            ));
            Ok(DriveUploadTarget {
                space_id: "space-recording".to_string(),
                node_id: "node-recording".to_string(),
            })
        }

        async fn prepare(
            &self,
            command: PrepareDriveUploadSessionCommand,
        ) -> Result<DriveUploadSession, DriveServiceError> {
            let session = self.session(&command.session_id, DriveUploadSessionState::Created);
            self.prepares.lock().unwrap().push(command);
            Ok(session)
        }

        async fn retrieve(
            &self,
            tenant_id: &str,
            upload_session_id: &str,
            subject_type: &str,
            subject_id: &str,
        ) -> Result<DriveUploadSession, DriveServiceError> {
            self.retrieves.lock().unwrap().push((
                tenant_id.to_string(),
                upload_session_id.to_string(),
                subject_type.to_string(),
                subject_id.to_string(),
            ));
            Ok(self.session(upload_session_id, DriveUploadSessionState::Uploading))
        }

        async fn complete(
            &self,
            command: CompleteDriveUploadSessionCommand,
        ) -> Result<DriveUploadSession, DriveServiceError> {
            let session = self.session(
                &command.upload_session_id,
                DriveUploadSessionState::Completed,
            );
            self.completes.lock().unwrap().push(command);
            Ok(session)
        }

        async fn cancel(
            &self,
            command: CancelDriveUploadSessionCommand,
        ) -> Result<DriveUploadSession, DriveServiceError> {
            let session =
                self.session(&command.upload_session_id, DriveUploadSessionState::Aborted);
            self.cancels.lock().unwrap().push(command);
            Ok(session)
        }
    }

    fn scope() -> DeployDriveScope {
        DeployDriveScope {
            tenant_id: 42,
            operator: Some(DeployDriveActor::user("77")),
        }
    }

    fn create_request() -> CreateDeployUploadSessionRequest {
        CreateDeployUploadSessionRequest {
            app_id: Some("app-1".to_string()),
            package_type: 1,
            file_name: "package.zip".to_string(),
            content_type: "application/zip".to_string(),
            content_length: 1_048_576,
            checksum: Some(format!("sha256:{}", "a".repeat(64))),
            idempotency_key: "idem-1".to_string(),
        }
    }

    fn port() -> (InProcessDeployDrivePort, Arc<RecordingGateway>) {
        let gateway = Arc::new(RecordingGateway::default());
        (InProcessDeployDrivePort::new(gateway.clone()), gateway)
    }

    #[tokio::test]
    async fn prepare_reaches_drive_with_the_expected_session_command() {
        let (port, gateway) = port();
        let response = port
            .prepare_package_upload(
                &DriveRequestCredentials::default(),
                PrepareDeployUploadCommand {
                    scope: scope(),
                    request: create_request(),
                },
            )
            .await
            .expect("prepare succeeds");

        let targets = gateway.targets.lock().unwrap();
        assert_eq!(
            targets.as_slice(),
            &[(
                "42".to_string(),
                "77".to_string(),
                "package.zip".to_string(),
                "idem-1".to_string(),
            )]
        );

        let prepares = gateway.prepares.lock().unwrap();
        assert_eq!(prepares.len(), 1);
        let command = &prepares[0];
        assert_eq!(command.tenant_id, "42");
        assert_eq!(command.space_id, "space-recording");
        assert_eq!(command.node_id, "node-recording");
        assert_eq!(command.idempotency_key, "idem-1");
        assert_eq!(command.operator_id, "77");
        assert_eq!(command.subject_type, "user");
        assert_eq!(command.subject_id, "77");
        assert!(command.expires_at_epoch_ms > 0);
        assert_eq!(response.drive_upload_session_id, command.session_id);
        assert_eq!(response.drive_space_id.as_deref(), Some("space-recording"));
        assert_eq!(response.drive_node_id.as_deref(), Some("node-recording"));
        assert_eq!(response.status, STATUS_PENDING);
    }

    #[tokio::test]
    async fn complete_and_cancel_reach_drive_with_the_expected_commands() {
        let (port, gateway) = port();
        let request = CompleteDeployUploadSessionRequest {
            checksum_sha256_hex: format!("sha256:{}", "b".repeat(64)),
            content_length: Some(1_048_576),
            content_type: Some("application/zip".to_string()),
            parts: vec![CompletedUploadPartInput {
                part_no: 1,
                etag: "etag-1".to_string(),
            }],
        };
        let completed = port
            .complete_upload_session(
                &DriveRequestCredentials::default(),
                &scope(),
                "session-1",
                &request,
            )
            .await
            .expect("complete succeeds");
        assert_eq!(completed.status, STATUS_COMPLETED);
        {
            let completes = gateway.completes.lock().unwrap();
            assert_eq!(completes.len(), 1);
            let command = &completes[0];
            assert_eq!(command.tenant_id, "42");
            assert_eq!(command.upload_session_id, "session-1");
            assert_eq!(command.content_type, "application/zip");
            assert_eq!(command.content_length, 1_048_576);
            assert_eq!(command.checksum_sha256_hex, request.checksum_sha256_hex);
            assert_eq!(
                command.parts,
                vec![DriveMultipartPart {
                    part_no: 1,
                    etag: "etag-1".to_string(),
                }]
            );
            assert_eq!(command.subject_id, "77");
        }

        let cancelled = port
            .cancel_upload_session(&DriveRequestCredentials::default(), &scope(), "session-1")
            .await
            .expect("cancel succeeds");
        assert_eq!(cancelled.status, STATUS_CANCELLED);
        {
            let cancels = gateway.cancels.lock().unwrap();
            assert_eq!(cancels.len(), 1);
            assert_eq!(cancels[0].tenant_id, "42");
            assert_eq!(cancels[0].upload_session_id, "session-1");
            assert_eq!(cancels[0].subject_type, "user");
            assert_eq!(cancels[0].subject_id, "77");
        }
    }

    #[tokio::test]
    async fn retrieve_is_scoped_to_the_request_tenant_and_subject() {
        let (port, gateway) = port();
        port.retrieve_upload_session(&DriveRequestCredentials::default(), &scope(), "session-9")
            .await
            .expect("retrieve succeeds");
        let retrieves = gateway.retrieves.lock().unwrap();
        assert_eq!(
            retrieves.as_slice(),
            &[(
                "42".to_string(),
                "session-9".to_string(),
                "user".to_string(),
                "77".to_string(),
            )]
        );
    }

    #[tokio::test]
    async fn a_missing_actor_is_rejected_before_drive_is_called() {
        let (port, gateway) = port();
        let error = port
            .prepare_package_upload(
                &DriveRequestCredentials::default(),
                PrepareDeployUploadCommand {
                    scope: DeployDriveScope {
                        tenant_id: 42,
                        operator: None,
                    },
                    request: create_request(),
                },
            )
            .await
            .expect_err("an unauthenticated caller cannot open a Drive session");
        assert_eq!(error.kind(), DeployServiceErrorKind::Forbidden);
        assert!(gateway.prepares.lock().unwrap().is_empty());
        assert!(gateway.targets.lock().unwrap().is_empty());
    }

    #[test]
    fn drive_error_kinds_keep_the_http_status_mapping() {
        assert_eq!(
            map_drive_error(DriveServiceError::NotFound("x".to_string())).kind(),
            DeployServiceErrorKind::NotFound
        );
        assert_eq!(
            map_drive_error(DriveServiceError::Conflict("x".to_string())).kind(),
            DeployServiceErrorKind::Conflict
        );
        assert_eq!(
            map_drive_error(DriveServiceError::Validation("x".to_string())).kind(),
            DeployServiceErrorKind::Validation
        );
        assert_eq!(
            map_drive_error(DriveServiceError::PermissionDenied("x".to_string())).kind(),
            DeployServiceErrorKind::Forbidden
        );
        assert_eq!(
            map_drive_error(DriveServiceError::Internal("x".to_string())).kind(),
            DeployServiceErrorKind::Internal
        );
    }

    #[test]
    fn the_upload_space_id_is_stable_and_owner_scoped() {
        let first = deploy_upload_space_id("42", "user", "77");
        assert_eq!(first, deploy_upload_space_id("42", "user", "77"));
        assert_ne!(first, deploy_upload_space_id("42", "user", "78"));
        assert_ne!(first, deploy_upload_space_id("43", "user", "77"));
    }

    #[test]
    fn the_upload_node_id_is_keyed_on_the_idempotency_intent() {
        let first = deploy_upload_node_id("42", "77", "idem-1");
        assert_eq!(first, deploy_upload_node_id("42", "77", "idem-1"));
        assert_ne!(first, deploy_upload_node_id("42", "77", "idem-2"));
        assert_ne!(first, deploy_upload_node_id("43", "77", "idem-1"));
    }
}
