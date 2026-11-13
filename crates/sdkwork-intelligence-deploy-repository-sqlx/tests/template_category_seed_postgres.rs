//! The baseline application-template taxonomy against a real PostgreSQL schema.
//!
//! The seed manifest is the production path; this test executes the same SQL the
//! lifecycle runs for every profile and asserts the rows a fresh tenant needs
//! before its first `appTemplates.create`, including idempotent reseeding.
//!
//! Skipped without `SDKWORK_DATABASE_TEST_POSTGRES_URL`.

mod common;

use sqlx::{AssertSqlSafe, PgPool};

const CATEGORIES: &str =
    include_str!("../../../database/seeds/common/015_deploy_app_template_categories.sql");

const EXPECTED_KEYS: [&str; 10] = [
    "web-h5",
    "pc-desktop",
    "mobile-native",
    "cross-platform",
    "mini-program",
    "api-service",
    "full-stack",
    "ai-agent",
    "developer-tooling",
    "other",
];

async fn apply_seed(pool: &PgPool) {
    sqlx::raw_sql(AssertSqlSafe(CATEGORIES.to_owned()))
        .execute(pool)
        .await
        .expect("apply template-category seed");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn baseline_taxonomy_is_complete_and_reseed_is_idempotent() {
    let pool = common::postgres_pool().await;
    apply_seed(&pool).await;
    apply_seed(&pool).await;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_app_template_category
         WHERE tenant_id = 100001 AND deleted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("category count");
    assert_eq!(total, 10);

    let keys: Vec<String> = sqlx::query_scalar(
        "SELECT category_key FROM deploy_app_template_category
         WHERE tenant_id = 100001 AND deleted_at IS NULL
         ORDER BY sort_order, id",
    )
    .fetch_all(&pool)
    .await
    .expect("category keys");
    assert_eq!(keys, EXPECTED_KEYS);

    let (active, bad_scope, first_id, last_id): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE status = 'ACTIVE'),
             COUNT(*) FILTER (WHERE organization_id <> 0 OR parent_id IS NOT NULL),
             MIN(id), MAX(id)
         FROM deploy_app_template_category
         WHERE tenant_id = 100001 AND deleted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("category invariants");
    assert_eq!(active, 10);
    assert_eq!(bad_scope, 0);
    assert_eq!(first_id, 910001300001);
    assert_eq!(last_id, 910001300010);

    let uuids: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT uuid) FROM deploy_app_template_category
         WHERE tenant_id = 100001 AND deleted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("category uuids");
    assert_eq!(uuids, 10);
}
