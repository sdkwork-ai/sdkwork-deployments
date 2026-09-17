// This module is shared by every integration test binary in the crate, and no
// single binary uses all of it.
#![allow(dead_code)]

use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool};

/// Deterministic AES-256 key for repository integration tests. Production
/// derives the key from `SDKWORK_DEPLOY_SECRET_ENCRYPTION_KEY`; tests only
/// exercise encryption round-trips and response masking, so a fixed key is
/// sufficient and keeps assertions stable.
#[allow(dead_code)]
pub fn test_secret_key() -> [u8; 32] {
    *b"sdkwork-deploy-test-secret-00000"
}

/// A real certificate bundle, both as plaintext files and as the sealed form the
/// service hands to the repository.
///
/// Version storage now writes the certificate files alongside the metadata row,
/// so a synthetic digest string no longer suffices: the digests have to describe
/// bytes that actually exist, and the private key has to arrive sealed, because
/// the table refuses plaintext key material.
pub struct SealedVersionMaterial {
    pub certificate_version_uuid: String,
    pub facts: sdkwork_deploy_certificate_material::CertificateFacts,
    /// What was assembled, for comparing a round trip against.
    pub plaintext: Vec<sdkwork_deploy_certificate_material::CertificateFile>,
    pub sealed: Vec<sdkwork_deploy_certificate_material::SealedCertificateFile>,
}

/// The custody key the fixtures seal under. Matches [`test_secret_key`].
#[allow(dead_code)]
pub const TEST_CUSTODY_KEK_REF: &str = "file:test/certificate-material/master.key";

#[allow(dead_code)]
pub fn test_custody_provider() -> sdkwork_deploy_certificate_material::FileKeyProvider {
    sdkwork_deploy_certificate_material::FileKeyProvider::from_material(
        &test_secret_key(),
        TEST_CUSTODY_KEK_REF,
    )
    .expect("key provider")
}

#[allow(dead_code)]
pub fn sealed_version_material() -> SealedVersionMaterial {
    use sdkwork_deploy_certificate_material::{assemble, SealedCertificateFile};

    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).expect("key pair");
    let mut params =
        rcgen::CertificateParams::new(vec!["api.sdkwork.dev".to_owned()]).expect("params");
    params.is_ca = rcgen::IsCa::ExplicitNoCa;
    let certificate = params.self_signed(&key).expect("self-signed certificate");
    let chain = certificate.pem();
    let private_key = key.serialize_pem();

    // A self-signed certificate is its own trust anchor, so no separate root is
    // needed and no anchor bundle has to be configured for the test.
    let bundle = assemble(chain.as_bytes(), private_key.as_bytes(), b"", None).expect("assemble");
    let facts = bundle.facts().clone();
    let plaintext = bundle.into_files();

    let certificate_version_uuid = sdkwork_database_id::uuid_v4();
    let provider = test_custody_provider();
    let sealed = plaintext
        .iter()
        .map(|file| {
            SealedCertificateFile::seal(
                file.kind,
                &file.content,
                &certificate_version_uuid,
                Some(&provider),
            )
            .expect("seal")
        })
        .collect();
    SealedVersionMaterial {
        certificate_version_uuid,
        facts,
        plaintext,
        sealed,
    }
}

const POSTGRES_BASELINE: &str =
    include_str!("../../../../database/ddl/baseline/postgres/0001_deploy_baseline.sql");

// `postgres_pool` is only used by the shared-module tests; upgrade tests use
// `postgres_schema_pool` directly, so silence the per-test dead-code warning.
#[allow(dead_code)]
pub async fn postgres_pool() -> PgPool {
    let pool = postgres_schema_pool().await;
    sqlx::raw_sql(POSTGRES_BASELINE)
        .execute(&pool)
        .await
        .expect("apply PostgreSQL baseline");
    pool
}

