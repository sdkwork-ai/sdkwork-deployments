//! App scaffold marketplace catalog route handlers: category listing,
//! marketplace browse, and author scaffold CRUD + submission + versions.

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use sdkwork_deploy_contract::{
    CreateApplicationScaffoldRequest, CreateApplicationScaffoldVersionRequest,
    DeployAppRequestContext, InstallApplicationScaffoldRequest, ListApplicationScaffoldsQuery,
    ListMarketplaceScaffoldsQuery, UpdateApplicationScaffoldRequest,
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
struct ScaffoldVersionsPageQuery {
    page: Option<i32>,
    page_size: Option<i32>,
}

/// Composable app scaffold marketplace router block.
pub fn build_scaffold_market_router() -> Router<AppState> {
    Router::<AppState>::new()
        .route(paths::SCAFFOLD_CATEGORIES, get(list_scaffold_categories))
        .route(
            paths::MARKETPLACE_SCAFFOLDS,
            get(list_marketplace_scaffolds),
        )
        .route(
            paths::MARKETPLACE_SCAFFOLD,
            get(retrieve_marketplace_scaffold),
        )
        .route(
            paths::MARKETPLACE_SCAFFOLD_INSTALL,
            post(install_marketplace_scaffold),
        )
        .route(
            paths::SCAFFOLDS,
            get(list_application_scaffolds).post(create_application_scaffold),
        )
        .route(
            paths::SCAFFOLD,
            get(retrieve_application_scaffold)
                .patch(update_application_scaffold)
                .delete(delete_application_scaffold),
        )
        .route(paths::SCAFFOLD_SUBMIT, post(submit_application_scaffold))
        .route(
            paths::SCAFFOLD_VERSIONS,
            get(list_application_scaffold_versions).post(create_application_scaffold_version),
        )
        .layer(axum::middleware::from_fn(
            sdkwork_routes_deploy_common::pagination::validate_pagination_query,
        ))
}

async fn list_scaffold_categories(
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
                .list_scaffold_categories(&context, query.include_disabled.unwrap_or(false))
                .await?;
            ok_json(envelope::scaffold_category_page(page))
        }
        .await,
    )
}

async fn list_marketplace_scaffolds(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<ListMarketplaceScaffoldsQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_marketplace_scaffolds(&context, &query)
                .await?;
            ok_json(envelope::application_scaffold_summary_page(page))
        }
        .await,
    )
}

async fn retrieve_marketplace_scaffold(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(scaffold_uuid): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .retrieve_marketplace_scaffold(&context, &scaffold_uuid)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn install_marketplace_scaffold(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(scaffold_uuid): Path<String>,
    headers: HeaderMap,
    Json(request): Json<InstallApplicationScaffoldRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let idempotency_key = required_header(&headers, "idempotency-key")?;
            let item = state
                .api
                .install_application_scaffold(&context, &scaffold_uuid, &idempotency_key, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn list_application_scaffolds(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Query(query): Query<ListApplicationScaffoldsQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_application_scaffolds(&context, &query)
                .await?;
            ok_json(envelope::application_scaffold_page(page))
        }
        .await,
    )
}

async fn create_application_scaffold(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    headers: HeaderMap,
    Json(request): Json<CreateApplicationScaffoldRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let idempotency_key = required_header(&headers, "idempotency-key")?;
            let item = state
                .api
                .create_application_scaffold(&context, &idempotency_key, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn retrieve_application_scaffold(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(scaffold_uuid): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .retrieve_application_scaffold(&context, &scaffold_uuid)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn update_application_scaffold(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(scaffold_uuid): Path<String>,
    Json(request): Json<UpdateApplicationScaffoldRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .update_application_scaffold(&context, &scaffold_uuid, &request)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn delete_application_scaffold(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(scaffold_uuid): Path<String>,
) -> Response {
    finish_no_content(
        &ctx,
        async {
            let context = require_app_context(context)?;
            service_result(
                state
                    .api
                    .delete_application_scaffold(&context, &scaffold_uuid)
                    .await,
            )
        }
        .await,
    )
}

async fn submit_application_scaffold(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(scaffold_uuid): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let item = state
                .api
                .submit_application_scaffold(&context, &scaffold_uuid)
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}

async fn list_application_scaffold_versions(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(scaffold_uuid): Path<String>,
    Query(query): Query<ScaffoldVersionsPageQuery>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let page = state
                .api
                .list_application_scaffold_versions(
                    &context,
                    &scaffold_uuid,
                    query.page.unwrap_or(1),
                    query.page_size.unwrap_or(20),
                )
                .await?;
            ok_json(envelope::application_scaffold_version_page(page))
        }
        .await,
    )
}

async fn create_application_scaffold_version(
    ctx: WebRequestContext,
    State(state): State<AppState>,
    context: Option<Extension<DeployAppRequestContext>>,
    Path(scaffold_uuid): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateApplicationScaffoldVersionRequest>,
) -> Response {
    finish_created_api_json(
        &ctx,
        async {
            let context = require_app_context(context)?;
            let idempotency_key = required_header(&headers, "idempotency-key")?;
            let item = state
                .api
                .create_application_scaffold_version(
                    &context,
                    &scaffold_uuid,
                    &idempotency_key,
                    &request,
                )
                .await?;
            ok_json(envelope::resource(item))
        }
        .await,
    )
}
