//! Opt-in split-deployment HTTP adapter for the Drive upload-session port.
//!
//! MODULE_SPEC.md §4.2.3 allows a deliberately split deployment to keep calling a
//! sibling module over HTTP when HTTP is never the default, requires an explicit
//! opt-in, and fails at startup when the opt-in lacks its configuration. That is
//! this module: it is only constructed by the explicit
//! `SDKWORK_DEPLOY_DRIVE_MODE=http` opt-in, and `from_env` refuses to build
//! without the endpoint.
//!
//! The delegation is a *delegated client* call: it presents the end user's own
//! Drive App API credentials, and the bytes travel directly from the client to
//! Drive, never through this process. The default deployment does not need it:
//! [`crate::InProcessDeployDrivePort`] runs the same upload-session lifecycle on
//! the host pool. This adapter survives only for a real split deployment whose
//! Drive runs in another process (MODULE_SPEC.md §4.2.3).

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    CompleteDeployUploadSessionRequest, DeployServiceError, DeployServiceResult,
    DeployUploadSessionResponse,
};
use sdkwork_drive_app_sdk_generated_rust::{
    CompleteUploadSessionRequest, DriveUploadSession, NodeCommandRequest,
    PrepareUploaderUploadRequest, SdkworkAppClient, SdkworkError,
};
use sdkwork_utils_rust::{format_datetime, now, string::trim};

use crate::{
    DeployDrivePort, DeployDriveScope, DriveRequestCredentials, PrepareDeployUploadCommand,
};

const DEPLOY_APP_RESOURCE_TYPE: &str = "deploy_package";
const DEPLOY_UPLOAD_SCENE: &str = "deploy_artifact";
const DEPLOY_UPLOAD_PROFILE: &str = "generic";
const DEPLOY_APP_ID: &str = "sdkwork-deploy";

/// Drive App API endpoint used by the split-deployment adapter. Read only by
/// this module, and only after the explicit opt-in.
pub const DRIVE_APP_API_URL_ENV: &str = "SDKWORK_DRIVE_FACADE_URL";

const STATUS_PENDING: i32 = 0;
const STATUS_COMPLETED: i32 = 1;
const STATUS_CANCELLED: i32 = 2;

#[derive(Clone)]
pub struct SdkDriveAppFacade {
    base_url: String,
}

impl SdkDriveAppFacade {
    pub fn from_env(facade_url: String) -> Result<Self, String> {
        let base_url = trim(&facade_url);
        if base_url.is_empty() {
            return Err(format!("{DRIVE_APP_API_URL_ENV} must not be blank"));
        }
        Ok(Self {
            base_url: base_url.to_string(),
        })
    }

    fn client(
        &self,
        credentials: &DriveRequestCredentials,
    ) -> Result<SdkworkAppClient, DeployServiceError> {
        // DISCOVERY_SPEC §12.1: registry-first, configured URL as degradation.
        let base_url = sdkwork_utils_rust::service_base_url::registry_base_url_or(
            "sdkwork-drive",
            &self.base_url,
        );
        let client = SdkworkAppClient::new_with_base_url(&base_url).map_err(map_drive_error)?;
        if let Some(token) = credentials
            .auth_token
            .as_deref()
            .map(trim)
            .filter(|value| !value.is_empty())
        {
            client.set_auth_token(token);
        }
        if let Some(token) = credentials
            .access_token
            .as_deref()
            .map(trim)
            .filter(|value| !value.is_empty())
        {
            client.set_access_token(token);
        }
        Ok(client)
    }
}

fn map_drive_error(error: SdkworkError) -> DeployServiceError {
    let message = error.to_string();
    if message.contains("404") || message.contains("not found") {
        DeployServiceError::not_found(message)
    } else if message.contains("409") || message.contains("conflict") {
        DeployServiceError::conflict(message)
    } else if message.contains("422") || message.contains("validation") {
        DeployServiceError::validation(message)
    } else if message.contains("403") || message.contains("forbidden") {
        DeployServiceError::forbidden("drive access forbidden")
    } else {
        DeployServiceError::Internal(message)
    }
}

fn map_session_state(state: &str) -> i32 {
    match state.to_ascii_lowercase().as_str() {
        "completed" | "complete" | "succeeded" => STATUS_COMPLETED,
        "aborted" | "cancelled" | "canceled" => STATUS_CANCELLED,
        _ => STATUS_PENDING,
    }
}

fn timestamp_now() -> String {
    format_datetime(now(), None)
}

fn response_from_prepare(
    request: &sdkwork_deploy_contract::CreateDeployUploadSessionRequest,
    upload_item_id: &str,
    session: &sdkwork_drive_app_sdk_generated_rust::UploadSessionMutationResponse,
    upload_item: &sdkwork_drive_app_sdk_generated_rust::UploaderUploadItem,
) -> DeployUploadSessionResponse {
    let now = timestamp_now();
    DeployUploadSessionResponse {
        id: upload_item_id.to_string(),
        app_id: request.app_id.clone(),
        package_type: request.package_type,
        file_name: request.file_name.clone(),
        content_type: request.content_type.clone(),
        content_length: request.content_length,
        checksum: request.checksum.clone(),
        status: map_session_state(&session.state),
        drive_upload_session_id: session.id.clone(),
        drive_upload_item_id: Some(upload_item.id.clone()),
        drive_space_id: Some(session.space_id.clone()),
        drive_node_id: Some(session.node_id.clone()),
        created_at: now.clone(),
        updated_at: now,
    }
}

