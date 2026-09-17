use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use sdkwork_deploy_contract::DeployAppApi;
use sdkwork_routes_deploy_app_api::{
    build_router_with_shared_app_api, web_bootstrap::wrap_router_with_web_framework,
};
use sdkwork_web_core::{
    access_token_jwt, auth_token_jwt_with_permissions, DefaultWebRequestContextResolver,
};
use std::sync::Arc;
use tower::util::ServiceExt;

fn authorized_request(path: &str, permission: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header(
            header::AUTHORIZATION,
            format!(
                "Bearer {}",
                auth_token_jwt_with_permissions("42", "7", "session-1", "web", permission,)
            ),
        )
        .header(
            "access-token",
            access_token_jwt("42", "7", "session-1", "web"),
        )
        .body(Body::empty())
        .unwrap()
}

/// API_ASSEMBLY_SPEC §4: when a host selects a dependency assembly, tests must
/// prove that a matched dependency error carries the dependency operation's
/// `instance` and `operationId` (an HTTP status alone is not integration
/// evidence). The Deployments blocks keep that contract under the Web
/// Framework layer that also serves them when composed into the Web Server
/// standalone gateway.
#[tokio::test]
async fn matched_dependency_error_preserves_operation_identity() {
    let app = wrap_router_with_web_framework(
        DefaultWebRequestContextResolver::default(),
        build_router_with_shared_app_api(Arc::new(StubAppApi)),
    );

    let response = app
        .oneshot(authorized_request(
            "/app/v3/api/domain_zones",
            "deploy.domainZones.read",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["instance"], "GET /app/v3/api/domain_zones");
    assert_eq!(payload["operationId"], "domainZones.list");
}

#[tokio::test]
async fn app_router_web_framework_rejects_unauthenticated_requests() {
    let app = wrap_router_with_web_framework(
        DefaultWebRequestContextResolver::default(),
        build_router_with_shared_app_api(Arc::new(StubAppApi)),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/apps")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

struct StubAppApi;

#[async_trait]
impl DeployAppApi for StubAppApi {
    // All trait methods have default "not implemented" implementations;
    // this stub only needs to exist for router construction.
}
