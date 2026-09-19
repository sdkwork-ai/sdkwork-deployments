use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
    routing::{get, post, put},
    Extension, Json, Router,
};
use sdkwork_deploy_contract::{
    CompleteDeployUploadSessionRequest, CreateArtifactRequest, CreateCertificateRequest,
    CreateCloudAccountRequest, CreateDeployUploadSessionRequest, CreateDomainHostnameRequest,
    CreateDomainZoneRequest, CreateEnvVariableRequest, CreateHealthCheckRequest, DeployAppApi,
    DeployAppRequestContext, EnsureDomainHostnameClaimsRequest, ListCloudAccountsQuery,
    ListDomainZonesQuery, UpdateAppCompositionRequest, UpdateDomainHostnameRequest,
    UpdateDomainZoneRequest,
};
use sdkwork_routes_deploy_common::{
    envelope, finish_api_json, finish_created_api_json, finish_no_content, ok_json, service_result,
};
use sdkwork_web_core::WebRequestContext;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{auth::require_app_context, paths};

pub(crate) fn required_header(
    headers: &HeaderMap,
    name: &str,
) -> Result<String, sdkwork_deploy_contract::DeployServiceError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_owned())
        .ok_or_else(|| {
            sdkwork_deploy_contract::DeployServiceError::validation(format!(
                "{name} header is required"
            ))
        })
}

#[derive(Clone)]
pub struct AppState {
    pub api: Arc<dyn DeployAppApi>,
}

pub fn build_router_with_app_api<A>(api: A) -> Router
where
    A: DeployAppApi + 'static,
{
    build_router_with_shared_app_api(Arc::new(api))
}

/// Composable domain management block: root-zone and hostname routes. The
/// block is mounted by the Deployments standalone gateway as part of the full
/// app API and by consuming hosts (for example the Web Server standalone
/// gateway) as an independent same-origin dependency contribution.
pub fn build_domain_management_router() -> Router<AppState> {
    Router::<AppState>::new()
        .route(
            paths::DOMAIN_ZONES,
            get(list_domain_zones).post(create_domain_zone),
        )
        .route(
            paths::DOMAIN_ZONE,
            get(retrieve_domain_zone)
                .patch(update_domain_zone)
                .delete(delete_domain_zone),
        )
        .route(
            paths::DOMAIN_ZONE_HOSTNAMES,
            get(list_domain_hostnames).post(create_domain_hostname),
        )
        .route(
            paths::DOMAIN_ZONE_HOSTNAME,
            get(retrieve_domain_hostname)
                .patch(update_domain_hostname)
                .delete(delete_domain_hostname),
        )
        .route(
            paths::DOMAIN_ZONE_HOSTNAME_VERIFY,
            post(verify_domain_hostname),
        )
        .route(
            paths::DOMAIN_ZONE_HOSTNAME_CLAIMS,
            post(ensure_domain_hostname_claims),
        )
        .layer(axum::middleware::from_fn(
            sdkwork_routes_deploy_common::pagination::validate_pagination_query,
        ))
}

/// Composable certificate management block: certificate and renewal routes.
pub fn build_certificate_management_router() -> Router<AppState> {
    Router::<AppState>::new()
        .route(
            paths::CERTIFICATES,
            get(list_certificates).post(create_certificate),
        )
        .route(
            paths::CERTIFICATE,
            get(retrieve_certificate).delete(delete_certificate),
        )
        .route(paths::CERTIFICATE_RENEW, post(renew_certificate))
        .route(paths::CERTIFICATE_RENEWALS, get(list_certificate_renewals))
        .layer(axum::middleware::from_fn(
            sdkwork_routes_deploy_common::pagination::validate_pagination_query,
        ))
}

