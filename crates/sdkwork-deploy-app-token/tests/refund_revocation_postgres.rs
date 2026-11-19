//! Refund revocation at the app-token lookup boundary, against real SQL.
//!
//! Skipped without `SDKWORK_DATABASE_TEST_POSTGRES_URL`. The lookup is the last
//! gate before an app access token is issued, so this is the test that proves a
//! refunded paid-template app can no longer mint one.

use sdkwork_deploy_app_token::{
    DeployApplicationLookup, DeployApplicationSelector, PgDeployApplicationLookup,
};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use std::str::FromStr;

const TENANT_ID: i64 = 987_655;
const APP_ID: i64 = 987_655_001;
const APP_UUID: &str = "bbbbbbbb-8888-4888-8888-bbbbbbbbbbbb";
const ORDER_ID: &str = "refunded-token-order";

async fn pool() -> Option<PgPool> {
    let url = std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL").ok()?;
    let discovery = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .ok()?;
    let schema: Option<String> = sqlx::query_scalar(
        "SELECT schemaname FROM pg_tables WHERE tablename = 'deploy_app' LIMIT 1",
    )
    .fetch_optional(&discovery)
    .await
    .ok()?;
    discovery.close().await;
    let options = PgConnectOptions::from_str(&url)
        .ok()?
        .options([("search_path", schema?.as_str())]);
    PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .ok()
}

async fn seed(pool: &PgPool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS commerce_order (
            tenant_id TEXT NOT NULL,
            id TEXT NOT NULL,
            payment_status TEXT,
            PRIMARY KEY (tenant_id, id)
        )",
    )
    .execute(pool)
    .await
    .expect("commerce_order fixture");
    sqlx::query("DELETE FROM deploy_app WHERE uuid = $1")
        .bind(APP_UUID)
        .execute(pool)
        .await
        .expect("clear stale app fixture");
    sqlx::query("DELETE FROM commerce_order WHERE id = $1")
        .bind(ORDER_ID)
        .execute(pool)
        .await
        .expect("clear stale order fixture");
    sqlx::query(
        "INSERT INTO deploy_app
            (id, uuid, tenant_id, organization_id, name, slug, app_kind, app_status,
             owner_type, created_by, metadata, created_at, updated_at, version)
         VALUES ($1, $2, $3, 0, 'Refunded token app', 'refunded-token-app',
                 'H5_WEB', 'ACTIVE', 'TENANT', 11,
                 jsonb_build_object('template', jsonb_build_object('orderId', $4)),
                 NOW(), NOW(), 1)",
    )
    .bind(APP_ID)
    .bind(APP_UUID)
    .bind(TENANT_ID)
    .bind(ORDER_ID)
    .execute(pool)
    .await
    .expect("deploy_app fixture");
    sqlx::query(
        "INSERT INTO commerce_order (tenant_id, id, payment_status)
         VALUES (CAST($1 AS TEXT), $2, 'refunded')",
    )
    .bind(TENANT_ID)
    .bind(ORDER_ID)
    .execute(pool)
    .await
    .expect("commerce_order fixture row");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL (PostgreSQL integration profile)"]
async fn a_refunded_template_order_stops_app_token_issuance() {
    let Some(pool) = pool().await else {
        return;
    };
    seed(&pool).await;
    let lookup = PgDeployApplicationLookup::new(pool.clone());
    let selector = DeployApplicationSelector::uuid(APP_UUID);

    assert!(
        lookup
            .find(&TENANT_ID.to_string(), &selector)
            .await
            .expect("refunded lookup")
            .is_none(),
        "a refunded order must not resolve an app for token issuance"
    );

    sqlx::query("UPDATE commerce_order SET payment_status = 'paid' WHERE id = $1")
        .bind(ORDER_ID)
        .execute(&pool)
        .await
        .expect("mark paid");
    assert!(
        lookup
            .find(&TENANT_ID.to_string(), &selector)
            .await
            .expect("paid lookup")
            .is_some(),
        "a paid order must still resolve the app"
    );

    sqlx::query("DELETE FROM deploy_app WHERE uuid = $1")
        .bind(APP_UUID)
        .execute(&pool)
        .await
        .expect("cleanup app");
    sqlx::query("DELETE FROM commerce_order WHERE id = $1")
        .bind(ORDER_ID)
        .execute(&pool)
        .await
        .expect("cleanup order");
}
