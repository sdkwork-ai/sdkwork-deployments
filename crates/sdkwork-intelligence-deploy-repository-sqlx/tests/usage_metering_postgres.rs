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

/// Regression: ingesting a traffic usage event attributed by `bindingUuid` (or
/// by `appUuid`) failed with a masked 500 because `resolve_usage_attribution`
/// selected `app.app_id` from `deploy_app` — a column that table never had (the
/// app's internal id is `id`; `app_id` is what *bindings* and *packages* carry).
///
/// Both attribution branches build their subquery as a raw SQL literal, so only
/// a call that actually enters `insert_usage_events_batch` catches a typo here.
/// The stored `tenant_id`/`app_id` are asserted so the test fails on the
/// attribution result, not merely on the absence of an error.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn usage_batch_ingest_resolves_app_uuid_through_binding_and_app_paths() {
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-usage-attribution-test").await;

    let app_id: i64 = 359_302_751_877_861_376;
    let app_uuid = "11111111-1111-4111-8111-111111111111";
    let binding_uuid = "22222222-2222-4222-8222-222222222222";
    let tenant_id: i64 = 100_001;

    // `sqlx 0.9` only accepts `&'static str` for `raw_sql`, and this statement is
    // formatted from test-local constants. It is audited: every interpolated
    // value is a literal defined above (two uuids, a tenant id, an app id) or a
    // `format!`-escaped `{{}}`; nothing originates from outside the test.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "INSERT INTO deploy_app (id, uuid, tenant_id, organization_id, name, slug, app_kind,
                                 app_status, type, runtime_config, metadata, data_scope,
                                 default_environment, created_at, updated_at, version)
         VALUES ({app_id}, '{app_uuid}', {tenant_id}, 0, 'Usage App', 'usage-app', 'SITE',
                 'ACTIVE', 1, '{{}}', '{{}}', 1, 'production', NOW(), NOW(), 1);

         INSERT INTO deploy_dns_zone (id, uuid, tenant_id, apex_hostname, status, created_at, updated_at)
         VALUES (900001, '33333333-3333-4333-8333-333333333333', {tenant_id},
                 'usage.example.com', 'ACTIVE', NOW(), NOW());

         INSERT INTO deploy_domain (id, uuid, tenant_id, zone_id, hostname_ascii, hostname_type,
                                    verification_status, verified_at, status, created_at, updated_at)
         VALUES (900002, '44444444-4444-4444-8444-444444444444', {tenant_id}, 900001,
                 'shop.usage.sdkwork.com', 'EXACT', 'VERIFIED', NOW(), 'ACTIVE', NOW(), NOW());

         INSERT INTO deploy_app_binding
             (id, uuid, tenant_id, app_id, binding_key, domain_id, hostname_ascii, environment,
              path_prefix, is_canonical, status, created_at, updated_at)
         VALUES (900003, '{binding_uuid}', {tenant_id}, {app_id}, 'appd-usage-app', 900002,
                 'shop.usage.sdkwork.com', 'production', '/', TRUE, 'ACTIVE', NOW(), NOW());"
    )))
    .execute(&pool)
    .await
    .expect("seed app, zone, domain, and binding");

    let traffic_event = |binding: Option<&str>, app: Option<&str>, key: &str| {
        sdkwork_deploy_contract::UsageEventIngestItem {
            // `0` means "unmanaged": the control plane has to resolve the tenant
            // from the binding/app, which is exactly the code path under test.
            tenant_id: 0,
            organization_id: 9,
            app_uuid: app.map(str::to_owned),
            binding_uuid: binding.map(str::to_owned),
            period_start: "2026-08-01T00:00:00.000Z".to_owned(),
            dimension: "traffic.requests".to_owned(),
            quantity: 3,
            unit: "REQUESTS".to_owned(),
            deduplication_key: key.to_owned(),
            attribution: Default::default(),
            observed_at: "2026-08-01T00:05:00.000Z".to_owned(),
        }
    };

    // Branch A: the binding uuid is authoritative and resolves the app uuid
    // through `deploy_app.id = deploy_app_binding.app_id`.
    let via_binding = repository
        .insert_usage_events_batch(&[traffic_event(
            Some(binding_uuid),
            None,
            "traffic:w1:via-binding",
        )])
        .await
        .expect("binding-attributed batch ingest must not fail");
    assert_eq!(
        via_binding.ingested, 1,
        "the binding-attributed event is ingested"
    );
    assert_eq!(via_binding.rejected, 0);

    // Branch B: with no binding, the app uuid resolves through
    // `deploy_app.id = deploy_app.id` (the same typo, second copy).
    let via_app = repository
        .insert_usage_events_batch(&[traffic_event(None, Some(app_uuid), "traffic:w1:via-app")])
        .await
        .expect("app-attributed batch ingest must not fail");
    assert_eq!(via_app.ingested, 1, "the app-attributed event is ingested");
    assert_eq!(via_app.rejected, 0);

    // Attribution actually landed: both events carry the resolved internal ids.
    let rows = sqlx::raw_sql(
        "SELECT deduplication_key, tenant_id, app_id, binding_id FROM deploy_usage_event
         ORDER BY deduplication_key",
    )
    .fetch_all(&pool)
    .await
    .expect("read ingested usage events");
    assert_eq!(rows.len(), 2, "both events were persisted");
    use sqlx::Row;
    for row in &rows {
        let tenant: i64 = row.try_get("tenant_id").expect("tenant_id");
        assert_eq!(tenant, tenant_id, "tenant is resolved from the binding/app");
        let stored_app: Option<i64> = row.try_get("app_id").expect("app_id");
        assert_eq!(stored_app, Some(app_id), "app internal id is attributed");
    }
    // The binding-attributed event additionally pins the binding internal id;
    // the app-attributed one has no binding to resolve.
    let via_binding_row = rows
        .iter()
        .find(|row| {
            row.try_get::<String, _>("deduplication_key")
                .map(|key| key == "traffic:w1:via-binding")
                .unwrap_or(false)
        })
        .expect("the binding-attributed event is listed");
    let stored_binding: Option<i64> = via_binding_row.try_get("binding_id").expect("binding_id");
    assert_eq!(stored_binding, Some(900_003), "binding internal id is attributed");
    for row in &rows {
        let tenant: i64 = row.try_get("tenant_id").expect("tenant_id");
        assert_eq!(tenant, tenant_id, "tenant is resolved from the binding/app");
        let stored_app: Option<i64> = row.try_get("app_id").expect("app_id");
        assert_eq!(stored_app, Some(app_id), "app internal id is attributed");
    }
}
