//! Helpers shared by the content provider adapters.
//!
//! Everything here is transport-neutral: the in-process ports and the opt-in
//! HTTP adapter use the same validation, the same stable Drive root key and the
//! same redacted error mapping, so the two transports cannot drift on the
//! response shape or on what reaches an HTTP status code.

use std::path::Path;

use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sdkwork_utils_rust::string::trim;

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

pub(crate) fn non_blank(value: Option<&str>) -> Option<String> {
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

pub(crate) fn valid_opaque_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

pub(crate) fn stable_root_key(tenant_id: i64, app_uuid: &str, resource_key: &str) -> String {
    let digest = sdkwork_utils_rust::sha256_hash(
        format!("{tenant_id}:{app_uuid}:{resource_key}").as_bytes(),
    );
    format!("deploy-{}", &digest[..32])
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
}
