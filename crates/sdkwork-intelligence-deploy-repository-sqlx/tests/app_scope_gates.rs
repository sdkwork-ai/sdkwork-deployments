//! Application-scope gates on the app-api data plane, asserted **without** a
//! database.
//!
//! Every refusal here happens before the repository is consulted, so the service
//! is built on a deliberately unreachable lazy pool and the assertion still
//! holds: the answer comes from the credential, not from the database. The `own
//! application` case proves the gates are not a blanket denial — it reaches the
//! repository and fails there, which is a different error kind.
//!
//! `APPLICATION_IDENTITY_SPEC.md` §4.1: "A token `MUST NOT` be accepted by
//! another application"; §4.3: data-plane queries filter by `(tenant_id,
//! app_id)`. Real-database coverage of the same tables lives in
//! `artifact_app_scope_postgres.rs`, which is skipped without
//! `SDKWORK_DATABASE_TEST_POSTGRES_URL`.

use std::sync::Arc;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{
    AppPublishEnvironment, CreateEnvVariableRequest, CreateHealthCheckRequest, DeployAppApi,
    DeployAppRequestContext, DeployServiceErrorKind, UpdateAppCompositionRequest,
};
use sdkwork_deploy_drive_port::MemoryDeployDrivePort;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployService;

const TENANT_ID: i64 = 7;
const APP_A_UUID: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
const APP_B_UUID: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";

/// A pool that is never dialed: one connection to a port nothing listens on,
/// created lazily so `DeployService::new` needs no live database. The acquire
/// timeout is kept tiny because the "own application" case legitimately reaches
/// the repository and only needs to observe *some* failure that is not a 403.
fn unreachable_repository() -> DeployRepository {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_millis(150))
        .connect_lazy("postgres://sdkwork:sdkwork@127.0.0.1:1/sdkwork_deploy_scope_guard")
        .expect("build lazy PostgreSQL pool");
    DeployRepository::new(
        pool,
        SnowflakeIdGenerator::new(4).expect("Snowflake generator"),
        *b"sdkwork-deploy-test-secret-00000",
    )
}

fn service() -> DeployService {
    DeployService::new(
        Arc::new(unreachable_repository()),
        Arc::new(MemoryDeployDrivePort::default()),
    )
}

fn caller(app_id: Option<&str>) -> DeployAppRequestContext {
    DeployAppRequestContext {
        tenant_id: TENANT_ID,
        actor_id: Some(11),
        organization_id: Some(9),
        session_id: Some("app-scope-gate".to_owned()),
        app_id: app_id.map(str::to_owned),
        auth_token: None,
        access_token: None,
    }
}

fn health_check() -> CreateHealthCheckRequest {
    CreateHealthCheckRequest {
        check_type: 1,
        url: "https://app-a.example.com/healthz".to_owned(),
    }
}

fn env_variable() -> CreateEnvVariableRequest {
    CreateEnvVariableRequest {
        key: "DATABASE_URL".to_owned(),
        value: "postgres://localhost/app".to_owned(),
        environment: "production".to_owned(),
        is_secret: true,
    }
}

/// Deliberately minimal — and deliberately *invalid*: the credential check runs
/// before request validation, so a refusal here proves the ordering as well as
/// the boundary.
fn composition() -> UpdateAppCompositionRequest {
    UpdateAppCompositionRequest {
        environment: AppPublishEnvironment::Production,
        default_variant_key: "primary".to_owned(),
        resources: Vec::new(),
        variants: Vec::new(),
        variant_rules: Vec::new(),
        mounts: Vec::new(),
        bindings: Vec::new(),
        source_specs: None,
        delivery_policy: Default::default(),
        security_policy: Default::default(),
        limits: Default::default(),
        observability_policy: Default::default(),
    }
}

macro_rules! assert_forbidden {
    ($label:expr, $result:expr) => {{
        let error = $result.expect_err(concat!($label, " must refuse another application"));
        assert_eq!(
            error.kind(),
            DeployServiceErrorKind::Forbidden,
            concat!($label, " must be refused with 403, got: {}"),
            error
        );
    }};
}

/// Application A's token must not read or write anything addressed to B — for
/// the app-scoped health-check and env-variable surfaces (which previously
/// trusted the path) and for the composition write path (which previously had no
/// application gate at all).
#[tokio::test]
async fn application_a_cannot_address_application_b() {
    let service = service();
    let api: &dyn DeployAppApi = &service;
    let app_a = caller(Some(APP_A_UUID));

    assert_forbidden!(
        "apps.healthChecks.list",
        api.list_health_checks(&app_a, APP_B_UUID).await
    );
    assert_forbidden!(
        "apps.healthChecks.create",
        api.create_health_check(&app_a, APP_B_UUID, &health_check())
            .await
    );
    assert_forbidden!(
        "apps.envVariables.list",
        api.list_env_variables(&app_a, APP_B_UUID, None).await
    );
    assert_forbidden!(
        "apps.envVariables.create",
        api.create_env_variable(&app_a, APP_B_UUID, &env_variable())
            .await
    );
    assert_forbidden!(
        "apps.composition.update",
        api.update_app_composition(&app_a, APP_B_UUID, 0, "idem-composition", &composition())
            .await
    );
}

/// The same calls addressed to the credential's *own* application are not refused
/// by the scope gate: they get past it and stop at the (unreachable) repository.
/// Without this case a blanket `Forbidden` would satisfy the test above.
#[tokio::test]
async fn application_a_still_addresses_itself() {
    let service = service();
    let api: &dyn DeployAppApi = &service;
    let app_a = caller(Some(APP_A_UUID));

    for error in [
        api.list_health_checks(&app_a, APP_A_UUID).await.unwrap_err(),
        api.create_health_check(&app_a, APP_A_UUID, &health_check())
            .await
            .unwrap_err(),
        api.list_env_variables(&app_a, APP_A_UUID, None)
            .await
            .unwrap_err(),
        api.create_env_variable(&app_a, APP_A_UUID, &env_variable())
            .await
            .unwrap_err(),
    ] {
        assert_ne!(
            error.kind(),
            DeployServiceErrorKind::Forbidden,
            "the credential's own application must pass the scope gate: {error}"
        );
    }
}

/// A credential that names no application (console / user auth token) must keep
/// the pre-existing tenant/owner boundary, which is decided downstream — never by
/// this scope gate.
#[tokio::test]
async fn unscoped_credential_is_decided_by_the_owner_gate_not_the_scope_gate() {
    let service = service();
    let api: &dyn DeployAppApi = &service;
    let unscoped = caller(None);

    let error = api
        .list_health_checks(&unscoped, APP_B_UUID)
        .await
        .expect_err("the unreachable repository must fail the call");
    assert_ne!(
        error.kind(),
        DeployServiceErrorKind::Forbidden,
        "an unscoped credential must not be refused by the application-scope gate: {error}"
    );
}
