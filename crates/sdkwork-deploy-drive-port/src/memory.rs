use async_trait::async_trait;
use sdkwork_database_id::uuid_v4;
use sdkwork_deploy_contract::{
    CompleteDeployUploadSessionRequest, CreateDeployUploadSessionRequest, DeployServiceResult,
    DeployUploadSessionResponse,
};
use sdkwork_utils_rust::{format_datetime, now};

use crate::{
    DeployDrivePort, DeployDriveScope, DriveRequestCredentials, PrepareDeployUploadCommand,
};

const STATUS_PENDING: i32 = 0;
const STATUS_COMPLETED: i32 = 1;
const STATUS_CANCELLED: i32 = 2;

#[derive(Clone, Default)]
pub struct MemoryDeployDrivePort;

impl MemoryDeployDrivePort {
    fn build_response(
        request: &CreateDeployUploadSessionRequest,
        drive_session_id: &str,
        status: i32,
    ) -> DeployUploadSessionResponse {
        DeployUploadSessionResponse {
            id: uuid_v4(),
            app_id: request.app_id.clone(),
            package_type: request.package_type,
            file_name: request.file_name.clone(),
            content_type: request.content_type.clone(),
            content_length: request.content_length,
            checksum: request.checksum.clone(),
            status,
            drive_upload_session_id: drive_session_id.to_string(),
            drive_upload_item_id: Some(format!("upload-item-{drive_session_id}")),
            drive_space_id: Some(format!("space-deploy-{drive_session_id}")),
            drive_node_id: None,
            created_at: format_datetime(now(), None),
            updated_at: format_datetime(now(), None),
        }
    }
}

#[async_trait]
impl DeployDrivePort for MemoryDeployDrivePort {
    async fn prepare_package_upload(
        &self,
        _credentials: &DriveRequestCredentials,
        command: PrepareDeployUploadCommand,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let drive_session_id = format!("mem-session-{}", uuid_v4());
        Ok(Self::build_response(
            &command.request,
            &drive_session_id,
            STATUS_PENDING,
        ))
    }

    async fn retrieve_upload_session(
        &self,
        _credentials: &DriveRequestCredentials,
        _scope: &DeployDriveScope,
        drive_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        Ok(DeployUploadSessionResponse {
            id: uuid_v4(),
            app_id: None,
            package_type: 1,
            file_name: "package.zip".to_string(),
            content_type: "application/zip".to_string(),
            content_length: 0,
            checksum: None,
            status: STATUS_PENDING,
            drive_upload_session_id: drive_session_id.to_string(),
            drive_upload_item_id: Some(format!("upload-item-{drive_session_id}")),
            drive_space_id: Some(format!("space-deploy-{drive_session_id}")),
            drive_node_id: None,
            created_at: format_datetime(now(), None),
            updated_at: format_datetime(now(), None),
        })
    }

    async fn complete_upload_session(
        &self,
        _credentials: &DriveRequestCredentials,
        _scope: &DeployDriveScope,
        drive_session_id: &str,
        request: &CompleteDeployUploadSessionRequest,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        Ok(DeployUploadSessionResponse {
            id: uuid_v4(),
            app_id: None,
            package_type: 1,
            file_name: "package.zip".to_string(),
            content_type: "application/zip".to_string(),
            content_length: request.content_length.unwrap_or(0),
            checksum: Some(request.checksum_sha256_hex.clone()),
            status: STATUS_COMPLETED,
            drive_upload_session_id: drive_session_id.to_string(),
            drive_upload_item_id: Some(format!("upload-item-{drive_session_id}")),
            drive_space_id: Some(format!("space-deploy-{drive_session_id}")),
            drive_node_id: Some(format!("node-{drive_session_id}")),
            created_at: format_datetime(now(), None),
            updated_at: format_datetime(now(), None),
        })
    }

    async fn cancel_upload_session(
        &self,
        _credentials: &DriveRequestCredentials,
        _scope: &DeployDriveScope,
        drive_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        Ok(DeployUploadSessionResponse {
            id: uuid_v4(),
            app_id: None,
            package_type: 1,
            file_name: "package.zip".to_string(),
            content_type: "application/zip".to_string(),
            content_length: 0,
            checksum: None,
            status: STATUS_CANCELLED,
            drive_upload_session_id: drive_session_id.to_string(),
            drive_upload_item_id: Some(format!("upload-item-{drive_session_id}")),
            drive_space_id: Some(format!("space-deploy-{drive_session_id}")),
            drive_node_id: None,
            created_at: format_datetime(now(), None),
            updated_at: format_datetime(now(), None),
        })
    }
}
