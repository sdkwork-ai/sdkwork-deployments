//! Certificate renewal integration tests: the lease that stops two workers from
//! ordering the same certificate twice, the backoff that keeps a refusal from
//! becoming a tight loop against a shared CA rate limit, the sweep that keeps the
//! status column honest, and the point where an issued version mirrors the new
//! window and closes the ledger.
//!
//! Every assertion here is about a property the unit tests cannot reach: leases,
//! partial unique indexes, and `FOR UPDATE` only mean anything against a real
//! database.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like the
//! other PostgreSQL integration tests in this crate.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::DefaultDatabaseModule;
use sdkwork_database_sqlx::DatabasePool;
use sdkwork_deploy_contract::{
    CertificateScope, CreateAcmeAccountRequest, CreateCertificateRequest, DeployAppApi,
    DeployAppRequestContext, DeployServiceErrorKind, RequestCertificateOrderRequest,
    ValidationMethod,
};
use sdkwork_deploy_drive_port::MemoryDeployDrivePort;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::{DeployRepositoryPort, DeployService};
use sqlx::PgPool;

const TENANT_ID: i64 = 7;
const ORGANIZATION_ID: i64 = 9;

fn deploy_module() -> Arc<DefaultDatabaseModule> {
    let app_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Arc::new(DefaultDatabaseModule::from_app_root(&app_root).expect("load deploy database module"))
}

fn database_pool(pool: PgPool) -> DatabasePool {
    // The migration lock opens its own connection from this config, so it has to
    // describe the pool that is actually in use; `DatabaseConfig::default()` is a
    // SQLite configuration with an empty URL and fails before any assertion.
    let url = std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL").unwrap_or_default();
    DatabasePool::Postgres(
        pool,
        sdkwork_database_sqlx::PoolContext {
            config: DatabaseConfig {
                engine: DatabaseEngine::Postgres,
                url,
                ..DatabaseConfig::default()
            },
        },
    )
}

async fn migrated_repository() -> (DeployRepository, PgPool) {
    let pool = common::postgres_schema_pool().await;
    let module = deploy_module();
    let orchestrator = LifecycleOrchestrator::new(database_pool(pool.clone()), module)
        .with_applied_by("sdkwork-deploy-certificate-renewal-test");
    orchestrator
        .init()
        .await
        .expect("init on an empty schema must bootstrap the baseline");
    orchestrator
        .migrate()
        .await
        .expect("migrate must apply the full forward migration chain");
    seed_hostnames(&pool).await;
    let repository = DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(4).expect("Snowflake generator"),
        common::test_secret_key(),
    );
    // The pool comes back alongside the repository because the scheduling columns
    // this file asserts on are not part of any read port: `active_not_after`,
    // `renewal_lease_owner` and the ledger's window columns are control-plane
    // state, and reading them through the DTO would only test the DTO.
    (repository, pool)
}

/// A verified, active hostname, because `create_certificate` refuses a
/// `domainId` that does not reference one.
async fn seed_hostnames(pool: &PgPool) {
    sqlx::raw_sql(
        "INSERT INTO deploy_dns_zone (
            id,uuid,tenant_id,organization_id,apex_hostname,status
         ) VALUES
            (30,'zone-renewal',7,9,'sdkwork.dev','ACTIVE');
         INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,verified_at,status
         ) VALUES
            (40,'domain-renewal',7,9,30,'api.sdkwork.dev','EXACT','VERIFIED',NOW(),'ACTIVE');",
    )
    .execute(pool)
    .await
    .expect("seed hostname resources");
}

async fn create_certificate(
    repository: &DeployRepository,
    idempotency_key: &str,
    renew_before_days: i32,
) -> String {
    repository
        .create_certificate(
            TENANT_ID,
            Some(ORGANIZATION_ID),
            Some(11),
            idempotency_key,
            &CreateCertificateRequest {
                cert_name: "api.sdkwork.dev".to_owned(),
                domain_ids: vec!["domain-renewal".to_owned()],
                ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
                certificate_scope: CertificateScope::SingleDomain,
                validation_method: ValidationMethod::Auto,
                preferred_key_algorithm: "ECDSA".to_owned(),
                auto_renew: true,
                renew_before_days,
                // Left unset: the renewal suite is about the lease and the sweep, and
                // resolution must reach the deployment-level configuration exactly as
                // it does for a certificate created without an account pin.
                provider_account_id: None,
            },
        )
        .await
        .expect("create certificate")
        .id
}

/// The console's request context, so a test drives the same entrypoint the UI does.
fn app_context() -> DeployAppRequestContext {
    DeployAppRequestContext {
        tenant_id: TENANT_ID,
        actor_id: Some(11),
        organization_id: Some(ORGANIZATION_ID),
        session_id: Some("certificate-creation-test".to_owned()),
        auth_token: None,
        access_token: None,
    }
}

/// The app surface with no issuance engine installed.
///
/// Requesting the first order is a control-plane act — it resolves the tenant's
/// ACME account and records the CAA observation — so it needs no CA to be
/// reachable, and a test that supplied one would hide whether creation works on
/// its own.
fn app_service(repository: Arc<DeployRepository>) -> DeployService {
    let repository_port: Arc<dyn DeployRepositoryPort> = repository;
    DeployService::new(repository_port, Arc::new(MemoryDeployDrivePort))
}

