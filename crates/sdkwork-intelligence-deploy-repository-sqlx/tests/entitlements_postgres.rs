//! Entitlement enforcement and backend fleet administration integration
//! tests: capacity creation is gated by the Commerce-backed projection when
//! enforcement is enabled, and the platform management read surfaces
//! (projections, build queue, runner health) return tenant-scoped data.
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
use sdkwork_deploy_contract::{CreateAppRequest, DeployAppRequestContext, DeployServiceErrorKind};
use sdkwork_deploy_core::env_test_lock;
use sdkwork_deploy_drive_port::MemoryDeployDrivePort;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::{DeployRepositoryPort, DeployService};
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
        .with_applied_by("sdkwork-deploy-entitlement-test");
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

async fn insert_entitlement_projection(
    repository: &DeployRepository,
    tenant_id: i64,
    limits_json: &str,
) {
    sqlx::query(
        "INSERT INTO deploy_tenant_entitlement_projection
            (id, uuid, tenant_id, organization_id, source_system, source_subscription_uuid,
             source_revision, plan_key, entitlements_json, effective_at, projection_status)
         VALUES (1001, '00000000-0000-4000-8000-000000000001', $1, 0, 'commerce', 'sub-1',
                 'rev-1', 'pro', $2::jsonb, NOW() - INTERVAL '1 day', 'ACTIVE')",
    )
    .bind(tenant_id)
    .bind(limits_json)
    .execute(repository.pool())
    .await
    .expect("insert entitlement projection");
}

fn app_request(name: &str) -> CreateAppRequest {
    CreateAppRequest {
        name: name.to_owned(),
        slug: Some(name.to_owned()),
        app_kind: sdkwork_deploy_contract::AppKind::ApiService,
        app_type: Some(2),
        runtime_config: None,
        metadata: None,
        description: None,
        default_environment: None,
        app_domain_label: None,
        app_domain_suffixes: None,
        idempotency_key: None,
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn entitlement_enforcement_blocks_capacity_over_the_limit() {
    let _lock = env_test_lock();
    std::env::set_var("SDKWORK_DEPLOY_ENTITLEMENT_ENFORCEMENT", "on");
    let repository = Arc::new(migrated_repository().await);

    // Capacity enforcement is a service-layer gate. `DeployService::create_app`
    // is the only entry point production code uses, and it consults the
    // Commerce-backed projection before delegating to the repository; the
    // repository itself never gates. Asserting against `DeployRepository`
    // directly would therefore test a layer that is not supposed to enforce,
    // and would pass vacuously no matter how broken the gate is.
    let service = DeployService::new(repository.clone(), Arc::new(MemoryDeployDrivePort));

    let context = |tenant_id: i64| DeployAppRequestContext {
        tenant_id,
        actor_id: Some(11),
        organization_id: Some(9),
        ..DeployAppRequestContext::default()
    };

    // Tenant 7 has a plan limiting it to one active app; the first app fits.
    insert_entitlement_projection(&repository, 7, r#"{"active_apps": 1}"#).await;
    let first = service
        .create_app(&context(7), None, &app_request("first-app"))
        .await
        .expect("first app within the plan limit");

    // The second app exceeds the plan: quota exceeded (429 semantics).
    let second = service
        .create_app(&context(7), None, &app_request("second-app"))
        .await;
    let error = second.expect_err("second app must exceed the plan limit");
    assert_eq!(error.kind(), DeployServiceErrorKind::QuotaExceeded);
    assert!(error.to_string().contains("active_apps"));

    // A tenant without any projection fails closed when enforcement is on.
    let unplanned = service
        .create_app(&context(8), None, &app_request("unplanned-app"))
        .await;
    let error = unplanned.expect_err("unplanned tenant must fail closed");
    assert_eq!(error.kind(), DeployServiceErrorKind::Forbidden);

    let _ = first;
    std::env::remove_var("SDKWORK_DEPLOY_ENTITLEMENT_ENFORCEMENT");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn entitlement_usage_aggregates_and_management_surfaces_work() {
    let _lock = env_test_lock();
    std::env::remove_var("SDKWORK_DEPLOY_ENTITLEMENT_ENFORCEMENT");
    let repository = migrated_repository().await;
    insert_entitlement_projection(&repository, 7, r#"{"active_apps": 5}"#).await;

    let app = repository
        .create_app(
            7,
            Some(9),
            Some(11),
            None,
            &CreateAppRequest {
                name: "usage-app".to_owned(),
                slug: Some("usage-app".to_owned()),
                app_kind: sdkwork_deploy_contract::AppKind::ApiService,
                app_type: Some(2),
                runtime_config: None,
                metadata: None,
                description: None,
                default_environment: None,
                app_domain_label: None,
                app_domain_suffixes: None,
                idempotency_key: None,
            },
        )
        .await
        .expect("create app with enforcement off");

    let usage = repository
        .entitlement_usage(7, "active_apps")
        .await
        .expect("active app aggregate");
    assert_eq!(usage, 1);
    // Unknown dimension fails closed.
    assert!(repository.entitlement_usage(7, "bogus").await.is_err());

    let projections = repository
        .list_entitlement_projections(Some(7), 1, 20)
        .await
        .expect("tenant projections");
    assert_eq!(projections.total, 1);
    assert_eq!(projections.items[0].plan_key.as_deref(), Some("pro"));

    let queue = repository
        .list_build_queue(None, 1, 20)
        .await
        .expect("platform-wide build queue");
    assert_eq!(queue.total, 0);

    let runners = repository
        .list_runner_health(1, 20)
        .await
        .expect("runner health");
    assert_eq!(runners.total, 0);
    let _ = app;
}
