//! TLS control plane integration tests: ACME account creation, certificate
//! order request with idempotency, the forward-only order state machine,
//! challenge result recording, and transactional version storage that
//! activates the certificate.
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
use sdkwork_deploy_certificate_material::MaterialKind;
use sdkwork_deploy_contract::{
    CertificateScope, CreateAcmeAccountRequest, CreateCertificateRequest,
    RequestCertificateOrderRequest, ValidationMethod,
};
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
    let orchestrator = LifecycleOrchestrator::new(database_pool(pool.clone()), module.clone())
        .with_applied_by("sdkwork-deploy-tls-test");
    orchestrator
        .init()
        .await
        .expect("init on an empty schema must bootstrap the baseline");
    orchestrator
        .migrate()
        .await
        .expect("migrate must apply the full forward migration chain");
    seed_tls_hostnames(&pool).await;
    let repository = DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(4).expect("Snowflake generator"),
        common::test_secret_key(),
    );
    // The pool is returned alongside the repository because the ACME order and
    // certificate version read ports do not expose version rows: the certificate
    // DTO carries only the *active* version uuid, so asserting that storage
    // activated the certificate requires reading the version table directly.
    (repository, pool)
}

/// A verified, active hostname for the certificate fixtures.
///
/// `create_certificate` requires every `domainId` to reference an active
/// verified hostname in the same tenant, so the TLS control plane tests need one
/// before they can create any certificate.
async fn seed_tls_hostnames(pool: &PgPool) {
    sqlx::raw_sql(
        "INSERT INTO deploy_dns_zone (
            id,uuid,tenant_id,organization_id,apex_hostname,status
         ) VALUES
            (30,'zone-tls',7,9,'sdkwork.dev','ACTIVE');
         INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,verified_at,status
         ) VALUES
            (40,'domain-1',7,9,30,'api.sdkwork.dev','EXACT','VERIFIED',NOW(),'ACTIVE');",
    )
    .execute(pool)
    .await
    .expect("seed TLS hostname resources");
}

