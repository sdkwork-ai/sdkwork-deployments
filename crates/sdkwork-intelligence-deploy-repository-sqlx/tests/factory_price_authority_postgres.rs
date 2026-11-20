//! Paid app-template order verification (`APPLICATION_FACTORY_DEPLOYMENT_SPEC.md`
//! section 2, one price authority; `TEMPLATE_PRICING_PAID` install gate).
//!
//! Deploy does not own the price: the scaffold columns are presentation data and
//! the settled order lives in the commerce module. Until appstore publishes a
//! listing-price authority, this check is the only gate between a paid template
//! and a free install, so its failure modes are security-relevant:
//!
//! - commerce tables absent => `false` (fail closed, never "assume paid");
//! - order not paid / not owned by the buyer / not an app-template order /
//!   different template => `false`;
//! - the matching settled order => `true`.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default.

mod common;

use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

const TENANT: i64 = 7301;
const BUYER: i64 = 11301;
const OTHER_BUYER: i64 = 11302;
const TEMPLATE: &str = "tmpl-0001";

async fn create_commerce_stubs(pool: &PgPool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS commerce_order (
             id TEXT PRIMARY KEY,
             tenant_id TEXT NOT NULL,
             owner_user_id TEXT,
             payment_status TEXT,
             subject TEXT
         )",
    )
    .execute(pool)
    .await
    .expect("create commerce_order stub");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS commerce_order_item (
             id TEXT PRIMARY KEY,
             tenant_id TEXT NOT NULL,
             order_id TEXT NOT NULL,
             sku_id TEXT,
             sku_snapshot_json TEXT
         )",
    )
    .execute(pool)
    .await
    .expect("create commerce_order_item stub");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_missing_commerce_authority_fails_closed() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-price-missing-test").await;
    let verified = repository
        .verify_paid_app_template_order(TENANT, BUYER, "order-1", TEMPLATE)
        .await
        .expect("verify with no commerce tables");
    assert!(
        !verified,
        "an absent commerce authority must never certify a paid order"
    );
}

async fn insert_order(
    pool: &PgPool,
    order_id: &str,
    owner: i64,
    status: &str,
    subject: &str,
    template: &str,
) {
    sqlx::query(
        "INSERT INTO commerce_order (id, tenant_id, owner_user_id, payment_status, subject)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(order_id)
    .bind(TENANT.to_string())
    .bind(owner.to_string())
    .bind(status)
    .bind(subject)
    .execute(pool)
    .await
    .expect("insert commerce_order");
    sqlx::query(
        "INSERT INTO commerce_order_item
           (id, tenant_id, order_id, sku_id, sku_snapshot_json)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(format!("item-{order_id}"))
    .bind(TENANT.to_string())
    .bind(order_id)
    .bind(template)
    .bind(serde_json::json!({ "templateUuid": template }).to_string())
    .execute(pool)
    .await
    .expect("insert commerce_order_item");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn only_a_settled_order_owned_by_the_buyer_for_this_template_certifies() {
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-price-verify-test").await;
    create_commerce_stubs(&pool).await;

    insert_order(&pool, "order-paid", BUYER, "paid", "app_template", TEMPLATE).await;
    insert_order(&pool, "order-pending", BUYER, "pending", "app_template", TEMPLATE).await;
    insert_order(
        &pool,
        "order-other-buyer",
        OTHER_BUYER,
        "paid",
        "app_template",
        TEMPLATE,
    )
    .await;
    insert_order(
        &pool,
        "order-other-subject",
        BUYER,
        "paid",
        "membership",
        TEMPLATE,
    )
    .await;
    insert_order(
        &pool,
        "order-other-template",
        BUYER,
        "paid",
        "app_template",
        "tmpl-0002",
    )
    .await;

    let mut results = Vec::new();
    for order_id in [
        "order-paid",
        "order-pending",
        "order-other-buyer",
        "order-other-subject",
        "order-other-template",
        "order-unknown",
    ] {
        results.push(
            repository
                .verify_paid_app_template_order(TENANT, BUYER, order_id, TEMPLATE)
                .await
                .expect("verify"),
        );
    }

    assert!(results[0], "a settled matching order certifies");
    assert!(!results[1], "an unsettled order does not certify");
    assert!(!results[2], "another buyer's order does not certify");
    assert!(!results[3], "a non-app-template order does not certify");
    assert!(!results[4], "an order for another template does not certify");
    assert!(!results[5], "an unknown order does not certify");
}
