//! Identity-provisioning repository contract on the forward-migrated schema.
//!
//! What only a real database can answer: that `retrieve_app_identity_status` is
//! side-effect free (it must not claim a key on the `active` + `NULL` anomaly
//! row the reconciliation runbook reports), that a retry replays the key an
//! earlier attempt stored, and that an unknown or foreign-tenant application is
//! `not_found` rather than a fabricated status.
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
use sdkwork_deploy_contract::{AppKind, CreateAppRequest, DeployServiceErrorKind};
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
    // describe the pool that is actually in use.
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
    let orchestrator = LifecycleOrchestrator::new(database_pool(pool.clone()), module)
        .with_applied_by("sdkwork-deploy-identity-provisioning-test");
    orchestrator
        .init()
        .await
        .expect("init on an empty schema must bootstrap the baseline");
    // The identity columns arrive in migration 0008, not in the baseline DDL, so
    // the full forward chain is required here.
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
    repository
        .create_app(
            tenant_id,
            Some(9),
            Some(11),
            None,
            &CreateAppRequest {
                name: format!("identity-app-{slug}"),
                slug: Some(slug.to_owned()),
                app_kind: AppKind::ApiService,
                app_type: Some(2),
                runtime_config: None,
                metadata: None,
                description: None,
                default_environment: None,
                app_domain_label: None,
                app_domain_suffixes: None,
                owner_type: None,
                source_specs: None,
                idempotency_key: None,
            },
        )
        .await
        .expect("create app")
        .id
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn identity_status_read_does_not_claim_a_key_and_retries_replay_it() {
    let repository = migrated_repository().await;
    let app_id = create_app(&repository, 7, "identity-retry").await;

    // Fresh rows start pending and are readable without any provisioning call.
    assert_eq!(
        repository
            .retrieve_app_identity_status(7, &app_id)
            .await
            .expect("read fresh identity status"),
        "pending_identity"
    );

    let first = repository
        .begin_app_identity_provisioning(7, &app_id)
        .await
        .expect("claim provisioning key");
    assert_eq!(
        first.provisioning_idempotency_key,
        format!("deploy:{}", first.deploy_application_id)
    );

    repository
        .record_app_identity_status(7, &app_id, "failed")
        .await
        .expect("record failed attempt");
    assert_eq!(
        repository
            .retrieve_app_identity_status(7, &app_id)
            .await
            .expect("read failed identity status"),
        "failed"
    );

    // A retry replays the stored key; it must never mint a second installation
    // identity for the same application.
    let retried = repository
        .begin_app_identity_provisioning(7, &app_id)
        .await
        .expect("replay provisioning key");
    assert_eq!(
        retried.provisioning_idempotency_key, first.provisioning_idempotency_key,
        "a retry must send IAM the key the original attempt persisted"
    );
    assert_eq!(retried.identity_status, "failed");

    // Reading the status is side-effect free: it still does not claim anything.
    assert_eq!(
        repository
            .retrieve_app_identity_status(7, &app_id)
            .await
            .expect("read status after retry"),
        "failed"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn identity_status_read_rejects_unknown_and_foreign_tenant_apps() {
    let repository = migrated_repository().await;
    let app_id = create_app(&repository, 7, "identity-scope").await;

    let unknown = repository
        .retrieve_app_identity_status(7, "0f8b7f26-2d7c-4f43-8a4b-0c5a51e1f0aa")
        .await
        .expect_err("an unknown application must not produce a status");
    assert_eq!(unknown.kind(), DeployServiceErrorKind::NotFound);

    let foreign = repository
        .retrieve_app_identity_status(8, &app_id)
        .await
        .expect_err("another tenant must not read this application");
    assert_eq!(foreign.kind(), DeployServiceErrorKind::NotFound);
}