/// Puts a certificate into a chosen validity window, as an issued one would be.
///
/// Written into the columns the scheduler reads rather than produced by a real
/// issuance, because the scheduler only ever reads those columns: a test that had
/// to mint a genuinely 85-day-old certificate would be testing `rcgen`.
///
/// The authoritative version row moves with the mirror, and it has to. The rule
/// reads `COALESCE(version.not_after, certificate.active_not_after)` while the
/// index is built on `active_not_after`, so moving only one of them would
/// describe a certificate the design guarantees cannot exist and the assertions
/// below would then be about fiction instead of about the scheduler.
async fn set_active_window(pool: &PgPool, certificate_id: &str, not_before: &str, not_after: &str) {
    let mut transaction = pool.begin().await.expect("begin window update");
    let affected = sqlx::query(
        "UPDATE deploy_certificate
            SET status = 'ACTIVE',
                active_not_before = CAST($1 AS TIMESTAMPTZ),
                active_not_after = CAST($2 AS TIMESTAMPTZ),
                renewal_status = 'NONE', renewal_lease_owner = NULL,
                renewal_lease_expires_at = NULL, renewal_next_attempt_at = NULL,
                renewal_failure_count = 0, updated_at = NOW(), version = version + 1
          WHERE tenant_id = $3 AND uuid = $4 AND deleted_at IS NULL",
    )
    .bind(not_before)
    .bind(not_after)
    .bind(TENANT_ID)
    .bind(certificate_id)
    .execute(&mut *transaction)
    .await
    .expect("set the certificate window");
    assert_eq!(affected.rows_affected(), 1, "certificate {certificate_id}");

    sqlx::query(
        "UPDATE deploy_certificate_version
            SET not_before = CAST($1 AS TIMESTAMPTZ), not_after = CAST($2 AS TIMESTAMPTZ)
          WHERE id = (SELECT current_version_id FROM deploy_certificate WHERE uuid = $3)",
    )
    .bind(not_before)
    .bind(not_after)
    .bind(certificate_id)
    .execute(&mut *transaction)
    .await
    .expect("age the active version with its certificate");

    transaction.commit().await.expect("commit window update");
}

/// The window as stored, read back so an assertion can name the value the claim
/// should have carried without recomputing it.
async fn stored_window(pool: &PgPool, certificate_id: &str) -> (String, String) {
    sqlx::query_as(
        "SELECT to_char(active_not_before, 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'),
                to_char(active_not_after,  'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')
           FROM deploy_certificate WHERE uuid = $1",
    )
    .bind(certificate_id)
    .fetch_one(pool)
    .await
    .expect("read the stored window")
}

/// `now` shifted by a whole number of days, rendered the way the repository
/// expects. Computed in SQL so it shares the database's clock and cannot drift
/// from the `NOW()` the repository writes for leases and backoffs.
async fn days_from_now(pool: &PgPool, days: i64) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT to_char(NOW() + make_interval(days => $1), 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')",
    )
    // `make_interval`'s `days` is `int`, not `double precision`: binding a float
    // makes PostgreSQL report that no such function exists, which reads like a
    // missing extension rather than a widened parameter.
    .bind(days as i32)
    .fetch_one(pool)
    .await
    .expect("compute a shifted instant")
}

async fn now_rfc3339(pool: &PgPool) -> String {
    days_from_now(pool, 0).await
}

/// How far in the future the certificate's own rule puts renewal, in days.
async fn days_until_due(pool: &PgPool, certificate_id: &str) -> f64 {
    sqlx::query_scalar::<_, f64>(
        // `EXTRACT(EPOCH FROM interval)` is `numeric` from PostgreSQL 14 on, so
        // the cast is what makes this a `float8` the driver can decode at all.
        "SELECT (EXTRACT(EPOCH FROM (
                    GREATEST(
                        active_not_before + ((active_not_after - active_not_before) / 3),
                        active_not_after - (renew_before_days * INTERVAL '1 day')
                    ) - NOW()
                )) / 86400.0)::float8
           FROM deploy_certificate WHERE uuid = $1",
    )
    .bind(certificate_id)
    .fetch_one(pool)
    .await
    .expect("measure the scheduled renewal instant")
}

/// The newest ledger attempt for a certificate: status, attempt number, error code.
async fn latest_attempt(pool: &PgPool, certificate_id: &str) -> Option<(String, i32, String)> {
    sqlx::query_as(
        "SELECT r.status, r.attempt_no, COALESCE(r.last_error_code, '')
           FROM deploy_certificate_renewal r
           JOIN deploy_certificate c ON c.id = r.certificate_id
          WHERE c.uuid = $1
          ORDER BY r.id DESC
          LIMIT 1",
    )
    .bind(certificate_id)
    .fetch_optional(pool)
    .await
    .expect("read the latest renewal attempt")
}

async fn attempt_count(pool: &PgPool, certificate_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_certificate_renewal r
           JOIN deploy_certificate c ON c.id = r.certificate_id
          WHERE c.uuid = $1",
    )
    .bind(certificate_id)
    .fetch_one(pool)
    .await
    .expect("count renewal attempts")
}

async fn seed_acme_account(repository: &DeployRepository) {
    repository
        .create_acme_account(
            TENANT_ID,
            &CreateAcmeAccountRequest {
                ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
                directory_url: "https://acme-staging-v02.api.letsencrypt.org/directory".to_owned(),
                contact_email: "ops@sdkwork.dev".to_owned(),
                external_account_digest: None,
            },
        )
        .await
        .expect("create acme account");
}

