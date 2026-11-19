//! Refund revocation sweep against a migrated PostgreSQL schema.
//!
//! Skipped without `SDKWORK_DATABASE_TEST_POSTGRES_URL`; it exercises the two
//! statements that matter (candidate selection and archive update) with real
//! SQL rather than a mock.

use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use std::str::FromStr;

const TENANT_ID: i64 = 987_654;
const APP_UUID: &str = "aaaaaaaa-7777-4777-8777-aaaaaaaaaaaa";
const ORDER_ID: &str = "refund-order-1";

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
    let schema = schema?;
    let options = PgConnectOptions::from_str(&url)
        .ok()?
        .options([("search_path", schema.as_str())]);
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
            owner_user_id TEXT,
            payment_status TEXT,
            subject TEXT,
            PRIMARY KEY (tenant_id, id)
        )",
    )
    .execute(pool)
    .await
    .expect("commerce_order fixture");
    sqlx::query(
        "INSERT INTO deploy_app
            (id, uuid, tenant_id, organization_id, name, slug, app_kind, app_status,
             owner_type, created_by, metadata, created_at, updated_at, version)
         VALUES (987654001, $1, $2, 0, 'Refunded template app', 'refunded-template-app',
                 'H5_WEB', 'ACTIVE', 'TENANT', 11,
                 jsonb_build_object('template', jsonb_build_object('orderId', $3)),
                 NOW(), NOW(), 1)",
    )
    .bind(APP_UUID)
    .bind(TENANT_ID)
    .bind(ORDER_ID)
    .execute(pool)
    .await
    .expect("deploy_app fixture");
    sqlx::query(
        "INSERT INTO commerce_order (tenant_id, id, owner_user_id, payment_status, subject)
         VALUES (CAST($1 AS TEXT), $2, '11', 'refunded', 'app_template')",
    )
    .bind(TENANT_ID)
    .bind(ORDER_ID)
    .execute(pool)
    .await
    .expect("refunded order fixture");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL (PostgreSQL integration profile)"]
async fn a_refunded_order_candidate_is_archived_once() {
    let Some(pool) = pool().await else {
        return;
    };
    seed(&pool).await;

    let dry = sdkwork_deploy_refund_revocation_worker::revoke_once(&pool, false)
        .await
        .expect("dry run");
    assert!(dry.candidates.iter().any(|uuid| uuid == APP_UUID));
    assert_eq!(dry.archived, 0);
    let archived_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT archived_at FROM deploy_app WHERE uuid = $1")
            .bind(APP_UUID)
            .fetch_one(&pool)
            .await
            .expect("archived_at after dry run");
    assert!(archived_at.is_none());

    let applied = sdkwork_deploy_refund_revocation_worker::revoke_once(&pool, true)
        .await
        .expect("apply");
    assert_eq!(applied.archived, 1);
    let status: String = sqlx::query_scalar("SELECT app_status FROM deploy_app WHERE uuid = $1")
        .bind(APP_UUID)
        .fetch_one(&pool)
        .await
        .expect("status after apply");
    assert_eq!(status, "ARCHIVED");

    // A second sweep must not rewrite the already archived row.
    let replay = sdkwork_deploy_refund_revocation_worker::revoke_once(&pool, true)
        .await
        .expect("replay");
    assert_eq!(replay.archived, 0);

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