/// Creates an isolated random test schema and returns a pool pinned to it
/// without applying any baseline DDL. Callers apply the baseline or legacy
/// fixtures themselves (for example database upgrade tests).
pub async fn postgres_schema_pool() -> PgPool {
    let database_url = std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL")
        .expect("SDKWORK_DATABASE_TEST_POSTGRES_URL is required for PostgreSQL integration tests");
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("connect PostgreSQL integration database");
    let schema = format!(
        "sdkwork_deploy_test_{}",
        sdkwork_database_id::uuid_v4().replace('-', "")
    );
    // schema is derived from a random UUID; the assertion is an audit of that derivation
    sqlx::raw_sql(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin_pool)
        .await
        .expect("create isolated PostgreSQL test schema");
    admin_pool.close().await;

    let connect_options = PgConnectOptions::from_str(&database_url)
        .expect("parse PostgreSQL integration database URL");
    let connection_schema = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .after_connect(move |connection, _metadata| {
            let schema = connection_schema.clone();
            Box::pin(async move {
                sqlx::raw_sql(AssertSqlSafe(format!("SET search_path TO {schema}")))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(connect_options)
        .await
        .expect("connect isolated PostgreSQL test schema");
    pool
}

// ---------------------------------------------------------------------------
// Migrated repository harness
// ---------------------------------------------------------------------------

/// Boots an isolated schema, applies the baseline through the real lifecycle
/// orchestrator, and returns a repository bound to it.
///
/// The harness was copied verbatim into every test binary that needed it, which
/// is how the per-file fixtures drifted away from the baseline. Keep the single
/// copy here so a new test file starts from a schema that actually exists.
pub async fn migrated_repository(
    applied_by: &str,
) -> sdkwork_intelligence_deploy_repository_sqlx::DeployRepository {
    migrated_repository_with_pool(applied_by).await.0
}

/// [`migrated_repository`], plus the pool it was built on.
///
/// A test that asserts on control-plane columns — a lease holder, a deadline, a
/// version's window — cannot go through a read port, because those columns are
/// deliberately absent from the DTOs. It needs the pool, and the pool must be the
/// *same* one the repository writes through or the assertion is about a different
/// schema.
pub async fn migrated_repository_with_pool(
    applied_by: &str,
) -> (
    sdkwork_intelligence_deploy_repository_sqlx::DeployRepository,
    PgPool,
) {
    let pool = postgres_schema_pool().await;
    let app_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let module = std::sync::Arc::new(
        sdkwork_database_spi::DefaultDatabaseModule::from_app_root(&app_root)
            .expect("load deploy database module"),
    );
    // The migration lock opens its own connection from this config, so it has to
    // describe the pool that is actually in use. `DatabaseConfig::default()` is a
    // SQLite configuration with an empty URL, which fails every test with
    // `migration_lock_open_failed (postgres): relative URL without a base`
    // before reaching a single assertion.
    let url = std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL").unwrap_or_default();
    let database = sdkwork_database_sqlx::DatabasePool::Postgres(
        pool.clone(),
        sdkwork_database_sqlx::PoolContext {
            config: sdkwork_database_config::DatabaseConfig {
                engine: sdkwork_database_config::DatabaseEngine::Postgres,
                url,
                ..sdkwork_database_config::DatabaseConfig::default()
            },
        },
    );
    let orchestrator = sdkwork_database_lifecycle::LifecycleOrchestrator::new(database, module)
        .with_applied_by(applied_by);
    orchestrator
        .init()
        .await
        .expect("init on an empty schema must bootstrap the baseline");
    orchestrator
        .migrate()
        .await
        .expect("migrate must apply the full forward migration chain");
    let repository = sdkwork_intelligence_deploy_repository_sqlx::DeployRepository::new(
        pool.clone(),
        sdkwork_database_id::SnowflakeIdGenerator::new(4).expect("Snowflake generator"),
        test_secret_key(),
    );
    (repository, pool)
}

// ---------------------------------------------------------------------------
// Delivery chain fixtures
// ---------------------------------------------------------------------------
//
// `deploy_build`, `deploy_package` and `deploy_release` each declare
// `platform_target_id BIGINT NOT NULL` with a foreign key onto
// `deploy_app_platform_target`, `deploy_package.build_id` points at
// `deploy_build`, and `deploy_release.package_id` points at `deploy_package`.
// A test that inserts any of those rows directly therefore has to seed the
// whole chain first. Seed it in one place so every fixture matches the baseline
// instead of each test guessing at the required columns — the earlier
// per-test copies omitted `platform_target_id` entirely and so could never run.

/// Tenant and organization every delivery fixture is written for.
pub const TEST_TENANT_ID: i64 = 7;
pub const TEST_ORGANIZATION_ID: i64 = 9;

/// Internal ids for a seeded delivery chain. The caller picks them so that
/// assertions can address the rows by id.
#[derive(Clone, Copy)]
pub struct DeliveryChainIds {
    pub platform_target: i64,
    pub build: i64,
    pub package: i64,
    pub release: i64,
}

/// A seeded delivery chain.
pub struct DeliveryChain {
    pub platform_target_id: i64,
    pub build_id: i64,
    /// The package the release references.
    pub package_id: i64,
    pub release_id: i64,
    pub release_uuid: String,
}

/// Deterministic, collision-free uuid for a fixture row keyed by its id.
fn fixture_uuid(id: i64) -> String {
    format!("00000000-0000-4000-8000-{id:012}")
}

pub async fn seed_platform_target(pool: &PgPool, id: i64, app_internal_id: i64) {
    sqlx::query(
        "INSERT INTO deploy_app_platform_target
            (id, uuid, tenant_id, organization_id, app_id, target_key, platform,
             tech_stack, target_status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'ANDROID', 'FLUTTER', 'ACTIVE', NOW(), NOW())",
    )
    .bind(id)
    .bind(fixture_uuid(id))
    .bind(TEST_TENANT_ID)
    .bind(TEST_ORGANIZATION_ID)
    .bind(app_internal_id)
    .bind(format!("target-{id}"))
    .execute(pool)
    .await
    .expect("insert platform target");
}

pub async fn seed_build(
    pool: &PgPool,
    id: i64,
    app_internal_id: i64,
    platform_target_id: i64,
    semantic_version: &str,
    age_days: i64,
) {
    sqlx::query(
        "INSERT INTO deploy_build
            (id, uuid, tenant_id, organization_id, app_id, platform_target_id,
             build_number, build_status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, 1, 'SUCCEEDED',
                 NOW() - make_interval(days => $7::int),
                 NOW() - make_interval(days => $7::int))",
    )
    .bind(id)
    .bind(fixture_uuid(id))
    .bind(TEST_TENANT_ID)
    .bind(TEST_ORGANIZATION_ID)
    .bind(app_internal_id)
    .bind(platform_target_id)
    .bind(age_days)
    .execute(pool)
    .await
    .unwrap_or_else(|error| panic!("insert build for {semantic_version}: {error}"));
}

/// Inserts a READY package on an existing build.
///
/// Package retention only retires packages that no release references, so a
/// test that wants a retention candidate seeds one of these without pointing a
/// release at it.
pub async fn seed_package(
    pool: &PgPool,
    id: i64,
    app_internal_id: i64,
    platform_target_id: i64,
    build_id: i64,
    semantic_version: &str,
    age_days: i64,
) {
    sqlx::query(
        "INSERT INTO deploy_package
            (id, uuid, tenant_id, organization_id, app_id, platform_target_id, build_id,
             package_format, semantic_version, package_size_bytes, checksum_sha256,
             manifest_sha256, package_status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'TAR_GZ', $8, 128,
                 REPEAT('a', 64), REPEAT('b', 64), 'READY',
                 NOW() - make_interval(days => $9::int),
                 NOW() - make_interval(days => $9::int))",
    )
    .bind(id)
    .bind(fixture_uuid(id))
    .bind(TEST_TENANT_ID)
    .bind(TEST_ORGANIZATION_ID)
    .bind(app_internal_id)
    .bind(platform_target_id)
    .bind(build_id)
    .bind(semantic_version)
    .bind(age_days)
    .execute(pool)
    .await
    .unwrap_or_else(|error| panic!("insert package {semantic_version}: {error}"));
}

/// Inserts a release pointing at `ids.package` on `ids.platform_target`.
///
/// `deploy_release.package_id` is NOT NULL with a foreign key to
/// `deploy_package`, so the release cannot be seeded independently of the
/// package it ships.
pub async fn seed_release(
    pool: &PgPool,
    ids: DeliveryChainIds,
    app_internal_id: i64,
    semantic_version: &str,
    release_status: &str,
    age_days: i64,
) -> String {
    let release_uuid = fixture_uuid(ids.release);
    sqlx::query(
        "INSERT INTO deploy_release
            (id, uuid, tenant_id, organization_id, app_id, platform_target_id, package_id,
             semantic_version, release_status, idempotency_key, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                 NOW() - make_interval(days => $11::int),
                 NOW() - make_interval(days => $11::int))",
    )
    .bind(ids.release)
    .bind(&release_uuid)
    .bind(TEST_TENANT_ID)
    .bind(TEST_ORGANIZATION_ID)
    .bind(app_internal_id)
    .bind(ids.platform_target)
    .bind(ids.package)
    .bind(semantic_version)
    .bind(release_status)
    .bind(format!("release-{semantic_version}"))
    .bind(age_days)
    .execute(pool)
    .await
    .unwrap_or_else(|error| panic!("insert release {semantic_version}: {error}"));
    release_uuid
}

/// Seeds the full platform-target -> build -> package -> release chain.
pub async fn seed_delivery_chain(
    pool: &PgPool,
    ids: DeliveryChainIds,
    app_internal_id: i64,
    semantic_version: &str,
    release_status: &str,
    age_days: i64,
) -> DeliveryChain {
    seed_platform_target(pool, ids.platform_target, app_internal_id).await;
    seed_build(
        pool,
        ids.build,
        app_internal_id,
        ids.platform_target,
        semantic_version,
        age_days,
    )
    .await;
    seed_package(
        pool,
        ids.package,
        app_internal_id,
        ids.platform_target,
        ids.build,
        semantic_version,
        age_days,
    )
    .await;
    let release_uuid = seed_release(
        pool,
        ids,
        app_internal_id,
        semantic_version,
        release_status,
        age_days,
    )
    .await;
    DeliveryChain {
        platform_target_id: ids.platform_target,
        build_id: ids.build,
        package_id: ids.package,
        release_id: ids.release,
        release_uuid,
    }
}