// ---------------------------------------------------------------------------
// The lease
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_claimed_certificate_is_not_claimed_again_until_its_lease_lapses() {
    let (repository, pool) = migrated_repository().await;
    let certificate_id = create_certificate(&repository, "lease-1", 30).await;
    // A 90-day certificate inside its last 20 days, so the window rule is what
    // selects it rather than a leftover status.
    set_active_window(
        &pool,
        &certificate_id,
        &days_from_now(&pool, -70).await,
        &days_from_now(&pool, 20).await,
    )
    .await;
    let window = stored_window(&pool, &certificate_id).await;
    let now = now_rfc3339(&pool).await;

    let first = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now)
        .await
        .expect("first claim");
    assert_eq!(first.len(), 1, "the due certificate is claimed");
    assert_eq!(first[0].certificate_uuid, certificate_id);
    assert_eq!(first[0].attempt_no, 1);
    assert_eq!(first[0].renew_before_days, 30);
    // The previous window travels with the claim: once the new version lands the
    // old one is still readable in the ledger, and "coverage was continuous" is
    // only auditable because both ends of the handover were recorded.
    assert_eq!(
        first[0].previous_not_before.as_deref(),
        Some(window.0.as_str())
    );
    assert_eq!(
        first[0].previous_not_after.as_deref(),
        Some(window.1.as_str())
    );

    // A second worker sweeping at the same instant sees the lease and leaves it
    // alone. This is the whole reason the claim is a transaction.
    let second = repository
        .claim_due_certificate_renewals("worker-b", 50, 300, &now)
        .await
        .expect("second claim");
    assert!(
        second.is_empty(),
        "a live lease must not be preempted, got {second:?}"
    );

    // Nor does the holder get a second attempt on its own lease.
    let third = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now)
        .await
        .expect("third claim");
    assert!(third.is_empty(), "the holder already owns it");

    assert_eq!(
        latest_attempt(&pool, &certificate_id).await,
        Some(("PLANNED".to_owned(), 1, String::new())),
        "exactly one attempt was recorded"
    );
    assert_eq!(attempt_count(&pool, &certificate_id).await, 1);

    let lease_owner: Option<String> =
        sqlx::query_scalar("SELECT renewal_lease_owner FROM deploy_certificate WHERE uuid = $1")
            .bind(&certificate_id)
            .fetch_one(&pool)
            .await
            .expect("read lease owner");
    assert_eq!(lease_owner.as_deref(), Some("worker-a"));
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_lapsed_lease_is_reclaimed_and_the_abandoned_attempt_is_retired() {
    let (repository, pool) = migrated_repository().await;
    let certificate_id = create_certificate(&repository, "lease-2", 30).await;
    set_active_window(
        &pool,
        &certificate_id,
        &days_from_now(&pool, -70).await,
        &days_from_now(&pool, 20).await,
    )
    .await;

    let now = now_rfc3339(&pool).await;
    let first = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now)
        .await
        .expect("first claim");
    assert_eq!(first.len(), 1);

    // The worker dies holding the lease. Nothing retires the attempt, so recovery
    // has to come from the next sweep — which is what makes "a crash is
    // survivable" a claim rather than a hope.
    let reclaimed = repository
        .claim_due_certificate_renewals("worker-b", 50, 300, &days_from_now(&pool, 1).await)
        .await
        .expect("reclaim after the lease lapsed");
    assert_eq!(reclaimed.len(), 1, "the lapsed lease is reclaimable");
    assert_eq!(reclaimed[0].attempt_no, 2, "the reclaim is a new attempt");

    // The abandoned attempt is closed rather than deleted: an operator reading
    // the ledger should see that automation tried and was interrupted, not that
    // nothing ever happened.
    let retired: (String, String) = sqlx::query_as(
        "SELECT r.status, COALESCE(r.last_error_code, '')
           FROM deploy_certificate_renewal r
           JOIN deploy_certificate c ON c.id = r.certificate_id
          WHERE c.uuid = $1
          ORDER BY r.id ASC
          LIMIT 1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("read the abandoned attempt");
    assert_eq!(retired.0, "FAILED");
    assert_eq!(retired.1, "RENEWAL_WORKER_LOST");
    // It also has to be finished, or the ledger would show two attempts in flight.
    let finished: bool = sqlx::query_scalar(
        "SELECT finished_at IS NOT NULL FROM deploy_certificate_renewal
          WHERE uuid = (SELECT r.uuid FROM deploy_certificate_renewal r
                          JOIN deploy_certificate c ON c.id = r.certificate_id
                         WHERE c.uuid = $1 ORDER BY r.id ASC LIMIT 1)",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("read the abandoned attempt's finish time");
    assert!(finished);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_certificate_nowhere_near_expiry_is_left_alone() {
    let (repository, pool) = migrated_repository().await;
    let certificate_id = create_certificate(&repository, "not-due", 30).await;
    // A 90-day certificate one day old: the lifetime floor puts renewal on day 60,
    // well after the 30-day lead time would.
    set_active_window(
        &pool,
        &certificate_id,
        &days_from_now(&pool, -1).await,
        &days_from_now(&pool, 89).await,
    )
    .await;

    let due_in = days_until_due(&pool, &certificate_id).await;
    assert!(
        due_in > 50.0,
        "the lifetime floor must push renewal past day 50, got {due_in:.1} days"
    );

    let claims = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now_rfc3339(&pool).await)
        .await
        .expect("claim");
    assert!(claims.is_empty(), "not due, so nothing to claim");
    assert_eq!(attempt_count(&pool, &certificate_id).await, 0);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_short_lived_certificate_is_not_claimed_the_moment_it_is_issued() {
    let (repository, pool) = migrated_repository().await;
    // The case that makes the floor load-bearing rather than decorative: a 7-day
    // certificate with a 30-day lead. Without the floor, `not_after - 30 days`
    // lands before `not_before`, the certificate is due the instant it is issued,
    // and the scheduler reopens an order on every tick forever.
    let certificate_id = create_certificate(&repository, "short-lived", 30).await;
    set_active_window(
        &pool,
        &certificate_id,
        &days_from_now(&pool, 0).await,
        &days_from_now(&pool, 7).await,
    )
    .await;

    let due_in = days_until_due(&pool, &certificate_id).await;
    assert!(
        due_in > 2.0,
        "a third of a 7-day life must elapse first, got {due_in:.2} days"
    );

    let claims = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now_rfc3339(&pool).await)
        .await
        .expect("claim");
    assert!(
        claims.is_empty(),
        "a freshly issued short-lived certificate is not due, got {} claim(s)",
        claims.len()
    );

    // And it does become due once a third of its life has passed, so the floor
    // defers renewal rather than suppressing it.
    let claims = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &days_from_now(&pool, 3).await)
        .await
        .expect("claim later");
    assert_eq!(claims.len(), 1, "due after a third of its 7-day life");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn the_sweep_only_claims_up_to_its_batch_size() {
    let (repository, pool) = migrated_repository().await;
    let mut ids = Vec::new();
    for index in 0..3 {
        let certificate_id = create_certificate(&repository, &format!("batch-{index}"), 30).await;
        set_active_window(
            &pool,
            &certificate_id,
            &days_from_now(&pool, -70).await,
            &days_from_now(&pool, 20 - index).await,
        )
        .await;
        ids.push(certificate_id);
    }

    let batch = repository
        .claim_due_certificate_renewals("worker-a", 2, 300, &now_rfc3339(&pool).await)
        .await
        .expect("bounded claim");
    assert_eq!(batch.len(), 2, "one tick does bounded work and no more");
    // Ordered by expiry, so the most urgent certificates go first.
    assert_eq!(batch[0].certificate_uuid, ids[2]);
    assert_eq!(batch[1].certificate_uuid, ids[1]);

    let next = repository
        .claim_due_certificate_renewals("worker-b", 2, 300, &now_rfc3339(&pool).await)
        .await
        .expect("second bounded claim");
    assert_eq!(next.len(), 1, "the remaining certificate is picked up next");
    assert_eq!(next[0].certificate_uuid, ids[0]);
}

// ---------------------------------------------------------------------------
// Ordering and failure
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_refused_order_backs_off_instead_of_retrying_every_tick() {
    let (repository, pool) = migrated_repository().await;
    let certificate_id = create_certificate(&repository, "backoff", 30).await;
    set_active_window(
        &pool,
        &certificate_id,
        &days_from_now(&pool, -70).await,
        &days_from_now(&pool, 20).await,
    )
    .await;

    let now = now_rfc3339(&pool).await;
    let claims = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now)
        .await
        .expect("claim");
    assert_eq!(claims.len(), 1);

    repository
        .fail_certificate_renewal(TENANT_ID, &claims[0].renewal_uuid, "ISSUANCE_REFUSED")
        .await
        .expect("record the refusal");

    assert_eq!(
        latest_attempt(&pool, &certificate_id).await,
        Some(("FAILED".to_owned(), 1, "ISSUANCE_REFUSED".to_owned()))
    );

    let (status, failures, backlogged): (String, i32, bool) = sqlx::query_as(
        "SELECT renewal_status, renewal_failure_count,
                renewal_next_attempt_at IS NOT NULL AND renewal_next_attempt_at > NOW()
           FROM deploy_certificate WHERE uuid = $1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("read the backoff state");
    assert_eq!(status, "FAILED");
    assert_eq!(failures, 1);
    assert!(
        backlogged,
        "a refused order must schedule its retry rather than return straight to the queue"
    );

    // The backoff is what the sweep honours: an immediate re-sweep finds nothing
    // even though the certificate is still well inside its renewal window.
    let immediate = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now)
        .await
        .expect("re-sweep");
    assert!(
        immediate.is_empty(),
        "the scheduled retry must gate the next attempt, got {immediate:?}"
    );

    // Past the backoff it is retried, as a new attempt rather than a replay.
    let retried = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &days_from_now(&pool, 1).await)
        .await
        .expect("claim after the backoff");
    assert_eq!(retried.len(), 1);
    assert_eq!(retried[0].attempt_no, 2, "a new attempt, not a replay");
    assert_eq!(attempt_count(&pool, &certificate_id).await, 2);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn marking_an_attempt_ordered_stops_the_sweep_from_reclaiming_it() {
    let (repository, pool) = migrated_repository().await;
    seed_acme_account(&repository).await;
    let certificate_id = create_certificate(&repository, "ordered", 30).await;
    set_active_window(
        &pool,
        &certificate_id,
        &days_from_now(&pool, -70).await,
        &days_from_now(&pool, 20).await,
    )
    .await;

    let now = now_rfc3339(&pool).await;
    let claims = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now)
        .await
        .expect("claim");
    assert_eq!(claims.len(), 1);

    let order = repository
        .request_certificate_order(
            TENANT_ID,
            &RequestCertificateOrderRequest {
                certificate_id: certificate_id.clone(),
                idempotency_key: format!("renew:{}", claims[0].renewal_uuid),
                challenge_type: Some("HTTP_01".to_owned()),
            },
        )
        .await
        .expect("open the renewal order");
    repository
        .mark_certificate_renewal_ordered(TENANT_ID, &claims[0].renewal_uuid, &order.id)
        .await
        .expect("link the attempt to its order");

    assert_eq!(
        latest_attempt(&pool, &certificate_id)
            .await
            .map(|row| row.0),
        Some("ORDERED".to_owned()),
        "the attempt recorded that its order exists"
    );

    // An order at the CA is real work in flight. Even after the lease lapses the
    // sweep leaves it alone, because ordering again would issue the certificate
    // twice and spend the tenant's shared rate limit on the second one.
    let reclaimed = repository
        .claim_due_certificate_renewals("worker-b", 50, 300, &days_from_now(&pool, 1).await)
        .await
        .expect("sweep past an outstanding order");
    assert!(
        reclaimed.is_empty(),
        "an ordered attempt is left to finish, got {reclaimed:?}"
    );
    assert_eq!(attempt_count(&pool, &certificate_id).await, 1);
}

