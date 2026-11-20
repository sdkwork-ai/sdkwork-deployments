//! Paid app-template order verification on the migrated schema.
//!
//! Skipped without `SDKWORK_DATABASE_TEST_POSTGRES_URL`. It creates the two
//! commerce tables the verifier reads (they are owned by the order module) so
//! the read-only buyer/payment/template predicate is exercised with real SQL.

mod common;

use sdkwork_intelligence_deploy_service::DeployRepositoryPort;

const TENANT_ID: i64 = 7;
const BUYER_ID: i64 = 11;
const TEMPLATE_UUID: &str = "eeeeeeee-5555-4555-8555-eeeeeeeeeeee";
const ORDER_ID: &str = "order-1";

async fn migrate_commerce_fixture(pool: &sqlx::PgPool) {
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
    .expect("create commerce_order fixture");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS commerce_order_item (
            tenant_id TEXT NOT NULL,
            order_id TEXT NOT NULL,
            sku_id TEXT,
            sku_snapshot_json JSONB
        )",
    )
    .execute(pool)
    .await
    .expect("create commerce_order_item fixture");
    sqlx::query(
        "INSERT INTO commerce_order (tenant_id, id, owner_user_id, payment_status, subject)
         VALUES (CAST($1 AS TEXT), $2, CAST($3 AS TEXT), 'paid', 'app_template')",
    )
    .bind(TENANT_ID)
    .bind(ORDER_ID)
    .bind(BUYER_ID)
    .execute(pool)
    .await
    .expect("insert paid order");
    sqlx::query(
        "INSERT INTO commerce_order_item (tenant_id, order_id, sku_id, sku_snapshot_json)
         VALUES (CAST($1 AS TEXT), $2, $3, $4)",
    )
    .bind(TENANT_ID)
    .bind(ORDER_ID)
    .bind(TEMPLATE_UUID)
    .bind(serde_json::json!({ "templateUuid": TEMPLATE_UUID }))
    .execute(pool)
    .await
    .expect("insert order item");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL (PostgreSQL integration profile)"]
async fn only_the_settled_buyer_order_for_the_listing_verifies() {
    let (repository, pool) =
        common::migrated_repository_with_pool("template_install::paid_order").await;
    migrate_commerce_fixture(&pool).await;

    assert!(repository
        .verify_paid_app_template_order(TENANT_ID, BUYER_ID, ORDER_ID, TEMPLATE_UUID)
        .await
        .expect("paid order"));

    // A different buyer, a different listing or an unpaid order must not pass.
    assert!(!repository
        .verify_paid_app_template_order(TENANT_ID, BUYER_ID + 1, ORDER_ID, TEMPLATE_UUID)
        .await
        .expect("other buyer"));
    assert!(!repository
        .verify_paid_app_template_order(
            TENANT_ID,
            BUYER_ID,
            ORDER_ID,
            "ffffffff-6666-4666-8666-ffffffffffff"
        )
        .await
        .expect("other template"));
    sqlx::query("UPDATE commerce_order SET payment_status = 'refunded' WHERE id = $1")
        .bind(ORDER_ID)
        .execute(&pool)
        .await
        .expect("refund order");
    assert!(!repository
        .verify_paid_app_template_order(TENANT_ID, BUYER_ID, ORDER_ID, TEMPLATE_UUID)
        .await
        .expect("refunded order"));
}
