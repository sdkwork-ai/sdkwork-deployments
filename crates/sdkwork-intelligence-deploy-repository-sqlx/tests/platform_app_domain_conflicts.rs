//! Conflict and overlap regressions for platform app-domain provisioning.
//!
//! These cover the ordering hazards that only appear once an apex is *already*
//! taken — by another tenant, or by an operator's own root domain — which is
//! exactly the case the initialization path has to survive. The provisioner
//! creates zones, so it has to answer the same questions the operator-facing
//! `create_domain_zone` answers before it writes; when it did not, the failures
//! ranged from a bare 500 to silently nesting zones and adopting hostnames the
//! operator owned.

mod common;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

async fn test_repository() -> (DeployRepository, PgPool) {
    let pool = common::postgres_pool().await;
    sqlx::query(
        "INSERT INTO deploy_app (
            id,uuid,tenant_id,organization_id,name,slug,app_kind,app_status,
            default_environment,created_at,updated_at,version
         ) VALUES (10,'site-10',7,9,'Shop','shop','WEB','ACTIVE','production',
                   '2026-07-22T00:00:00Z','2026-07-22T00:00:00Z',1)",
    )
    .execute(&pool)
    .await
    .expect("seed deploy_app");
    (
        DeployRepository::new(
            pool.clone(),
            SnowflakeIdGenerator::new(3).expect("Snowflake generator"),
            common::test_secret_key(),
        ),
        pool,
    )
}

/// An apex another tenant already holds must answer a diagnosable conflict, not
/// a raw unique-violation.
///
/// `uk_deploy_dns_zone_active_apex` is a global unique index, but the platform
/// provisioner looked the apex up scoped to its own tenant. For any other
/// tenant the lookup answered "free" and the INSERT died on the constraint,
/// surfacing as a masked 500 with no hint about whose zone was in the way.
#[tokio::test]
async fn platform_zone_provisioning_reports_foreign_tenant_apex_as_conflict() {
    let (repository, pool) = test_repository().await;
    sqlx::query(
        "INSERT INTO deploy_dns_zone (
            id,uuid,tenant_id,organization_id,apex_hostname,display_name,dns_provider,
            provider_zone_ref,status,user_id,created_by,updated_by
         ) VALUES (770,'zone-770',999,9,'app.sdkwork.com','Other tenant platform zone',
            'platform','app.*.sdkwork.com','ACTIVE',NULL,NULL,NULL)",
    )
    .execute(&pool)
    .await
    .expect("seed foreign tenant zone");

    let error = repository
        .ensure_platform_app_zones(7, 9, Some(1), &["sdkwork.com".to_owned()])
        .await
        .expect_err("foreign tenant apex must not be stolen");
    assert!(
        matches!(
            error,
            sdkwork_deploy_contract::DeployServiceError::Conflict(_)
        ),
        "expected a conflict, got {error:?}"
    );
}

/// An operator-defined root domain must block its platform child zone.
///
/// An operator who has already defined `example.com` owns that namespace. The
/// platform wants `app.example.com` for app publishing; creating it nests two
/// zones, which makes DNS-01 challenge resolution ambiguous, mints `VERIFIED`
/// hostnames inside a domain the operator controls, and permanently blocks the
/// operator from deleting their own zone.
///
/// The refusal must be per-suffix rather than per-batch: a catalog where one
/// suffix collides must not abort the suffixes that are still free, or a single
/// operator domain would block app publishing platform-wide.
#[tokio::test]
async fn platform_zone_provisioning_refuses_to_nest_under_a_user_zone() {
    let (repository, pool) = test_repository().await;
    sqlx::query(
        "INSERT INTO deploy_dns_zone (
            id,uuid,tenant_id,organization_id,apex_hostname,display_name,status,
            user_id,created_by,updated_by
         ) VALUES (771,'zone-771',7,9,'example.com','Operator root domain','ACTIVE',1,1,1)",
    )
    .execute(&pool)
    .await
    .expect("seed operator zone");
    sqlx::query(
        "INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,status,created_by,updated_by
         ) VALUES (7711,'domain-7711',7,9,771,'example.com','EXACT','PENDING','ACTIVE',1,1)",
    )
    .execute(&pool)
    .await
    .expect("seed operator apex hostname");

    let error = repository
        .ensure_platform_app_zones(7, 9, Some(1), &["example.com".to_owned()])
        .await
        .expect_err("must not nest app.example.com under example.com");
    assert!(
        matches!(
            error,
            sdkwork_deploy_contract::DeployServiceError::Conflict(_)
        ),
        "expected a conflict, got {error:?}"
    );

    // The operator's own zone is the only one that exists: no nested
    // `app.example.com` was created next to it.
    let zones: Vec<String> = sqlx::query_scalar(
        "SELECT apex_hostname FROM deploy_dns_zone WHERE tenant_id = 7 AND deleted_at IS NULL ORDER BY apex_hostname",
    )
    .fetch_all(&pool)
    .await
    .expect("list zones");
    assert_eq!(zones, vec!["example.com".to_owned()]);
}