/// Composable cloud account block: the collection a zone or certificate binds to.
///
/// Mounted alongside the domain and certificate blocks rather than under either,
/// because both use it: a zone pins an account for every hostname it owns, and a
/// certificate may override that pin for itself. Keeping it a sibling means the
/// consuming hosts that mount only one of the two still get the picker's backing
/// endpoint instead of a 404 from a route that lives in the other block.
pub fn build_cloud_account_router() -> Router<AppState> {
    Router::<AppState>::new()
        .route(
            paths::CLOUD_ACCOUNTS,
            get(list_cloud_accounts).post(create_cloud_account),
        )
        .layer(axum::middleware::from_fn(
            sdkwork_routes_deploy_common::pagination::validate_pagination_query,
        ))
}

pub fn build_router_with_shared_app_api(api: Arc<dyn DeployAppApi>) -> Router {
    Router::<AppState>::new()
        .merge(build_domain_management_router())
        .merge(build_certificate_management_router())
        .merge(build_cloud_account_router())
        .merge(crate::app_delivery_routes::build_app_delivery_router())
        .route(paths::UPLOAD_SESSIONS, post(create_upload_session))
        .route(paths::UPLOAD_SESSION, get(retrieve_upload_session))
        .route(
            paths::UPLOAD_SESSION_COMPLETE,
            post(complete_upload_session),
        )
        .route(paths::UPLOAD_SESSION_CANCEL, post(cancel_upload_session))
        .route(paths::ARTIFACTS, get(list_artifacts).post(create_artifact))
        .route(
            paths::ARTIFACT,
            get(retrieve_artifact).delete(retain_artifact),
        )
        .route(paths::APP_COMPOSITION, put(update_app_composition))
        .route(paths::APP_ACTIVATE, post(activate_app))
        .route(paths::APP_PAUSE, post(pause_app))
        .route(
            paths::APP_ENV_VARIABLES,
            get(list_env_variables).post(create_env_variable),
        )
        .route(
            paths::APP_HEALTH_CHECKS,
            get(list_health_checks).post(create_health_check),
        )
        .layer(axum::middleware::from_fn(
            sdkwork_routes_deploy_common::pagination::validate_pagination_query,
        ))
        .with_state(AppState { api })
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    #[serde(default = "default_page")]
    page: i32,
    #[serde(default = "default_page_size")]
    page_size: i32,
}

#[derive(Debug, Deserialize)]
struct EnvVariableListQuery {
    environment: Option<String>,
}

fn default_page() -> i32 {
    1
}

fn default_page_size() -> i32 {
    20
}

/// Lists the cloud accounts the caller may bind a zone or certificate to.
///
/// Doubles as the console's "已存在则复用" pre-check: queried with the family the
/// form is configuring, a non-empty page means the credential fields can be skipped
/// in favour of pinning an account that already exists.
async fn list_cloud_accounts(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<ListCloudAccountsQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state.api.list_cloud_accounts(&context, &query).await?;
            ok_json(envelope::cloud_account_page(
                page,
                query.page,
                query.page_size,
            ))
        }
        .await,
    )
}

/// Registers a DNS cloud account, or reuses the caller's existing one.
///
/// Answers `201` either way, because the endpoint is idempotent and the caller's
/// next step is identical. The body's `reused` flag is what the console reports
/// ("已存在，直接复用" versus "已创建"), so a `201` here never claims a row was
/// inserted when one was merely found.
async fn create_cloud_account(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Json(request): Json<CreateCloudAccountRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let registration = state.api.create_cloud_account(&context, &request).await?;
            ok_json(envelope::resource(registration))
        }
        .await,
    )
}

async fn list_domain_zones(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<ListDomainZonesQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state.api.list_domain_zones(&context, &query).await?;
            ok_json(envelope::domain_zone_page(page))
        }
        .await,
    )
}

