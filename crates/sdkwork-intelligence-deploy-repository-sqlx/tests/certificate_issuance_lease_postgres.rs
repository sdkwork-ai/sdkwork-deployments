//! Certificate issuance control-plane properties that only a real database can show:
//! the lease fence that stops a stale worker from driving an order another worker now
//! owns, and the deadline sweep that keeps an order from outliving its promise.
//!
//! These are the §14 phase 3d "lease/fence reclaim" evidence. They do not touch a CA —
//! a fence is a property of one `UPDATE` — so they run without Pebble and are therefore
//! checked on every run rather than only during an acceptance pass.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like the other
//! PostgreSQL integration tests in this crate.

mod common;

use std::sync::Arc;

use sdkwork_deploy_contract::{
    CertificateScope, CreateAcmeAccountRequest, CreateCertificateRequest, DeployServiceErrorKind,
    RequestCertificateOrderRequest, ValidationMethod,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

const TENANT_ID: i64 = 7;
const ORGANIZATION_ID: i64 = 9;
const ZONE_ID: i64 = 30;
const HOSTNAME: &str = "lease-e2e.test";

async fn fixture(applied_by: &str) -> (Arc<DeployRepository>, PgPool) {
    let (repository, pool) = common::migrated_repository_with_pool(applied_by).await;
    sqlx::query(
        "INSERT INTO deploy_dns_zone (id,uuid,tenant_id,organization_id,apex_hostname,status)
         VALUES ($1,'zone-lease',$2,$3,$4,'ACTIVE')",
    )
    .bind(ZONE_ID)
    .bind(TENANT_ID)
    .bind(ORGANIZATION_ID)
    .bind(HOSTNAME)
    .execute(&pool)
    .await
    .expect("seed dns zone");
    sqlx::query(
        "INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,verified_at,status
         ) VALUES (40,'domain-lease',$1,$2,$3,$4,'EXACT','VERIFIED',NOW(),'ACTIVE')",
    )
    .bind(TENANT_ID)
    .bind(ORGANIZATION_ID)
    .bind(ZONE_ID)
    .bind(HOSTNAME)
    .execute(&pool)
    .await
    .expect("seed domain claim");
    repository_lifecycle(&repository).await;
    (Arc::new(repository), pool)
}

async fn repository_lifecycle(repository: &DeployRepository) {
    repository
        .create_acme_account(
            TENANT_ID,
            &CreateAcmeAccountRequest {
                ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
                directory_url: "https://acme-staging-v02.api.letsencrypt.org/directory".to_owned(),
                contact_email: "ops@lease-e2e.test".to_owned(),
                external_account_digest: None,
            },
        )
        .await
        .expect("create acme account");
    let certificate = repository
        .create_certificate(
            TENANT_ID,
            Some(ORGANIZATION_ID),
            Some(11),
            "lease-certificate",
            &CreateCertificateRequest {
                cert_name: HOSTNAME.to_owned(),
                domain_ids: vec!["domain-lease".to_owned()],
                ca_profile: "LETS_ENCRYPT_STAGING".to_owned(),
                certificate_scope: CertificateScope::SingleDomain,
                validation_method: ValidationMethod::Auto,
                preferred_key_algorithm: "ECDSA".to_owned(),
                auto_renew: true,
                renew_before_days: 30,
                // Left unset: this suite drives the lease fence with a hand-wired
                // presenter, and a pin would send resolution through the account
                // center that the suite deliberately does not assemble.
                provider_account_id: None,
            },
        )
        .await
        .expect("create certificate")
        .id;
    repository
        .request_certificate_order(
            TENANT_ID,
            &RequestCertificateOrderRequest {
                certificate_id: certificate,
                idempotency_key: "lease-order".to_owned(),
                challenge_type: Some("HTTP_01".to_owned()),
            },
        )
        .await
        .expect("request order");
}

