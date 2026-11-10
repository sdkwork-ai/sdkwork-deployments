//! Backend download catalog management routes.
//!
//! Operator surface for the download page: list/read/publish catalogs, patch
//! cards and installers, and configure the per-installer update feed. Every
//! handler requires the backend request context, so the tenant scope comes from
//! the authenticated operator and never from the request body.

use axum::{
    extract::{Path, State},
    response::Response,
    routing::{get, post, put},
    Extension, Json, Router,
};
use sdkwork_deploy_contract::{
    DeployBackendRequestContext, DeployDownloadsApi, UpsertDownloadCardRequest,
    UpsertDownloadCatalogRequest, UpsertDownloadItemRequest, UpsertDownloadUpdateRequest,
};
use sdkwork_routes_deploy_common::{finish_api_json, finish_no_content, ok_json, ApiProblem};
use sdkwork_web_core::WebRequestContext;
use std::sync::Arc;

use crate::{auth::require_backend_context, paths};

/// State for the backend download router.
#[derive(Clone)]
pub struct DownloadsBackendState {
    pub api: Arc<dyn DeployDownloadsApi>,
}

/// Mounts the backend download catalog endpoints.
pub fn build_downloads_backend_router(api: Arc<dyn DeployDownloadsApi>) -> Router {
    Router::new()
        .route(paths::DOWNLOADS_CATALOGS, get(list_download_catalogs))
        .route(
            paths::DOWNLOADS_CATALOG,
            get(retrieve_download_catalog).put(upsert_download_catalog),
        )
        .route(
            paths::DOWNLOADS_CATALOG_PUBLISH,
            post(publish_download_catalog),
        )
        .route(
            paths::DOWNLOADS_CARD,
            put(upsert_download_card).delete(delete_download_card),
        )
        .route(
            paths::DOWNLOADS_ITEM,
            put(upsert_download_item).delete(delete_download_item),
        )
        .route(paths::DOWNLOADS_UPDATE, put(upsert_download_update))
        .with_state(DownloadsBackendState { api })
}

async fn list_download_catalogs(
    ctx: WebRequestContext,
    State(state): State<DownloadsBackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = context_from(context)?;
            let catalogs = state.api.list_download_catalogs(&context).await?;
            ok_json(catalogs)
        }
        .await,
    )
}

async fn retrieve_download_catalog(
    ctx: WebRequestContext,
    State(state): State<DownloadsBackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(catalog_key): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = context_from(context)?;
            let catalog = state
                .api
                .retrieve_download_catalog(&context, &catalog_key)
                .await?;
            ok_json(catalog)
        }
        .await,
    )
}

async fn upsert_download_catalog(
    ctx: WebRequestContext,
    State(state): State<DownloadsBackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(catalog_key): Path<String>,
    Json(request): Json<UpsertDownloadCatalogRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = context_from(context)?;
            let catalog = state
                .api
                .upsert_download_catalog(&context, &catalog_key, &request)
                .await?;
            ok_json(catalog)
        }
        .await,
    )
}

async fn publish_download_catalog(
    ctx: WebRequestContext,
    State(state): State<DownloadsBackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path(catalog_key): Path<String>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = context_from(context)?;
            let catalog = state
                .api
                .publish_download_catalog(&context, &catalog_key)
                .await?;
            ok_json(catalog)
        }
        .await,
    )
}

async fn upsert_download_card(
    ctx: WebRequestContext,
    State(state): State<DownloadsBackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path((catalog_key, card_key)): Path<(String, String)>,
    Json(request): Json<UpsertDownloadCardRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = context_from(context)?;
            let card = state
                .api
                .upsert_download_card(&context, &catalog_key, &card_key, &request)
                .await?;
            ok_json(card)
        }
        .await,
    )
}

async fn delete_download_card(
    ctx: WebRequestContext,
    State(state): State<DownloadsBackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path((catalog_key, card_key)): Path<(String, String)>,
) -> Response {
    finish_no_content(
        &ctx,
        async {
            let context = context_from(context)?;
            state
                .api
                .delete_download_card(&context, &catalog_key, &card_key)
                .await?;
            Ok(())
        }
        .await,
    )
}

async fn upsert_download_item(
    ctx: WebRequestContext,
    State(state): State<DownloadsBackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path((catalog_key, card_key, item_key)): Path<(String, String, String)>,
    Json(request): Json<UpsertDownloadItemRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = context_from(context)?;
            let item = state
                .api
                .upsert_download_item(&context, &catalog_key, &card_key, &item_key, &request)
                .await?;
            ok_json(item)
        }
        .await,
    )
}

async fn delete_download_item(
    ctx: WebRequestContext,
    State(state): State<DownloadsBackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path((catalog_key, card_key, item_key)): Path<(String, String, String)>,
) -> Response {
    finish_no_content(
        &ctx,
        async {
            let context = context_from(context)?;
            state
                .api
                .delete_download_item(&context, &catalog_key, &card_key, &item_key)
                .await?;
            Ok(())
        }
        .await,
    )
}

async fn upsert_download_update(
    ctx: WebRequestContext,
    State(state): State<DownloadsBackendState>,
    context: Option<Extension<DeployBackendRequestContext>>,
    Path((catalog_key, card_key, item_key)): Path<(String, String, String)>,
    Json(request): Json<UpsertDownloadUpdateRequest>,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let context = context_from(context)?;
            let update = state
                .api
                .upsert_download_update(&context, &catalog_key, &card_key, &item_key, &request)
                .await?;
            ok_json(update)
        }
        .await,
    )
}

fn context_from(
    context: Option<Extension<DeployBackendRequestContext>>,
) -> Result<DeployBackendRequestContext, ApiProblem> {
    require_backend_context(context)
}
