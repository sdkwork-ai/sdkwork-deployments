//! Drive upload-port selection.
//!
//! MODULE_SPEC.md §4.2: a backend module reaches Drive in-process. The default
//! is [`DeployDrivePortSelection::InProcess`] on the host PostgreSQL pool — no
//! sibling endpoint and no extra configuration, in every environment. HTTP is
//! only for a deliberate split deployment and requires an explicit
//! `SDKWORK_DEPLOY_DRIVE_MODE=http` opt-in plus its endpoint, validated at
//! startup; the in-memory port is a development-only convenience.

use std::sync::Arc;

use sdkwork_utils_rust::{parse_bool, string::trim};
use sqlx::PgPool;

use crate::drive_http_adapter::{SdkDriveAppFacade, DRIVE_APP_API_URL_ENV};
use crate::{
    DeployDrivePort, DeployDrivePortAdapter, InProcessDeployDrivePort, MemoryDeployDrivePort,
};

/// `in_process` (default) | `http` (explicit split deployment) | `memory` (dev only).
pub const DRIVE_MODE_ENV: &str = "SDKWORK_DEPLOY_DRIVE_MODE";
/// Legacy development switch: `true` keeps the in-memory port.
pub const USE_MEMORY_DRIVE_ENV: &str = "SDKWORK_DEPLOY_USE_MEMORY_DRIVE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployDrivePortSelectionInput {
    pub mode: Option<String>,
    pub use_memory_drive: Option<bool>,
    pub facade_url: Option<String>,
    pub production_like: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeployDrivePortSelection {
    Memory,
    InProcess,
    Http { facade_url: String },
}

/// Resolves the Drive upload transport.
///
/// * `mode` wins: `memory`, `in_process`, `http`. Anything else is a startup
///   error — a typo must not silently fall back to another transport.
/// * Legacy `SDKWORK_DEPLOY_USE_MEMORY_DRIVE`: `true` keeps the development
///   in-memory port, `false` (what production profiles already set) now selects
///   the in-process port instead of the HTTP adapter.
/// * Otherwise the in-process port is the default for every environment.
///
/// Returns `Err` when the selected mode lacks its configuration, which the host
/// propagates as a startup failure.
pub fn select_deploy_drive_port(
    input: DeployDrivePortSelectionInput,
) -> Result<DeployDrivePortSelection, String> {
    let facade_url = input
        .facade_url
        .as_ref()
        .map(|value| trim(value))
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    if let Some(mode) = input.mode.as_deref() {
        return match mode.trim().to_ascii_lowercase().as_str() {
            "http" => match facade_url {
                Some(facade_url) => Ok(DeployDrivePortSelection::Http { facade_url }),
                None => Err(format!(
                    "{DRIVE_MODE_ENV}=http requires {DRIVE_APP_API_URL_ENV}"
                )),
            },
            "in_process" | "in-process" | "embedded" => Ok(DeployDrivePortSelection::InProcess),
            "memory" if input.production_like => Err(format!(
                "production-like environments cannot select {DRIVE_MODE_ENV}=memory"
            )),
            "memory" => Ok(DeployDrivePortSelection::Memory),
            other => Err(format!(
                "{DRIVE_MODE_ENV} must be in_process, http, or memory (got {other})"
            )),
        };
    }
    match input.use_memory_drive {
        Some(true) if input.production_like => Err(format!(
            "production-like environments cannot set {USE_MEMORY_DRIVE_ENV}=true"
        )),
        Some(true) => Ok(DeployDrivePortSelection::Memory),
        Some(false) | None => Ok(DeployDrivePortSelection::InProcess),
    }
}

fn selection_from_env() -> Result<DeployDrivePortSelection, String> {
    let production_like = sdkwork_deploy_core::deploy_is_production_like_environment();
    let use_memory_drive = std::env::var(USE_MEMORY_DRIVE_ENV)
        .ok()
        .and_then(|value| parse_bool(&value));
    let mode = std::env::var(DRIVE_MODE_ENV).ok();
    let facade_url = std::env::var(DRIVE_APP_API_URL_ENV).ok();
    select_deploy_drive_port(DeployDrivePortSelectionInput {
        mode,
        use_memory_drive,
        facade_url,
        production_like,
    })
}

/// The in-process Drive upload port bound to a caller-provided pool.
///
/// This is the published entry point for an in-process consumer that already
/// owns the shared PostgreSQL pool (for example a sibling module host).
pub fn deploy_drive_port_from_pool(pool: PgPool) -> Arc<dyn DeployDrivePort> {
    Arc::new(InProcessDeployDrivePort::with_pool(pool))
}

