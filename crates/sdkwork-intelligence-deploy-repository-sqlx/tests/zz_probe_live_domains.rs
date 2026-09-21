//! Scratch probe: run the real `list_app_domains` repository path against the
//! **live** `sdkwork_ai_dev` schema (read-only) to surface the error currently
//! masked as HTTP 500 `code: 50001` on `GET /app/v3/api/apps/{appId}/domains`.
//!
//! Not a regression test — delete once the root cause is fixed.

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::repository::DeployRepositoryPort;
use sqlx::postgres::PgPoolOptions;

mod common;

#[tokio::test]
#[ignore = "manual probe against the live sdkwork_ai_dev schema"]
async fn probe_live_app_domains() {
    let url = std::env::var("SDKWORK_LIVE_DATABASE_URL")
        .expect("SDKWORK_LIVE_DATABASE_URL required (live dev schema, read-only probe)");
    let schema = std::env::var("SDKWORK_LIVE_SCHEMA").unwrap_or_else(|_| "sdkwork_ai_dev".to_owned());
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect live dev database");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("SET search_path TO {schema}")))
        .execute(&pool)
        .await
        .expect("set search_path");

    let repository = DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(3).expect("Snowflake generator"),
        common::test_secret_key(),
    );

    // The app from the failing trace: tenant 100001, id 359302751877861376,
    // uuid 1b5eb653-cdd8-4ba3-a0bd-61357f343f4e, slug 'aaa'.
    let app_ids = ["359302751877861376", "1b5eb653-cdd8-4ba3-a0bd-61357f343f4e"];
    for app_id in app_ids {
        println!("=== list_app_domains(100001, {app_id}) ===");
        match repository.list_app_domains(100001, app_id).await {
            Ok(page) => {
                println!("OK: total={} items={}", page.total, page.items.len());
                for item in page.items.iter().take(3) {
                    println!("   {item:?}");
                }
            }
            Err(error) => {
                println!("ERR kind={:?}", error.kind());
                println!("ERR display={error}");
                println!("ERR debug={error:?}");
            }
        }
    }
}
