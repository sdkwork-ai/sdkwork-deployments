use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use sdkwork_deploy_contract::{
    AppResourceLinkPage, DeployAppApi, DeployAppRequestContext, DeployCredentialKind,
    DeployServiceResult, ListAppResourceLinksQuery,
};
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

/// The host boundary must tell the two credential families apart: a dual-token
/// request resolved to a **user** subject is the tenant console (its `app_id` is
/// the console application, not a data-access binding), while a machine
/// credential keeps the application binding. Getting this wrong is the
/// production `40301` this test guards.
#[tokio::test]
async fn console_user_session_is_classified_as_a_user_session() {
    let app = wrap_router_with_web_framework(
        DefaultWebRequestContextResolver::default(),
        build_router_with_shared_app_api(Arc::new(ClassifyingAppApi)),
    );

    let response = app
        .oneshot(authorized_request(
            "/app/v3/api/apps/bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb/resource_links",
            "deploy.apps.read",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload["code"], 0,
        "the classification stub answered: {payload}"
    );
}

struct StubAppApi;

#[async_trait]
impl DeployAppApi for StubAppApi {
    // All trait methods have default "not implemented" implementations;
    // this stub only needs to exist for router construction.
}

/// Asserts the injected domain context before answering, so the HTTP status proves
/// the classification rather than the stub's return value alone.
struct ClassifyingAppApi;

#[async_trait]
impl DeployAppApi for ClassifyingAppApi {
    async fn list_app_resource_links(
        &self,
        context: &DeployAppRequestContext,
        _app_id: &str,
        _query: &ListAppResourceLinksQuery,
    ) -> DeployServiceResult<AppResourceLinkPage> {
        assert_eq!(
            context.credential_kind,
            DeployCredentialKind::UserSession,
            "a dual-token user session is the console, not an application binding"
        );
        assert_eq!(context.app_scope(), Some("web"));
        Ok(AppResourceLinkPage::default())
    }
}
