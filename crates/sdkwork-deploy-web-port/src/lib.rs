//! Generated Web Internal SDK adapter for runtime assignment publication.
//!
//! identity:service - a background runtime-assignment publisher with a declared
//! machine identity (ingress token + IAM application access token); there is no
//! end-user request to borrow a caller pair from.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sdkwork_deploy_runtime_compiler::CompiledRuntimeSet;
use sdkwork_utils_rust::service_identity::{
    missing_service_credentials, ServiceCredentials, ServiceTokenKind,
};
use sdkwork_utils_rust::string::trim;
use sdkwork_webserver_internal_sdk::{
    PublishRuntimeAssignmentRequest, RuntimeAssignment, RuntimeObservation, SdkworkCustomClient,
    SdkworkError, WebsiteRuntimeSetSnapshot,
};

pub const WEB_INTERNAL_API_URL_ENV: &str = "SDKWORK_DEPLOY_WEB_INTERNAL_API_URL";
pub const WEB_INTERNAL_API_INGRESS_TOKEN_FILE_ENV: &str =
    "SDKWORK_DEPLOY_WEB_INTERNAL_API_INGRESS_TOKEN_FILE";
/// Optional IAM application access token file.
///
/// The internal-api credential profile requires X-SDKWork-Ingress-Token
/// **and** an Access-Token that resolves to an IAM application/tenant scope
/// (APP_SDK_INTEGRATION_SPEC.md §5.2.1). Without this file the adapter refuses
/// to dispatch and reports exactly what is missing instead of sending a header
/// the runtime rejects.
pub const WEB_INTERNAL_API_ACCESS_TOKEN_FILE_ENV: &str =
    "SDKWORK_DEPLOY_WEB_INTERNAL_API_ACCESS_TOKEN_FILE";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAssignmentReceipt {
    pub assignment_uuid: String,
    pub node_uuid: String,
    pub environment: String,
    pub generation: String,
    pub snapshot_uuid: String,
    pub snapshot_sha256: String,
    pub assigned_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeObservationReceipt {
    pub observation_uuid: String,
    pub assignment_uuid: String,
    pub tenant_id: String,
    pub node_uuid: String,
    pub environment: String,
    pub generation: String,
    pub snapshot_uuid: String,
    pub snapshot_sha256: String,
    pub state: String,
    pub node_version: Option<String>,
    pub reason_code: Option<String>,
    pub detail: Option<String>,
    pub observed_at: String,
}

#[async_trait]
pub trait DeployWebRuntimePort: Send + Sync {
    async fn publish_runtime_assignment(
        &self,
        runtime_set: &CompiledRuntimeSet,
    ) -> DeployServiceResult<RuntimeAssignmentReceipt>;

    async fn retrieve_latest_runtime_observation(
        &self,
        snapshot_uuid: &str,
    ) -> DeployServiceResult<RuntimeObservationReceipt>;
}

#[derive(Clone, Debug, Default)]
pub struct UnconfiguredWebRuntimePort;

#[async_trait]
impl DeployWebRuntimePort for UnconfiguredWebRuntimePort {
    async fn publish_runtime_assignment(
        &self,
        _runtime_set: &CompiledRuntimeSet,
    ) -> DeployServiceResult<RuntimeAssignmentReceipt> {
        Err(DeployServiceError::Internal(
            "Web runtime publication is not configured".to_owned(),
        ))
    }

    async fn retrieve_latest_runtime_observation(
        &self,
        _snapshot_uuid: &str,
    ) -> DeployServiceResult<RuntimeObservationReceipt> {
        Err(DeployServiceError::Internal(
            "Web runtime observation retrieval is not configured".to_owned(),
        ))
    }
}

#[derive(Clone, Debug)]
pub struct SdkWebRuntimeFacade {
    base_url: String,
    ingress_token_file: PathBuf,
    access_token_file: Option<PathBuf>,
}

impl SdkWebRuntimeFacade {
    pub fn new(base_url: String, ingress_token_file: PathBuf) -> Result<Self, String> {
        let base_url = trim(&base_url);
        if base_url.is_empty() {
            return Err("Web Internal API URL must not be blank".to_owned());
        }
        if ingress_token_file.as_os_str().is_empty() {
            return Err("Web Internal API ingress token file must not be blank".to_owned());
        }
        Ok(Self {
            base_url: base_url.to_owned(),
            ingress_token_file,
            access_token_file: None,
        })
    }

    /// Declares the IAM application access token that scopes this machine
    /// caller. Without it the adapter fails closed instead of dispatching.
    #[must_use]
    pub fn with_access_token_file(mut self, access_token_file: PathBuf) -> Self {
        self.access_token_file = Some(access_token_file);
        self
    }

    pub fn from_env() -> Result<Self, String> {
        let base_url = std::env::var(WEB_INTERNAL_API_URL_ENV)
            .map_err(|_| format!("{WEB_INTERNAL_API_URL_ENV} is required"))?;
        let token_file = std::env::var(WEB_INTERNAL_API_INGRESS_TOKEN_FILE_ENV)
            .map_err(|_| format!("{WEB_INTERNAL_API_INGRESS_TOKEN_FILE_ENV} is required"))?;
        let mut facade = Self::new(base_url, PathBuf::from(token_file))?;
        if let Ok(access_token_file) = std::env::var(WEB_INTERNAL_API_ACCESS_TOKEN_FILE_ENV) {
            if !access_token_file.trim().is_empty() {
                facade = facade.with_access_token_file(PathBuf::from(access_token_file));
            }
        }
        Ok(facade)
    }

    fn service_identity_headers(&self) -> DeployServiceResult<Vec<(&'static str, String)>> {
        let service_token = read_secret_file(&self.ingress_token_file)?;
        let access_token = match &self.access_token_file {
            Some(path) => read_secret_file(path)?,
            None => String::new(),
        };
        let credentials = ServiceCredentials::new(service_token, access_token);
        if !credentials.is_complete() {
            return Err(DeployServiceError::Internal(missing_service_credentials(
                "sdkwork-webserver internal-api (runtime assignment publication)",
                WEB_INTERNAL_API_INGRESS_TOKEN_FILE_ENV,
                WEB_INTERNAL_API_ACCESS_TOKEN_FILE_ENV,
            )));
        }
        Ok(credentials.wire_headers(ServiceTokenKind::Ingress))
    }

    fn client(&self) -> DeployServiceResult<SdkworkCustomClient> {
        // Render the declared machine identity first so a half-configured
        // deployment never dispatches.
        let headers = self.service_identity_headers()?;
        // DISCOVERY_SPEC §12.1: registry-first, configured URL as degradation.
        let base_url = sdkwork_utils_rust::service_base_url::registry_base_url_or(
            "sdkwork-webserver",
            &self.base_url,
        );
        let client =
            SdkworkCustomClient::new_with_base_url(&base_url).map_err(map_web_sdk_error)?;
        for (name, value) in headers {
            client.set_header(name, value);
        }
        Ok(client)
    }
}

#[async_trait]
impl DeployWebRuntimePort for SdkWebRuntimeFacade {
    async fn publish_runtime_assignment(
        &self,
        runtime_set: &CompiledRuntimeSet,
    ) -> DeployServiceResult<RuntimeAssignmentReceipt> {
        let snapshot: WebsiteRuntimeSetSnapshot =
            serde_json::from_value(runtime_set.snapshot.clone()).map_err(|error| {
                DeployServiceError::Internal(format!(
                    "map compiled runtime-set to generated Web SDK model: {error}"
                ))
            })?;
        let node_uuid = snapshot.node_uuid.clone();
        let environment = snapshot
            .environment
            .as_str()
            .ok_or_else(|| {
                DeployServiceError::Internal(
                    "compiled runtime-set environment is not a string".to_owned(),
                )
            })?
            .to_owned();
        let idempotency_key = snapshot.snapshot_uuid.clone();
        let response = self
            .client()?
            .runtime()
            .assignments_update(
                &node_uuid,
                &environment,
                &PublishRuntimeAssignmentRequest {
                    runtime_set: snapshot,
                },
                &idempotency_key,
            )
            .await
            .map_err(map_web_sdk_error)?;
        Ok(response.into())
    }

    async fn retrieve_latest_runtime_observation(
        &self,
        snapshot_uuid: &str,
    ) -> DeployServiceResult<RuntimeObservationReceipt> {
        let response = self
            .client()?
            .runtime()
            .assignments_observations_latest_retrieve(snapshot_uuid)
            .await
            .map_err(map_web_observation_sdk_error)?;
        Ok(response.into())
    }
}

impl From<RuntimeAssignment> for RuntimeAssignmentReceipt {
    fn from(value: RuntimeAssignment) -> Self {
        Self {
            assignment_uuid: value.assignment_uuid,
            node_uuid: value.node_uuid,
            environment: value.environment,
            generation: value.generation,
            snapshot_uuid: value.snapshot_uuid,
            snapshot_sha256: value.snapshot_sha256,
            assigned_at: value.assigned_at,
        }
    }
}

impl From<RuntimeObservation> for RuntimeObservationReceipt {
    fn from(value: RuntimeObservation) -> Self {
        Self {
            observation_uuid: value.observation_uuid,
            assignment_uuid: value.assignment_uuid,
            tenant_id: value.tenant_id,
            node_uuid: value.node_uuid,
            environment: value.environment,
            generation: value.generation,
            snapshot_uuid: value.snapshot_uuid,
            snapshot_sha256: value.snapshot_sha256,
            state: value.state,
            node_version: value.node_version,
            reason_code: value.reason_code,
            detail: value.detail,
            observed_at: value.observed_at,
        }
    }
}

fn read_secret_file(path: &Path) -> DeployServiceResult<String> {
    let value = std::fs::read_to_string(path).map_err(|error| {
        DeployServiceError::Internal(format!(
            "read Web Internal API ingress token file failed: {error}"
        ))
    })?;
    let value = trim(&value);
    if !(16..=4_096).contains(&value.len()) || value.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(DeployServiceError::Internal(
            "Web Internal API ingress token file contains an invalid credential".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

fn map_web_sdk_error(error: SdkworkError) -> DeployServiceError {
    match error {
        SdkworkError::HttpStatus { status: 404, .. } => {
            DeployServiceError::not_found("Web runtime assignment target was not found")
        }
        SdkworkError::HttpStatus { status: 409, .. } => {
            DeployServiceError::conflict("Web runtime assignment generation conflicts")
        }
        SdkworkError::HttpStatus { status: 400, .. } => {
            DeployServiceError::validation("Web rejected the compiled runtime assignment")
        }
        SdkworkError::HttpStatus {
            status: 401 | 403, ..
        } => DeployServiceError::forbidden("web runtime forbidden"),
        _ => DeployServiceError::Internal("Web runtime assignment publication failed".to_owned()),
    }
}

fn map_web_observation_sdk_error(error: SdkworkError) -> DeployServiceError {
    match error {
        SdkworkError::HttpStatus { status: 404, .. } => {
            DeployServiceError::not_found("Web runtime observation was not found")
        }
        SdkworkError::HttpStatus { status: 400, .. } => {
            DeployServiceError::validation("Web rejected the runtime observation lookup")
        }
        SdkworkError::HttpStatus {
            status: 401 | 403, ..
        } => DeployServiceError::forbidden("web runtime forbidden"),
        _ => DeployServiceError::Internal("Web runtime observation retrieval failed".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn secret_file_validation_accepts_bounded_rotatable_credential() {
        let path = std::env::temp_dir().join(format!(
            "sdkwork-deploy-web-token-{}",
            sdkwork_utils_rust::sha256_hash(b"web-port-test")
        ));
        fs::write(&path, "0123456789abcdef\n").unwrap();
        assert_eq!(read_secret_file(&path).unwrap(), "0123456789abcdef");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_complete_pair_renders_the_ingress_and_access_token_headers() {
        let path = std::env::temp_dir().join(format!(
            "sdkwork-deploy-web-ingress-ok-{}",
            sdkwork_utils_rust::sha256_hash(b"web-port-pair-test")
        ));
        let access_path = std::env::temp_dir().join(format!(
            "sdkwork-deploy-web-access-ok-{}",
            sdkwork_utils_rust::sha256_hash(b"web-port-pair-access-test")
        ));
        fs::write(&path, "0123456789abcdef\n").unwrap();
        fs::write(&access_path, "fedcba9876543210\n").unwrap();
        let facade = SdkWebRuntimeFacade::new("http://127.0.0.1:1".to_owned(), path.clone())
            .unwrap()
            .with_access_token_file(access_path.clone());
        assert_eq!(
            facade.service_identity_headers().unwrap(),
            vec![
                ("X-SDKWork-Ingress-Token", "0123456789abcdef".to_owned()),
                ("Access-Token", "fedcba9876543210".to_owned()),
            ],
            "the internal-api machine profile needs both headers"
        );
        fs::remove_file(path).unwrap();
        fs::remove_file(access_path).unwrap();
    }

    #[test]
    fn a_missing_access_token_fails_closed_with_the_required_setting() {
        let path = std::env::temp_dir().join(format!(
            "sdkwork-deploy-web-ingress-{}",
            sdkwork_utils_rust::sha256_hash(b"web-port-access-test")
        ));
        fs::write(&path, "0123456789abcdef\n").unwrap();
        let facade =
            SdkWebRuntimeFacade::new("http://127.0.0.1:1".to_owned(), path.clone()).unwrap();
        let error = facade
            .client()
            .err()
            .expect("an incomplete service identity must not dispatch");
        let message = error.to_string();
        assert!(
            message.contains(WEB_INTERNAL_API_ACCESS_TOKEN_FILE_ENV),
            "the diagnostic names the missing setting: {message}"
        );
        assert!(
            message.contains(WEB_INTERNAL_API_INGRESS_TOKEN_FILE_ENV),
            "the diagnostic names the service-token setting too: {message}"
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn sdk_errors_are_redacted_at_the_provider_boundary() {
        let error = SdkworkError::HttpStatus {
            status: 500,
            body: "upstream token=do-not-leak; nested text mentions 404".to_owned(),
        };
        let mapped = map_web_sdk_error(error);
        assert_eq!(
            mapped.to_string(),
            "internal error: Web runtime assignment publication failed"
        );
    }

    #[test]
    fn sdk_errors_are_classified_by_structured_http_status() {
        let not_found = map_web_sdk_error(SdkworkError::HttpStatus {
            status: 404,
            body: "opaque upstream response".to_owned(),
        });
        assert_eq!(
            not_found.to_string(),
            "not found: Web runtime assignment target was not found"
        );

        let forbidden = map_web_observation_sdk_error(SdkworkError::HttpStatus {
            status: 403,
            body: "opaque upstream response".to_owned(),
        });
        assert!(matches!(forbidden, DeployServiceError::Forbidden(_)));
    }

    #[test]
    fn generated_observation_is_normalized_without_losing_identity() {
        let receipt = RuntimeObservationReceipt::from(RuntimeObservation {
            observation_uuid: "observation-1".to_owned(),
            assignment_uuid: "assignment-1".to_owned(),
            tenant_id: "42".to_owned(),
            node_uuid: "node-1".to_owned(),
            environment: "production".to_owned(),
            generation: "7".to_owned(),
            snapshot_uuid: "snapshot-7".to_owned(),
            snapshot_sha256: "a".repeat(64),
            state: "ACTIVE".to_owned(),
            node_version: Some("1.0.0".to_owned()),
            reason_code: None,
            detail: None,
            observed_at: "2026-07-22T00:00:00Z".to_owned(),
        });
        assert_eq!(receipt.tenant_id, "42");
        assert_eq!(receipt.environment, "production");
        assert_eq!(receipt.state, "ACTIVE");
    }
}
