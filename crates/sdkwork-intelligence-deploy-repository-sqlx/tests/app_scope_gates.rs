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
//!
//! A **machine** credential carries the binding. An **interactive user session**
//! does not: its `app_id` is the application the user signed in through — the
//! tenant console is itself a deploy application — so binding it to the path
//! `appId` made every console management read answer `40301`. The tests at the
//! bottom pin that distinction down for both credential families.

use std::sync::Arc;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{
    AppPublishEnvironment, AppResourceKind, AppResourceLinkStatus, AppSourceGitInput,
    AppSourceKind, CreateEnvVariableRequest, CreateHealthCheckRequest, DeployAppApi,
    DeployAppRequestContext, DeployCredentialKind, DeployServiceErrorKind,
    ListAppResourceLinksQuery, UpdateAppCompositionRequest, UpdateAppRemixRequest,
    UpdateAppSourceRequest, UpsertAppResourceLinkRequest,
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

/// A machine credential: the token acts as the application named in `app_id`.
fn caller(app_id: Option<&str>) -> DeployAppRequestContext {
    DeployAppRequestContext {
        tenant_id: TENANT_ID,
        actor_id: Some(11),
        organization_id: Some(9),
        session_id: Some("app-scope-gate".to_owned()),
        app_id: app_id.map(str::to_owned),
        credential_kind: DeployCredentialKind::Application,
        auth_token: None,
        access_token: None,
        permissions: Vec::new(),
    }
}

/// The console operator: an interactive user session signed in through the
/// tenant console, which is itself deployed as application `app_id`.
fn console_session(app_id: &str) -> DeployAppRequestContext {
    DeployAppRequestContext {
        credential_kind: DeployCredentialKind::UserSession,
        ..caller(Some(app_id))
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
        api.list_health_checks(&app_a, APP_A_UUID)
            .await
            .unwrap_err(),
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

/// A machine credential that names no application (a token without an `app_id`
/// claim) must keep the pre-existing tenant/owner boundary, which is decided
/// downstream — never by this scope gate.
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

// ---------------------------------------------------------------------------
// Remix source and remix state
// ---------------------------------------------------------------------------

fn git_source() -> UpdateAppSourceRequest {
    UpdateAppSourceRequest {
        kind: AppSourceKind::Git,
        git: Some(AppSourceGitInput {
            repo_provider: "GITHUB".to_owned(),
            repo_url: "https://github.com/sdkwork/app.git".to_owned(),
            repo_ref: None,
            sub_path: None,
            credential_secret_ref: None,
            clone_mode: None,
        }),
        archive: None,
    }
}

fn remix_state(enabled: bool) -> UpdateAppRemixRequest {
    UpdateAppRemixRequest { enabled }
}

/// The remix surface is addressed by `appId` on every route, so it carries the same
/// credential gate as the other app-scoped surfaces: application A's token must not
/// read or write B's source, and must not toggle B's remix.
#[tokio::test]
async fn application_a_cannot_address_application_b_remix_source() {
    let service = service();
    let api: &dyn DeployAppApi = &service;
    let app_a = caller(Some(APP_A_UUID));

    assert_forbidden!(
        "apps.source.retrieve",
        api.retrieve_app_source(&app_a, APP_B_UUID).await
    );
    assert_forbidden!(
        "apps.source.update",
        api.update_app_source(&app_a, APP_B_UUID, &git_source())
            .await
    );
    assert_forbidden!(
        "apps.source.delete",
        api.delete_app_source(&app_a, APP_B_UUID).await
    );
    assert_forbidden!(
        "apps.remix.retrieve",
        api.retrieve_app_remix(&app_a, APP_B_UUID).await
    );
    assert_forbidden!(
        "apps.remix.update",
        api.update_app_remix(&app_a, APP_B_UUID, &remix_state(true))
            .await
    );
}

/// The same calls addressed to the credential's own application pass the gate and
/// stop at the unreachable repository — otherwise the refusal above could be a
/// blanket denial.
#[tokio::test]
async fn application_a_still_addresses_its_own_remix_source() {
    let service = service();
    let api: &dyn DeployAppApi = &service;
    let app_a = caller(Some(APP_A_UUID));

    for error in [
        api.retrieve_app_source(&app_a, APP_A_UUID)
            .await
            .unwrap_err(),
        api.update_app_source(&app_a, APP_A_UUID, &git_source())
            .await
            .unwrap_err(),
        api.delete_app_source(&app_a, APP_A_UUID).await.unwrap_err(),
        api.retrieve_app_remix(&app_a, APP_A_UUID)
            .await
            .unwrap_err(),
        api.update_app_remix(&app_a, APP_A_UUID, &remix_state(false))
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

// ---------------------------------------------------------------------------
// Account bindings (deploy_application_resource_link)
// ---------------------------------------------------------------------------

fn resource_link() -> UpsertAppResourceLinkRequest {
    UpsertAppResourceLinkRequest {
        environment: AppPublishEnvironment::Production,
        resource_kind: AppResourceKind::OfficialAccount,
        resource_ref: "wx-a1b2c3d4e5f6".to_owned(),
        provider_code: Some("wechat".to_owned()),
        is_default: false,
        binding_status: AppResourceLinkStatus::Active,
        metadata: serde_json::json!({ "displayName": "Application A" }),
    }
}

fn resource_link_query() -> ListAppResourceLinksQuery {
    ListAppResourceLinksQuery {
        page: 1,
        page_size: 20,
        environment: None,
        resource_kind: None,
    }
}

/// Account bindings are addressed by `appId` like every other app-surface route,
/// so the credential's application must bound them too: a token bound to
/// application A can neither read nor rebind B's official account, and cannot
/// retire B's binding.
#[tokio::test]
async fn application_a_cannot_address_application_b_resource_links() {
    let service = service();
    let api: &dyn DeployAppApi = &service;
    let app_a = caller(Some(APP_A_UUID));

    assert_forbidden!(
        "apps.resourceLinks.list",
        api.list_app_resource_links(&app_a, APP_B_UUID, &resource_link_query())
            .await
    );
    assert_forbidden!(
        "apps.resourceLinks.update",
        api.upsert_app_resource_link(&app_a, APP_B_UUID, &resource_link())
            .await
    );
    assert_forbidden!(
        "apps.resourceLinks.delete",
        api.delete_app_resource_link(&app_a, APP_B_UUID, APP_A_UUID)
            .await
    );
}

/// The same calls addressed to the credential's own application are not refused
/// by the scope gate: they reach the (unreachable) repository instead. Without
/// this case a blanket `Forbidden` would satisfy the test above.
#[tokio::test]
async fn application_a_still_addresses_its_own_resource_links() {
    let service = service();
    let api: &dyn DeployAppApi = &service;
    let app_a = caller(Some(APP_A_UUID));

    for error in [
        api.list_app_resource_links(&app_a, APP_A_UUID, &resource_link_query())
            .await
            .unwrap_err(),
        api.upsert_app_resource_link(&app_a, APP_A_UUID, &resource_link())
            .await
            .unwrap_err(),
        api.delete_app_resource_link(&app_a, APP_A_UUID, APP_A_UUID)
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

// ---------------------------------------------------------------------------
// Interactive user sessions (the tenant console)
// ---------------------------------------------------------------------------

/// The tenant console is itself a deploy application, so its operator's user
/// session proves an `app_id` that is **not** the application being managed.
/// Treating that claim as a data-access binding answered `40301` ("a credential
/// bound to one application cannot act on another application") for every
/// row on the applications page — the production incident this regression test
/// pins down. The owner gate keeps deciding whether the session may reach the
/// addressed application; the scope gate must not.
#[tokio::test]
async fn console_user_session_may_address_another_application() {
    let service = service();
    let api: &dyn DeployAppApi = &service;
    // The console's own numeric anchor, addressing a different application's
    // uuid — exactly the production shape.
    let console = console_session("381764687714324480");

    for error in [
        api.list_health_checks(&console, APP_B_UUID)
            .await
            .unwrap_err(),
        api.create_health_check(&console, APP_B_UUID, &health_check())
            .await
            .unwrap_err(),
        api.list_env_variables(&console, APP_B_UUID, None)
            .await
            .unwrap_err(),
        api.create_env_variable(&console, APP_B_UUID, &env_variable())
            .await
            .unwrap_err(),
        api.list_app_resource_links(&console, APP_B_UUID, &resource_link_query())
            .await
            .unwrap_err(),
        api.upsert_app_resource_link(&console, APP_B_UUID, &resource_link())
            .await
            .unwrap_err(),
        api.delete_app_resource_link(&console, APP_B_UUID, APP_A_UUID)
            .await
            .unwrap_err(),
        api.retrieve_app_source(&console, APP_B_UUID)
            .await
            .unwrap_err(),
        api.update_app_remix(&console, APP_B_UUID, &remix_state(true))
            .await
            .unwrap_err(),
        api.update_app_composition(&console, APP_B_UUID, 0, "idem-console", &composition())
            .await
            .unwrap_err(),
    ] {
        assert_ne!(
            error.kind(),
            DeployServiceErrorKind::Forbidden,
            "a console user session must not be refused by the application-scope \
             gate; the owner gate decides: {error}"
        );
    }
}

/// The binding still holds for a machine credential of the **same** shape and
/// app identity. This is what makes the exemption a credential-class rule rather
/// than a blanket removal of the gate. The console's wire uuid is used so the two
/// identities stay comparable before any I/O (the decimal-anchor-versus-uuid case
/// is decided at the row, covered by the service unit tests).
#[tokio::test]
async fn machine_credential_with_the_console_app_identity_is_still_bound() {
    let service = service();
    let api: &dyn DeployAppApi = &service;
    const CONSOLE_UUID: &str = "ef45ca0e-0093-4eaf-ab03-964d8ef6227c";
    let machine = caller(Some(CONSOLE_UUID));

    assert_forbidden!(
        "apps.resourceLinks.list (machine)",
        api.list_app_resource_links(&machine, APP_B_UUID, &resource_link_query())
            .await
    );
    assert_forbidden!(
        "apps.envVariables.list (machine)",
        api.list_env_variables(&machine, APP_B_UUID, None).await
    );
}
