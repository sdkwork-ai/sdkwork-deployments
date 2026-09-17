//! Certificate issuance against a controlled CA: PLAN-2026-0003 §9 driving the
//! order state machine to `VERSION_STORED`, with §14 phase 3d as the exit evidence.
//!
//! The unit tests beside the service prove the *decisions* — which challenge a scope
//! resolves to, how a lost lease is treated, which name set a response covers. None of
//! them can prove that a real ACME conversation happens at all, because nothing short
//! of a real CA produces an order, a challenge the CA fetches, and a signed chain.
//! That is what this file is for, and it is why it runs against Pebble rather than a
//! hand-written fake: a fake would agree with whatever the engine already believed.
//!
//! Both automatic paths are covered, because they fail in different places:
//!
//! * **HTTP-01** (exact name) — the token is written to a webroot the edge serves, so
//!   the assertion that matters is that the file the worker wrote is the file the CA
//!   fetched. `pebble-challtestsrv` answers DNS so the name resolves, and a static
//!   server stands in for the edge's challenge read; both are outside this repository.
//! * **DNS-01** (wildcard, and therefore also its apex) — a wildcard name has no HTTP
//!   proof path at all, so this is the only route to a wildcard certificate. The
//!   presenter here publishes into `pebble-challtestsrv`'s TXT API instead of a real
//!   provider; the provider adapter is the one thing swapped, and it is swapped at the
//!   port that exists for exactly this reason.
//!
//! Requires a running Pebble and the environment described in
//! `.workbuddy/tmp/pebble/run-e2e.sh`; ignored by default like the other PostgreSQL
//! integration tests in this crate.

mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_deploy_certificate_material::{FileKeyProvider, TrustAnchorBundle};
use sdkwork_deploy_contract::{
    CertificateScope, CreateAcmeAccountRequest, CreateCertificateRequest, DeployAppApi,
    DeployAppRequestContext, DeployServiceResult, RequestCertificateOrderRequest, ValidationMethod,
};
use sdkwork_deploy_drive_port::MemoryDeployDrivePort;
use sdkwork_deploy_service_host::acme_issuance_from_env;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::{
    CertificateDns01Context, CertificateDns01PresenterPort, CertificateDns01Selector,
    DeployRepositoryPort, DeployService,
};
use sdkwork_webserver_acme_service::{
    Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, DnsProviderKind,
};
use sqlx::PgPool;

const TENANT_ID: i64 = 7;
const ORGANIZATION_ID: i64 = 9;
const ZONE_ID: i64 = 30;
/// A name no public CA would ever validate, which is the point: this suite must never
/// be able to reach beyond a CA the operator started on purpose.
const E2E_HOSTNAME: &str = "deploy-e2e.test";
const WORKER_ID: &str = "pebble-e2e-worker";

/// Everything the run needs from the environment, read once so a missing value fails
/// with its own name instead of as a timeout inside the ACME engine.
struct PebbleEnv {
    directory_url: String,
    trust_roots_file: String,
    root_pem_file: String,
    webroot: String,
    cert_live_root: String,
    management_url: String,
}

impl PebbleEnv {
    fn from_env() -> Self {
        // Read through to the production factory's vocabulary. Setting the engine's
        // own variable names here (rather than teaching the factory a test dialect) is
        // what keeps this suite honest: it exercises the real configuration path.
        let env = Self {
            directory_url: require_env("SDKWORK_DEPLOY_PEBBLE_DIRECTORY_URL"),
            trust_roots_file: require_env("SDKWORK_DEPLOY_PEBBLE_TRUST_ROOTS_FILE"),
            root_pem_file: require_env("SDKWORK_DEPLOY_PEBBLE_ROOT_PEM_FILE"),
            webroot: require_env("SDKWORK_DEPLOY_PEBBLE_WEBROOT"),
            cert_live_root: require_env("SDKWORK_DEPLOY_PEBBLE_CERT_LIVE_ROOT"),
            management_url: require_env("SDKWORK_DEPLOY_PEBBLE_MANAGEMENT_URL"),
        };
        std::env::set_var("SDKWORK_DEPLOY_ACME_DIRECTORY_URL", &env.directory_url);
        std::env::set_var(
            "SDKWORK_DEPLOY_ACME_TRUST_ROOTS_FILE",
            &env.trust_roots_file,
        );
        std::env::set_var("SDKWORK_WEBSERVER_ACME_WEBROOT", &env.webroot);
        std::env::set_var("SDKWORK_DEPLOY_CERT_LIVE_ROOT", &env.cert_live_root);
        std::env::set_var("SDKWORK_DEPLOY_ACME_CONTACT_EMAIL", "e2e@deploy-e2e.test");
        // Staging keeps `use_production` false, so the only directory this can reach is
        // the one named above.
        std::env::set_var("SDKWORK_DEPLOY_ACME_PROFILE", "staging");
        std::env::remove_var("SDKWORK_DEPLOY_ACME_ACCOUNT_ROOT");
        env
    }

