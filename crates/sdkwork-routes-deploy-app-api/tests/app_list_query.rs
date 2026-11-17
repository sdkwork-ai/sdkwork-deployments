//! `GET /app/v3/api/apps` filter parsing at the HTTP boundary.
//!
//! The generated SDKs serialize `app_kind` as a repeated query key
//! (`app_kind=A&app_kind=B`, `style: form, explode: true`). `serde_urlencoded`
//! cannot bind that to a struct field, so the handler reads it from the raw
//! query — a path no unit test of the parser exercises end to end, and exactly
//! the sort of gap that produces a route which accepts the parameter in the
//! contract and ignores it on the wire.
//!
//! The stub records the decoded query; the router, the auth layer, the
//! permission gate, and the real extractor stack all run.

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use sdkwork_deploy_contract::{
    AppKind, AppOwnerType, AppPage, DeployAppApi, DeployAppRequestContext, ListAppsQuery,
};
use sdkwork_routes_deploy_app_api::{
    build_router_with_shared_app_api, web_bootstrap::wrap_router_with_web_framework,
};
use sdkwork_web_core::{
    access_token_jwt, auth_token_jwt_with_permissions, DefaultWebRequestContextResolver,
};
use std::sync::{Arc, Mutex};
use tower::util::ServiceExt;

struct RecordingAppApi {
    captured: Arc<Mutex<Option<ListAppsQuery>>>,
}

#[async_trait]
impl DeployAppApi for RecordingAppApi {
    async fn list_apps(
        &self,
        _context: &DeployAppRequestContext,
        query: &ListAppsQuery,
    ) -> sdkwork_deploy_contract::DeployServiceResult<AppPage> {
        *self.captured.lock().expect("capture lock") = Some(query.clone());
        Ok(AppPage {
            items: Vec::new(),
            total: 0,
            page: query.page,
            page_size: query.page_size,
        })
    }
}

fn authorized_request(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header(
            header::AUTHORIZATION,
            format!(
                "Bearer {}",
                auth_token_jwt_with_permissions("42", "7", "session-1", "web", "deploy.apps.read"),
            ),
        )
        .header(
            "access-token",
            access_token_jwt("42", "7", "session-1", "web"),
        )
        .body(Body::empty())
        .unwrap()
}

fn router() -> (axum::Router, Arc<Mutex<Option<ListAppsQuery>>>) {
    let captured = Arc::new(Mutex::new(None));
    let app = wrap_router_with_web_framework(
        DefaultWebRequestContextResolver::default(),
        build_router_with_shared_app_api(Arc::new(RecordingAppApi {
            captured: captured.clone(),
        })),
    );
    (app, captured)
}

#[tokio::test]
async fn repeated_app_kind_values_and_the_ownership_facets_reach_the_service() {
    let (app, captured) = router();
    let response = app
        .oneshot(authorized_request(
            "/app/v3/api/apps?ownership_scope=ORGANIZATION&owner_organization_id=9001\
             &app_kind=H5_WEB&app_kind=PC_WEB&keyword=shop&page=2&page_size=5",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let query = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("the handler must reach the service");
    assert_eq!(
        query.resolved_ownership_scope().unwrap(),
        Some(AppOwnerType::Organization)
    );
    assert_eq!(query.organization_filter().unwrap(), Some(9001));
    assert_eq!(query.app_kind, vec![AppKind::H5Web, AppKind::PcWeb]);
    assert_eq!(query.keyword.as_deref(), Some("shop"));
    assert_eq!(query.page, 2);
    assert_eq!(query.page_size, 5);
}

#[tokio::test]
async fn an_unknown_app_kind_is_a_validation_error_rather_than_an_empty_page() {
    let (app, captured) = router();
    let response = app
        .oneshot(authorized_request("/app/v3/api/apps?app_kind=NOT_A_KIND"))
        .await
        .unwrap();
    // The contract's validation failure is a 4xx with a ProblemDetail. The
    // shared route mapper reports `DeployServiceError::Validation` as 400 on the
    // app surface (see `ApiProblem::framework_error`), so that is what a client
    // sees; what matters here is that the rejected filter never reaches the
    // service.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        captured.lock().expect("capture lock").is_none(),
        "a rejected filter must not reach the service"
    );
}