async fn create_certificate(repository: &DeployRepository, tenant_id: i64) -> (String, String) {
    let certificate = repository
        .create_certificate(
            tenant_id,
            Some(9),
            Some(11),
            "cert-api-sdkwork-dev",
            &CreateCertificateRequest {
                cert_name: "api.sdkwork.dev".to_owned(),
                domain_ids: vec!["domain-1".to_owned()],
                ca_profile: "LETS_ENCRYPT_PRODUCTION".to_owned(),
                certificate_scope: CertificateScope::SingleDomain,
                validation_method: ValidationMethod::Auto,
                preferred_key_algorithm: "ECDSA".to_owned(),
                auto_renew: true,
                renew_before_days: sdkwork_deploy_core::CERTIFICATE_DEFAULT_RENEW_BEFORE_DAYS,
                provider_account_id: None,
            },
        )
        .await
        .expect("create certificate");
    (certificate.id, certificate.identifiers[0].clone())
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn tls_order_state_machine_completes_end_to_end() {
    let (repository, pool) = migrated_repository().await;

    let account = repository
        .create_acme_account(
            7,
            &CreateAcmeAccountRequest {
                ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
                directory_url: "https://acme-staging-v02.api.letsencrypt.org/directory".to_owned(),
                contact_email: "ops@sdkwork.dev".to_owned(),
                external_account_digest: None,
            },
        )
        .await
        .expect("create acme account");
    assert_eq!(account.ca_profile, "LETS_ENCRYPT_STAGING");

    let (certificate_id, hostname) = create_certificate(&repository, 7).await;

    let order = repository
        .request_certificate_order(
            7,
            &RequestCertificateOrderRequest {
                certificate_id: certificate_id.clone(),
                idempotency_key: "order-1".to_owned(),
                challenge_type: Some("HTTP_01".to_owned()),
            },
        )
        .await
        .expect("request order");
    assert_eq!(order.status, "REQUESTED");
    assert_eq!(order.requested_version_no, 1);

    // Idempotent replay returns the same order.
    let replay = repository
        .request_certificate_order(
            7,
            &RequestCertificateOrderRequest {
                certificate_id,
                idempotency_key: "order-1".to_owned(),
                challenge_type: Some("HTTP_01".to_owned()),
            },
        )
        .await
        .expect("idempotent replay");
    assert_eq!(replay.id, order.id);

    // One HTTP_01 challenge per identifier, referencing the hostname.
    let challenges = repository
        .list_certificate_challenges(7, &order.id, 1, 20)
        .await
        .expect("list challenges");
    assert_eq!(challenges.total, 1);
    assert_eq!(challenges.items[0].hostname, hostname);
    assert_eq!(challenges.items[0].challenge_type, "HTTP_01");
    assert_eq!(challenges.items[0].status, "PENDING");
    let challenge_id = challenges.items[0].id.clone();

    // Walk the canonical state machine to CHALLENGE_VALIDATING.
    let mut status = order.status.clone();
    for expected in [
        "ACCOUNT_READY",
        "ORDER_PENDING",
        "CHALLENGE_PRESENTING",
        "CHALLENGE_VALIDATING",
    ] {
        let advanced = repository
            .advance_certificate_order(7, &order.id, &status, expected)
            .await
            .expect("advance order");
        assert_eq!(advanced, expected, "advance to {expected}");
        status = expected.to_owned();
    }

    // A valid challenge result advances the order to FINALIZING.
    repository
        .record_challenge_result(7, &order.id, Some(&challenge_id), true, None)
        .await
        .expect("valid challenge result");
    let finalizing = repository
        .retrieve_certificate_order(7, &order.id)
        .await
        .expect("retrieve order");
    assert_eq!(finalizing.status, "FINALIZING");

    // Storing the issued version completes the order and activates the cert.
    let fixture = common::sealed_version_material();
    let facts = &fixture.facts;
    let version_uuid = fixture.certificate_version_uuid.clone();
    let completed = repository
        .store_certificate_version(
            7,
            &version_uuid,
            &order.id,
            1,
            &facts.serial_sha256,
            &facts.fingerprint_sha256,
            &facts.spki_sha256,
            &facts.chain_sha256,
            &facts.issuer,
            &facts.subject,
            &facts.key_algorithm,
            &facts.not_before,
            &facts.not_after,
            "secret://tls/api.sdkwork.dev/v1",
            &fixture.sealed,
        )
        .await
        .expect("store version");
    assert_eq!(completed.status, "VERSION_STORED");

    // The material was written with the version, in the same transaction, and the
    // private key is the one file that must not be readable as stored.
    let stored: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT m.material_kind, m.file_name, m.protection, m.content_size_bytes
           FROM deploy_certificate_material m
           JOIN deploy_certificate_version v ON v.id = m.certificate_version_id
          WHERE v.uuid = $1
          ORDER BY m.material_kind",
    )
    .bind(&version_uuid)
    .fetch_all(&pool)
    .await
    .expect("the stored material is readable");
    assert_eq!(
        stored.len(),
        5,
        "the canonical bundle is five files, got {stored:?}"
    );
    let private_key = stored
        .iter()
        .find(|row| row.0 == "PRIVATE_KEY")
        .expect("a private key row");
    assert_eq!(
        private_key.2, "ENVELOPE_AES_256_GCM",
        "the private key must never be stored as plaintext"
    );
    assert_eq!(private_key.1, "privkey.pem");
    for row in &stored {
        if row.0 != "PRIVATE_KEY" {
            assert_eq!(row.2, "NONE", "{} must be stored verbatim", row.0);
        }
    }
    // A public file's recorded size is the length of what is stored, which the
    // table enforces; the sealed one's is the length of its plaintext.
    let full_chain = stored
        .iter()
        .find(|row| row.0 == "FULL_CHAIN")
        .expect("a full chain row");
    assert!(full_chain.3 > 0);

    // `storeCertificateVersion` answers with the order (now VERSION_STORED) per
    // the OpenAPI contract, so `completed.id` is the *order* uuid and cannot
    // stand in for the version. The version identity is read back from the row
    // the transaction wrote: asserting the certificate points at it is what
    // proves storage activated the certificate rather than merely inserting a
    // candidate row.
    let active_version_uuid: String = sqlx::query_scalar(
        "SELECT v.uuid
           FROM deploy_certificate_version v
           JOIN deploy_certificate c ON c.id = v.certificate_id
          WHERE c.uuid = $1 AND v.version_no = 1 AND v.status = 'ACTIVE'",
    )
    .bind(&replay.certificate_id)
    .fetch_one(&pool)
    .await
    .expect("the stored version is ACTIVE");

    // At most one version may be ACTIVE, and it must be the one the certificate
    // now points at.
    let active_version_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM deploy_certificate_version v
           JOIN deploy_certificate c ON c.id = v.certificate_id
          WHERE c.uuid = $1 AND v.status = 'ACTIVE'",
    )
    .bind(&replay.certificate_id)
    .fetch_one(&pool)
    .await
    .expect("count ACTIVE versions");
    assert_eq!(active_version_count, 1);

    // The certificate now references the active version (visible via the
    // certificate list which joins the current version).
    let certificates = repository
        .list_certificates(7, 1, 20)
        .await
        .expect("list certificates");
    let certificate = certificates
        .items
        .iter()
        .find(|certificate| certificate.id == replay.certificate_id)
        .expect("certificate present");
    assert_eq!(certificate.current_version_id, Some(active_version_uuid));
    assert_eq!(certificate.status, "ACTIVE");
    assert_eq!(certificate.renewal_status, "NONE");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn tls_orders_fail_closed_on_invalid_transitions_and_tenants() {
    let (repository, pool) = migrated_repository().await;
    repository
        .create_acme_account(
            7,
            &CreateAcmeAccountRequest {
                ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
                directory_url: "https://acme-staging-v02.api.letsencrypt.org/directory".to_owned(),
                contact_email: "ops@sdkwork.dev".to_owned(),
                external_account_digest: None,
            },
        )
        .await
        .expect("create acme account");
    let (certificate_id, _) = create_certificate(&repository, 7).await;

    let order = repository
        .request_certificate_order(
            7,
            &RequestCertificateOrderRequest {
                certificate_id,
                idempotency_key: "order-fail".to_owned(),
                challenge_type: None,
            },
        )
        .await
        .expect("request order");

    // Skipping a state is rejected: REQUESTED -> CHALLENGE_PRESENTING is not
    // a canonical step (the optimistic UPDATE matches nothing).
    let skipped = repository
        .advance_certificate_order(7, &order.id, "REQUESTED", "CHALLENGE_PRESENTING")
        .await
        .expect("advance returns applied status");
    assert_eq!(skipped, "REQUESTED", "non-canonical step is a no-op");

    // Cross-tenant access fails closed.
    let cross_tenant = repository.retrieve_certificate_order(8, &order.id).await;
    assert!(
        cross_tenant.is_err(),
        "cross-tenant order read must fail closed"
    );

    // Failing the order with an error code lands on FAILED.
    repository
        .fail_certificate_order(7, &order.id, "ACME_NETWORK_ERROR")
        .await
        .expect("fail order");
    let failed = repository
        .retrieve_certificate_order(7, &order.id)
        .await
        .expect("retrieve failed order");
    assert_eq!(failed.status, "FAILED");
    assert_eq!(
        failed.last_error_code.as_deref(),
        Some("ACME_NETWORK_ERROR")
    );

    // Storing a version on a FAILED order is rejected. The material is real here
    // on purpose: the rejection has to come from the order's state, not from the
    // material being unusable.
    let fixture = common::sealed_version_material();
    let facts = &fixture.facts;
    let version_uuid = fixture.certificate_version_uuid.clone();
    let store = repository
        .store_certificate_version(
            7,
            &version_uuid,
            &order.id,
            1,
            &facts.serial_sha256,
            &facts.fingerprint_sha256,
            &facts.spki_sha256,
            &facts.chain_sha256,
            &facts.issuer,
            &facts.subject,
            &facts.key_algorithm,
            &facts.not_before,
            &facts.not_after,
            "secret://tls/api.sdkwork.dev/v1",
            &fixture.sealed,
        )
        .await;
    assert!(
        store.is_err(),
        "version storage on a failed order must be rejected"
    );

    // Nothing was written: the transaction rolled back whole.
    let material_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_certificate_material WHERE certificate_version_id IN
            (SELECT id FROM deploy_certificate_version WHERE uuid = $1)",
    )
    .bind(&version_uuid)
    .fetch_one(&pool)
    .await
    .expect("count material rows");
    assert_eq!(
        material_rows, 0,
        "a refused storage must not leave material behind"
    );
}