    fn issuing_root_pem(&self) -> Vec<u8> {
        std::fs::read(&self.root_pem_file)
            .unwrap_or_else(|error| panic!("read {} failed: {error}", self.root_pem_file))
    }
}

fn require_env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("{key} is required; run the Pebble E2E script"))
}

/// Turns on the engine's own logging.
///
/// Without this a failed acceptance reports "the order did not advance" and nothing
/// else, because every reason the engine knows is emitted as a structured event rather
/// than returned as an error. The subscriber is idempotent-installed once per process.
fn init_tracing() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
        let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    });
}

/// Fails with the order's own recorded reason rather than a bare count.
///
/// The batch result says an order failed; the order row says why, and that is the only
/// way to tell a CA refusal from a lease that was lost from a missing credential.
async fn assert_stored(pool: &PgPool, order_id: &str, stored: i64, claim: &str) {
    if stored == 1 {
        return;
    }
    let diagnostic: (String, Option<String>, i32) = sqlx::query_as(
        "SELECT status, last_error_code, attempt_count FROM deploy_certificate_order WHERE uuid = $1",
    )
    .bind(order_id)
    .fetch_one(pool)
    .await
    .expect("read the order for diagnosis");
    panic!(
        "{claim}: the order did not reach a stored version (stored={stored}); \
         status={} last_error_code={:?} attempt_count={}",
        diagnostic.0, diagnostic.1, diagnostic.2
    );
}

/// Seeds one verified claim per requested hostname type.
///
/// `hostname_type` decides the *identifier* the plan derives — a `WILDCARD` row yields
/// `*.name` — while `hostname_ascii` stays the bare apex, which is why a wildcard
/// request needs two rows for one name.
async fn seed_domain(pool: &PgPool, uuid: &str, id: i64, hostname: &str, hostname_type: &str) {
    sqlx::query(
        "INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,verified_at,status
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,'VERIFIED',NOW(),'ACTIVE')",
    )
    .bind(id)
    .bind(uuid)
    .bind(TENANT_ID)
    .bind(ORGANIZATION_ID)
    .bind(ZONE_ID)
    .bind(hostname)
    .bind(hostname_type)
    .execute(pool)
    .await
    .expect("seed domain claim");
}

async fn seed_zone(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO deploy_dns_zone (
            id,uuid,tenant_id,organization_id,apex_hostname,status
         ) VALUES ($1,'zone-e2e',$2,$3,$4,'ACTIVE')",
    )
    .bind(ZONE_ID)
    .bind(TENANT_ID)
    .bind(ORGANIZATION_ID)
    .bind(E2E_HOSTNAME)
    .execute(pool)
    .await
    .expect("seed dns zone");
}

/// An order cannot be opened without an account to open it against.
///
/// The CA profile and directory here are fixtures — the order row records them, while
/// the engine reads its own directory from the environment — so they only have to be
/// a consistent pair.
async fn seed_acme_account(repository: &DeployRepository) {
    repository
        .create_acme_account(
            TENANT_ID,
            &CreateAcmeAccountRequest {
                ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
                directory_url: "https://acme-staging-v02.api.letsencrypt.org/directory".to_owned(),
                contact_email: "ops@deploy-e2e.test".to_owned(),
                external_account_digest: None,
            },
        )
        .await
        .expect("create acme account");
}

async fn create_certificate(
    repository: &DeployRepository,
    idempotency_key: &str,
    cert_name: &str,
    domain_ids: Vec<String>,
    scope: CertificateScope,
    method: ValidationMethod,
) -> String {
    repository
        .create_certificate(
            TENANT_ID,
            Some(ORGANIZATION_ID),
            Some(11),
            idempotency_key,
            &CreateCertificateRequest {
                cert_name: cert_name.to_owned(),
                domain_ids,
                ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
                certificate_scope: scope,
                validation_method: method,
                preferred_key_algorithm: "ECDSA".to_owned(),
                auto_renew: true,
                renew_before_days: 30,
                // Left unset because this suite installs a `FixedDns01Resolver` that
                // answers every hostname the same way. A pin here would be ignored
                // rather than tested, and a fixture that names a path it never walks
                // is how a suite starts lying about its coverage.
                provider_account_id: None,
            },
        )
        .await
        .expect("create certificate")
        .id
}

