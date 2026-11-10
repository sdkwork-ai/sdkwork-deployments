//! End-to-end public download surface over a real PostgreSQL schema.
//!
//! Unlike `downloads_public_routes.rs` (stub read API), this test wires the real
//! chain: HTTP router -> `DeployService` -> `DeployRepository` -> PostgreSQL.
//! It is the only place that proves the published catalog a visitor sees is the
//! same row the operator published.
//!
//! Skipped without `SDKWORK_DATABASE_TEST_POSTGRES_URL`.

use std::str::FromStr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{
    UpsertDownloadCardRequest, UpsertDownloadCatalogRequest, UpsertDownloadItemRequest,
    UpsertDownloadUpdateRequest,
};
use sdkwork_deploy_drive_port::MemoryDeployDrivePort;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::{DeployRepositoryPort, DeployService};
use sdkwork_routes_deploy_app_api::{
    build_downloads_public_router, web_bootstrap::wrap_router_with_web_framework,
};
use sdkwork_web_core::DefaultWebRequestContextResolver;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool, Row};
use tower::util::ServiceExt;

const BASELINE: &str =
    include_str!("../../../database/ddl/baseline/postgres/0001_deploy_baseline.sql");
const TENANT: i64 = 91_800_001;

async fn isolated_pool() -> PgPool {
    let database_url = std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL")
        .expect("SDKWORK_DATABASE_TEST_POSTGRES_URL is required for PostgreSQL integration tests");
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("connect integration database");
    let schema = format!(
        "sdkwork_downloads_e2e_{}",
        sdkwork_database_id::uuid_v4().replace('-', "")
    );
    sqlx::raw_sql(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin_pool)
        .await
        .expect("create isolated schema");
    admin_pool.close().await;

    let connection_schema = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .after_connect(move |connection, _metadata| {
            let schema = connection_schema.clone();
            Box::pin(async move {
                sqlx::raw_sql(AssertSqlSafe(format!("SET search_path TO {schema}")))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(PgConnectOptions::from_str(&database_url).expect("parse database url"))
        .await
        .expect("connect isolated schema");
    sqlx::raw_sql(BASELINE)
        .execute(&pool)
        .await
        .expect("apply baseline");
    pool
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn published_catalog_is_served_end_to_end() {
    let pool = isolated_pool().await;
    let repository = Arc::new(DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(7).expect("snowflake"),
        [0u8; 32],
    ));
    let service = Arc::new(DeployService::new(
        repository.clone(),
        Arc::new(MemoryDeployDrivePort),
    ));

    // Draft is invisible to the public surface.
    let router = wrap_router_with_web_framework(
        DefaultWebRequestContextResolver::default(),
        build_downloads_public_router(service.clone()),
    );
    let draft = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/downloads/catalog/e2e-catalog")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(draft.status(), StatusCode::NOT_FOUND);

    repository
        .upsert_download_catalog(
            TENANT,
            None,
            "e2e-catalog",
            &UpsertDownloadCatalogRequest {
                product_key: Some("app.e2e".to_owned()),
                product_name: Some("E2E".to_owned()),
                product_version: Some("2.0.0".to_owned()),
                default_channel_key: Some("nightly".to_owned()),
                app_id: None,
            },
        )
        .await
        .expect("catalog");
    repository
        .upsert_download_card(
            TENANT,
            None,
            "e2e-catalog",
            "desktop",
            &UpsertDownloadCardRequest {
                title: Some("Desktop".to_owned()),
                description: Some("Installers".to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("card");
    repository
        .upsert_download_item(
            TENANT,
            None,
            "e2e-catalog",
            "desktop",
            "win-x64",
            &UpsertDownloadItemRequest {
                platform: Some("windows".to_owned()),
                architecture: Some("x64".to_owned()),
                label: Some("Windows".to_owned()),
                file_name: Some("E2E.exe".to_owned()),
                version: Some("2.0.0".to_owned()),
                download_source: Some(serde_json::json!({
                    "href": "https://download.example.com/E2E.exe"
                })),
                size_bytes: Some(2048),
                primary_action: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("installer");
    repository
        .upsert_download_update(
            TENANT,
            None,
            "e2e-catalog",
            "desktop",
            "win-x64",
            &UpsertDownloadUpdateRequest {
                enabled: Some(true),
                provider: Some("generic".to_owned()),
                feed_url: Some("https://download.example.com/feed/".to_owned()),
                channel_key: Some("nightly".to_owned()),
                allowed_channels: Some(vec!["nightly".to_owned()]),
                check_interval_minutes: Some(15),
                rollout_percentage: Some(50),
                ..Default::default()
            },
        )
        .await
        .expect("update configuration");
    repository
        .publish_download_catalog(TENANT, None, "e2e-catalog")
        .await
        .expect("publish");

    let catalog = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/downloads/catalog/e2e-catalog")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(catalog.status(), StatusCode::OK);
    let body = catalog.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["data"]["product"]["id"], "app.e2e");
    assert_eq!(
        payload["data"]["cards"][0]["actions"][0]["href"],
        "https://download.example.com/E2E.exe"
    );

    let config = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/downloads/config?product_key=app.e2e&platform=windows&architecture=x64")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(config.status(), StatusCode::OK);
    let body = config.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["data"]["update"]["provider"], "generic");
    assert_eq!(payload["data"]["update"]["checkIntervalMinutes"], 15);
    assert_eq!(payload["data"]["update"]["rolloutPercentage"], 50);

    let item_uuid: String =
        sqlx::query("SELECT uuid FROM deploy_download_item WHERE tenant_id = $1")
            .bind(TENANT)
            .fetch_one(&pool)
            .await
            .expect("item uuid")
            .try_get("uuid")
            .expect("uuid column");

    let artifact = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/app/v3/api/downloads/artifact/{item_uuid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(artifact.status(), StatusCode::FOUND);
    assert_eq!(
        artifact
            .headers()
            .get(axum::http::header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("https://download.example.com/E2E.exe")
    );
}
