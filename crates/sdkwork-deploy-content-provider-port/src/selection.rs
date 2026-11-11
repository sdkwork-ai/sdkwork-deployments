//! Port selection for the content provider adapters.
//!
//! Default: in-process on the host PostgreSQL pool (MODULE_SPEC.md §4.2). The
//! HTTP adapter is reachable only through an explicit
//! `SDKWORK_DEPLOY_CONTENT_PROVIDER_MODE=http` opt-in, and that opt-in fails at
//! startup when its endpoints or ingress token files are missing — never at the
//! first publication request.

use std::sync::Arc;

use sdkwork_utils_rust::parse_bool;
use sqlx::PgPool;

use crate::content_provider_http_adapter::HttpContentProviderAdapter;
use crate::{
    ContentProviderPort, InProcessContentProviderPort, InProcessWebsiteProviderEventDeliveryPort,
    MemoryContentProviderPort, NoopWebsiteProviderEventDeliveryPort,
    WebsiteProviderEventDeliveryPort,
};

pub const USE_MEMORY_CONTENT_PROVIDER_ENV: &str = "SDKWORK_DEPLOY_USE_MEMORY_CONTENT_PROVIDER";
/// `in_process` (default) | `http` (explicit split deployment) | `memory` (dev only).
pub const CONTENT_PROVIDER_MODE_ENV: &str = "SDKWORK_DEPLOY_CONTENT_PROVIDER_MODE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentProviderPortSelection {
    Memory,
    InProcess,
    Http,
}

/// Resolves the selection from the mode variable, the legacy memory switch and
/// the environment class.
///
/// * `mode` wins: `memory`, `in_process`, `http`. Anything else is a startup
///   error — a typo must not silently fall back to another transport.
/// * Legacy `SDKWORK_DEPLOY_USE_MEMORY_CONTENT_PROVIDER`: `true` keeps the
///   development in-memory port, `false` (what production profiles already set)
///   now selects the in-process port instead of the HTTP adapter.
/// * Otherwise development defaults to memory and production-like environments
///   default to in-process.
pub fn select_content_provider_port(
    production_like: bool,
    use_memory: Option<bool>,
    mode: Option<&str>,
) -> Result<ContentProviderPortSelection, String> {
    if let Some(mode) = mode {
        let selection = match mode.trim().to_ascii_lowercase().as_str() {
            "memory" => ContentProviderPortSelection::Memory,
            "in_process" | "in-process" | "embedded" => ContentProviderPortSelection::InProcess,
            "http" => ContentProviderPortSelection::Http,
            other => {
                return Err(format!(
                    "{CONTENT_PROVIDER_MODE_ENV} must be in_process, http, or memory (got {other})"
                ))
            }
        };
        if production_like && selection == ContentProviderPortSelection::Memory {
            return Err(format!(
                "production-like environments cannot select {CONTENT_PROVIDER_MODE_ENV}=memory"
            ));
        }
        return Ok(selection);
    }
    if use_memory == Some(true) {
        if production_like {
            return Err(format!(
                "production-like environments cannot set {USE_MEMORY_CONTENT_PROVIDER_ENV}=true"
            ));
        }
        return Ok(ContentProviderPortSelection::Memory);
    }
    if use_memory == Some(false) || production_like {
        return Ok(ContentProviderPortSelection::InProcess);
    }
    Ok(ContentProviderPortSelection::Memory)
}

fn selection_from_env() -> Result<ContentProviderPortSelection, String> {
    let production_like = sdkwork_deploy_core::deploy_is_production_like_environment();
    let use_memory = std::env::var(USE_MEMORY_CONTENT_PROVIDER_ENV)
        .ok()
        .and_then(|value| parse_bool(&value));
    let mode = std::env::var(CONTENT_PROVIDER_MODE_ENV).ok();
    select_content_provider_port(production_like, use_memory, mode.as_deref())
}

/// The in-process content provider port bound to a caller-provided pool.
///
/// This is the published entry point for an in-process consumer that already
/// owns the shared PostgreSQL pool (for example a sibling module host).
pub fn content_provider_port_from_pool(pool: PgPool) -> Arc<dyn ContentProviderPort> {
    Arc::new(InProcessContentProviderPort::new(pool))
}

pub fn content_provider_port_from_env(
    pool: &PgPool,
) -> Result<Arc<dyn ContentProviderPort>, String> {
    match selection_from_env()? {
        ContentProviderPortSelection::Memory => Ok(Arc::new(MemoryContentProviderPort)),
        ContentProviderPortSelection::InProcess => {
            Ok(content_provider_port_from_pool(pool.clone()))
        }
        ContentProviderPortSelection::Http => Ok(Arc::new(HttpContentProviderAdapter::from_env()?)),
    }
}

pub fn website_provider_event_delivery_port_from_env(
    pool: &PgPool,
) -> Result<Arc<dyn WebsiteProviderEventDeliveryPort>, String> {
    let production_like = sdkwork_deploy_core::deploy_is_production_like_environment();
    match selection_from_env()? {
        ContentProviderPortSelection::Memory => Ok(Arc::new(NoopWebsiteProviderEventDeliveryPort)),
        // Drive's provider-event rows live in the same database in both the
        // integrated and the split-process topologies this module supports, so
        // the registration is always in-process; there is no second endpoint to
        // configure and none is read.
        ContentProviderPortSelection::InProcess | ContentProviderPortSelection::Http => {
            Ok(Arc::new(
                InProcessWebsiteProviderEventDeliveryPort::from_env(pool.clone(), production_like)?,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_defaults_to_in_process() {
        assert_eq!(
            select_content_provider_port(true, None, None).unwrap(),
            ContentProviderPortSelection::InProcess
        );
        assert_eq!(
            select_content_provider_port(true, Some(false), None).unwrap(),
            ContentProviderPortSelection::InProcess
        );
    }

    #[test]
    fn development_defaults_to_memory_but_can_select_in_process() {
        assert_eq!(
            select_content_provider_port(false, None, None).unwrap(),
            ContentProviderPortSelection::Memory
        );
        assert_eq!(
            select_content_provider_port(false, Some(false), None).unwrap(),
            ContentProviderPortSelection::InProcess
        );
    }

    #[test]
    fn http_requires_an_explicit_mode_and_is_never_the_default() {
        assert_eq!(
            select_content_provider_port(true, Some(false), Some("http")).unwrap(),
            ContentProviderPortSelection::Http
        );
        assert_eq!(
            select_content_provider_port(false, None, Some("in_process")).unwrap(),
            ContentProviderPortSelection::InProcess
        );
        assert!(select_content_provider_port(true, None, Some("htp")).is_err());
    }

    #[test]
    fn production_never_selects_the_memory_port() {
        assert!(select_content_provider_port(true, Some(true), None).is_err());
        assert!(select_content_provider_port(true, None, Some("memory")).is_err());
    }
}