async fn open_order(
    repository: &DeployRepository,
    certificate_id: &str,
    idempotency_key: &str,
    challenge_type: &str,
) -> String {
    repository
        .request_certificate_order(
            TENANT_ID,
            &RequestCertificateOrderRequest {
                certificate_id: certificate_id.to_owned(),
                idempotency_key: idempotency_key.to_owned(),
                challenge_type: Some(challenge_type.to_owned()),
            },
        )
        .await
        .expect("request certificate order")
        .id
}

/// The console's request context, so a test drives the same entrypoint the UI does.
fn app_context() -> DeployAppRequestContext {
    DeployAppRequestContext {
        tenant_id: TENANT_ID,
        actor_id: Some(11),
        organization_id: Some(ORGANIZATION_ID),
        session_id: Some("pebble-e2e".to_owned()),
        auth_token: None,
        access_token: None,
    }
}

/// The certificate the console asks for, for `E2E_HOSTNAME`.
fn console_request(
    domain_ids: Vec<String>,
    scope: CertificateScope,
    method: ValidationMethod,
) -> CreateCertificateRequest {
    CreateCertificateRequest {
        cert_name: E2E_HOSTNAME.to_owned(),
        domain_ids,
        ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
        certificate_scope: scope,
        validation_method: method,
        preferred_key_algorithm: "ECDSA".to_owned(),
        auto_renew: true,
        renew_before_days: 30,
        provider_account_id: None,
    }
}

/// Moves a certificate into the window where renewal is due.
///
/// Written rather than waited for: a genuinely expiring certificate would have to have
/// been minted months ago, and this suite has to run in seconds. The scheduler reads
/// only these columns, so this is the state it would be looking at.
///
/// The certificate's mirror and the version it points at move together, because the rule
/// reads `COALESCE(version.not_after, certificate.active_not_after)` while the scheduler's
/// index is built on `active_not_after`. Moving only one would describe a state the design
/// guarantees cannot exist, and the assertion after the renewal would be about fiction.
async fn age_into_renewal_window(pool: &PgPool, certificate_id: &str) {
    sqlx::query(
        "UPDATE deploy_certificate_version
            SET not_before = NOW() - INTERVAL '85 days',
                not_after  = NOW() + INTERVAL '5 days'
          WHERE certificate_id = (SELECT id FROM deploy_certificate WHERE uuid = $1)",
    )
    .bind(certificate_id)
    .execute(pool)
    .await
    .expect("age the certificate version into its renewal window");
    sqlx::query(
        "UPDATE deploy_certificate
            SET active_not_before = NOW() - INTERVAL '85 days',
                active_not_after  = NOW() + INTERVAL '5 days'
          WHERE uuid = $1",
    )
    .bind(certificate_id)
    .execute(pool)
    .await
    .expect("age the certificate mirror into its renewal window");
}

/// The version the certificate currently serves, addressed by its uuid.
async fn serving_version_uuid(pool: &PgPool, certificate_id: &str) -> String {
    sqlx::query_scalar(
        "SELECT v.uuid
           FROM deploy_certificate_version v
           JOIN deploy_certificate c ON c.current_version_id = v.id
          WHERE c.uuid = $1",
    )
    .bind(certificate_id)
    .fetch_one(pool)
    .await
    .expect("read the serving version")
}

/// Builds the service with the *production* issuance engine and custody wiring.
///
/// The engine comes from the host's own factory, so a break in the environment
/// contract between the worker and the engine shows up here rather than in production.
fn issuance_service(
    repository: &Arc<DeployRepository>,
    env: &PebbleEnv,
    dns01: Option<Arc<dyn CertificateDns01PresenterPort>>,
) -> DeployService {
    let repository_port: Arc<dyn DeployRepositoryPort> = repository.clone();
    let issuance = acme_issuance_from_env()
        .expect("ACME issuance configuration")
        .expect("an ACME engine must be configured for this suite");
    let anchors = TrustAnchorBundle::from_pem(&env.issuing_root_pem())
        .expect("the controlled CA's root must parse as a trust anchor");
    let mut service = DeployService::new(repository_port, Arc::new(MemoryDeployDrivePort))
        .with_certificate_issuance(issuance)
        .with_certificate_material_key_provider(Arc::new(
            FileKeyProvider::from_material(&common::test_secret_key(), "file:e2e/master.key")
                .expect("custody provider"),
        ))
        .with_certificate_trust_anchors(Arc::new(anchors));
    if let Some(dns01) = dns01 {
        service = service.with_certificate_dns01_presenter(dns01);
    }
    service
}

