mod common;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{CertificateScope, CreateCertificateRequest, ValidationMethod};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;

async fn seed_domains(pool: &sqlx::PgPool) {
    sqlx::raw_sql(
        "INSERT INTO deploy_dns_zone (
            id,uuid,tenant_id,organization_id,apex_hostname,status
         ) VALUES
            (10,'zone-primary',7,9,'example.com','ACTIVE'),
            (11,'zone-foreign',8,9,'foreign.example','ACTIVE');
         INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,verified_at,status
         ) VALUES
            (20,'domain-apex',7,9,10,'example.com','EXACT','VERIFIED',NOW(),'ACTIVE'),
            (21,'domain-docs',7,9,10,'docs.example.com','EXACT','VERIFIED',NOW(),'ACTIVE'),
            (22,'domain-pending',7,9,10,'pending.example.com','EXACT','PENDING',NULL,'ACTIVE'),
            (23,'domain-foreign',8,9,11,'foreign.example','EXACT','VERIFIED',NOW(),'ACTIVE'),
            (24,'domain-wildcard',7,9,10,'*.example.com','WILDCARD','VERIFIED',NOW(),'ACTIVE');",
    )
    .execute(pool)
    .await
    .expect("seed certificate hostname resources");
}

/// The zone-ownership half of the creation gate.
///
/// `deploy_dns_zone.user_id` is the unit of ownership: `NULL` marks a
/// tenant-level zone every member reaches, a non-NULL value marks a zone private
/// to that user (the DDL states this next to the column). The certificate gate
/// used to test the tenant alone, which accepted every verified hostname in the
/// tenant — including one in another user's private zone. These rows are
/// `VERIFIED` + `ACTIVE`, so nothing but the zone check distinguishes them.
async fn seed_zone_ownership(pool: &sqlx::PgPool) {
    sqlx::raw_sql(
        "INSERT INTO deploy_dns_zone (
            id,uuid,tenant_id,organization_id,apex_hostname,status,user_id
         ) VALUES
            (30,'zone-mine',7,9,'mine.example','ACTIVE',11),
            (31,'zone-theirs',7,9,'theirs.example','ACTIVE',12),
            (32,'zone-tenant-level',7,9,'app.example','ACTIVE',NULL);
         INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,verified_at,status
         ) VALUES
            (40,'domain-mine',7,9,30,'mine.example','EXACT','VERIFIED',NOW(),'ACTIVE'),
            (41,'domain-theirs',7,9,31,'theirs.example','EXACT','VERIFIED',NOW(),'ACTIVE'),
            (42,'domain-tenant-level',7,9,32,'app.example','EXACT','VERIFIED',NOW(),'ACTIVE');",
    )
    .execute(pool)
    .await
    .expect("seed zone ownership resources");
}

/// A caller may only build a certificate over hostnames in zones they reach.
///
/// The escalation this pins: a hostname in another user's private zone is
/// `VERIFIED` and `ACTIVE`, so a tenant-only predicate accepted it and let any
/// member of the tenant order a certificate covering someone else's name. The
/// fix gates on the zone, which is the ownership unit the domain inventory
/// already speaks in.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn certificates_reject_hostnames_from_another_users_private_zone() {
    let pool = common::postgres_pool().await;
    seed_zone_ownership(&pool).await;
    let repository = DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(5).expect("Snowflake generator"),
        common::test_secret_key(),
    );

    // The caller's own zone: the certificate they are entitled to.
    repository
        .create_certificate(
            7,
            Some(9),
            Some(11),
            "certificate-own-zone",
            &request("Own zone", &["domain-mine"]),
        )
        .await
        .expect("a caller reaches a hostname in their own zone");

    // Another user's private zone: the escalation. Same tenant, same shape of
    // row, `VERIFIED` + `ACTIVE` — only the zone's owner differs.
    let foreign = repository
        .create_certificate(
            7,
            Some(9),
            Some(11),
            "certificate-other-private-zone",
            &request("Other private zone", &["domain-theirs"]),
        )
        .await;
    assert!(
        foreign.is_err(),
        "a caller must not reach a hostname in another user's private zone, got {foreign:?}"
    );

    // A tenant-level zone (`user_id IS NULL`) is shared with every member, which
    // is what the inventory documents. The gate must keep both halves distinct:
    // denying this would be the fix overreaching past the model.
    repository
        .create_certificate(
            7,
            Some(9),
            Some(11),
            "certificate-tenant-level-zone",
            &request("Tenant level zone", &["domain-tenant-level"]),
        )
        .await
        .expect("a tenant-level zone is reachable by every member");
}

fn request(cert_name: &str, domain_ids: &[&str]) -> CreateCertificateRequest {
    scoped_request(cert_name, domain_ids, CertificateScope::SingleDomain)
}