/// Builds the Drive upload port for this deployment.
///
/// All configuration is validated here, at startup.
pub fn deploy_drive_port_from_env(pool: &PgPool) -> Result<Arc<dyn DeployDrivePort>, String> {
    let adapter = match selection_from_env()? {
        DeployDrivePortSelection::Memory => DeployDrivePortAdapter::Memory(MemoryDeployDrivePort),
        DeployDrivePortSelection::InProcess => {
            DeployDrivePortAdapter::InProcess(InProcessDeployDrivePort::with_pool(pool.clone()))
        }
        DeployDrivePortSelection::Http { facade_url } => {
            DeployDrivePortAdapter::Http(SdkDriveAppFacade::from_env(facade_url)?)
        }
    };
    Ok(Arc::new(adapter))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        mode: Option<&str>,
        use_memory_drive: Option<bool>,
        facade_url: Option<&str>,
        production_like: bool,
    ) -> DeployDrivePortSelectionInput {
        DeployDrivePortSelectionInput {
            mode: mode.map(str::to_owned),
            use_memory_drive,
            facade_url: facade_url.map(str::to_owned),
            production_like,
        }
    }

    fn lazy_pool() -> PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://deploy:deploy@127.0.0.1:1/deploy")
            .expect("lazy pool")
    }

    #[test]
    fn production_defaults_to_in_process_with_no_drive_configuration() {
        assert_eq!(
            select_deploy_drive_port(input(None, None, None, true)).unwrap(),
            DeployDrivePortSelection::InProcess
        );
        // The legacy production profiles that set `...=false` now select the
        // in-process port instead of the HTTP adapter.
        assert_eq!(
            select_deploy_drive_port(input(None, Some(false), None, true)).unwrap(),
            DeployDrivePortSelection::InProcess
        );
        // A leftover endpoint must not turn the default into an HTTP call.
        assert_eq!(
            select_deploy_drive_port(input(None, None, Some("https://api.sdkwork.com"), true))
                .unwrap(),
            DeployDrivePortSelection::InProcess
        );
    }

    #[test]
    fn development_defaults_to_in_process_but_keeps_the_memory_switch() {
        assert_eq!(
            select_deploy_drive_port(input(None, None, None, false)).unwrap(),
            DeployDrivePortSelection::InProcess
        );
        assert_eq!(
            select_deploy_drive_port(input(None, Some(true), None, false)).unwrap(),
            DeployDrivePortSelection::Memory
        );
        assert_eq!(
            select_deploy_drive_port(input(Some("memory"), None, None, false)).unwrap(),
            DeployDrivePortSelection::Memory
        );
    }

    #[test]
    fn production_never_selects_memory_drive() {
        for (mode, use_memory) in [(None, Some(true)), (Some("memory"), None)] {
            assert!(select_deploy_drive_port(input(mode, use_memory, None, true)).is_err());
        }
    }

    #[test]
    fn http_is_an_explicit_opt_in_that_requires_its_endpoint() {
        assert_eq!(
            select_deploy_drive_port(input(
                Some("http"),
                None,
                Some("https://api.sdkwork.com"),
                true
            ))
            .unwrap(),
            DeployDrivePortSelection::Http {
                facade_url: "https://api.sdkwork.com".to_owned(),
            }
        );
        assert!(select_deploy_drive_port(input(Some("http"), None, None, true)).is_err());
        assert!(select_deploy_drive_port(input(Some("http"), None, None, false)).is_err());
    }

    #[test]
    fn unknown_modes_are_rejected() {
        assert!(select_deploy_drive_port(input(Some("htp"), None, None, false)).is_err());
    }

    /// The composition gate: a production-like host that configures *nothing*
    /// sibling-specific must still build its Drive port.
    #[tokio::test]
    async fn composition_succeeds_without_any_drive_endpoint_configuration() {
        let _guard = sdkwork_deploy_core::env_test_lock();
        let previous_environment = std::env::var("SDKWORK_DEPLOY_ENVIRONMENT").ok();
        let previous_mode = std::env::var(DRIVE_MODE_ENV).ok();
        let previous_memory = std::env::var(USE_MEMORY_DRIVE_ENV).ok();
        let previous_facade = std::env::var(DRIVE_APP_API_URL_ENV).ok();

        std::env::set_var("SDKWORK_DEPLOY_ENVIRONMENT", "production");
        std::env::remove_var(DRIVE_MODE_ENV);
        std::env::remove_var(USE_MEMORY_DRIVE_ENV);
        std::env::remove_var(DRIVE_APP_API_URL_ENV);

        let result = deploy_drive_port_from_env(&lazy_pool());

        restore_env("SDKWORK_DEPLOY_ENVIRONMENT", previous_environment);
        restore_env(DRIVE_MODE_ENV, previous_mode);
        restore_env(USE_MEMORY_DRIVE_ENV, previous_memory);
        restore_env(DRIVE_APP_API_URL_ENV, previous_facade);

        assert!(
            result.is_ok(),
            "the module must boot with no drive endpoint configuration: {:?}",
            result.err()
        );
    }

    fn restore_env(key: &str, value: Option<String>) {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}
