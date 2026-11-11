//! Drive-backed upload delegation for SDKWork Deploy package artifacts.
//!
//! MODULE_SPEC.md §4.2 governs the transport: the default implementation is
//! in-process on the host PostgreSQL pool ([`InProcessDeployDrivePort`], which
//! calls Drive's own upload-session service); the HTTP adapter exists only for
//! an explicitly selected split deployment.

mod drive_http_adapter;
mod in_process;
mod memory;
mod selection;

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    CompleteDeployUploadSessionRequest, CreateDeployUploadSessionRequest, DeployServiceResult,
    DeployUploadSessionResponse,
};

pub use drive_http_adapter::{SdkDriveAppFacade, DRIVE_APP_API_URL_ENV};
pub use in_process::{
    DriveUploadSessionGateway, DriveUploadTarget, InProcessDeployDrivePort,
    InProcessDriveSessionGateway,
};
pub use memory::MemoryDeployDrivePort;
pub use selection::{
    deploy_drive_port_from_env, deploy_drive_port_from_pool, select_deploy_drive_port,
    DeployDrivePortSelection, DeployDrivePortSelectionInput, DRIVE_MODE_ENV, USE_MEMORY_DRIVE_ENV,
};

/// Per-request caller credentials, forwarded to the split-deployment HTTP
/// adapter; the in-process port authenticates through the host pool and needs
/// no forwarded token.
#[derive(Clone, Debug, Default)]
pub struct DriveRequestCredentials {
    pub auth_token: Option<String>,
    pub access_token: Option<String>,
}

/// The acting subject the sibling module's own ACL must resolve.
///
/// The Drive App API route derived this from the authenticated request; the
/// in-process port has to be handed the same identity, so it travels with the
/// command instead of being read from configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeployDriveActor {
    pub subject_type: String,
    pub subject_id: String,
}

impl DeployDriveActor {
    pub fn user(user_id: impl Into<String>) -> Self {
        Self {
            subject_type: "user".to_owned(),
            subject_id: user_id.into(),
        }
    }
}

/// Tenant and acting subject for one upload-session operation.
///
/// Neither value is ever defaulted: the host pool is shared by every tenant of
/// the process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeployDriveScope {
    pub tenant_id: i64,
    pub operator: Option<DeployDriveActor>,
}

#[derive(Clone, Debug)]
pub struct PrepareDeployUploadCommand {
    pub scope: DeployDriveScope,
    pub request: CreateDeployUploadSessionRequest,
}

#[async_trait]
pub trait DeployDrivePort: Send + Sync {
    async fn prepare_package_upload(
        &self,
        credentials: &DriveRequestCredentials,
        command: PrepareDeployUploadCommand,
    ) -> DeployServiceResult<DeployUploadSessionResponse>;

    async fn retrieve_upload_session(
        &self,
        credentials: &DriveRequestCredentials,
        scope: &DeployDriveScope,
        drive_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse>;

    async fn complete_upload_session(
        &self,
        credentials: &DriveRequestCredentials,
        scope: &DeployDriveScope,
        drive_session_id: &str,
        request: &CompleteDeployUploadSessionRequest,
    ) -> DeployServiceResult<DeployUploadSessionResponse>;

    async fn cancel_upload_session(
        &self,
        credentials: &DriveRequestCredentials,
        scope: &DeployDriveScope,
        drive_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse>;
}

#[derive(Clone)]
pub enum DeployDrivePortAdapter {
    Memory(MemoryDeployDrivePort),
    InProcess(InProcessDeployDrivePort),
    Http(SdkDriveAppFacade),
}

#[async_trait]
impl DeployDrivePort for DeployDrivePortAdapter {
    async fn prepare_package_upload(
        &self,
        credentials: &DriveRequestCredentials,
        command: PrepareDeployUploadCommand,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        match self {
            Self::Memory(port) => port.prepare_package_upload(credentials, command).await,
            Self::InProcess(port) => port.prepare_package_upload(credentials, command).await,
            Self::Http(port) => port.prepare_package_upload(credentials, command).await,
        }
    }

    async fn retrieve_upload_session(
        &self,
        credentials: &DriveRequestCredentials,
        scope: &DeployDriveScope,
        drive_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        match self {
            Self::Memory(port) => {
                port.retrieve_upload_session(credentials, scope, drive_session_id)
                    .await
            }
            Self::InProcess(port) => {
                port.retrieve_upload_session(credentials, scope, drive_session_id)
                    .await
            }
            Self::Http(port) => {
                port.retrieve_upload_session(credentials, scope, drive_session_id)
                    .await
            }
        }
    }

    async fn complete_upload_session(
        &self,
        credentials: &DriveRequestCredentials,
        scope: &DeployDriveScope,
        drive_session_id: &str,
        request: &CompleteDeployUploadSessionRequest,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        match self {
            Self::Memory(port) => {
                port.complete_upload_session(credentials, scope, drive_session_id, request)
                    .await
            }
            Self::InProcess(port) => {
                port.complete_upload_session(credentials, scope, drive_session_id, request)
                    .await
            }
            Self::Http(port) => {
                port.complete_upload_session(credentials, scope, drive_session_id, request)
                    .await
            }
        }
    }

    async fn cancel_upload_session(
        &self,
        credentials: &DriveRequestCredentials,
        scope: &DeployDriveScope,
        drive_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        match self {
            Self::Memory(port) => {
                port.cancel_upload_session(credentials, scope, drive_session_id)
                    .await
            }
            Self::InProcess(port) => {
                port.cancel_upload_session(credentials, scope, drive_session_id)
                    .await
            }
            Self::Http(port) => {
                port.cancel_upload_session(credentials, scope, drive_session_id)
                    .await
            }
        }
    }
}
