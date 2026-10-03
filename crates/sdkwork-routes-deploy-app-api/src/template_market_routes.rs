//! App template marketplace route handlers: category listing, marketplace
//! browse/acquire, author template CRUD + submission + versions, and the
//! buyer's purchase inventory.

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use sdkwork_deploy_contract::{
    CreateAppTemplateRequest, CreateAppTemplateVersionRequest, CreateTemplatePurchaseRequest,
    DeployAppRequestContext, ListAppTemplatesQuery, ListMarketplaceTemplatesQuery,
    ListTemplatePurchasesQuery, UpdateAppTemplateRequest,
};
use sdkwork_routes_deploy_common::{
    envelope, finish_api_json, finish_created_api_json, finish_no_content, ok_json, service_result,
};
use sdkwork_web_core::WebRequestContext;
use serde::Deserialize;

use crate::{
    auth::require_app_context,
    paths,
    routes::{required_header, AppState},
};

#[derive(Deserialize)]
struct CategoryListQuery {
    #[serde(default)]
    include_disabled: Option<bool>,
}

#[derive(Deserialize)]
struct TemplateVersionsPageQuery {
    page: Option<i32>,
    page_size: Option<i32>,
}

/// Composable app template marketplace router block.
pub fn build_template_market_router() -> Router<AppState> {
    Router::<AppState>::new()
        .route(paths::TEMPLATE_CATEGORIES, get(list_template_categories))
        .route(
            paths::MARKETPLACE_TEMPLATES,
            get(list_marketplace_templates),
        )
        .route(
            paths::MARKETPLACE_TEMPLATE,
            get(retrieve_marketplace_template),
        )
        .route(
            paths::MARKETPLACE_TEMPLATE_PURCHASE,
            post(create_template_purchase),
        )
        .route(
            paths::APP_TEMPLATES,
            get(list_app_templates).post(create_app_template),
        )
        .route(
            paths::APP_TEMPLATE,
            get(retrieve_app_template)
                .patch(update_app_template)
                .delete(delete_app_template),
        )
        .route(paths::APP_TEMPLATE_SUBMIT, post(submit_app_template))
        .route(
            paths::APP_TEMPLATE_VERSIONS,
            get(list_app_template_versions).post(create_app_template_version),
        )
        .route(paths::TEMPLATE_PURCHASES, get(list_template_purchases))
        .layer(axum::middleware::from_fn(
            sdkwork_routes_deploy_common::pagination::validate_pagination_query,
        ))
}

async fn list_template_categories(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<CategoryListQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_template_categories(&context, query.include_disabled.unwrap_or(false))
                .await?;
            ok_json(envelope::template_category_page(page))
        }
        .await,
    )
}

async fn list_marketplace_templates(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<ListMarketplaceTemplatesQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_marketplace_templates(&context, &query)
                .await?;
            ok_json(envelope::app_template_summary_page(page))
        }
        .await,
    )
}

async fn retrieve_marketplace_template(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(template_uuid): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .retrieve_marketplace_template(&context, &template_uuid)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn create_template_purchase(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(template_uuid): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateTemplatePurchaseRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            // `templatePurchases.create` is declared `x-sdkwork-idempotent`;
            // the key is forwarded so a retry replays the acquisition the
            // first attempt wrote instead of colliding on the entitlement.
            let idempotency_key = required_header(&headers, "idempotency-key")?;
            let item = state
                .api
                .create_template_purchase(&context, &template_uuid, &idempotency_key, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn list_app_templates(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<ListAppTemplatesQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state.api.list_app_templates(&context, &query).await?;
            ok_json(envelope::app_template_page(page))
        }
        .await,
    )
}

async fn create_app_template(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    headers: HeaderMap,
    Json(request): Json<CreateAppTemplateRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let idempotency_key = required_header(&headers, "idempotency-key")?;
            let item = state
                .api
                .create_app_template(&context, &idempotency_key, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn retrieve_app_template(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(template_uuid): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .retrieve_app_template(&context, &template_uuid)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn update_app_template(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(template_uuid): Path<String>,
    Json(request): Json<UpdateAppTemplateRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .update_app_template(&context, &template_uuid, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn delete_app_template(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(template_uuid): Path<String>,
) -> Response {
    finish_no_content(
        &ctx,
        async {
            let context = require_app_context(context)?;
            service_result(
                state
                    .api
                    .delete_app_template(&context, &template_uuid)
                    .await,
            )
        }
        .await,
    )
}

async fn submit_app_template(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(template_uuid): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .submit_app_template(&context, &template_uuid)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn list_app_template_versions(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(template_uuid): Path<String>,
    Query(query): Query<TemplateVersionsPageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_app_template_versions(
                    &context,
                    &template_uuid,
                    query.page.unwrap_or(1),
                    query.page_size.unwrap_or(20),
                )
                .await?;
            ok_json(envelope::app_template_version_page(page))
        }
        .await,
    )
}

async fn create_app_template_version(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(template_uuid): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateAppTemplateVersionRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let idempotency_key = required_header(&headers, "idempotency-key")?;
            let item = state
                .api
                .create_app_template_version(&context, &template_uuid, &idempotency_key, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn list_template_purchases(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<ListTemplatePurchasesQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state.api.list_template_purchases(&context, &query).await?;
            ok_json(envelope::template_purchase_page(page))
        }
        .await,
    )
}
