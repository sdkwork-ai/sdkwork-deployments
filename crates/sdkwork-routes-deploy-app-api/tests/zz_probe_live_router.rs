//! Scratch probe: mount the **real** `sdkwork-routes-deploy-app-api` router over
//! a live-schema `DeployService` and issue the genuine HTTP request for
//! `apps.domains.list`, to surface the error masked as HTTP 500 `code: 50001`.
//!
//! Not a regression test — delete once the root cause is fixed.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::DeployAppRequestContext;
use sdkwork_intelligence_deploy_service::DeployService;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

#[tokio::test]
#[ignore = "manual probe against the live sdkwork_ai_dev schema"]
async fn probe_live_router_domains() {
    let url = std::env::var("SDKWORK_LIVE_DATABASE_URL")
        .expect("SDKWORK_LIVE_DATABASE_URL required (live dev schema, read-only probe)");
    let schema =
        std::env::var("SDKWORK_LIVE_SCHEMA").unwrap_or_else(|_| "sdkwork_ai_dev".to_owned());
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect live dev database");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("SET search_path TO {schema}")))
        .execute(&pool)
        .await
        .expect("set search_path");

    let repository = Arc::new(sdkwork_intelligence_deploy_repository_sqlx::DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(3).expect("Snowflake generator"),
        *b"sdkwork-deploy-test-secret-00000",
    ));
    // Mirror the gateway: the service is built from the repository port plus a
    // Drive port this endpoint never touches.
    let drive: Arc<dyn sdkwork_deploy_drive_port::DeployDrivePort> =
        Arc::new(sdkwork_deploy_drive_port::MemoryDeployDrivePort::default());
    let service: Arc<dyn sdkwork_routes_deploy_app_api::DeployAppApi> = Arc::new(
        DeployService::new(repository, drive),
    );
    let router = sdkwork_routes_deploy_app_api::build_router_with_shared_app_api(service.clone());

    // The route reads the context from a request extension installed by the host
    // framework layer; the bare router still needs it, so inject it.
    let context = DeployAppRequestContext {
        tenant_id: 100001,
        actor_id: Some(1),
        organization_id: Some(0),
        session_id: Some("probe".to_owned()),
        auth_token: None,
        access_token: None,
    };

    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/app/v3/api/apps/1b5eb653-cdd8-4ba3-a0bd-61357f343f4e/domains")
                .extension(context.clone())
                .body(Body::empty())
                .expect("build request"),
        )
        .await
        .expect("router responded");

    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    println!("router status = {}", status.as_u16());
    println!("router body   = {}", String::from_utf8_lossy(&bytes));

    // Step-by-step: isolate which stage of the handler body fails.
    println!("\n--- step 1: service call ---");
    let page = service
        .list_app_domains(&context, "1b5eb653-cdd8-4ba3-a0bd-61357f343f4e")
        .await;
    match &page {
        Ok(page) => println!("service OK total={} items={}", page.total, page.items.len()),
        Err(error) => println!("service ERR kind={:?} display={error}", error.kind()),
    }
    println!("--- step 2: envelope + JSON ---");
    if let Ok(page) = page {
        let envelope = sdkwork_routes_deploy_common::envelope::app_domain_page(page);
        match serde_json::to_string(&envelope) {
            Ok(json) => println!("envelope JSON OK bytes={}", json.len()),
            Err(error) => println!("envelope JSON ERR {error}"),
        }
    }

    assert_ne!(status, StatusCode::INTERNAL_SERVER_ERROR, "probe reproduced a 500");
}
