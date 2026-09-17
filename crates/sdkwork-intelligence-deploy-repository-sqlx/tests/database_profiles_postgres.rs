//! Application database structure contract integration tests: profile
//! lifecycle (create/retrieve/update), migration definition binding with
//! checksum validation, and tenant scoping on the forward-migrated schema.
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
use sdkwork_deploy_contract::{
    CreateAppDatabaseMigrationRequest, CreateAppDatabaseProfileRequest, CreateAppRequest,
    UpdateAppDatabaseProfileRequest,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
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

async fn migrated_repository() -> DeployRepository {
    let pool = common::postgres_schema_pool().await;
    let module = deploy_module();
    let orchestrator = LifecycleOrchestrator::new(database_pool(pool.clone()), module.clone())
        .with_applied_by("sdkwork-deploy-db-profile-test");
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

async fn create_app(repository: &DeployRepository, tenant_id: i64, slug: &str) -> String {
    let app = repository
        .create_app(
            tenant_id,
            Some(9),
            Some(11),
            &CreateAppRequest {
                name: format!("db-profile-app-{slug}"),
                slug: Some(slug.to_owned()),
                app_kind: sdkwork_deploy_contract::AppKind::ApiService,
                app_type: Some(2),
                runtime_config: None,
                description: None,
                default_environment: None,
                idempotency_key: None,
            },
        )
        .await
        .expect("create app");
    app.id
}

fn profile_request(profile_key: &str) -> CreateAppDatabaseProfileRequest {
    CreateAppDatabaseProfileRequest {
        profile_key: profile_key.to_owned(),
        db_engine: "POSTGRES".to_owned(),
        catalog_name: "sdkwork_app_prod".to_owned(),
        schema_version: Some("1.0.0".to_owned()),
        baseline_version: Some("0001".to_owned()),
        migration_strategy: Some("VERSIONED".to_owned()),
    }
}

fn migration_request(version: &str) -> CreateAppDatabaseMigrationRequest {
    CreateAppDatabaseMigrationRequest {
        migration_version: version.to_owned(),
        migration_name: "create_users_table".to_owned(),
        checksum_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_owned(),
        script_ref: Some("drive://migrations/0001_create_users_table.sql".to_owned()),
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn database_profile_lifecycle_binds_migrations_to_the_app() {
    let repository = migrated_repository().await;
    let app_id = create_app(&repository, 7, "profile-app").await;

    let profile = repository
        .create_app_database_profile(7, Some(11), &app_id, &profile_request("primary"))
        .await
        .expect("create profile");
    assert_eq!(profile.db_engine, "POSTGRES");
    assert_eq!(profile.catalog_name, "sdkwork_app_prod");
    assert_eq!(profile.profile_status, "DRAFT");
    assert_eq!(profile.migration_count, 0);

    let migration = repository
        .create_app_database_migration(
            7,
            Some(11),
            &app_id,
            &profile.id,
            &migration_request("0001"),
        )
        .await
        .expect("create migration");
    assert_eq!(migration.migration_version, "0001");
    assert_eq!(migration.migration_status, "PENDING");

    // Duplicate migration version on the same profile is rejected.
    let duplicate = repository
        .create_app_database_migration(
            7,
            Some(11),
            &app_id,
            &profile.id,
            &migration_request("0001"),
        )
        .await;
    assert!(
        duplicate.is_err(),
        "duplicate migration version must be rejected"
    );

    let listed = repository
        .list_app_database_migrations(7, &app_id, &profile.id, 1, 20)
        .await
        .expect("list migrations");
    assert_eq!(listed.total, 1);
    assert_eq!(listed.items[0].checksum_sha256.len(), 64);

    // Profile count reflects the migration, and update transitions status.
    let retrieved = repository
        .retrieve_app_database_profile(7, &app_id, &profile.id)
        .await
        .expect("retrieve profile");
    assert_eq!(retrieved.migration_count, 1);

    let updated = repository
        .update_app_database_profile(
            7,
            Some(11),
            &app_id,
            &profile.id,
            &UpdateAppDatabaseProfileRequest {
                schema_version: Some("1.1.0".to_owned()),
                baseline_version: None,
                migration_strategy: None,
                profile_status: Some("ACTIVE".to_owned()),
            },
        )
        .await
        .expect("update profile");
    assert_eq!(updated.schema_version.as_deref(), Some("1.1.0"));
    assert_eq!(updated.profile_status, "ACTIVE");

    let profiles = repository
        .list_app_database_profiles(7, &app_id, 1, 20)
        .await
        .expect("list profiles");
    assert_eq!(profiles.total, 1);
    assert_eq!(profiles.items[0].id, profile.id);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn database_profiles_are_tenant_scoped() {
    let repository = migrated_repository().await;
    let app_id = create_app(&repository, 7, "scoped-app").await;

    let profile = repository
        .create_app_database_profile(7, Some(11), &app_id, &profile_request("primary"))
        .await
        .expect("create profile for tenant 7");

    // Cross-tenant read of the same profile uuid fails closed.
    let cross_tenant = repository
        .retrieve_app_database_profile(8, &app_id, &profile.id)
        .await;
    assert!(cross_tenant.is_err(), "cross-tenant read must fail closed");

    // Cross-tenant migration creation against the profile is rejected.
    let cross_tenant_migration = repository
        .create_app_database_migration(
            8,
            Some(11),
            &app_id,
            &profile.id,
            &migration_request("0001"),
        )
        .await;
    assert!(
        cross_tenant_migration.is_err(),
        "cross-tenant migration must fail closed"
    );
}