/// An apex the platform zone already holds for *this* tenant is reused, even
/// when the caller asks twice. Guards against the overlap check above rejecting
/// the provisioner's own zone.
#[tokio::test]
async fn platform_zone_provisioning_stays_idempotent_for_its_own_tenant() {
    let (repository, pool) = test_repository().await;
    let created = repository
        .ensure_platform_app_zones(7, 9, Some(1), &["sdkwork.com".to_owned()])
        .await
        .expect("ensure zones");
    assert_eq!(created, 1);
    let again = repository
        .ensure_platform_app_zones(7, 9, Some(1), &["sdkwork.com".to_owned()])
        .await
        .expect("ensure zones again");
    assert_eq!(again, 0, "the tenant's own platform zone is reused");

    let owner: Option<i64> = sqlx::query_scalar(
        "SELECT user_id FROM deploy_dns_zone WHERE tenant_id = 7 AND apex_hostname = 'app.sdkwork.com'",
    )
    .fetch_one(&pool)
    .await
    .expect("read user_id");
    assert!(owner.is_none(), "a platform zone carries no owner");
}

/// Provisioning an app must not silently adopt a hostname that lives in an
/// operator-owned zone.
///
/// The `deploy_domain` lookup keys on `hostname_ascii` alone (the column the
/// global unique index covers), so a name the operator already created under
/// their own zone used to be reused verbatim: the app got bound to an
/// operator-controlled hostname while the platform reported the domain as
/// provisioned, and the operator's zone silently became undeletable.
#[tokio::test]
async fn provisioning_refuses_to_adopt_an_operator_owned_hostname() {
    let (repository, pool) = test_repository().await;
    // Pin the app to a single suffix so the assertion is about the collision
    // and not about the breadth of the platform catalog.
    sqlx::query(
        "UPDATE deploy_app SET app_domain_suffixes = '[\"sdkwork.com\"]'::jsonb WHERE id = 10",
    )
    .execute(&pool)
    .await
    .expect("pin app suffix override");
    repository
        .ensure_platform_app_zones(7, 9, Some(1), &["sdkwork.com".to_owned()])
        .await
        .expect("ensure platform zone");
    // The operator's own root domain, and the exact hostname the app's default
    // publishing domain would want, already declared under it.
    sqlx::query(
        "INSERT INTO deploy_dns_zone (
            id,uuid,tenant_id,organization_id,apex_hostname,display_name,status,
            user_id,created_by,updated_by
         ) VALUES (772,'zone-772',7,9,'sdkwork.com','Operator root domain','ACTIVE',1,1,1)",
    )
    .execute(&pool)
    .await
    .expect("seed operator zone");
    sqlx::query(
        "INSERT INTO deploy_domain (
            id,uuid,tenant_id,organization_id,zone_id,hostname_ascii,hostname_type,
            verification_status,status,created_by,updated_by
         ) VALUES (7721,'domain-7721',7,9,772,'shop.app.sdkwork.com','EXACT','PENDING','ACTIVE',1,1)",
    )
    .execute(&pool)
    .await
    .expect("seed operator hostname");

    let error = repository
        .provision_app_default_domains(7, 9, Some(1), "site-10", "production")
        .await
        .expect_err("must not adopt an operator-owned hostname");
    assert!(
        matches!(
            error,
            sdkwork_deploy_contract::DeployServiceError::Conflict(_)
        ),
        "expected a conflict, got {error:?}"
    );

    let bindings: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_app_binding WHERE app_id = 10 AND deleted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("count bindings");
    assert_eq!(
        bindings, 0,
        "the app must not be bound to the operator's name"
    );

    // The operator's row is untouched: still theirs, still awaiting their own
    // verification rather than flipped to VERIFIED on the app's behalf.
    let status: String = sqlx::query_scalar(
        "SELECT verification_status FROM deploy_domain WHERE hostname_ascii = 'shop.app.sdkwork.com'",
    )
    .fetch_one(&pool)
    .await
    .expect("read operator hostname status");
    assert_eq!(status, "PENDING");
}