// ---------------------------------------------------------------------------
// Creating a certificate requests its first order
// ---------------------------------------------------------------------------

/// The certificate the split function below creates, addressed by the fields the
/// first-order tests assert on.
fn console_create_request() -> CreateCertificateRequest {
    CreateCertificateRequest {
        cert_name: "api.sdkwork.dev".to_owned(),
        domain_ids: vec!["domain-renewal".to_owned()],
        ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
        certificate_scope: CertificateScope::SingleDomain,
        validation_method: ValidationMethod::Auto,
        preferred_key_algorithm: "ECDSA".to_owned(),
        auto_renew: true,
        renew_before_days: 30,
        // Unset on purpose: this is the create the console performs, and the console
        // only sends a pin when the operator picked an account.
        provider_account_id: None,
    }
}

/// A certificate the console creates is an accepted intent, and something has to turn
/// that intent into a version.
///
/// Nothing did, and all three candidate paths declined it for a different reason:
/// `create_certificate_repo` writes the row `PENDING` with no order; the renewal sweep
/// only ever looks at `status IN ('ACTIVE','EXPIRED')` rows that already carry a
/// validity window; and the renew endpoint refuses anything that is not already
/// `ACTIVE` or `FAILED`. A row that no path will advance is not "accepted intent", it
/// is a dead end.
///
/// Creation therefore requests the first order itself, through the same CAA-checked
/// function the operator endpoint and the renewal sweep already share. The renewal
/// ledger is left alone: this is not a renewal, and recording one would make renewal
/// history lie about a certificate that had never been issued.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn creating_a_certificate_requests_its_first_order() {
    let (repository, pool) = migrated_repository().await;
    seed_acme_account(&repository).await;
    let service = app_service(Arc::new(repository));

    let certificate_id = service
        .create_certificate(&app_context(), "console-create", &console_create_request())
        .await
        .expect("create a certificate through the app surface")
        .id;

    let orders: Vec<(String, String)> = sqlx::query_as(
        "SELECT o.uuid, o.status
           FROM deploy_certificate_order o
           JOIN deploy_certificate c ON c.id = o.certificate_id
          WHERE c.uuid = $1
          ORDER BY o.id",
    )
    .bind(&certificate_id)
    .fetch_all(&pool)
    .await
    .expect("read the certificate's orders");

    let ledger: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM deploy_certificate_renewal r
           JOIN deploy_certificate c ON c.id = r.certificate_id
          WHERE c.uuid = $1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("count the renewal ledger");

    let (status, window_known): (String, bool) = sqlx::query_as(
        "SELECT status, active_not_after IS NOT NULL
           FROM deploy_certificate WHERE uuid = $1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("read the certificate after creation");

    assert_eq!(
        orders.len(),
        1,
        "creating a certificate must request exactly one order"
    );
    assert_eq!(
        orders[0].1, "REQUESTED",
        "the first order starts at the head of the state machine"
    );
    assert_eq!(
        ledger, 0,
        "the first issuance is not a renewal and must not enter the renewal ledger"
    );
    assert_eq!(
        status, "PENDING",
        "an order is not a version; the certificate is ACTIVE only once one is stored"
    );
    assert!(
        !window_known,
        "a certificate with no stored version cannot know its validity window"
    );
}