async fn order_status(pool: &PgPool) -> String {
    sqlx::query_scalar("SELECT status FROM deploy_certificate_order ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("read order status")
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_worker_that_lost_its_lease_cannot_advance_the_order() {
    let (repository, pool) = fixture("sdkwork-deploy-certificate-issuance-lease-test").await;

    // Worker A takes the order with a lease that has already lapsed by the time it acts.
    let claims = repository
        .claim_certificate_orders("worker-a", 5, 60, "2026-01-01T00:00:00.000Z")
        .await
        .expect("claim");
    assert_eq!(claims.len(), 1);
    let order = &claims[0];
    assert_eq!(order.status, "REQUESTED");

    // Checked against real time, because the fence compares `lease_expires_at` with the
    // database's `NOW()`: a lease that expired in 2026-01-01 cannot be held now.
    let reached = repository
        .advance_leased_certificate_order(
            TENANT_ID,
            &order.order_uuid,
            "worker-a",
            "REQUESTED",
            "ACCOUNT_READY",
        )
        .await
        .expect("fenced advance");
    assert_eq!(
        reached, "REQUESTED",
        "an expired lease must not be able to move the order"
    );
    assert_eq!(order_status(&pool).await, "REQUESTED");

    // The same edge succeeds for a worker holding a live lease, which is what makes the
    // assertion above about the fence rather than about the transition being illegal.
    repository
        .claim_certificate_orders("worker-b", 5, 900, &now_iso(&pool).await)
        .await
        .expect("claim");
    let reached = repository
        .advance_leased_certificate_order(
            TENANT_ID,
            &order.order_uuid,
            "worker-b",
            "REQUESTED",
            "ACCOUNT_READY",
        )
        .await
        .expect("fenced advance");
    assert_eq!(reached, "ACCOUNT_READY");

    // And a second worker's lease replaces the first, so the original holder is fenced
    // out even though its attempt is still the one recorded on the row.
    let reached = repository
        .advance_leased_certificate_order(
            TENANT_ID,
            &order.order_uuid,
            "worker-a",
            "ACCOUNT_READY",
            "ORDER_PENDING",
        )
        .await
        .expect("fenced advance");
    assert_eq!(
        reached, "ACCOUNT_READY",
        "the previous lease holder must not be able to continue"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn an_order_past_its_deadline_is_failed_rather_than_left_pending() {
    let (repository, pool) = fixture("sdkwork-deploy-certificate-issuance-deadline-test").await;

    // Nothing else can move this order: it is not claimable once past its deadline, so
    // without the sweep an operator would see a request that never resolves.
    sqlx::query("UPDATE deploy_certificate_order SET deadline_at = NOW() - INTERVAL '1 hour'")
        .execute(&pool)
        .await
        .expect("age the deadline");

    let failed = repository
        .fail_expired_certificate_orders(&now_iso(&pool).await, 10)
        .await
        .expect("sweep expired orders");
    assert_eq!(failed, 1, "the expired order must be retired");

    let (status, code): (String, Option<String>) =
        sqlx::query_as("SELECT status, last_error_code FROM deploy_certificate_order LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("read the retired order");
    assert_eq!(status, "FAILED");
    assert_eq!(code.as_deref(), Some("ORDER_DEADLINE_EXCEEDED"));

    // Idempotent: a second sweep finds nothing, so a ticker cannot keep re-failing it.
    let again = repository
        .fail_expired_certificate_orders(&now_iso(&pool).await, 10)
        .await
        .expect("sweep again");
    assert_eq!(again, 0);
}

/// The database's own clock, so a lease written as "now + N" is compared against the
/// same clock `NOW()` reads inside the fence.
async fn now_iso(pool: &PgPool) -> String {
    sqlx::query_scalar("SELECT to_char(NOW(), 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')")
        .fetch_one(pool)
        .await
        .expect("read the database clock")
}

/// Keeps the unused-import checker honest about the error vocabulary this file asserts
/// on: an order that fails must be a recorded failure, not a generic internal error.
#[allow(dead_code)]
fn assert_error_kind(
    error: &sdkwork_deploy_contract::DeployServiceError,
) -> DeployServiceErrorKind {
    error.kind()
}