/// The order's terminal status, a stored version, and the freshness of both.
async fn assert_version_stored(pool: &PgPool, certificate_id: &str, order_id: &str) -> String {
    let order_status: String =
        sqlx::query_scalar("SELECT status FROM deploy_certificate_order WHERE uuid = $1")
            .bind(order_id)
            .fetch_one(pool)
            .await
            .expect("read the order status");
    assert_eq!(order_status, "VERSION_STORED", "order {order_id}");

    let row: (String, String, i64, Option<String>) = sqlx::query_as(
        "SELECT v.status, c.status, v.version_no, v.issuer
           FROM deploy_certificate_version v
           JOIN deploy_certificate c ON c.current_version_id = v.id
          WHERE c.uuid = $1",
    )
    .bind(certificate_id)
    .fetch_one(pool)
    .await
    .expect("the certificate must have a current version");
    // `ACTIVE` on both the version and the certificate is the whole point: an order that
    // reached VERSION_STORED but left the certificate without a usable version would be
    // a green order and a red certificate.
    assert_eq!(row.0, "ACTIVE", "stored version status");
    assert_eq!(row.1, "ACTIVE", "certificate status");
    assert!(row.2 >= 1, "version number");
    let issuer = row.3.expect("the stored version must record its issuer");
    assert!(
        issuer.to_ascii_lowercase().contains("pebble"),
        "the version must have been issued by the controlled CA, got {issuer}"
    );
    // The sealed material must exist, or the certificate is metadata without a key.
    let material_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_certificate_material m
           JOIN deploy_certificate_version v ON m.certificate_version_id = v.id
           JOIN deploy_certificate c ON c.current_version_id = v.id
          WHERE c.uuid = $1",
    )
    .bind(certificate_id)
    .fetch_one(pool)
    .await
    .expect("count stored material");
    assert!(
        material_rows >= 2,
        "a leaf and a private key must both be stored, found {material_rows}"
    );
    issuer
}

#[tokio::test]
#[ignore = "requires a running Pebble CA; see .workbuddy/tmp/pebble/run-e2e.sh"]
async fn http01_exact_domain_reaches_version_stored() {
    init_tracing();
    let env = PebbleEnv::from_env();
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-certificate-issuance-pebble-test")
            .await;
    seed_zone(&pool).await;
    seed_acme_account(&repository).await;
    seed_domain(&pool, "domain-e2e-exact", 40, E2E_HOSTNAME, "EXACT").await;

    let certificate_id = create_certificate(
        &repository,
        "e2e-http01-certificate",
        E2E_HOSTNAME,
        vec!["domain-e2e-exact".to_owned()],
        CertificateScope::SingleDomain,
        // `AUTO` on a single name is the edge path, so this asserts the default a real
        // request takes rather than an explicitly DNS-only request.
        ValidationMethod::Auto,
    )
    .await;
    let order_id = open_order(&repository, &certificate_id, "e2e-http01-order", "HTTP_01").await;

    let service = issuance_service(&Arc::new(repository.clone()), &env, None);
    let result = service
        .process_due_certificate_orders(WORKER_ID, 5, 900)
        .await
        .expect("the issuance batch must run");

    assert_eq!(result.claimed, 1, "the pending order must be claimed");
    assert_stored(&pool, &order_id, result.stored, "HTTP-01").await;
    assert_eq!(result.failed, 0, "no order may fail: {result:?}");
    let issuer = assert_version_stored(&pool, &certificate_id, &order_id).await;
    println!("HTTP-01 issuance stored by issuer: {issuer}");
}