/// Replaying creation must not open a second order.
///
/// `Idempotency-Key` already makes creation replay-safe at the certificate, and the
/// first order has to inherit that: two orders for one intent would spend the tenant's
/// shared CA rate limit twice for the same certificate.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn replaying_creation_does_not_open_a_second_order() {
    let (repository, pool) = migrated_repository().await;
    seed_acme_account(&repository).await;
    let service = app_service(Arc::new(repository));

    let first = service
        .create_certificate(&app_context(), "console-replay", &console_create_request())
        .await
        .expect("create a certificate")
        .id;
    let second = service
        .create_certificate(&app_context(), "console-replay", &console_create_request())
        .await
        .expect("replay the same creation")
        .id;

    let orders: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM deploy_certificate_order o
           JOIN deploy_certificate c ON c.id = o.certificate_id
          WHERE c.uuid = $1",
    )
    .bind(&first)
    .fetch_one(&pool)
    .await
    .expect("count the certificate's orders");

    assert_eq!(first, second, "the replay must return the same certificate");
    assert_eq!(
        orders, 1,
        "a replayed creation must not open a second order"
    );
}

/// A missing ACME account must not fail the creation.
///
/// `PENDING` is an accepted intent: the console accepts the request and the account is
/// a separate resource the operator can add later. Failing the whole request would make
/// certificate creation depend on an unrelated setup step, so the order is skipped and
/// the row stays `PENDING` — visibly not issued, rather than silently pretending to be.
/// Adding the account and asking again then has to work, which is why `renew` accepts a
/// certificate that has never been issued.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_missing_acme_account_defers_issuance_without_losing_the_intent() {
    let (repository, pool) = migrated_repository().await;
    let repository = Arc::new(repository);
    let service = app_service(repository.clone());

    let certificate_id = service
        .create_certificate(
            &app_context(),
            "console-deferred",
            &console_create_request(),
        )
        .await
        .expect("an accepted intent must not fail because an account is missing")
        .id;

    let orders_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_certificate_order o
           JOIN deploy_certificate c ON c.id = o.certificate_id WHERE c.uuid = $1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("count orders before the account exists");
    let status_before: String =
        sqlx::query_scalar("SELECT status FROM deploy_certificate WHERE uuid = $1")
            .bind(&certificate_id)
            .fetch_one(&pool)
            .await
            .expect("status before the account exists");
    assert_eq!(
        orders_before, 0,
        "an order cannot exist without an account; none may be invented"
    );
    assert_eq!(status_before, "PENDING");

    // Now the operator adds the account and asks again.
    seed_acme_account(&repository).await;
    service
        .renew_certificate(&app_context(), &certificate_id)
        .await
        .expect("a certificate that was never issued can be asked for again");

    let orders_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_certificate_order o
           JOIN deploy_certificate c ON c.id = o.certificate_id WHERE c.uuid = $1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("count orders after the account exists");
    assert_eq!(
        orders_after, 1,
        "the deferred intent must become issuable once the account exists"
    );
}

