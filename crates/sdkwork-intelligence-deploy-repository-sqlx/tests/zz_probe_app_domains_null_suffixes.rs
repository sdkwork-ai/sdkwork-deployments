//! Scratch reproduction for the `apps.domains.list` 500 on production-shaped data.
//!
//! The regression test in `platform_app_domains.rs` covers the `z.apex` typo.
//! This probe covers the *next* failure: an app row whose `app_domain_suffixes`
//! is SQL NULL (the shape every pre-existing app has, because the column was
//! added with no DEFAULT and no backfill).
mod common;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

async fn repository_with_null_suffixes() -> (DeployRepository, PgPool) {
    let pool = common::postgres_pool().await;
    // Mirror the production row exactly: NULL suffixes, real slug, no label.
    sqlx::query(
        "INSERT INTO deploy_app (
            id,uuid,tenant_id,organization_id,name,slug,app_kind,app_status,
            default_environment,app_domain_label,app_domain_suffixes,
            created_at,updated_at,version
         ) VALUES (10,'site-10',7,9,'Shop','aaa','WEB','ACTIVE','production',
                   NULL, NULL,
                   '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .execute(&pool)
    .await
    .expect("seed deploy_app with NULL suffixes");
    (
        DeployRepository::new(
            pool.clone(),
            SnowflakeIdGenerator::new(3).expect("Snowflake generator"),
            common::test_secret_key(),
        ),
        pool,
    )
}

#[tokio::test]
async fn list_app_domains_works_when_suffixes_are_null() {
    let (repository, _pool) = repository_with_null_suffixes().await;
    let page = repository
        .list_app_domains(7, "site-10")
        .await
        .expect("list must not fail when app_domain_suffixes is NULL");
    // No bindings provisioned, so the page is empty but the read must succeed.
    assert_eq!(page.total, 0);
}