#[tokio::test]
#[ignore = "requires a running Pebble CA; see .workbuddy/tmp/pebble/run-e2e.sh"]
async fn dns01_wildcard_reaches_version_stored() {
    init_tracing();
    let env = PebbleEnv::from_env();
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-certificate-issuance-pebble-test")
            .await;
    seed_zone(&pool).await;
    seed_acme_account(&repository).await;
    // Two rows with *different* hostname strings, which is what the unique index on
    // `deploy_domain.hostname_ascii` permits and requires: the wildcard identifier is
    // derived from the string itself (`*.name`), and the apex is a separate claim that
    // the plan adds and then re-resolves against this same table.
    seed_domain(
        &pool,
        "domain-e2e-wildcard",
        41,
        "*.deploy-e2e.test",
        "WILDCARD",
    )
    .await;
    seed_domain(&pool, "domain-e2e-apex", 42, E2E_HOSTNAME, "EXACT").await;

    let certificate_id = create_certificate(
        &repository,
        "e2e-dns01-certificate",
        E2E_HOSTNAME,
        vec![
            "domain-e2e-wildcard".to_owned(),
            "domain-e2e-apex".to_owned(),
        ],
        CertificateScope::Wildcard,
        ValidationMethod::Auto,
    )
    .await;
    let order_id = open_order(&repository, &certificate_id, "e2e-dns01-order", "DNS_01").await;

    // The engine derives the TXT values; this presenter only carries them to the
    // authoritative server the CA will query, which is the whole job of a provider
    // adapter.
    let presenter: Arc<dyn Dns01Presenter> =
        Arc::new(ChallTestSrvDns01Presenter::new(env.management_url.clone()));
    let resolver = FixedDns01Resolver {
        context: CertificateDns01Context {
            presenter,
            zone_apex: E2E_HOSTNAME.to_owned(),
        },
    };

    let service = issuance_service(
        &Arc::new(repository.clone()),
        &env,
        Some(Arc::new(resolver)),
    );
    let result = service
        .process_due_certificate_orders(WORKER_ID, 5, 900)
        .await
        .expect("the issuance batch must run");

    assert_eq!(result.claimed, 1, "the pending order must be claimed");
    assert_stored(&pool, &order_id, result.stored, "wildcard DNS-01").await;
    assert_eq!(result.failed, 0, "no order may fail: {result:?}");
    let issuer = assert_version_stored(&pool, &certificate_id, &order_id).await;

    // Both identifiers must appear in the issued leaf: a CA that returned only the
    // wildcard would leave the apex unserved, and the service's name-set check is what
    // rejects that.
    let identifiers: Vec<String> = sqlx::query_scalar(
        "SELECT hostname_ascii FROM deploy_certificate_identifier
          WHERE certificate_id = (SELECT id FROM deploy_certificate WHERE uuid = $1)
          ORDER BY position",
    )
    .bind(&certificate_id)
    .fetch_all(&pool)
    .await
    .expect("read the planned identifiers");
    assert!(
        identifiers.iter().any(|name| name.starts_with("*.")),
        "the plan must carry a wildcard identifier, got {identifiers:?}"
    );
    assert!(
        identifiers.iter().any(|name| name == E2E_HOSTNAME),
        "a wildcard certificate must also cover its apex, got {identifiers:?}"
    );
    println!("DNS-01 wildcard issuance stored by issuer: {issuer}");
}

/// Creating a certificate is enough to get it issued.
///
/// The console path used to dead-end: creation wrote a `PENDING` row and requested no
/// order, the renewal sweep will not touch a certificate that has no validity window, and
/// the renew endpoint refused anything that was not already `ACTIVE`. Nothing here opens
/// an order by hand — creation does it — and that is the whole assertion.
#[tokio::test]
#[ignore = "requires a running Pebble CA; see .workbuddy/tmp/pebble/run-e2e.sh"]
async fn creating_a_certificate_issues_it_against_the_controlled_ca() {
    init_tracing();
    let env = PebbleEnv::from_env();
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-certificate-creation-pebble-test")
            .await;
    seed_zone(&pool).await;
    seed_acme_account(&repository).await;
    seed_domain(&pool, "domain-e2e-create", 40, E2E_HOSTNAME, "EXACT").await;

    let service = issuance_service(&Arc::new(repository), &env, None);
    let certificate_id = service
        .create_certificate(
            &app_context(),
            "e2e-create-certificate",
            &console_request(
                vec!["domain-e2e-create".to_owned()],
                CertificateScope::SingleDomain,
                ValidationMethod::Auto,
            ),
        )
        .await
        .expect("create a certificate through the app surface")
        .id;

    let orders: Vec<String> = sqlx::query_scalar(
        "SELECT o.uuid
           FROM deploy_certificate_order o
           JOIN deploy_certificate c ON c.id = o.certificate_id
          WHERE c.uuid = $1",
    )
    .bind(&certificate_id)
    .fetch_all(&pool)
    .await
    .expect("read the order creation requested");
    assert_eq!(
        orders.len(),
        1,
        "creation must request exactly one order, without the caller asking"
    );

    let result = service
        .process_due_certificate_orders(WORKER_ID, 5, 900)
        .await
        .expect("the issuance batch must run");
    assert_eq!(result.claimed, 1, "the requested order must be claimed");
    assert_eq!(result.failed, 0, "no order may fail: {result:?}");
    assert_stored(&pool, &orders[0], result.stored, "created-then-issued").await;

    let issuer = assert_version_stored(&pool, &certificate_id, &orders[0]).await;
    println!("issuance after console creation stored by issuer: {issuer}");
}