// ---------------------------------------------------------------------------
// Issuance mirrors the window and closes the ledger
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn the_first_issuance_mirrors_its_window_without_claiming_to_be_a_renewal() {
    let (repository, pool) = migrated_repository().await;
    seed_acme_account(&repository).await;
    let certificate_id = create_certificate(&repository, "first-issue", 30).await;

    let order = repository
        .request_certificate_order(
            TENANT_ID,
            &RequestCertificateOrderRequest {
                certificate_id: certificate_id.clone(),
                idempotency_key: "first-issue-order".to_owned(),
                challenge_type: Some("HTTP_01".to_owned()),
            },
        )
        .await
        .expect("request the first order");
    issue_through_finalizing(&repository, &pool, &certificate_id, &order.id).await;

    let state: (String, bool, bool, i32, bool, i64) = sqlx::query_as(
        "SELECT c.status,
                c.active_not_before IS NOT NULL AND c.active_not_after IS NOT NULL,
                c.active_not_after = v.not_after AND c.active_not_before = v.not_before,
                c.renewal_failure_count,
                c.last_renewal_at IS NULL,
                (SELECT COUNT(*) FROM deploy_certificate_renewal r
                  WHERE r.certificate_id = c.id)
           FROM deploy_certificate c
           LEFT JOIN deploy_certificate_version v ON v.id = c.current_version_id
          WHERE c.uuid = $1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("read the certificate after its first issuance");
    assert_eq!(state.0, "ACTIVE");
    assert!(
        state.1,
        "an ACTIVE certificate must know its own validity window"
    );
    assert!(
        state.2,
        "the mirror must equal the version's window, or the scheduler would read a different answer"
    );
    assert_eq!(state.3, 0);
    assert!(
        state.4,
        "the first issuance is not a renewal and must not claim to be one"
    );
    assert_eq!(state.5, 0, "no renewal attempt was needed");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn storing_a_renewed_version_mirrors_the_new_window_and_closes_the_ledger() {
    let (repository, pool) = migrated_repository().await;
    seed_acme_account(&repository).await;
    let certificate_id = create_certificate(&repository, "renew-e2e", 30).await;

    // First issuance, so the certificate has a real version to replace.
    let first_order = repository
        .request_certificate_order(
            TENANT_ID,
            &RequestCertificateOrderRequest {
                certificate_id: certificate_id.clone(),
                idempotency_key: "renew-e2e-order-1".to_owned(),
                challenge_type: Some("HTTP_01".to_owned()),
            },
        )
        .await
        .expect("request the first order");
    issue_through_finalizing(&repository, &pool, &certificate_id, &first_order.id).await;

    // Age it into its renewal window.
    set_active_window(
        &pool,
        &certificate_id,
        &days_from_now(&pool, -85).await,
        &days_from_now(&pool, 5).await,
    )
    .await;
    let previous = stored_window(&pool, &certificate_id).await;

    let now = now_rfc3339(&pool).await;
    let claims = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now)
        .await
        .expect("claim");
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].attempt_no, 1, "the first renewal attempt");
    assert_eq!(
        claims[0].previous_not_before.as_deref(),
        Some(previous.0.as_str())
    );
    assert_eq!(
        claims[0].previous_not_after.as_deref(),
        Some(previous.1.as_str())
    );
    // And the claim names the version it is replacing, which is what lets the
    // ledger show the exact version-to-version handover rather than two windows
    // that happen to be adjacent.
    assert!(
        claims[0].previous_version_uuid.is_some(),
        "the claim knows which version it supersedes"
    );

    let renewal_order = repository
        .request_certificate_order(
            TENANT_ID,
            &RequestCertificateOrderRequest {
                certificate_id: certificate_id.clone(),
                idempotency_key: format!("renew:{}", claims[0].renewal_uuid),
                challenge_type: Some("HTTP_01".to_owned()),
            },
        )
        .await
        .expect("request the renewal order");
    repository
        .mark_certificate_renewal_ordered(TENANT_ID, &claims[0].renewal_uuid, &renewal_order.id)
        .await
        .expect("link the attempt");
    issue_through_finalizing(&repository, &pool, &certificate_id, &renewal_order.id).await;

    // The ledger closes with what the attempt replaced and what it produced, which
    // is the only evidence that coverage was continuous across the handover.
    let ledger: (String, bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT r.status,
                r.previous_version_id IS NOT NULL,
                r.resulting_version_id IS NOT NULL,
                r.previous_not_before IS NOT NULL AND r.previous_not_after IS NOT NULL,
                r.new_not_before IS NOT NULL AND r.new_not_after IS NOT NULL,
                r.finished_at IS NOT NULL
           FROM deploy_certificate_renewal r
           JOIN deploy_certificate c ON c.id = r.certificate_id
          WHERE c.uuid = $1 AND r.attempt_no = 1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("the closed renewal attempt");
    assert_eq!(ledger.0, "SUCCEEDED");
    assert!(ledger.1, "the superseded version is named");
    assert!(ledger.2, "the version it produced is named");
    assert!(ledger.3, "the replaced window is recorded");
    assert!(ledger.4, "the new window is recorded");
    assert!(ledger.5, "a terminal attempt is finished");

    // The certificate points at the renewed version, its mirror matches that
    // version exactly, and the renewal bookkeeping has been reset.
    let state: (bool, bool, bool, i32, bool) = sqlx::query_as(
        "SELECT c.active_not_before = v.not_before AND c.active_not_after = v.not_after,
                c.active_not_after > NOW(),
                c.last_renewal_at IS NOT NULL,
                c.renewal_failure_count,
                c.renewal_next_attempt_at IS NULL
                  AND c.renewal_lease_owner IS NULL
                  AND c.renewal_lease_expires_at IS NULL
           FROM deploy_certificate c
           JOIN deploy_certificate_version v ON v.id = c.current_version_id
          WHERE c.uuid = $1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("read the renewed certificate");
    assert!(state.0, "the mirror must equal the active version's window");
    assert!(state.1, "coverage must not have lapsed");
    assert!(state.2, "a fulfilled renewal advances last_renewal_at");
    assert_eq!(state.3, 0, "a success resets the failure count");
    assert!(
        state.4,
        "a fulfilled renewal leaves no lease and no scheduled retry"
    );

    // Renewing again right away must not be possible: the renewed certificate is
    // past the window that made it due, so the scheduler settles instead of
    // looping.
    let settled = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now_rfc3339(&pool).await)
        .await
        .expect("claim after the renewal");
    assert!(
        settled.is_empty(),
        "a renewed certificate is not immediately due again, got {settled:?}"
    );
}

