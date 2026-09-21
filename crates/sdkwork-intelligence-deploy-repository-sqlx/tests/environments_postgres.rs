//! Environment promotion chain and CI source event integration tests:
//! environment CRUD, chain-enforced promotion with immutable history, and
//! webhook event ingestion deduplicated per commit with default-branch build
//! triggering.
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
    CreateAppEnvironmentRequest, CreateAppRequest, PromoteEnvironmentRequest,
    UpdateAppEnvironmentRequest,
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
        .with_applied_by("sdkwork-deploy-env-test");
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

async fn create_app(repository: &DeployRepository, slug: &str) -> String {
    let app = repository
        .create_app(
            7,
            Some(9),
            Some(11),
            None,
            &CreateAppRequest {
                name: format!("env-app-{slug}"),
                slug: Some(slug.to_owned()),
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
        .expect("create app");
    app.id
}

/// Seeds the release the promotion chain promotes through environments.
///
/// `deploy_release` requires a platform target and a package, and
/// `deploy_package` requires a build, so the whole chain goes through the shared
/// fixture rather than a hand-written insert that omits the required columns.
async fn insert_release(pool: &PgPool, app_internal_id: i64, version: &str) -> String {
    common::seed_delivery_chain(
        pool,
        common::DeliveryChainIds {
            platform_target: 8001,
            build: 8002,
            package: 8003,
            release: 8004,
        },
        app_internal_id,
        version,
        "ACTIVE",
        0,
    )
    .await
    .release_uuid
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn environment_promotion_chain_is_enforced_and_immutable() {
    let repository = migrated_repository().await;
    let app_id = create_app(&repository, "chain").await;

    let staging = repository
        .create_app_environment(
            7,
            Some(11),
            &app_id,
            &CreateAppEnvironmentRequest {
                env_key: "staging".to_owned(),
                env_name: "Staging".to_owned(),
                env_level: "STAGING".to_owned(),
                approval_required: false,
            },
        )
        .await
        .expect("create staging");
    let production = repository
        .create_app_environment(
            7,
            Some(11),
            &app_id,
            &CreateAppEnvironmentRequest {
                env_key: "production".to_owned(),
                env_name: "Production".to_owned(),
                env_level: "PRODUCTION".to_owned(),
                approval_required: true,
            },
        )
        .await
        .expect("create production");
    assert_eq!(staging.env_status, "DRAFT");

    // Duplicate env_key is rejected.
    let duplicate = repository
        .create_app_environment(
            7,
            Some(11),
            &app_id,
            &CreateAppEnvironmentRequest {
                env_key: "staging".to_owned(),
                env_name: "Staging 2".to_owned(),
                env_level: "STAGING".to_owned(),
                approval_required: false,
            },
        )
        .await;
    assert!(duplicate.is_err(), "duplicate env_key must be rejected");

    let app_internal_id: i64 = sqlx::query_scalar("SELECT id FROM deploy_app WHERE uuid = $1")
        .bind(&app_id)
        .fetch_one(repository.pool())
        .await
        .expect("app internal id");
    let release_uuid = insert_release(repository.pool(), app_internal_id, "1.0.0").await;

    // Activate both environments and promote into staging.
    repository
        .update_app_environment(
            7,
            Some(11),
            &app_id,
            &staging.id,
            &UpdateAppEnvironmentRequest {
                env_name: None,
                approval_required: None,
                env_status: Some("ACTIVE".to_owned()),
            },
        )
        .await
        .expect("activate staging");
    let first = repository
        .promote_environment(
            7,
            Some(11),
            &app_id,
            &staging.id,
            &PromoteEnvironmentRequest {
                release_id: release_uuid.clone(),
                from_environment_id: None,
                note: Some("first promotion".to_owned()),
            },
        )
        .await
        .expect("promote to staging");
    assert_eq!(first.environment_key, "staging");
    assert_eq!(first.release_version, "1.0.0");

    // Chain enforcement: promoting to production without the release being
    // current in staging is rejected.
    let skipped = repository
        .promote_environment(
            7,
            Some(11),
            &app_id,
            &production.id,
            &PromoteEnvironmentRequest {
                release_id: release_uuid.clone(),
                from_environment_id: Some("00000000-0000-4000-8000-0000000000aa".to_owned()),
                note: None,
            },
        )
        .await;
    assert!(skipped.is_err(), "chain skip must be rejected");

    // Chain promotion from staging succeeds.
    let chained = repository
        .promote_environment(
            7,
            Some(11),
            &app_id,
            &production.id,
            &PromoteEnvironmentRequest {
                release_id: release_uuid.clone(),
                from_environment_id: Some(staging.id.clone()),
                note: None,
            },
        )
        .await
        .expect("chained promotion to production");
    assert_eq!(chained.from_environment_key.as_deref(), Some("staging"));

    // Promoting the same release again into the same environment is rejected.
    let again = repository
        .promote_environment(
            7,
            Some(11),
            &app_id,
            &production.id,
            &PromoteEnvironmentRequest {
                release_id: release_uuid.clone(),
                from_environment_id: None,
                note: None,
            },
        )
        .await;
    assert!(
        again.is_err(),
        "re-promotion of the current release is rejected"
    );

    // The immutable history shows both promotions.
    let history = repository
        .list_environment_promotions(7, &app_id, &production.id, 1, 20)
        .await
        .expect("list promotions");
    assert_eq!(history.total, 1);
    assert_eq!(history.items[0].release_version, "1.0.0");

    // The environment lists the current release.
    let environments = repository
        .list_app_environments(7, &app_id, 1, 20)
        .await
        .expect("list environments");
    assert_eq!(environments.total, 2);
    let production_view = environments
        .items
        .iter()
        .find(|environment| environment.id == production.id)
        .expect("production present");
    assert_eq!(
        production_view.current_release_version.as_deref(),
        Some("1.0.0")
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn source_events_are_deduplicated_and_skipped_off_default_branch() {
    let repository = migrated_repository().await;
    // A source repository bound to the app.
    let app_id = create_app(&repository, "events").await;
    let repo = repository
        .create_source_repository(
            7,
            &app_id,
            Some(11),
            &sdkwork_deploy_contract::CreateSourceRepositoryRequest {
                repo_key: "main-repo".to_owned(),
                repo_provider: "GITHUB".to_owned(),
                repo_url: "https://github.com/sdkwork/example-app.git".to_owned(),
                default_branch: Some("main".to_owned()),
                clone_mode: Some("SHALLOW".to_owned()),
                credential_secret_ref: None,
                idempotency_key: None,
            },
        )
        .await
        .expect("create source repository");

    let matched = repository
        .match_repository_by_url("https://github.com/sdkwork/example-app")
        .await
        .expect("match repository")
        .expect("repository matched by normalized url");
    assert_eq!(matched.repository_id, repo.id);
    assert_eq!(matched.default_branch, "main");

    // Ingest the same commit twice: the second is a duplicate.
    let (first, fresh) = repository
        .ingest_source_event(
            &matched,
            "PUSH",
            "refs/heads/main",
            "0123456789abcdef0123456789abcdef01234567",
            Some("feat: initial"),
            Some("alice"),
            &"f".repeat(64),
        )
        .await
        .expect("ingest first event");
    assert!(fresh);
    let (second, fresh) = repository
        .ingest_source_event(
            &matched,
            "PUSH",
            "refs/heads/main",
            "0123456789abcdef0123456789abcdef01234567",
            Some("feat: initial"),
            Some("alice"),
            &"f".repeat(64),
        )
        .await
        .expect("ingest duplicate event");
    assert!(!fresh, "redelivered commit must deduplicate");
    assert_eq!(second.id, first.id);

    // A feature-branch push is recorded and skipped (no builds triggered).
    let (feature, _) = repository
        .ingest_source_event(
            &matched,
            "PUSH",
            "refs/heads/feature-x",
            "abcdef0123456789abcdef0123456789abcdef01",
            Some("feat: branch"),
            Some("bob"),
            &"e".repeat(64),
        )
        .await
        .expect("ingest feature event");
    repository
        .update_source_event_result(matched.tenant_id, &feature.id, false, 0, None)
        .await
        .expect("mark skipped");
    let feature_view = repository
        .list_source_events(Some(7), 1, 20)
        .await
        .expect("list events");
    assert_eq!(feature_view.total, 2);

    // An unmatched repository URL yields no match.
    let missing = repository
        .match_repository_by_url("https://github.com/unknown/other")
        .await
        .expect("match query");
    assert!(missing.is_none(), "unbound repository must not match");
}
