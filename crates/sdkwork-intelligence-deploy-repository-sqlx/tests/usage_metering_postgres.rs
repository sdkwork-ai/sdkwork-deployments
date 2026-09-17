//! Usage metering repository integration tests (TECH §4.6): deduplication
//! idempotency, tenant-scoped pagination, and period/dimension attribution on
//! the forward-migrated contract schema.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like the
//! other PostgreSQL integration tests in this crate.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use sdkwork_database_config::DatabaseConfig;
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::DefaultDatabaseModule;
use sdkwork_database_sqlx::DatabasePool;
use sdkwork_deploy_contract::UsageEventQuery;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::repository::InsertUsageEventCommand;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

/// The deploy module lives at the sdkwork-deployments repository root.
fn deploy_module() -> Arc<DefaultDatabaseModule> {
    let app_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Arc::new(DefaultDatabaseModule::from_app_root(&app_root).expect("load deploy database module"))
}

fn database_pool(pool: PgPool) -> DatabasePool {
    // The migration lock opens its own connection from this config, so it has to
    // describe the pool that is actually in use. `DatabaseConfig::default()` is a
    // SQLite configuration with an empty URL, which made every test in this file
    // fail with `migration_lock_open_failed (postgres): relative URL without a
    // base` before reaching a single assertion.
    let url = std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL").unwrap_or_default();
    DatabasePool::Postgres(
        pool,
        sdkwork_database_sqlx::PoolContext {
            config: DatabaseConfig {
                engine: sdkwork_database_config::DatabaseEngine::Postgres,
                url,
                ..DatabaseConfig::default()
            },
        },
    )
}

/// Bootstraps a fresh schema through the full init + migrate pipeline so the
/// metering tables exist exactly as the forward migrations define them.
async fn migrated_repository() -> DeployRepository {
    let pool = common::postgres_schema_pool().await;
    let module = deploy_module();
    let orchestrator = LifecycleOrchestrator::new(database_pool(pool.clone()), module.clone())
        .with_applied_by("sdkwork-deploy-usage-test");
    orchestrator
        .init()
        .await
        .expect("init on an empty schema must bootstrap the baseline");
    orchestrator
        .migrate()
        .await
        .expect("migrate must apply the full forward migration chain");
    DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(4).expect("Snowflake generator"),
        common::test_secret_key(),
    )
}

fn usage_command(tenant_id: i64, deduplication_key: &str) -> InsertUsageEventCommand {
    InsertUsageEventCommand {
        tenant_id,
        organization_id: 9,
        app_id: None,
        binding_id: None,
        attribution: None,
        period_start: "2026-08-01T00:00:00.000Z".to_owned(),
        dimension: "build_minutes".to_owned(),
        quantity: 7,
        unit: "MINUTES".to_owned(),
        source_target_uuid: Some("target-uuid-1".to_owned()),
        source_window_id: Some(format!("build:{deduplication_key}")),
        deduplication_key: deduplication_key.to_owned(),
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn usage_event_insert_is_idempotent_on_dedup_key() {
    let repository = migrated_repository().await;
    let first = repository
        .insert_usage_event(&usage_command(7, "build:build-42"))
        .await
        .expect("insert usage fact");
    assert_eq!(first.dimension, "build_minutes");
    assert_eq!(first.quantity, 7);
    assert_eq!(first.period_start, "2026-08-01T00:00:00.000Z");

    // Replay with the same tenant dedup key must return the same fact.
    let replay = repository
        .insert_usage_event(&usage_command(7, "build:build-42"))
        .await
        .expect("idempotent replay");
    assert_eq!(replay.id, first.id, "replay must return the original fact");

    let page = repository
        .list_usage_events(
            7,
            &UsageEventQuery {
                page: 1,
                page_size: 20,
                ..Default::default()
            },
        )
        .await
        .expect("list usage facts");
    assert_eq!(page.total, 1, "replay must not duplicate the fact");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, first.id);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn usage_events_are_tenant_scoped_and_paginated() {
    let repository = migrated_repository().await;
    repository
        .insert_usage_event(&usage_command(7, "build:build-1"))
        .await
        .expect("insert tenant 7 fact");
    repository
        .insert_usage_event(&usage_command(7, "build:build-2"))
        .await
        .expect("insert second tenant 7 fact");
    // A different tenant must never observe the facts above.
    repository
        .insert_usage_event(&usage_command(8, "build:build-3"))
        .await
        .expect("insert tenant 8 fact");

    let page = repository
        .list_usage_events(
            7,
            &UsageEventQuery {
                page: 1,
                page_size: 1,
                ..Default::default()
            },
        )
        .await
        .expect("list tenant 7 page");
    assert_eq!(page.total, 2);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.page, 1);
    assert_eq!(page.page_size, 1);
    assert!(page.items[0].deduplication_key.starts_with("build:build-"));

    let other = repository
        .list_usage_events(
            8,
            &UsageEventQuery {
                page: 1,
                page_size: 20,
                ..Default::default()
            },
        )
        .await
        .expect("list tenant 8");
    assert_eq!(other.total, 1);
    assert_eq!(other.items[0].deduplication_key, "build:build-3");
}