/// Walks the order to `FINALIZING` and stores the issued version.
async fn issue_through_finalizing(
    repository: &DeployRepository,
    _pool: &PgPool,
    certificate_id: &str,
    order_id: &str,
) {
    let order = repository
        .retrieve_certificate_order(TENANT_ID, order_id)
        .await
        .expect("retrieve the order");
    let version_no = order.requested_version_no;
    let mut status = order.status.clone();
    for expected in [
        "ACCOUNT_READY",
        "ORDER_PENDING",
        "CHALLENGE_PRESENTING",
        "CHALLENGE_VALIDATING",
    ] {
        let advanced = repository
            .advance_certificate_order(TENANT_ID, order_id, &status, expected)
            .await
            .expect("advance the order");
        assert_eq!(advanced, expected);
        status = expected.to_owned();
    }
    // Read through the port rather than the table: the challenge identifier the
    // recorder expects is whatever the contract says it is, and a test that
    // guessed the column would be asserting its own guess.
    let challenges = repository
        .list_certificate_challenges(TENANT_ID, order_id, 1, 20)
        .await
        .expect("list challenges");
    let challenge_id = challenges
        .items
        .first()
        .expect("the order has a challenge")
        .id
        .clone();
    repository
        .record_challenge_result(TENANT_ID, order_id, Some(&challenge_id), true, None)
        .await
        .expect("record a valid challenge result");

    let fixture = common::sealed_version_material();
    let facts = &fixture.facts;
    repository
        .store_certificate_version(
            TENANT_ID,
            &fixture.certificate_version_uuid,
            order_id,
            version_no,
            &facts.serial_sha256,
            &facts.fingerprint_sha256,
            &facts.spki_sha256,
            &facts.chain_sha256,
            &facts.issuer,
            &facts.subject,
            &facts.key_algorithm,
            &facts.not_before,
            &facts.not_after,
            &format!("secret://tls/{certificate_id}/v{version_no}"),
            &fixture.sealed,
        )
        .await
        .expect("store the issued version");
}

