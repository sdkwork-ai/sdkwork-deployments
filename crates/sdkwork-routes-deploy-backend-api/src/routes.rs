use axum::{
    extract::{Path, Query, State},
    response::Response,
    routing::{get, post, put},
    Extension, Json, Router,
};
use sdkwork_deploy_contract::{
    AuditLogQuery, ChallengeResultRequest, CreateAcmeAccountRequest, CreateNginxConfigRequest,
    CreateNodeClusterRequest, CreateServerRequest, DeployBackendApi, DeployBackendRequestContext,
    FailCertificateOrderRequest, IngestUsageEventsRequest, ListNginxConfigsQuery,
    RequestCertificateOrderRequest, RetentionRunRequest, StoreCertificateVersionRequest,
    UpdateNginxConfigRequest, UpdateNodeClusterRequest, UpdateServerRequest,
    UsageReconciliationRequest,
};
use sdkwork_routes_deploy_common::{envelope, finish_api_json, finish_created_api_json, ok_json};
use sdkwork_web_core::WebRequestContext;
use serde::Deserialize;
use std::sync::Arc;

use crate::{auth::require_backend_context, paths};

#[derive(Clone)]
struct BackendState {
    api: Arc<dyn DeployBackendApi>,
}

pub fn build_router_with_backend_api<A>(api: A) -> Router
where
    A: DeployBackendApi + 'static,
{
    build_router_with_shared_backend_api(Arc::new(api))
}

pub fn build_router_with_shared_backend_api(api: Arc<dyn DeployBackendApi>) -> Router {
    Router::new()
        .route(
            paths::NGINX_CONFIGS,
            get(list_nginx_configs).post(create_nginx_config),
        )
        .route(
            paths::NGINX_CONFIG,
            get(retrieve_nginx_config).put(update_nginx_config),
        )
        .route(paths::NGINX_CONFIG_VALIDATE, post(validate_nginx_config))
        .route(paths::NGINX_CONFIG_DEPLOY, post(deploy_nginx_config))
        .route(paths::NGINX_RELOAD, post(reload_nginx))
        .route(paths::NGINX_STATUS, get(retrieve_nginx_status))
        .route(paths::SERVERS, get(list_servers).post(create_server))
        .route(paths::SERVER, put(update_server))
        .route(
            paths::NODE_CLUSTERS,
            get(list_node_clusters).post(create_node_cluster),
        )
        .route(paths::NODE_CLUSTER, put(update_node_cluster))
        .route(paths::AUDIT_LOGS, get(list_audit_logs))
        .route(paths::ENTITLEMENTS, get(list_entitlements))
        .route(paths::BUILD_QUEUE, get(list_build_queue))
        .route(paths::RUNNERS, get(list_runners))
        .route(
            paths::TLS_ACCOUNTS,
            get(list_acme_accounts).post(create_acme_account),
        )
        .route(paths::TLS_ORDERS, post(request_certificate_order))
        .route(paths::TLS_ORDER_ADVANCE, post(advance_certificate_order))
        .route(paths::TLS_ORDER_FAIL, post(fail_certificate_order))
        .route(
            paths::TLS_ORDER_CHALLENGE_RESULT,
            post(record_challenge_result),
        )
        .route(paths::TLS_ORDER_VERSIONS, post(store_certificate_version))
        .route(
            paths::TLS_ORDER_CHALLENGES,
            get(list_certificate_challenges),
        )
        .route(paths::CERTIFICATE_ORDERS, get(list_certificate_orders))
        .route(paths::RETENTION_RUN, post(run_retention))
        .route(paths::USAGE_INGEST, post(ingest_usage_events))
        .route(paths::USAGE_RECONCILE, post(reconcile_usage_daily))
        .route(
            paths::SIGNING_IDENTITY_HEALTH,
            get(list_signing_identity_health),
        )
        .route(
            paths::SOURCE_EVENTS,
            get(list_source_events).post(ingest_source_event),
        )
        .layer(axum::middleware::from_fn(
            sdkwork_routes_deploy_common::pagination::validate_pagination_query,
        ))
        .with_state(BackendState { api })
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    #[serde(default = "default_page")]
    page: i32,
    #[serde(default = "default_page_size")]
    page_size: i32,
    // 规范 wire 词汇（PAGINATION_SPEC §3）：lower_snake_case，无 camelCase 别名。
    #[serde(default)]
    cluster_id: Option<String>,
}

fn default_page() -> i32 {
    1
}

fn default_page_size() -> i32 {
    20
}

async fn list_nginx_configs(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Query(query): Query<ListNginxConfigsQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state.api.list_nginx_configs(&context, &query).await?;
            ok_json(envelope::nginx_config_page(page))
        }
        .await,
    )
}

async fn create_nginx_config(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Json(request): Json<CreateNginxConfigRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state.api.create_nginx_config(&context, &request).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn retrieve_nginx_config(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(config_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state
                .api
                .retrieve_nginx_config(&context, &config_id)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn update_nginx_config(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(config_id): Path<String>,
    Json(request): Json<UpdateNginxConfigRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state
                .api
                .update_nginx_config(&context, &config_id, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn validate_nginx_config(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(config_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state
                .api
                .validate_nginx_config(&context, &config_id)
                .await?;
            ok_json(envelope::nginx_validate(item))
        }
        .await,
    )
}

async fn deploy_nginx_config(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(config_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state.api.deploy_nginx_config(&context, &config_id).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn reload_nginx(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state.api.reload_nginx(&context).await?;
            ok_json(envelope::nginx_reload(item))
        }
        .await,
    )
}

async fn retrieve_nginx_status(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state.api.retrieve_nginx_status(&context).await?;
            ok_json(envelope::nginx_status(item))
        }
        .await,
    )
}

async fn list_servers(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_servers(&context, query.page, query.page_size, query.cluster_id)
                .await?;
            ok_json(envelope::server_page(page, query.page, query.page_size))
        }
        .await,
    )
}

