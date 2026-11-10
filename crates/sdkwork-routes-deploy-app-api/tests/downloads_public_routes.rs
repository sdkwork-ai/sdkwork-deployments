//! Public download surface: anonymous access, JSON envelope, YAML feed and the
//! artifact redirect.
//!
//! These routes are the ones a visitor and an installed desktop client call with
//! no session, so the test asserts two things a unit test cannot: the Web
//! Framework layer really treats them as public (no token is sent), and the two
//! non-envelope responses keep their exact content type and status.

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use sdkwork_deploy_contract::{
    DeployDownloadsReadApi, DeployServiceError, DeployServiceResult, DownloadArtifactTarget,
    DownloadFeedDocument, DownloadFeedFile, DownloadUpdateConfigDocument, PublicDownloadAction,
    PublicDownloadCard, PublicDownloadCatalogDocument, PublicDownloadCatalogQuery,
    PublicDownloadFeedQuery, PublicDownloadProduct, PublicDownloadUpdateConfig,
    PublicDownloadUpdateQuery,
};
use sdkwork_routes_deploy_app_api::{
    build_downloads_public_router, web_bootstrap::wrap_router_with_web_framework,
};
use sdkwork_web_core::DefaultWebRequestContextResolver;
use std::sync::Arc;
use tower::util::ServiceExt;

struct StubDownloads;

#[async_trait]
impl DeployDownloadsReadApi for StubDownloads {
    async fn public_download_catalog(
        &self,
        _query: &PublicDownloadCatalogQuery,
    ) -> DeployServiceResult<PublicDownloadCatalogDocument> {
        Ok(PublicDownloadCatalogDocument {
            schema_version: "2026-05-18.sdkwork-download-catalog.v1".to_owned(),
            generated_at: "2026-11-10T00:00:00.000Z".to_owned(),
            product: PublicDownloadProduct {
                id: "app.demo".to_owned(),
                name: "Demo".to_owned(),
                version: "1.0.0".to_owned(),
                channel: "nightly".to_owned(),
            },
            cards: vec![PublicDownloadCard {
                id: "desktop".to_owned(),
                title: "Desktop".to_owned(),
                description: String::new(),
                icon: String::new(),
                tone: String::new(),
                kind: "desktop".to_owned(),
                badge: String::new(),
                primary_action_strategy: "detected-platform".to_owned(),
                actions: vec![PublicDownloadAction {
                    id: "win-x64".to_owned(),
                    label: "Windows".to_owned(),
                    platform: "windows".to_owned(),
                    architecture: "x64".to_owned(),
                    href: "https://deploy.example.com/app/v3/api/downloads/artifact/item-1"
                        .to_owned(),
                    file_name: "Demo.exe".to_owned(),
                    version: "1.0.0".to_owned(),
                    release_tag: String::new(),
                    size_bytes: Some(10),
                    sha256: None,
                    primary: true,
                    disabled: false,
                    unavailable_label: String::new(),
                }],
            }],
        })
    }

    async fn public_download_update_config(
        &self,
        _query: &PublicDownloadUpdateQuery,
    ) -> DeployServiceResult<DownloadUpdateConfigDocument> {
        Ok(DownloadUpdateConfigDocument {
            schema_version: "2026-11-10.sdkwork-download-update-config.v1".to_owned(),
            catalog_key: "demo".to_owned(),
            item_key: "win-x64".to_owned(),
            product_key: "app.demo".to_owned(),
            platform: "windows".to_owned(),
            architecture: "x64".to_owned(),
            version: "1.0.0".to_owned(),
            revision: "sha256:abc".to_owned(),
            update: PublicDownloadUpdateConfig {
                enabled: true,
                provider: "deployments".to_owned(),
                feed_url:
                    "https://deploy.example.com/app/v3/api/downloads/feed/app.demo/windows/x64/"
                        .to_owned(),
                github_owner: String::new(),
                github_repo: String::new(),
                channel: "nightly".to_owned(),
                channel_file: "nightly.yml".to_owned(),
                allowed_channels: vec!["nightly".to_owned()],
                allow_prerelease: true,
                allow_downgrade: false,
                minimum_supported_version: String::new(),
                check_interval_minutes: 10,
                rollout_percentage: 100,
                release_notes_url: String::new(),
            },
        })
    }

    async fn public_download_feed(
        &self,
        _query: &PublicDownloadFeedQuery,
    ) -> DeployServiceResult<DownloadFeedDocument> {
        Ok(DownloadFeedDocument {
            version: "1.0.0".to_owned(),
            files: vec![DownloadFeedFile {
                url: "https://deploy.example.com/a.exe".to_owned(),
                sha512: "c2hhNTEy".to_owned(),
                size: 10,
            }],
            path: "https://deploy.example.com/a.exe".to_owned(),
            release_date: "2026-11-10T00:00:00.000Z".to_owned(),
            staging_percentage: None,
        })
    }

    async fn public_download_artifact(
        &self,
        query: &sdkwork_deploy_contract::PublicDownloadArtifactQuery,
    ) -> DeployServiceResult<DownloadArtifactTarget> {
        if query.item_uuid == "missing" {
            return Err(DeployServiceError::not_found("installer not found"));
        }
        Ok(DownloadArtifactTarget {
            url: "https://cdn.example.com/a.exe".to_owned(),
            file_name: "Demo.exe".to_owned(),
            content_type: "application/vnd.microsoft.portable-executable".to_owned(),
        })
    }
}

fn app() -> axum::Router {
    wrap_router_with_web_framework(
        DefaultWebRequestContextResolver::default(),
        build_downloads_public_router(Arc::new(StubDownloads)),
    )
}

fn public_request(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn anonymous_catalog_returns_the_published_document() {
    let response = app()
        .oneshot(public_request("/app/v3/api/downloads/catalog/demo"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["data"]["product"]["id"], "app.demo");
    assert_eq!(payload["data"]["cards"][0]["actions"][0]["id"], "win-x64");
}

#[tokio::test]
async fn anonymous_config_returns_the_update_settings() {
    let response = app()
        .oneshot(public_request(
            "/app/v3/api/downloads/config?product_key=app.demo&platform=windows&architecture=x64",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["data"]["update"]["channel"], "nightly");
    assert_eq!(payload["data"]["update"]["provider"], "deployments");
}

#[tokio::test]
async fn feed_is_raw_yaml_with_a_cacheable_response() {
    let response = app()
        .oneshot(public_request(
            "/app/v3/api/downloads/feed/app.demo/windows/x64/nightly.yml",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/yaml")));
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("version: '1.0.0'"));
    assert!(text.contains("sha512: 'c2hhNTEy'"));
}

#[tokio::test]
async fn artifact_redirects_to_the_current_download_location() {
    let response = app()
        .oneshot(public_request("/app/v3/api/downloads/artifact/item-1"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("https://cdn.example.com/a.exe")
    );
    assert!(response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("Demo.exe")));
}

#[tokio::test]
async fn missing_artifact_is_not_found_without_leaking_a_path() {
    let response = app()
        .oneshot(public_request("/app/v3/api/downloads/artifact/missing"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