// ---------------------------------------------------------------------------
// The expiry sweep
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn the_sweep_retires_a_lapsed_certificate_and_is_then_idle() {
    let (repository, pool) = migrated_repository().await;
    let certificate_id = create_certificate(&repository, "sweep-1", 30).await;
    set_active_window(
        &pool,
        &certificate_id,
        &days_from_now(&pool, -40).await,
        &days_from_now(&pool, -1).await,
    )
    .await;

    let sweep = repository
        .sweep_expired_certificates(50, &now_rfc3339(&pool).await)
        .await
        .expect("sweep");
    assert_eq!(sweep.certificates_expired, 1);
    assert!(!sweep.is_empty());

    let retired: (String, String) =
        sqlx::query_as("SELECT status, renewal_status FROM deploy_certificate WHERE uuid = $1")
            .bind(&certificate_id)
            .fetch_one(&pool)
            .await
            .expect("read the retired certificate");
    assert_eq!(
        retired.0, "EXPIRED",
        "a certificate past notAfter must not keep reporting ACTIVE"
    );
    assert_eq!(retired.1, "NONE");

    // Running again changes nothing: the sweep only selects `ACTIVE` rows, so it
    // converges instead of rewriting the same rows on every tick.
    let again = repository
        .sweep_expired_certificates(50, &now_rfc3339(&pool).await)
        .await
        .expect("second sweep");
    assert_eq!(again.certificates_expired, 0);
    assert!(again.is_empty());
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn an_open_attempt_is_cancelled_only_after_the_recovery_grace_period() {
    let (repository, pool) = migrated_repository().await;
    let certificate_id = create_certificate(&repository, "sweep-2", 30).await;
    set_active_window(
        &pool,
        &certificate_id,
        &days_from_now(&pool, -100).await,
        &days_from_now(&pool, -10).await,
    )
    .await;

    // An attempt left open by a worker that vanished while the certificate was
    // only recently expired: automatic recovery is still worth attempting, so it
    // has to survive the sweep.
    sqlx::query(
        "INSERT INTO deploy_certificate_renewal
            (id, uuid, tenant_id, certificate_id, trigger_kind, status, attempt_no,
             scheduled_at, created_at, updated_at, version)
         SELECT 900001, 'renewal-within-grace', $1, c.id, 'SCHEDULED', 'PLANNED', 1,
                NOW(), NOW(), NOW(), 1
           FROM deploy_certificate c WHERE c.uuid = $2",
    )
    .bind(TENANT_ID)
    .bind(&certificate_id)
    .execute(&pool)
    .await
    .expect("seed an open attempt");

    let sweep = repository
        .sweep_expired_certificates(50, &now_rfc3339(&pool).await)
        .await
        .expect("sweep inside the grace period");
    assert_eq!(sweep.certificates_expired, 1);
    assert_eq!(
        sweep.renewals_cancelled, 0,
        "an attempt ten days into the grace period is still recoverable"
    );
    let status: String = sqlx::query_scalar(
        "SELECT status FROM deploy_certificate_renewal WHERE uuid = 'renewal-within-grace'",
    )
    .fetch_one(&pool)
    .await
    .expect("read the open attempt");
    assert_eq!(status, "PLANNED");

    // Past the grace period the attempt is abandoned: retrying forever would spend
    // the tenant's shared CA rate limit on a certificate automation cannot fix, at
    // the cost of every other certificate under the same account.
    sqlx::query(
        "UPDATE deploy_certificate SET active_not_after = NOW() - INTERVAL '40 days',
                updated_at = NOW(), version = version + 1
          WHERE uuid = $1",
    )
    .bind(&certificate_id)
    .execute(&pool)
    .await
    .expect("age the certificate past the grace period");

    let sweep = repository
        .sweep_expired_certificates(50, &now_rfc3339(&pool).await)
        .await
        .expect("sweep past the grace period");
    assert_eq!(sweep.renewals_cancelled, 1);
    let cancelled: (String, bool) = sqlx::query_as(
        "SELECT status, finished_at IS NOT NULL FROM deploy_certificate_renewal
          WHERE uuid = 'renewal-within-grace'",
    )
    .fetch_one(&pool)
    .await
    .expect("read the cancelled attempt");
    assert_eq!(cancelled.0, "CANCELLED");
    assert!(cancelled.1, "a terminal attempt is finished");
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn renewal_history_tells_never_tried_apart_from_unknown() {
    let (repository, pool) = migrated_repository().await;
    let certificate_id = create_certificate(&repository, "history-1", 30).await;

    let error = repository
        .list_certificate_renewals(TENANT_ID, "no-such-certificate", 1, 20)
        .await
        .expect_err("an unknown certificate is not an empty history");
    assert_eq!(error.kind(), DeployServiceErrorKind::NotFound);

    let empty = repository
        .list_certificate_renewals(TENANT_ID, &certificate_id, 1, 20)
        .await
        .expect("a certificate that has never been renewed");
    assert_eq!(empty.total, 0);
    assert!(empty.items.is_empty());

    set_active_window(
        &pool,
        &certificate_id,
        &days_from_now(&pool, -70).await,
        &days_from_now(&pool, 20).await,
    )
    .await;
    let claims = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &now_rfc3339(&pool).await)
        .await
        .expect("claim");
    assert_eq!(claims.len(), 1);

    let history = repository
        .list_certificate_renewals(TENANT_ID, &certificate_id, 1, 20)
        .await
        .expect("history after an attempt");
    assert_eq!(history.total, 1);
    let attempt = &history.items[0];
    assert_eq!(attempt.certificate_id, certificate_id);
    assert_eq!(attempt.status, "PLANNED");
    assert_eq!(attempt.attempt_no, 1);
    assert_eq!(attempt.trigger_kind, "SCHEDULED");
    assert!(
        attempt.previous_not_after.is_some(),
        "the attempt records the window it was going to replace"
    );
    assert!(
        attempt.new_not_after.is_none(),
        "nothing has been issued yet, so the new window must not be invented"
    );

    // A manual request is labelled as one: it is the only signal separating an
    // operator's act from the scheduler's, and it is read before the claim
    // overwrites the status.
    sqlx::query("UPDATE deploy_certificate SET renewal_status = 'PLANNED' WHERE uuid = $1")
        .bind(&certificate_id)
        .execute(&pool)
        .await
        .expect("mark a manual request");
    let manual = repository
        .claim_due_certificate_renewals("worker-a", 50, 300, &days_from_now(&pool, 1).await)
        .await
        .expect("claim the manual request");
    assert_eq!(manual.len(), 1);
    let history = repository
        .list_certificate_renewals(TENANT_ID, &certificate_id, 1, 20)
        .await
        .expect("history with a manual attempt");
    assert_eq!(history.total, 2);
    assert_eq!(
        history.items[0].trigger_kind, "MANUAL",
        "the newest attempt is the operator's"
    );
    assert_eq!(
        history.items[0].attempt_no, 2,
        "attempt numbers advance rather than repeat"
    );
    assert_eq!(history.items[1].attempt_no, 1, "history is newest first");
}
