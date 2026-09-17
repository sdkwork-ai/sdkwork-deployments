//! Coverage evidence for the composed app API surface (API_ASSEMBLY_SPEC §6.1
//! "a same-origin dependency the host declares as served must not answer 404
//! on an authenticated operator action").
//!
//! The generated manifest (`http_route_manifest.rs`) is the published route
//! inventory: the same list the assembly turns into a route manifest, an
//! OpenAPI document, and a permission catalog. A route that is inventoried but
//! not mounted is invisible to every gate — the inventory still looks complete —
//! and only shows up as an opaque 404 in production. This test closes that gap
//! by probing the executable router with one request per manifest route.
//!
//! Probe semantics (verified against the contract, not assumed):
//! - mounted route → the handler runs. Without a Web Framework layer there is no
//!   `DeployAppRequestContext` extension, so `require_app_context` answers
//!   `401`; a stub service answers `500`; a body-consuming route answers `400`
//!   or `415`. None of these is `404`.
//! - unmounted route → Axum's fallback answers `404`.
//! `DeployServiceError::NotFound` appears zero times in `sdkwork-deploy-contract`
//! `app_ports.rs`, so no default trait body can fake a 404 either.

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use sdkwork_deploy_contract::DeployAppApi;
use sdkwork_routes_deploy_app_api::{app_route_manifest, build_router_with_shared_app_api};
use sdkwork_web_core::HttpMethod;
use std::sync::Arc;
use tower::util::ServiceExt;

fn http_method(method: HttpMethod) -> Method {
    match method {
        HttpMethod::Get => Method::GET,
        HttpMethod::Post => Method::POST,
        HttpMethod::Put => Method::PUT,
        HttpMethod::Patch => Method::PATCH,
        HttpMethod::Delete => Method::DELETE,
    }
}

/// Replaces every `{param}` segment with a literal so the request reaches the
/// route instead of being rejected by the path matcher.
fn concrete_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment.starts_with('{') && segment.ends_with('}') {
                "probe-id"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

async fn probe(method: Method, path: &str) -> StatusCode {
    let router = build_router_with_shared_app_api(Arc::new(StubAppApi));
    let request = Request::builder()
        .method(method)
        .uri(path)
        .body(Body::empty())
        .expect("probe request must build");
    router
        .oneshot(request)
        .await
        .expect("the app surface must answer every request")
        .status()
}

/// Guards the probe itself: an unknown path must answer 404, otherwise the
/// coverage assertion below would pass vacuously.
#[tokio::test]
async fn the_404_probe_is_not_vacuous() {
    assert_eq!(
        probe(Method::GET, "/app/v3/api/not_a_deploy_route").await,
        StatusCode::NOT_FOUND,
        "an unmounted path must answer 404 for the coverage probe to mean anything"
    );
}

#[tokio::test]
async fn every_manifest_route_is_mounted_by_the_app_surface() {
    let mut unmounted = Vec::new();
    for route in app_route_manifest().routes() {
        let path = concrete_path(route.path);
        if probe(http_method(route.method), &path).await == StatusCode::NOT_FOUND {
            unmounted.push(format!("{:?} {}", route.method, route.path));
        }
    }

    assert!(
        unmounted.is_empty(),
        "the app API manifest publishes {} route(s) the executable surface does not mount, so a \
         consuming gateway would answer 404 for a dependency it declares as served:\n{:#?}",
        unmounted.len(),
        unmounted,
    );
}

/// The routes whose absence from the same-origin contribution produced the
/// `/app/v3/api/apps` 404. Named individually so a regression reports the
/// operator-facing capability that broke, not just a count.
#[tokio::test]
async fn the_application_lifecycle_routes_are_mounted() {
    for (method, path) in [
        (Method::GET, "/app/v3/api/apps"),
        (Method::POST, "/app/v3/api/apps"),
        (Method::GET, "/app/v3/api/apps/probe-id"),
        (Method::PUT, "/app/v3/api/apps/probe-id/composition"),
        (Method::POST, "/app/v3/api/apps/probe-id/activate"),
        (Method::POST, "/app/v3/api/apps/probe-id/pause"),
        (Method::GET, "/app/v3/api/apps/probe-id/env_variables"),
        (Method::POST, "/app/v3/api/apps/probe-id/env_variables"),
        (Method::GET, "/app/v3/api/apps/probe-id/health_checks"),
        (Method::POST, "/app/v3/api/apps/probe-id/health_checks"),
    ] {
        let status = probe(method.clone(), path).await;
        assert_ne!(
            status,
            StatusCode::NOT_FOUND,
            "{method} {path} is unmounted: the console's application surface would 404",
        );
    }
}

struct StubAppApi;

#[async_trait]
impl DeployAppApi for StubAppApi {
    // All trait methods have default "not implemented" (`Internal`) bodies;
    // this stub only needs to exist for router construction.
}
