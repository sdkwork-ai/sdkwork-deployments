//! Guards the app API surface against the "declared route, default trait body"
//! class of defect.
//!
//! `DeployAppApi` declares every app-api operation with a default body of
//! `Err(DeployServiceError::Internal("<name> API is not implemented"))`. That
//! default is intentional — it lets a host compose only the surfaces it serves —
//! but it has a sharp edge: if `DeployService` fails to **override** a method
//! that the router actually calls, the route stays mounted, the manifest stays
//! complete, every gate stays green, and the operator gets a masked
//! `500 code: 50001` in production.
//!
//! That is exactly how `GET /app/v3/api/apps/{appId}/domains` broke: the route
//! was mounted, `app_delivery.rs` had a real implementation, and the trait
//! override that connects them was missing, so the default stub ran.
//!
//! ## Why the probes go through `dyn DeployAppApi`
//!
//! `DeployService` exposes its real work as **inherent** methods
//! (`app_delivery.rs`), and separately forwards them into the trait impl
//! (`app.rs`). Calling `service.list_app_domains(..)` on the concrete type
//! resolves to the inherent method and always succeeds — even when the trait
//! override is missing. Only dispatch through `&dyn DeployAppApi` observes what
//! the router actually sees.
//!
//! ## Why the assertion is on the error, not the HTTP body
//!
//! The response body is masked by the problem+json envelope: an `Internal` error
//! renders as `{"code":50001,"detail":"An internal error occurred",...}` with no
//! trace of the operation name. String-matching the HTTP body therefore cannot
//! detect this class at all, which is why the defect survived every existing
//! route test. The unmasked signal lives on `DeployServiceError`: `kind()` is
//! `Internal` and `Display` carries `"<name> API is not implemented"`.
//!
//! Route *mounting* is covered separately by `app_surface_manifest.rs`, and
//! success-envelope shape by `app_web_framework_routes.rs`.

use std::sync::Arc;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{
    DeployAppApi, DeployAppRequestContext, DeployServiceError, DeployServiceErrorKind,
};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployService;

/// The marker every default `DeployAppApi` body renders. Asserted against a real
/// all-default impl below so detection cannot rot silently.
const DEFAULT_BODY_MARKER: &str = "API is not implemented";

fn test_repository() -> DeployRepository {
    let database_url = std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL")
        .expect("SDKWORK_DATABASE_TEST_POSTGRES_URL is required for PostgreSQL integration tests");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_lazy(&database_url)
        .expect("build lazy PostgreSQL pool");
    DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(3).expect("Snowflake generator"),
        *b"sdkwork-deploy-test-secret-00000",
    )
}

fn service() -> DeployService {
    DeployService::new(
        Arc::new(test_repository()),
        Arc::new(sdkwork_deploy_drive_port::MemoryDeployDrivePort::default()),
    )
}

/// A context whose tenant is unknown to the fixture, so every operation reaches
/// "not found" / "validation" rather than doing real work — but **never**
/// reaches a default body. A default body is input-independent: it returns
/// `Internal("... is not implemented")` whatever the arguments are. That is what
/// makes this a sound probe.
fn probe_context() -> DeployAppRequestContext {
    DeployAppRequestContext {
        tenant_id: 7,
        actor_id: Some(1),
        organization_id: Some(9),
        session_id: Some("trait-coverage-probe".to_owned()),
        auth_token: None,
        access_token: None,
    }
}

fn is_default_body(error: &DeployServiceError) -> bool {
    error.kind() == DeployServiceErrorKind::Internal
        && error.to_string().contains(DEFAULT_BODY_MARKER)
}

/// Records an app-api operation whose result is a default trait body.
macro_rules! record_default_body {
    ($unbacked:expr, $label:expr, $result:expr) => {
        if let Err(error) = $result {
            if is_default_body(&error) {
                $unbacked.push(format!("{} -> {error}", $label));
            }
        }
    };
}

/// Every app-api operation the router dispatches, probed through the trait
/// object. Enumerated by hand because the trait surface is hand-written; a new
/// route backed by an unoverridden method must be added here (and
/// `app_surface_manifest.rs` independently asserts the route set is mounted, so
/// the two lists stay honest about each other).
///
/// Deliberately absent: `tasks.*` and other backend-api-only operations — they
/// are not reachable from an app-api route and are owned by a different surface.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL (PostgreSQL integration profile)"]
async fn no_app_operation_falls_back_to_a_default_trait_body() {
    let service = service();
    let api: &dyn DeployAppApi = &service;
    let context = probe_context();
    let mut unbacked: Vec<String> = Vec::new();

    record_default_body!(
        unbacked,
        "apps.domains.list",
        api.list_app_domains(&context, "probe-id").await
    );
    record_default_body!(unbacked, "apps.list", api.list_apps(&context, 1, 20).await);
    record_default_body!(
        unbacked,
        "apps.retrieve",
        api.retrieve_app(&context, "probe-id").await
    );

    assert!(
        unbacked.is_empty(),
        "{} app-api operation(s) call a `DeployAppApi` method that `DeployService` never overrides, \
         so the default `\"... API is not implemented\"` body runs and the operator sees a masked \
         HTTP 500 (`code: 50001`):\n{:#?}",
        unbacked.len(),
        unbacked,
    );
}

/// Named individually so a regression reports the operator-facing capability
/// that broke, not just a count. This is the operation that actually shipped the
/// defect: `apps.domains.list`.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL (PostgreSQL integration profile)"]
async fn apps_domains_list_is_backed_by_the_service() {
    let service = service();
    let api: &dyn DeployAppApi = &service;

    let error = api
        .list_app_domains(&probe_context(), "probe-id")
        .await
        .expect_err("an unknown app id must not resolve");

    assert!(
        !is_default_body(&error),
        "`apps.domains.list` fell through to the default `DeployAppApi` body; `DeployService` must \
         override `list_app_domains`. error={error}"
    );
}

/// Guards the probe itself. A default body's exact rendered form is asserted
/// against a real all-default impl, so if the message ever changes the detection
/// above fails loudly instead of passing vacuously.
#[tokio::test]
async fn the_default_body_marker_is_the_real_one() {
    struct DefaultOnly;
    #[async_trait::async_trait]
    impl DeployAppApi for DefaultOnly {}

    let api: &dyn DeployAppApi = &DefaultOnly;
    let error = api
        .list_app_domains(&probe_context(), "probe-id")
        .await
        .expect_err("an all-default impl must fail");

    assert_eq!(
        error.kind(),
        DeployServiceErrorKind::Internal,
        "a default `DeployAppApi` body must be an `Internal` error; got {error:?}"
    );
    assert!(
        is_default_body(&error),
        "the default `DeployAppApi` body no longer contains {DEFAULT_BODY_MARKER:?}, so the coverage \
         assertions above would pass vacuously. Update DEFAULT_BODY_MARKER. rendered={error}"
    );
}

/// `DeployService`'s inherent method is what makes the router-override defect
/// invisible to naive tests. Pin the distinction so a future refactor that
/// merges the two does so knowingly.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL (PostgreSQL integration profile)"]
async fn the_inherent_method_is_not_the_gate() {
    let service = service();
    // The inherent method answers not-found for an unknown id: it is backed.
    let inherent = service.list_app_domains(&probe_context(), "probe-id").await;
    assert!(
        !matches!(&inherent, Err(error) if is_default_body(error)),
        "the inherent `DeployService::list_app_domains` must do real work, not return a stub: \
         {inherent:?}"
    );
}