/// CAA pre-flight surface: the subject the service layer needs before an order
/// exists, and the bounded decision recorded on the order afterwards.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn caa_preflight_subject_and_decision_recording() {
    let (repository, _pool) = migrated_repository().await;

    repository
        .create_acme_account(
            7,
            &CreateAcmeAccountRequest {
                ca_profile: "LETS_ENCRYPT_PRODUCTION".to_owned(),
                directory_url: "https://acme-v02.api.letsencrypt.org/directory".to_owned(),
                contact_email: "ops@sdkwork.dev".to_owned(),
                external_account_digest: None,
            },
        )
        .await
        .expect("create acme account");

    let (certificate_id, hostname) = create_certificate(&repository, 7).await;

    // The subject must describe the certificate that is about to be ordered, and
    // must name the same directory the order will use.
    let subject = repository
        .certificate_order_caa_subject(7, &certificate_id)
        .await
        .expect("caa subject");
    assert_eq!(subject.identifiers, vec![hostname.clone()]);
    assert_eq!(subject.validation_method, "HTTP_01");
    assert_eq!(
        subject.directory_url.as_deref(),
        Some("https://acme-v02.api.letsencrypt.org/directory")
    );
    assert_eq!(subject.acme_method(), Some("http-01"));

    // An unknown certificate is a not-found, not an empty subject.
    assert!(repository
        .certificate_order_caa_subject(7, "00000000-0000-0000-0000-000000000000")
        .await
        .is_err());

    let order = repository
        .request_certificate_order(
            7,
            &RequestCertificateOrderRequest {
                certificate_id: certificate_id.clone(),
                idempotency_key: "caa-order-1".to_owned(),
                challenge_type: Some("HTTP_01".to_owned()),
            },
        )
        .await
        .expect("request order");
    // A freshly created order carries no CAA observation yet.
    assert_eq!(order.caa_decision, None);
    assert_eq!(order.caa_checked_at, None);

    let checked_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    repository
        .record_certificate_order_caa_decision(7, &order.id, "UNAUTHORIZED_CA", &checked_at)
        .await
        .expect("record caa decision");

    let recorded = repository
        .retrieve_certificate_order(7, &order.id)
        .await
        .expect("retrieve order");
    assert_eq!(recorded.caa_decision.as_deref(), Some("UNAUTHORIZED_CA"));
    assert!(
        recorded.caa_checked_at.is_some(),
        "a decision is only auditable together with the instant it was observed"
    );

    // Re-recording an identical decision does not burn a row version.
    let version_before = recorded.version.clone();
    repository
        .record_certificate_order_caa_decision(7, &order.id, "UNAUTHORIZED_CA", &checked_at)
        .await
        .expect("idempotent re-record");
    let after = repository
        .retrieve_certificate_order(7, &order.id)
        .await
        .expect("retrieve order");
    assert_eq!(after.version, version_before);

    // A decision outside the contract's three values is rejected before it can
    // reach the CHECK constraint.
    for invalid in ["MAYBE", "permitted", ""] {
        assert!(
            repository
                .record_certificate_order_caa_decision(7, &order.id, invalid, &checked_at)
                .await
                .is_err(),
            "decision {invalid:?} must be rejected"
        );
    }
    // A non-timestamp observation instant is rejected too.
    assert!(repository
        .record_certificate_order_caa_decision(7, &order.id, "PERMITTED", "not-a-timestamp")
        .await
        .is_err());
}

