//! Retention, usage reconciliation, and signing identity health integration
//! tests (PRD §5.8, TECH §8): dry-run and real retention runs, the
//! idempotent daily aggregate rebuild from retained usage facts, and the
//! expiry health surface.
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
use sdkwork_deploy_contract::{CreateSigningIdentityRequest, SigningKind};
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
        .with_applied_by("sdkwork-deploy-retention-test");
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

/// The rows the fixture created, so assertions can address them by id without
/// repeating magic numbers.
struct RetentionFixture {
    /// The unreferenced package: the retention candidate.
    package_id: i64,
    /// The package the release references: retention must leave it alone.
    referenced_package_id: i64,
    release_id: i64,
}

/// Seeds a minimal app holding an old **unreferenced** package (the retention
/// candidate) alongside an old release.
///
/// `deploy_release.package_id` is NOT NULL and carries a foreign key, so the
/// release has to point at a package. It points at its own package, which keeps
/// the package under assertion unreferenced — exactly the condition package
/// retention looks for.
async fn seed_old_package_and_release(pool: &PgPool) -> RetentionFixture {
    const APP_ID: i64 = 5001;
    sqlx::query(
        "INSERT INTO deploy_app (id, uuid, tenant_id, organization_id, name, slug, app_kind,
             app_status, created_at, updated_at)
         VALUES ($1, '00000000-0000-4000-8000-000000000002', 7, 9, 'retention-app', 'retention-app',
                 'API_SERVICE', 'ACTIVE', NOW() - INTERVAL '400 days', NOW() - INTERVAL '400 days')",
    )
    .bind(APP_ID)
    .execute(pool)
    .await
    .expect("insert app");

    // The chain the release needs, seeded through the shared fixture because
    // deploy_release requires a platform target and a package, and
    // deploy_package requires a build.
    let chain = common::seed_delivery_chain(
        pool,
        common::DeliveryChainIds {
            platform_target: 5004,
            build: 5005,
            package: 5006,
            release: 5003,
        },
        APP_ID,
        "1.0.0",
        "ACTIVE",
        400,
    )
    .await;

    // The retention candidate: old, READY, and referenced by nothing.
    let package_id: i64 = 5002;
    common::seed_package(
        pool,
        package_id,
        APP_ID,
        chain.platform_target_id,
        chain.build_id,
        "0.9.0",
        400,
    )
    .await;

    RetentionFixture {
        package_id,
        referenced_package_id: chain.package_id,
        release_id: chain.release_id,
    }
}