async fn create_domain_zone(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Json(request): Json<CreateDomainZoneRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state.api.create_domain_zone(&context, &request).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn retrieve_domain_zone(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(zone_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state.api.retrieve_domain_zone(&context, &zone_id).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn update_domain_zone(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(zone_id): Path<String>,
    Json(request): Json<UpdateDomainZoneRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .update_domain_zone(&context, &zone_id, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn delete_domain_zone(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(zone_id): Path<String>,
) -> Response {
    finish_no_content(
        &ctx,
        async {
            let context = require_app_context(context)?;
            service_result(state.api.delete_domain_zone(&context, &zone_id).await)
        }
        .await,
    )
}

async fn list_domain_hostnames(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(zone_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_domain_hostnames(&context, &zone_id, query.page, query.page_size)
                .await?;
            ok_json(envelope::domain_hostname_page(page))
        }
        .await,
    )
}

async fn create_domain_hostname(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(zone_id): Path<String>,
    Json(request): Json<CreateDomainHostnameRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .create_domain_hostname(&context, &zone_id, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

/// Declares whatever hostnames are missing and reports each one's ownership
/// state, so an order covering `N` names costs one round trip instead of `2N`.
///
/// This is the answer-shaped half of the domain API: the caller states the SAN
/// list it wants to be issued for and reads back what is proven, rather than
/// driving `create` + `verify` per name and assembling the result itself.
async fn ensure_domain_hostname_claims(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(zone_id): Path<String>,
    Json(request): Json<EnsureDomainHostnameClaimsRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let claims = state
                .api
                .ensure_domain_hostname_claims(&context, &zone_id, &request)
                .await?;
            // The batch DTO already is the wire shape (`data.items`), so this
            // endpoint needs no envelope mapping — unlike the item/page helpers
            // above, there is no domain concept to translate here.
            ok_json(claims)
        }
        .await,
    )
}

async fn retrieve_domain_hostname(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((zone_id, hostname_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .retrieve_domain_hostname(&context, &zone_id, &hostname_id)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn update_domain_hostname(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((zone_id, hostname_id)): Path<(String, String)>,
    Json(request): Json<UpdateDomainHostnameRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .update_domain_hostname(&context, &zone_id, &hostname_id, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn delete_domain_hostname(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((zone_id, hostname_id)): Path<(String, String)>,
) -> Response {
    finish_no_content(
        &ctx,
        async {
            let context = require_app_context(context)?;
            service_result(
                state
                    .api
                    .delete_domain_hostname(&context, &zone_id, &hostname_id)
                    .await,
            )
        }
        .await,
    )
}

async fn verify_domain_hostname(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path((zone_id, hostname_id)): Path<(String, String)>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .verify_domain_hostname(&context, &zone_id, &hostname_id)
                .await?;
            ok_json(envelope::domain_verify(item))
        }
        .await,
    )
}

/// Parse the `If-Match` header as a strong entity tag carrying a decimal
/// version number. Rejects wildcards (`*`) and weak entity tags (`W/"..."`).
fn parse_if_match(headers: &HeaderMap) -> Result<i64, sdkwork_deploy_contract::DeployServiceError> {
    let raw = required_header(headers, "if-match")?;
    let tagged = raw
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| {
            sdkwork_deploy_contract::DeployServiceError::validation(
                "If-Match must be a strong entity tag (quoted decimal version)",
            )
        })?;
    tagged.parse::<i64>().map_err(|_| {
        sdkwork_deploy_contract::DeployServiceError::validation(
            "If-Match must be a decimal version number",
        )
    })
}

async fn update_app_composition(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(params): Path<HashMap<String, String>>,
    headers: HeaderMap,
    Json(request): Json<UpdateAppCompositionRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let app_id = params.get("appId").cloned().ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::validation("appId is required")
            })?;
            let expected_version = parse_if_match(&headers)?;
            let idempotency_key = required_header(&headers, "idempotency-key")?;
            let result = state
                .api
                .update_app_composition(
                    &context,
                    &app_id,
                    expected_version,
                    &idempotency_key,
                    &request,
                )
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn activate_app(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(params): Path<HashMap<String, String>>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let app_id = params.get("appId").cloned().ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::validation("appId is required")
            })?;
            let item = state.api.activate_app(&context, &app_id).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn pause_app(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(params): Path<HashMap<String, String>>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let app_id = params.get("appId").cloned().ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::validation("appId is required")
            })?;
            let item = state.api.pause_app(&context, &app_id).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn list_artifacts(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_artifacts(&context, query.page, query.page_size)
                .await?;
            ok_json(envelope::artifact_page(page, query.page, query.page_size))
        }
        .await,
    )
}

async fn create_artifact(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Json(request): Json<CreateArtifactRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state.api.create_artifact(&context, &request).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn retrieve_artifact(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(artifact_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state.api.retrieve_artifact(&context, &artifact_id).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn retain_artifact(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(artifact_id): Path<String>,
) -> Response {
    finish_no_content(
        &ctx,
        async {
            let context = require_app_context(context)?;
            service_result(state.api.retain_artifact(&context, &artifact_id).await)
        }
        .await,
    )
}

async fn list_env_variables(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Query(query): Query<EnvVariableListQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_env_variables(&context, &app_id, query.environment.as_deref())
                .await?;
            ok_json(envelope::env_variable_page(page))
        }
        .await,
    )
}

async fn create_env_variable(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Json(request): Json<CreateEnvVariableRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .create_env_variable(&context, &app_id, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn list_certificates(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_certificates(&context, query.page, query.page_size)
                .await?;
            ok_json(envelope::certificate_page(
                page,
                query.page,
                query.page_size,
            ))
        }
        .await,
    )
}

async fn create_certificate(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    headers: HeaderMap,
    Json(request): Json<CreateCertificateRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let idempotency_key = required_header(&headers, "idempotency-key")?;
            let item = state
                .api
                .create_certificate(&context, &idempotency_key, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn retrieve_certificate(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(certificate_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .retrieve_certificate(&context, &certificate_id)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn delete_certificate(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(certificate_id): Path<String>,
) -> Response {
    finish_no_content(
        &ctx,
        async {
            let context = require_app_context(context)?;
            service_result(
                state
                    .api
                    .delete_certificate(&context, &certificate_id)
                    .await,
            )
        }
        .await,
    )
}

async fn renew_certificate(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(certificate_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .renew_certificate(&context, &certificate_id)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

/// Renewal history for one certificate.
///
/// Returns the paged SDKWork envelope rather than a bare payload, so no
/// `ok_json` passthrough is involved here and nothing needs an envelope mapping
/// exemption.
async fn list_certificate_renewals(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(certificate_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_certificate_renewals(&context, &certificate_id, query.page, query.page_size)
                .await?;
            ok_json(envelope::certificate_renewal_page(
                page,
                query.page,
                query.page_size,
            ))
        }
        .await,
    )
}

async fn list_health_checks(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state.api.list_health_checks(&context, &app_id).await?;
            ok_json(envelope::health_check_page(page))
        }
        .await,
    )
}

async fn create_health_check(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(app_id): Path<String>,
    Json(request): Json<CreateHealthCheckRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .create_health_check(&context, &app_id, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn create_upload_session(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Json(request): Json<CreateDeployUploadSessionRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state.api.create_upload_session(&context, &request).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn retrieve_upload_session(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(upload_session_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .retrieve_upload_session(&context, &upload_session_id)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn complete_upload_session(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(upload_session_id): Path<String>,
    Json(request): Json<CompleteDeployUploadSessionRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .complete_upload_session(&context, &upload_session_id, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn cancel_upload_session(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(upload_session_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .cancel_upload_session(&context, &upload_session_id)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_match_accepts_decimal_strong_entity_tag() {
        let mut headers = HeaderMap::new();
        headers.insert("if-match", "\"42\"".parse().unwrap());
        assert_eq!(parse_if_match(&headers).unwrap(), 42);
    }

    #[test]
    fn if_match_rejects_wildcard_and_weak_entity_tags() {
        for value in ["*", "W/\"42\"", "not-a-version"] {
            let mut headers = HeaderMap::new();
            headers.insert("if-match", value.parse().unwrap());
            assert!(parse_if_match(&headers).is_err());
        }
    }

    #[test]
    fn composition_precondition_headers_are_required() {
        let headers = HeaderMap::new();
        assert!(parse_if_match(&headers)
            .expect_err("If-Match must be required")
            .to_string()
            .contains("if-match header is required"));
        assert!(required_header(&headers, "idempotency-key")
            .expect_err("Idempotency-Key must be required")
            .to_string()
            .contains("idempotency-key header is required"));
    }
}