fn response_from_session(
    session: &DriveUploadSession,
    fallback: &DeployUploadSessionResponse,
) -> DeployUploadSessionResponse {
    let now = timestamp_now();
    DeployUploadSessionResponse {
        status: map_session_state(&session.state),
        drive_upload_session_id: session.id.clone(),
        drive_space_id: Some(session.space_id.clone()),
        drive_node_id: Some(session.node_id.clone()),
        updated_at: now,
        ..fallback.clone()
    }
}

#[async_trait]
impl DeployDrivePort for SdkDriveAppFacade {
    async fn prepare_package_upload(
        &self,
        credentials: &DriveRequestCredentials,
        command: PrepareDeployUploadCommand,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let client = self.client(credentials)?;
        let request = &command.request;
        let upload_item_id = request.idempotency_key.clone();
        let resource_id = request
            .app_id
            .clone()
            .unwrap_or_else(|| format!("tenant-{}", command.scope.tenant_id));
        let body = PrepareUploaderUploadRequest {
            id: upload_item_id.clone(),
            task_id: upload_item_id.clone(),
            app_resource_type: DEPLOY_APP_RESOURCE_TYPE.to_string(),
            app_resource_id: resource_id,
            upload_profile_code: Some(DEPLOY_UPLOAD_PROFILE.to_string()),
            file_fingerprint: request
                .checksum
                .clone()
                .unwrap_or_else(|| upload_item_id.clone()),
            original_file_name: request.file_name.clone(),
            content_type: request.content_type.clone(),
            content_length: request.content_length.to_string(),
            chunk_size_bytes: request.content_length.max(5 * 1024 * 1024).to_string(),
            space_id: None,
            parent_node_id: None,
            retention: None,
            now_epoch_ms: Some(sdkwork_utils_rust::to_unix_millis(now()).to_string()),
            scene: Some(DEPLOY_UPLOAD_SCENE.to_string()),
            source: Some(DEPLOY_APP_ID.to_string()),
            share_token: None,
        };
        let response = client
            .drive()
            .uploader_uploads_create(&body)
            .await
            .map_err(map_drive_error)?;
        Ok(response_from_prepare(
            request,
            &upload_item_id,
            &response.upload_session,
            &response.upload_item,
        ))
    }

    async fn retrieve_upload_session(
        &self,
        credentials: &DriveRequestCredentials,
        _scope: &DeployDriveScope,
        drive_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let client = self.client(credentials)?;
        let session = client
            .drive()
            .upload_sessions_retrieve(drive_session_id)
            .await
            .map_err(map_drive_error)?;
        let fallback = DeployUploadSessionResponse {
            id: drive_session_id.to_string(),
            app_id: None,
            package_type: 1,
            file_name: "package.bin".to_string(),
            content_type: "application/octet-stream".to_string(),
            content_length: 0,
            checksum: None,
            status: STATUS_PENDING,
            drive_upload_session_id: drive_session_id.to_string(),
            drive_upload_item_id: None,
            drive_space_id: None,
            drive_node_id: None,
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        };
        Ok(response_from_session(&session, &fallback))
    }

    async fn complete_upload_session(
        &self,
        credentials: &DriveRequestCredentials,
        _scope: &DeployDriveScope,
        drive_session_id: &str,
        request: &CompleteDeployUploadSessionRequest,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let client = self.client(credentials)?;
        let body = CompleteUploadSessionRequest {
            upload_id: None,
            content_type: request
                .content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            content_length: request.content_length.unwrap_or(0).to_string(),
            checksum_sha256_hex: request.checksum_sha256_hex.clone(),
            parts: request
                .parts
                .iter()
                .map(
                    |part| sdkwork_drive_app_sdk_generated_rust::CompletedUploadPart {
                        part_no: part.part_no,
                        etag: part.etag.clone(),
                    },
                )
                .collect(),
        };
        let session = client
            .drive()
            .upload_sessions_complete(drive_session_id, &body)
            .await
            .map_err(map_drive_error)?;
        let fallback = DeployUploadSessionResponse {
            id: drive_session_id.to_string(),
            app_id: None,
            package_type: 1,
            file_name: "package.bin".to_string(),
            content_type: body.content_type.clone(),
            content_length: body.content_length.parse::<i64>().unwrap_or(0),
            checksum: Some(body.checksum_sha256_hex.clone()),
            status: STATUS_COMPLETED,
            drive_upload_session_id: drive_session_id.to_string(),
            drive_upload_item_id: None,
            drive_space_id: None,
            drive_node_id: None,
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        };
        Ok(response_from_session(&session, &fallback))
    }

    async fn cancel_upload_session(
        &self,
        credentials: &DriveRequestCredentials,
        _scope: &DeployDriveScope,
        drive_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let client = self.client(credentials)?;
        let body = NodeCommandRequest::default();
        let session = client
            .drive()
            .upload_sessions_abort(drive_session_id, &body)
            .await
            .map_err(map_drive_error)?;
        let fallback = DeployUploadSessionResponse {
            id: drive_session_id.to_string(),
            app_id: None,
            package_type: 1,
            file_name: "package.bin".to_string(),
            content_type: "application/octet-stream".to_string(),
            content_length: 0,
            checksum: None,
            status: STATUS_CANCELLED,
            drive_upload_session_id: drive_session_id.to_string(),
            drive_upload_item_id: None,
            drive_space_id: None,
            drive_node_id: None,
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        };
        Ok(response_from_session(&session, &fallback))
    }
}