/// The custody loop: seal, store, read back, open, and compare.
///
/// This is the test that proves the feature rather than the plumbing. It checks
/// three separate things that a weaker test would conflate: that the private key
/// is not readable in the column, that what comes back out is byte-identical to
/// what went in, and that a row moved to another version refuses to open.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn certificate_material_round_trips_through_storage() {
    let (repository, pool) = migrated_repository().await;

    repository
        .create_acme_account(
            7,
            &CreateAcmeAccountRequest {
                ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
                directory_url: "https://acme-staging-v02.api.letsencrypt.org/directory".to_owned(),
                contact_email: "ops@sdkwork.dev".to_owned(),
                external_account_digest: None,
            },
        )
        .await
        .expect("create acme account");
    let (certificate_id, _hostname) = create_certificate(&repository, 7).await;

    let order = repository
        .request_certificate_order(
            7,
            &RequestCertificateOrderRequest {
                certificate_id,
                idempotency_key: "material-order-1".to_owned(),
                challenge_type: Some("HTTP_01".to_owned()),
            },
        )
        .await
        .expect("request order");
    let mut status = order.status.clone();
    for expected in [
        "ACCOUNT_READY",
        "ORDER_PENDING",
        "CHALLENGE_PRESENTING",
        "CHALLENGE_VALIDATING",
        "FINALIZING",
    ] {
        status = repository
            .advance_certificate_order(7, &order.id, &status, expected)
            .await
            .expect("advance order");
        assert_eq!(status, expected);
    }

    let fixture = common::sealed_version_material();
    let facts = &fixture.facts;
    let version_uuid = fixture.certificate_version_uuid.clone();
    repository
        .store_certificate_version(
            7,
            &version_uuid,
            &order.id,
            1,
            &facts.serial_sha256,
            &facts.fingerprint_sha256,
            &facts.spki_sha256,
            &facts.chain_sha256,
            &facts.issuer,
            &facts.subject,
            &facts.key_algorithm,
            &facts.not_before,
            &facts.not_after,
            "secret://tls/api.sdkwork.dev/v1",
            &fixture.sealed,
        )
        .await
        .expect("store version");

    // 1. The private key column does not hold the key. It holds ciphertext, and
    //    the table itself refuses anything else — see the CHECK constraint on
    //    `protection`.
    let (stored_protection, stored_content, stored_kek): (String, Vec<u8>, Option<String>) =
        sqlx::query_as(
            "SELECT m.protection, m.content, m.kek_ref
               FROM deploy_certificate_material m
               JOIN deploy_certificate_version v ON v.id = m.certificate_version_id
              WHERE v.uuid = $1 AND m.material_kind = 'PRIVATE_KEY'",
        )
        .bind(&version_uuid)
        .fetch_one(&pool)
        .await
        .expect("the private key row");
    assert_eq!(stored_protection, "ENVELOPE_AES_256_GCM");
    assert_eq!(
        stored_kek.as_deref(),
        Some(common::TEST_CUSTODY_KEK_REF),
        "the row must record which master key wrapped its data key"
    );
    let plaintext_key = fixture
        .plaintext
        .iter()
        .find(|file| file.kind == MaterialKind::PrivateKey)
        .expect("the fixture has a private key")
        .content
        .clone();
    assert_ne!(
        stored_content, plaintext_key,
        "the key was stored in the clear"
    );
    assert!(
        !stored_content
            .windows(b"PRIVATE KEY".len())
            .any(|window| window == b"PRIVATE KEY"),
        "the stored bytes still look like PEM"
    );

    // 2. Reading back through the service reproduces every file byte for byte.
    let service = DeployService::new(
        Arc::new(repository.clone()),
        Arc::new(MemoryDeployDrivePort),
    )
    .with_certificate_material_key_provider(Arc::new(common::test_custody_provider()));
    let opened = service
        .open_certificate_material(7, &version_uuid)
        .await
        .expect("open the stored material");
    assert_eq!(opened.len(), MaterialKind::ALL.len());
    for (restored, original) in opened.iter().zip(fixture.plaintext.iter()) {
        assert_eq!(
            restored.kind, original.kind,
            "bundle order must be canonical"
        );
        assert_eq!(
            restored.content, original.content,
            "{:?} did not survive the round trip",
            original.kind
        );
    }

    // 3. Another tenant cannot read it, and a row moved to a different version
    //    fails authentication rather than being served under the wrong identity.
    assert!(
        service
            .open_certificate_material(8, &version_uuid)
            .await
            .is_err(),
        "cross-tenant material read must fail closed"
    );
    let other_uuid = sdkwork_database_id::uuid_v4();
    sqlx::query("UPDATE deploy_certificate_version SET uuid = $1 WHERE uuid = $2")
        .bind(&other_uuid)
        .bind(&version_uuid)
        .execute(&pool)
        .await
        .expect("repoint the version");
    let moved = service
        .open_certificate_material(7, &other_uuid)
        .await
        .expect_err("material bound to another version must not open");
    assert!(
        format!("{moved}").contains("different version"),
        "unexpected error: {moved}"
    );
}