async fn create_server(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Json(request): Json<CreateServerRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state.api.create_server(&context, &request).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn update_server(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(server_id): Path<String>,
    Json(request): Json<UpdateServerRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state
                .api
                .update_server(&context, &server_id, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn list_node_clusters(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_node_clusters(&context, query.page, query.page_size)
                .await?;
            ok_json(envelope::node_cluster_page(
                page,
                query.page,
                query.page_size,
            ))
        }
        .await,
    )
}

async fn create_node_cluster(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Json(request): Json<CreateNodeClusterRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state.api.create_node_cluster(&context, &request).await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn update_node_cluster(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(cluster_id): Path<String>,
    Json(request): Json<UpdateNodeClusterRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let item = state
                .api
                .update_node_cluster(&context, &cluster_id, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn list_audit_logs(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Query(query): Query<AuditLogQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_audit_logs(&context, &query, query.cursor.as_deref())
                .await?;
            // The page mode follows the request: supplying a cursor selects
            // keyset mode. It must not follow the presence of `nextCursor` /
            // `hasMore`, because an offset page also carries them — that is how
            // a client obtains its first cursor and switches to keyset
            // continuation — and switching on presence would relabel that offset
            // page as cursor mode and drop its `page` and `totalItems`.
            if query.cursor.is_some() {
                ok_json(envelope::cursor_page(
                    page.items,
                    page.page_size,
                    page.next_cursor,
                    page.has_more,
                ))
            } else {
                ok_json(envelope::audit_log_page(page))
            }
        }
        .await,
    )
}

// -- build fleet administration (TECH §8) -------------------------------------

async fn list_entitlements(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_entitlement_projections(&context, query.page, query.page_size)
                .await?;
            ok_json(envelope::entitlement_projection_page(page))
        }
        .await,
    )
}

async fn list_build_queue(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_build_queue(&context, query.page, query.page_size)
                .await?;
            ok_json(envelope::build_queue_page(page))
        }
        .await,
    )
}

async fn list_runners(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_runner_health(&context, query.page, query.page_size)
                .await?;
            ok_json(envelope::runner_health_page(page))
        }
        .await,
    )
}

// -- TLS control plane (TECH-cloud-site-publishing §4.5) ----------------------

async fn create_acme_account(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Json(request): Json<CreateAcmeAccountRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let result = state.api.create_acme_account(&context, &request).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_acme_accounts(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_acme_accounts(&context, query.page, query.page_size)
                .await?;
            ok_json(envelope::acme_account_page(page))
        }
        .await,
    )
}

async fn request_certificate_order(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Json(request): Json<RequestCertificateOrderRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let result = state
                .api
                .request_certificate_order(&context, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn advance_certificate_order(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(order_id): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let result = state
                .api
                .advance_certificate_order(&context, &order_id)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn fail_certificate_order(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(order_id): Path<String>,
    Json(request): Json<FailCertificateOrderRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            state
                .api
                .fail_certificate_order(&context, &order_id, &request.error_code)
                .await?;
            ok_json(envelope::resource(
                serde_json::json!({ "status": "FAILED" }),
            ))
        }
        .await,
    )
}

async fn record_challenge_result(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(order_id): Path<String>,
    Json(request): Json<ChallengeResultRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            state
                .api
                .record_challenge_result(
                    &context,
                    &order_id,
                    request.challenge_id.as_deref(),
                    request.valid,
                )
                .await?;
            ok_json(envelope::resource(
                serde_json::json!({ "status": "recorded" }),
            ))
        }
        .await,
    )
}

async fn store_certificate_version(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Json(request): Json<StoreCertificateVersionRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let result = state
                .api
                .store_certificate_version(&context, &request)
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_certificate_orders(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(certificate_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_certificate_orders(&context, &certificate_id, query.page, query.page_size)
                .await?;
            ok_json(envelope::certificate_order_page(page))
        }
        .await,
    )
}

async fn list_certificate_challenges(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(order_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_certificate_challenges(&context, &order_id, query.page, query.page_size)
                .await?;
            ok_json(envelope::certificate_challenge_page(page))
        }
        .await,
    )
}

// -- retention, reconciliation, and signing health (TECH §8, PRD §5.8) ---------

async fn run_retention(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Json(request): Json<RetentionRunRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let result = state.api.run_retention(&context, &request).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn ingest_usage_events(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Json(request): Json<IngestUsageEventsRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let result = state.api.ingest_usage_events(&context, &request).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn reconcile_usage_daily(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Json(request): Json<UsageReconciliationRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let result = state.api.rebuild_usage_daily(&context, &request).await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_signing_identity_health(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_signing_identity_health(&context, query.page, query.page_size)
                .await?;
            ok_json(envelope::signing_identity_health_page(page))
        }
        .await,
    )
}

// -- CI source events (webhook ingestion, P0 product gap) ---------------------

async fn ingest_source_event(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let signature = headers
                .get("x-hub-signature-256")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.to_owned());
            let result = state
                .api
                .ingest_source_event(&context, &body, signature.as_deref())
                .await?;
            ok_json(envelope::resource(result))
        }
        .await,
    )
}

async fn list_source_events(
    ctx: WebRequestContext,
    State(state): State<BackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_backend_context(context)?;
            let page = state
                .api
                .list_source_events(&context, query.page, query.page_size)
                .await?;
            ok_json(envelope::source_event_page(page))
        }
        .await,
    )
}