fn scoped_request(
    cert_name: &str,
    domain_ids: &[&str],
    certificate_scope: CertificateScope,
) -> CreateCertificateRequest {
    CreateCertificateRequest {
        cert_name: cert_name.to_owned(),
        domain_ids: domain_ids.iter().map(|value| (*value).to_owned()).collect(),
        ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
        certificate_scope,
        validation_method: ValidationMethod::Auto,
        preferred_key_algorithm: "ECDSA".to_owned(),
        // Spelled out rather than left to `Default`, because these are the values
        // the scheduling tests below depend on: a certificate created here has to
        // be renewable on the same terms one created through the API would be.
        auto_renew: true,
        renew_before_days: sdkwork_deploy_core::CERTIFICATE_DEFAULT_RENEW_BEFORE_DAYS,
        // Left unset: resolution then walks past the pin to the zone and the
        // deployment-level configuration, which is the path these tests exercise.
        provider_account_id: None,
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn certificates_enforce_scope_identifier_shapes_and_creation_boundaries() {
    let pool = common::postgres_pool().await;
    seed_domains(&pool).await;
    let repository = DeployRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(5).expect("Snowflake generator"),
        common::test_secret_key(),
    );

    // The only multi-identifier shape the scope model allows is a wildcard
    // certificate: `*.example.com` does not cover `example.com`, so the apex is
    // planned in addition (§5.1). A single-domain request is pinned to exactly
    // one hostname, which the invalid-request block below asserts.
    let wildcard_request = scoped_request(
        "Primary ECDSA",
        &["domain-wildcard"],
        CertificateScope::Wildcard,
    );
    let first = repository
        .create_certificate(
            7,
            Some(9),
            Some(11),
            "certificate-primary-ecdsa",
            &wildcard_request,
        )
        .await
        .expect("create one wildcard certificate covering its apex");
    assert_eq!(
        first.identifiers,
        vec!["*.example.com".to_owned(), "example.com".to_owned()]
    );
    assert_eq!(first.certificate_scope, CertificateScope::Wildcard);
    // A wildcard cannot be authorized over HTTP-01, so `AUTO` has to resolve to
    // DNS-01 instead of keeping a preference the order could never use.
    assert_eq!(first.validation_method, ValidationMethod::Dns01);
    assert_eq!(first.status, "PENDING");

    let replay = repository
        .create_certificate(
            7,
            Some(9),
            Some(11),
            "certificate-primary-ecdsa",
            &wildcard_request,
        )
        .await
        .expect("replay identical certificate request");
    assert_eq!(replay.id, first.id);

    let second = repository
        .create_certificate(
            7,
            Some(9),
            Some(11),
            "certificate-primary-rsa",
            &CreateCertificateRequest {
                preferred_key_algorithm: "RSA".to_owned(),
                ..request("Primary RSA", &["domain-apex"])
            },
        )
        .await
        .expect("associate another certificate with the same hostname");
    assert_ne!(second.id, first.id);
    let apex_certificate_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT ci.certificate_id)
         FROM deploy_certificate_identifier ci
         JOIN deploy_domain d ON d.id = ci.domain_id
         WHERE d.uuid = 'domain-apex'",
    )
    .fetch_one(&pool)
    .await
    .expect("count certificates for apex hostname");
    assert_eq!(apex_certificate_count, 2);

    let idempotency_conflict = repository
        .create_certificate(
            7,
            Some(9),
            Some(11),
            "certificate-primary-ecdsa",
            &request("Changed request", &["domain-apex"]),
        )
        .await
        .expect_err("reject idempotency key reuse with another request");
    assert!(idempotency_conflict.to_string().contains("Idempotency-Key"));

    for (key, invalid_request) in [
        (
            "certificate-duplicate-domain",
            request("Duplicate", &["domain-apex", "domain-apex"]),
        ),
        // §5.2 rule 3: a single-domain certificate covers exactly one hostname,
        // so a second one is refused rather than shipped under a scope that
        // claims a single domain.
        (
            "certificate-single-scope-multiple-hostnames",
            request("Two hostnames", &["domain-apex", "domain-docs"]),
        ),
        // §5.2 rule 2: a wildcard identifier needs the WILDCARD scope.
        (
            "certificate-single-scope-wildcard",
            request("Wildcard under single scope", &["domain-wildcard"]),
        ),
        (
            "certificate-pending-domain",
            request("Pending", &["domain-pending"]),
        ),
        (
            "certificate-cross-tenant-domain",
            request("Foreign", &["domain-foreign"]),
        ),
    ] {
        assert!(repository
            .create_certificate(7, Some(9), Some(11), key, &invalid_request)
            .await
            .is_err());
    }
}