/// An expiring certificate is renewed all the way to a second version.
///
/// Both halves had been proven in isolation: the renewal sweep claims and orders, and the
/// executor issues an order it is handed. Nobody had run them one after the other, and
/// that is the only way to know the order the sweep opens is one the executor can finish —
/// a challenge type the planner resolves differently from the one the executor expects
/// would pass both halves and fail only here.
#[tokio::test]
#[ignore = "requires a running Pebble CA; see .workbuddy/tmp/pebble/run-e2e.sh"]
async fn an_expiring_certificate_is_renewed_to_a_second_version() {
    init_tracing();
    let env = PebbleEnv::from_env();
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-certificate-renewal-pebble-test")
            .await;
    seed_zone(&pool).await;
    seed_acme_account(&repository).await;
    seed_domain(&pool, "domain-e2e-renew", 40, E2E_HOSTNAME, "EXACT").await;

    let service = issuance_service(&Arc::new(repository), &env, None);

    // The first issuance, through the console path, so there is a version to replace.
    let certificate_id = service
        .create_certificate(
            &app_context(),
            "e2e-renew-certificate",
            &console_request(
                vec!["domain-e2e-renew".to_owned()],
                CertificateScope::SingleDomain,
                ValidationMethod::Auto,
            ),
        )
        .await
        .expect("create a certificate")
        .id;
    let first_issuance = service
        .process_due_certificate_orders(WORKER_ID, 5, 900)
        .await
        .expect("the first issuance must run");
    assert_eq!(
        first_issuance.stored, 1,
        "the order creation requested must be issued: {first_issuance:?}"
    );
    let first_version = serving_version_uuid(&pool, &certificate_id).await;

    age_into_renewal_window(&pool, &certificate_id).await;

    // The sweep opens the renewal order. Nothing here asks for one by hand.
    let planned = service
        .plan_due_certificate_renewals(WORKER_ID, 5, 900)
        .await
        .expect("the renewal sweep must run");
    assert_eq!(
        planned.claimed, 1,
        "the expiring certificate must be claimed"
    );
    assert_eq!(planned.ordered, 1, "the claim must open exactly one order");
    assert_eq!(planned.failed, 0, "no renewal may fail: {planned:?}");

    let renewal_issuance = service
        .process_due_certificate_orders(WORKER_ID, 5, 900)
        .await
        .expect("the renewal issuance must run");
    assert_eq!(
        renewal_issuance.stored, 1,
        "the renewal order must be issued: {renewal_issuance:?}"
    );
    assert_eq!(
        renewal_issuance.failed, 0,
        "no order may fail: {renewal_issuance:?}"
    );

    // The renewed version is the one being served, and its window extends past the one it
    // replaced: a renewal that shortened coverage would be worse than none.
    let (version_no, replaced, later_window, mirror_agrees): (i64, bool, bool, bool) =
        sqlx::query_as(
            "SELECT v.version_no,
                    v.uuid <> $2,
                    v.not_after > (SELECT not_after FROM deploy_certificate_version
                                    WHERE uuid = $2),
                    c.active_not_after = v.not_after
               FROM deploy_certificate c
               JOIN deploy_certificate_version v ON v.id = c.current_version_id
              WHERE c.uuid = $1",
        )
        .bind(&certificate_id)
        .bind(&first_version)
        .fetch_one(&pool)
        .await
        .expect("read the renewed certificate");
    assert_eq!(version_no, 2, "a renewal produces the next version");
    assert!(replaced, "the served version must be the new one");
    assert!(
        later_window,
        "the renewed window must extend past the one it replaced"
    );
    assert!(
        mirror_agrees,
        "the mirror must equal the renewed version's window"
    );

    // The ledger closes with both ends named, which is the only record that coverage was
    // continuous across the handover.
    let ledger: (String, bool, bool) = sqlx::query_as(
        "SELECT r.status,
                r.previous_version_id IS NOT NULL,
                r.resulting_version_id IS NOT NULL
           FROM deploy_certificate_renewal r
           JOIN deploy_certificate c ON c.id = r.certificate_id
          WHERE c.uuid = $1 AND r.attempt_no = 1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("read the renewal attempt");
    assert_eq!(
        ledger.0, "SUCCEEDED",
        "the renewal must close its own ledger"
    );
    assert!(ledger.1, "the ledger names the version it replaced");
    assert!(ledger.2, "the ledger names the version it produced");

    // And it settles: a renewed certificate is not immediately due again, or the sweep
    // would renew the same certificate on every tick.
    let settled = service
        .plan_due_certificate_renewals(WORKER_ID, 5, 900)
        .await
        .expect("the sweep after the renewal");
    assert_eq!(
        settled.claimed, 0,
        "a renewed certificate settles: {settled:?}"
    );
}

/// A wildcard certificate renews over DNS-01, which is the only method a wildcard can use.
///
/// This is the case the single-name test above cannot cover: a wildcard SAN has no HTTP
/// proof path at all, so a renewal that resolved its challenge the way the first issuance
/// did not would still leave the certificate to expire, with a perfectly successful-looking
/// first issuance behind it. The renewal performs a real DNS-01 conversation with the CA,
/// and the challenge type is read back off the renewal order so a change that quietly routed
/// wildcards through HTTP-01 fails here instead of in production.
#[tokio::test]
#[ignore = "requires a running Pebble CA; see .workbuddy/tmp/pebble/run-e2e.sh"]
async fn a_wildcard_certificate_is_renewed_through_dns01() {
    init_tracing();
    let env = PebbleEnv::from_env();
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-certificate-wildcard-pebble-test")
            .await;
    seed_zone(&pool).await;
    seed_acme_account(&repository).await;
    seed_domain(
        &pool,
        "domain-e2e-wc-renew",
        40,
        "*.deploy-e2e.test",
        "WILDCARD",
    )
    .await;
    seed_domain(&pool, "domain-e2e-wc-apex", 41, E2E_HOSTNAME, "EXACT").await;

    // The provider adapter is the one thing swapped for the controlled CA: everything
    // above it — scope resolution, plan, order, challenge — is production code.
    let presenter: Arc<dyn Dns01Presenter> =
        Arc::new(ChallTestSrvDns01Presenter::new(env.management_url.clone()));
    let resolver = FixedDns01Resolver {
        context: CertificateDns01Context {
            presenter,
            zone_apex: E2E_HOSTNAME.to_owned(),
        },
    };
    let service = issuance_service(&Arc::new(repository), &env, Some(Arc::new(resolver)));

    let certificate_id = service
        .create_certificate(
            &app_context(),
            "e2e-wildcard-renew-certificate",
            &console_request(
                vec![
                    "domain-e2e-wc-renew".to_owned(),
                    "domain-e2e-wc-apex".to_owned(),
                ],
                CertificateScope::Wildcard,
                ValidationMethod::Auto,
            ),
        )
        .await
        .expect("create a wildcard certificate")
        .id;
    let first_issuance = service
        .process_due_certificate_orders(WORKER_ID, 5, 900)
        .await
        .expect("the first issuance must run");
    assert_eq!(
        first_issuance.stored, 1,
        "the wildcard must be issued once: {first_issuance:?}"
    );
    let first_version = serving_version_uuid(&pool, &certificate_id).await;

    age_into_renewal_window(&pool, &certificate_id).await;

    let planned = service
        .plan_due_certificate_renewals(WORKER_ID, 5, 900)
        .await
        .expect("the renewal sweep must run");
    assert_eq!(planned.claimed, 1, "the expiring wildcard must be claimed");
    assert_eq!(planned.ordered, 1, "the claim must open exactly one order");
    assert_eq!(planned.failed, 0, "no renewal may fail: {planned:?}");

    let renewal_issuance = service
        .process_due_certificate_orders(WORKER_ID, 5, 900)
        .await
        .expect("the renewal issuance must run");
    assert_eq!(
        renewal_issuance.stored, 1,
        "the renewal must be issued: {renewal_issuance:?}"
    );
    assert_eq!(
        renewal_issuance.failed, 0,
        "no order may fail: {renewal_issuance:?}"
    );

    // The sweep's order, not the first one: `renew:` is the idempotency prefix the
    // renewal path anchors to its own attempt.
    let renewal_challenge: String = sqlx::query_scalar(
        "SELECT ct.challenge_type
           FROM deploy_certificate_challenge ct
           JOIN deploy_certificate_order o ON o.id = ct.order_id
          WHERE o.certificate_id = (SELECT id FROM deploy_certificate WHERE uuid = $1)
            AND o.idempotency_key LIKE 'renew:%'
          ORDER BY ct.id DESC
          LIMIT 1",
    )
    .bind(&certificate_id)
    .fetch_one(&pool)
    .await
    .expect("read the renewal challenge");
    assert_eq!(
        renewal_challenge, "DNS_01",
        "a wildcard can only be renewed over DNS-01; an HTTP-01 renewal would never validate"
    );

    let (version_no, replaced): (i64, bool) = sqlx::query_as(
        "SELECT v.version_no, v.uuid <> $2
           FROM deploy_certificate c
           JOIN deploy_certificate_version v ON v.id = c.current_version_id
          WHERE c.uuid = $1",
    )
    .bind(&certificate_id)
    .bind(&first_version)
    .fetch_one(&pool)
    .await
    .expect("read the renewed wildcard");
    assert_eq!(
        version_no, 2,
        "the wildcard renewal produces the next version"
    );
    assert!(replaced, "the served version must be the renewed one");

    println!("wildcard renewal over {renewal_challenge} stored version {version_no}");
}

/// Resolves every hostname to one presenter.
///
/// The production resolver walks certificate pin, zone pin, account center, then
/// the deployment-level credential; this suite has one zone and one provider, so
/// that walk is not what is under test — but the selector shape still has to be
/// honoured, so the fields it would consult are deliberately ignored here rather
/// than read from a fixture that would imply they mattered.
struct FixedDns01Resolver {
    context: CertificateDns01Context,
}

#[async_trait]
impl CertificateDns01PresenterPort for FixedDns01Resolver {
    async fn resolve(
        &self,
        _selector: CertificateDns01Selector<'_>,
    ) -> DeployServiceResult<Option<CertificateDns01Context>> {
        Ok(Some(CertificateDns01Context {
            presenter: self.context.presenter.clone(),
            zone_apex: self.context.zone_apex.clone(),
        }))
    }
}

/// Publishes TXT records into `pebble-challtestsrv`, standing in for a provider adapter.
struct ChallTestSrvDns01Presenter {
    management_url: String,
}

impl ChallTestSrvDns01Presenter {
    fn new(management_url: String) -> Self {
        Self { management_url }
    }
}

#[async_trait]
impl Dns01Presenter for ChallTestSrvDns01Presenter {
    fn provider_kind(&self) -> Option<DnsProviderKind> {
        None
    }

    async fn publish(
        &self,
        request: &Dns01RecordRequest,
    ) -> sdkwork_webserver_acme_service::AcmeServiceResult<Dns01RecordHandle> {
        let body = format!(
            r#"{{"host":"{}","value":"{}"}}"#,
            request.record_name, request.record_value
        );
        post_json(&self.management_url, "/set-txt", &body).map_err(|error| {
            sdkwork_webserver_acme_service::AcmeServiceError::provider(format!(
                "publish TXT {} failed: {error}",
                request.record_name
            ))
        })?;
        Ok(Dns01RecordHandle::from(request))
    }

    async fn withdraw(
        &self,
        handle: &Dns01RecordHandle,
    ) -> sdkwork_webserver_acme_service::AcmeServiceResult<()> {
        let body = format!(r#"{{"host":"{}"}}"#, handle.record_name);
        post_json(&self.management_url, "/clear-txt", &body).map_err(|error| {
            sdkwork_webserver_acme_service::AcmeServiceError::provider(format!(
                "withdraw TXT {} failed: {error}",
                handle.record_name
            ))
        })
    }
}

/// A minimal blocking HTTP/1.1 POST.
///
/// Deliberately not a client library: the only thing this suite needs from
/// `pebble-challtestsrv` is one JSON POST, and pulling an HTTP stack in as a
/// dev-dependency would be more moving parts than the request it carries.
fn post_json(base_url: &str, path: &str, body: &str) -> Result<(), String> {
    let authority = base_url
        .strip_prefix("http://")
        .ok_or_else(|| format!("management url must be plain HTTP, got {base_url}"))?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream =
        TcpStream::connect(authority).map_err(|error| format!("connect {authority}: {error}"))?;
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("send request: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("read response: {error}"))?;
    let status = response.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(format!("{path} answered {status}"));
    }
    Ok(())
}
