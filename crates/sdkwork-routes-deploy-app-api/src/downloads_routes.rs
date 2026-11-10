//! Anonymous download surface: catalog, update configuration, feed and artifact.
//!
//! These routes are public by design: the download page is read by visitors, and
//! desktop clients read their update configuration before they have any user
//! session. They therefore never extract the app request context; the only
//! request fact they use is the public origin the caller reached, which becomes
//! the base of generated artifact and feed links.
//!
//! Two responses are deliberately not JSON envelopes:
//!
//! - the feed is the electron-updater YAML document, byte-for-byte what the
//!   client parses;
//! - the artifact is a redirect to wherever the installer actually lives, so a
//!   presigned URL never appears in the catalog.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use sdkwork_deploy_contract::{
    DeployDownloadsReadApi, PublicDownloadArtifactQuery, PublicDownloadCatalogQuery,
    PublicDownloadFeedQuery, PublicDownloadUpdateQuery,
};
use sdkwork_deploy_feed::render_yaml;
use sdkwork_routes_deploy_common::{finish_api_json, ok_json};
use sdkwork_web_core::WebRequestContext;
use std::sync::Arc;

use crate::paths;

/// State for the public download router.
#[derive(Clone)]
pub struct DownloadsReadState {
    pub api: Arc<dyn DeployDownloadsReadApi>,
}

/// Mounts the anonymous download endpoints.
pub fn build_downloads_public_router(api: Arc<dyn DeployDownloadsReadApi>) -> Router {
    Router::new()
        .route(paths::DOWNLOAD_CATALOG, get(public_download_catalog))
        .route(paths::DOWNLOAD_CONFIG, get(public_download_config))
        .route(paths::DOWNLOAD_FEED, get(public_download_feed))
        .route(paths::DOWNLOAD_ARTIFACT, get(public_download_artifact))
        .with_state(DownloadsReadState { api })
}

/// The public origin of this request, used to build absolute links.
fn request_base(headers: &HeaderMap) -> Option<String> {
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| *value == "http" || *value == "https")
        .unwrap_or("https");
    Some(format!("{proto}://{host}"))
}

async fn public_download_catalog(
    ctx: WebRequestContext,
    State(state): State<DownloadsReadState>,
    Path(catalog_key): Path<String>,
    headers: HeaderMap,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let document = state
                .api
                .public_download_catalog(&PublicDownloadCatalogQuery {
                    catalog_key,
                    locale: None,
                    base_url: request_base(&headers),
                })
                .await?;
            ok_json(document)
        }
        .await,
    )
}

async fn public_download_config(
    ctx: WebRequestContext,
    State(state): State<DownloadsReadState>,
    Query(query): Query<PublicDownloadUpdateQuery>,
    headers: HeaderMap,
) -> Response {
    finish_api_json(
        &ctx,
        async {
            let document = state
                .api
                .public_download_update_config(&PublicDownloadUpdateQuery {
                    product_key: query.product_key,
                    platform: query.platform,
                    architecture: query.architecture,
                    channel: query.channel,
                    base_url: request_base(&headers),
                })
                .await?;
            ok_json(document)
        }
        .await,
    )
}

async fn public_download_feed(
    ctx: WebRequestContext,
    State(state): State<DownloadsReadState>,
    Path((product_key, platform, architecture, channel_file)): Path<(
        String,
        String,
        String,
        String,
    )>,
    headers: HeaderMap,
) -> Response {
    let channel = channel_file
        .trim_end_matches(".yml")
        .trim_end_matches("-mac")
        .to_owned();
    match state
        .api
        .public_download_feed(&PublicDownloadFeedQuery {
            product_key,
            platform,
            architecture,
            channel,
            base_url: request_base(&headers),
        })
        .await
    {
        Ok(document) => {
            let body = render_yaml(&document);
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "text/yaml; charset=utf-8"),
                    (
                        header::CACHE_CONTROL,
                        "public, max-age=60, stale-while-revalidate=300",
                    ),
                ],
                body,
            )
                .into_response()
        }
        Err(error) => {
            let problem: sdkwork_routes_deploy_common::ApiProblem = error.into();
            problem.into_response_for(&ctx)
        }
    }
}

async fn public_download_artifact(
    ctx: WebRequestContext,
    State(state): State<DownloadsReadState>,
    Path(item_uuid): Path<String>,
) -> Response {
    match state
        .api
        .public_download_artifact(&PublicDownloadArtifactQuery { item_uuid })
        .await
    {
        Ok(target) => {
            let mut response = Response::new(axum::body::Body::empty());
            *response.status_mut() = StatusCode::FOUND;
            let location = HeaderValue::from_str(&target.url)
                .unwrap_or_else(|_| HeaderValue::from_static("/"));
            response.headers_mut().insert(header::LOCATION, location);
            if let Ok(disposition) =
                HeaderValue::from_str(&format!("attachment; filename=\"{}\"", target.file_name))
            {
                response
                    .headers_mut()
                    .insert(header::CONTENT_DISPOSITION, disposition);
            }
            response
        }
        Err(error) => {
            let problem: sdkwork_routes_deploy_common::ApiProblem = error.into();
            problem.into_response_for(&ctx)
        }
    }
}