async fn package_status(pool: &PgPool, package_id: i64) -> String {
    sqlx::query_scalar("SELECT package_status FROM deploy_package WHERE id = $1")
        .bind(package_id)
        .fetch_one(pool)
        .await
        .expect("package status")
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn retention_dry_run_reports_and_real_run_retires() {
    let repository = migrated_repository().await;
    let fixture = seed_old_package_and_release(repository.pool()).await;

    // Dry run reports both candidates without mutating.
    let dry = repository
        .run_retention(true, 365, 365, 365)
        .await
        .expect("dry run retention");
    assert_eq!(dry.packages_retired, 1);
    assert_eq!(dry.releases_retired, 1);
    assert_eq!(dry.build_logs_purged, 0);
    assert_eq!(
        package_status(repository.pool(), fixture.package_id).await,
        "READY",
        "dry run must not mutate"
    );

    // Real run retires both; a second real run finds nothing left.
    let real = repository
        .run_retention(false, 365, 365, 365)
        .await
        .expect("real retention run");
    assert_eq!(real.packages_retired, 1);
    assert_eq!(real.releases_retired, 1);
    assert_eq!(
        package_status(repository.pool(), fixture.package_id).await,
        "RETIRED"
    );
    // A package a release still references is immutable build evidence and is
    // never retired, however old it is.
    assert_eq!(
        package_status(repository.pool(), fixture.referenced_package_id).await,
        "READY",
        "a referenced package must survive retention"
    );
    let release_status: String =
        sqlx::query_scalar("SELECT release_status FROM deploy_release WHERE id = $1")
            .bind(fixture.release_id)
            .fetch_one(repository.pool())
            .await
            .expect("release status");
    assert_eq!(release_status, "RETIRED");

    let again = repository
        .run_retention(false, 365, 365, 365)
        .await
        .expect("second retention run");
    assert_eq!(again.packages_retired, 0);
    assert_eq!(again.releases_retired, 0);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn usage_daily_rebuild_is_idempotent_and_reconcilable() {
    let repository = migrated_repository().await;
    const APP_ID: i64 = 6101;
    sqlx::query(
        "INSERT INTO deploy_app (id, uuid, tenant_id, organization_id, name, slug, app_kind,
             app_status, created_at, updated_at)
         VALUES ($1, '00000000-0000-4000-8000-000000000010', 7, 9, 'usage-app', 'usage-app',
                 'API_SERVICE', 'ACTIVE', NOW(), NOW())",
    )
    .bind(APP_ID)
    .execute(repository.pool())
    .await
    .expect("insert app");

    // Two app-attributed facts for the same tenant/app/dimension/date, so the
    // aggregate has to sum them, plus one fact with no app attribution at all.
    // `deploy_app_usage_daily.app_id` is NOT NULL with a foreign key to
    // `deploy_app`, so an unmanaged fact can only ever reach the tenant rollup
    // — that boundary is what this test pins down.
    for (idx, app_id, quantity) in [
        (1, Some(APP_ID), 7_i64),
        (2, Some(APP_ID), 3_i64),
        (3, None, 5_i64),
    ] {
        sqlx::query(
            "INSERT INTO deploy_usage_event
                (id, uuid, tenant_id, organization_id, app_id, period_start, dimension,
                 quantity, unit, deduplication_key, observed_at, ingested_at, created_at)
             VALUES ($1, $2, 7, 9, $3, NOW() - INTERVAL '1 day', 'build_minutes', $4, 'MINUTES',
                     $5, NOW(), NOW(), NOW())",
        )
        .bind(6000 + idx)
        .bind(format!("00000000-0000-4000-8000-00000000000{}", idx + 4))
        .bind(app_id)
        .bind(quantity)
        .bind(format!("build:retention-{idx}"))
        .execute(repository.pool())
        .await
        .expect("insert usage fact");
    }

    let window_start = "2026-01-01T00:00:00.000Z";
    let window_end = "2030-01-01T00:00:00.000Z";
    let first = repository
        .rebuild_usage_daily(Some(window_start), Some(window_end))
        .await
        .expect("rebuild daily");
    // One row per rollup: the app-attributed day and the tenant-wide day.
    assert_eq!(first.rebuilt_rows, 2);

    // The app rollup summed the two app-attributed facts — and only those.
    let app_quantity: i64 = sqlx::query_scalar(
        "SELECT quantity FROM deploy_app_usage_daily
         WHERE tenant_id = 7 AND app_id = $1 AND dimension = 'build_minutes'
           AND usage_date = (NOW() - INTERVAL '1 day')::date",
    )
    .bind(APP_ID)
    .fetch_one(repository.pool())
    .await
    .expect("app daily quantity");
    assert_eq!(app_quantity, 10, "app facts must sum into the app rollup");

    // The tenant rollup is the superset: every fact in the window, including
    // the one no app attributes.
    let tenant_quantity: i64 = sqlx::query_scalar(
        "SELECT quantity FROM deploy_tenant_usage_daily
         WHERE tenant_id = 7 AND dimension = 'build_minutes'
           AND usage_date = (NOW() - INTERVAL '1 day')::date",
    )
    .fetch_one(repository.pool())
    .await
    .expect("tenant daily quantity");
    assert_eq!(tenant_quantity, 15, "the tenant rollup carries every fact");

    // Idempotent rebuild: the same rows, upserted rather than duplicated.
    let second = repository
        .rebuild_usage_daily(Some(window_start), Some(window_end))
        .await
        .expect("rebuild daily again");
    assert_eq!(second.rebuilt_rows, 2);
    let app_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM deploy_app_usage_daily WHERE tenant_id = 7")
            .fetch_one(repository.pool())
            .await
            .expect("app daily row count");
    assert_eq!(app_count, 1, "rebuild must upsert, never duplicate");
    let tenant_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM deploy_tenant_usage_daily WHERE tenant_id = 7")
            .fetch_one(repository.pool())
            .await
            .expect("tenant daily row count");
    assert_eq!(tenant_count, 1, "rebuild must upsert, never duplicate");

    // A finalized day is billing evidence. Rebinding the same facts must leave
    // it finalized; the entitlement surface reads exactly the PENDING rows, so
    // an unconditional reset would reopen a closed period, and never resetting
    // would let the rebuild rewrite a number that has already been billed.
    sqlx::query(
        "UPDATE deploy_tenant_usage_daily
         SET finalization_status = 'FINALIZED', finalized_at = NOW()
         WHERE tenant_id = 7",
    )
    .execute(repository.pool())
    .await
    .expect("mark the tenant day finalized");
    repository
        .rebuild_usage_daily(Some(window_start), Some(window_end))
        .await
        .expect("rebuild over a finalized day");
    let unchanged_status: String = sqlx::query_scalar(
        "SELECT finalization_status FROM deploy_tenant_usage_daily WHERE tenant_id = 7",
    )
    .fetch_one(repository.pool())
    .await
    .expect("finalization status after an unchanged rebuild");
    assert_eq!(
        unchanged_status, "FINALIZED",
        "an unchanged rebuild must not reopen a closed day"
    );

    // Move a fact, then rebuild: the aggregate changed, so the day reopens.
    sqlx::query("UPDATE deploy_usage_event SET quantity = 99 WHERE id = 6003")
        .execute(repository.pool())
        .await
        .expect("move a usage fact");
    repository
        .rebuild_usage_daily(Some(window_start), Some(window_end))
        .await
        .expect("rebuild after the facts moved");
    let (reopened_status, reopened_quantity): (String, i64) = sqlx::query_as(
        "SELECT finalization_status, quantity FROM deploy_tenant_usage_daily WHERE tenant_id = 7",
    )
    .fetch_one(repository.pool())
    .await
    .expect("reopened tenant day");
    assert_eq!(reopened_quantity, 109, "7 + 3 + 99");
    assert_eq!(
        reopened_status, "PENDING",
        "a changed aggregate must reopen the day"
    );

    // Invalid windows fail closed.
    assert!(repository
        .rebuild_usage_daily(Some("not-a-date"), None)
        .await
        .is_err());
    assert!(repository
        .rebuild_usage_daily(
            Some("2030-01-01T00:00:00.000Z"),
            Some("2026-01-01T00:00:00.000Z")
        )
        .await
        .is_err());
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn signing_identity_health_reports_expiry_urgency() {
    let repository = migrated_repository().await;
    sqlx::query(
        "INSERT INTO deploy_signing_identity
            (id, uuid, tenant_id, organization_id, identity_name, signing_kind, expires_at,
             identity_status, created_at, updated_at)
         VALUES (7001, '00000000-0000-4000-8000-000000000009', 7, 9, 'prod-pfx',
                 'WINDOWS_AUTHENTICODE', NOW() + INTERVAL '30 days', 'VALID',
                 NOW(), NOW())",
    )
    .execute(repository.pool())
    .await
    .expect("insert signing identity");

    let page = repository
        .list_signing_identity_health(Some(7), 1, 20)
        .await
        .expect("list signing identity health");
    assert_eq!(page.total, 1);
    let item = &page.items[0];
    assert_eq!(item.signing_kind, "WINDOWS_AUTHENTICODE");
    assert_eq!(item.identity_status, "VALID");
    let days = item.days_until_expiry.expect("days until expiry");
    assert!(
        (20..=40).contains(&days),
        "expiry urgency in the expected window: {days}"
    );

    // Cross-tenant scope returns nothing.
    let other = repository
        .list_signing_identity_health(Some(8), 1, 20)
        .await
        .expect("other tenant health");
    assert_eq!(other.total, 0);

    // Platform-wide (None) sees the tenant's identity.
    let platform = repository
        .list_signing_identity_health(None, 1, 20)
        .await
        .expect("platform-wide health");
    assert_eq!(platform.total, 1);
}

/// The signing identity surface round-trips its expiry instant.
///
/// `deploy_signing_identity.expires_at` is TIMESTAMPTZ. The write bound an
/// RFC3339 `String` without a cast, which PostgreSQL refuses outright, and the
/// read decoded the column into a `String`, which it also refuses — silently,
/// behind `.ok()`. Both halves of the round trip are asserted here because the
/// expiry is the record's entire purpose.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn signing_identity_expiry_round_trips_through_the_surface() {
    let repository = migrated_repository().await;
    let created = repository
        .create_signing_identity(
            7,
            Some(11),
            &CreateSigningIdentityRequest {
                identity_name: "prod-authenticode".to_owned(),
                signing_kind: SigningKind::WindowsAuthenticode,
                platform_target_id: None,
                fingerprint_sha256: None,
                expires_at: Some("2027-03-04T05:06:07.000Z".to_owned()),
                secret_ref: None,
                idempotency_key: None,
            },
        )
        .await
        .expect("create signing identity with an expiry");
    assert_eq!(
        created.expires_at.as_deref(),
        Some("2027-03-04T05:06:07.000Z"),
        "the create response must carry the expiry it stored"
    );

    // The list surface maps the same row through the same helper.
    let page = repository
        .list_signing_identities(7, 1, 20)
        .await
        .expect("list signing identities");
    assert_eq!(page.total, 1);
    assert_eq!(
        page.items[0].expires_at.as_deref(),
        Some("2027-03-04T05:06:07.000Z"),
        "the list surface must carry the expiry too"
    );

    // An identity without an expiry stays without one.
    let undated = repository
        .create_signing_identity(
            7,
            Some(11),
            &CreateSigningIdentityRequest {
                identity_name: "no-expiry".to_owned(),
                signing_kind: SigningKind::AndroidKeystore,
                platform_target_id: None,
                fingerprint_sha256: None,
                expires_at: None,
                secret_ref: None,
                idempotency_key: None,
            },
        )
        .await
        .expect("create signing identity without an expiry");
    assert_eq!(undated.expires_at, None);
}
