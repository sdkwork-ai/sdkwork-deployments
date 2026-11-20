//! Forward-only uninstall bookkeeping (`APPLICATION_FACTORY_DEPLOYMENT_SPEC.md`
//! section 3 stage 6).
//!
//! The command sequence lives in the service (`factory_ops.rs`), but its safety
//! properties are database facts: the stage marker must be durable, the
//! environments must stop before the application is reported archived, no row may
//! be deleted, and a replay after completion must not regress the stage.
//!
//! These tests pin the repository half of that contract against a real migrated
//! schema. Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like
//! the other PostgreSQL integration tests in this crate.

mod common;

use sdkwork_deploy_contract::{AppKind, CreateAppEnvironmentRequest, CreateAppRequest};
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

const TENANT: i64 = 7101;
const ACTOR: i64 = 11101;

fn app_request(name: &str, slug: &str) -> CreateAppRequest {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "slug": slug,
        "appKind": AppKind::PcWeb.as_str(),
    }))
    .expect("CreateAppRequest parses from its own wire shape")
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn an_application_is_created_with_a_not_started_uninstall_stage() {
    let (repository, _pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-factory-create-test").await;

    let app = repository
        .create_app(
            TENANT,
            None,
            Some(ACTOR),
            None,
            &app_request("Factory App", "factory-app"),
        )
        .await
        .expect("create a factory application");

    assert_eq!(app.uninstall_status, "not_started");
    assert_eq!(app.app_status, "DRAFT");
    assert!(
        app.app_surface.is_none(),
        "no surface is recorded by default"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn archiving_an_application_disables_every_environment_and_keeps_history() {
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-factory-archive-test").await;

    let app = repository
        .create_app(
            TENANT,
            None,
            Some(ACTOR),
            None,
            &app_request("Archive App", "archive-app"),
        )
        .await
        .expect("create an application");
    let environment = repository
        .create_app_environment(
            TENANT,
            Some(ACTOR),
            &app.id,
            &CreateAppEnvironmentRequest {
                env_key: "staging".to_owned(),
                env_name: "Staging".to_owned(),
                env_level: "STAGING".to_owned(),
                approval_required: false,
            },
        )
        .await
        .expect("create an environment");

    // The service's step order, expressed against the repository: mark the
    // identity stage, then archive.
    repository
        .mark_uninstall_stage(TENANT, &app.id, "pending_identity_disable")
        .await
        .expect("mark pending identity disable");
    repository
        .mark_uninstall_stage(TENANT, &app.id, "identity_disabled")
        .await
        .expect("mark identity disabled");
    let (stage, status) = repository
        .retrieve_uninstall_state(TENANT, &app.id)
        .await
        .expect("read uninstall state");
    assert_eq!(stage, "identity_disabled");
    assert_eq!(status, "DRAFT");

    repository
        .archive_app(TENANT, &app.id)
        .await
        .expect("archive the application");
    let (stage, status) = repository
        .retrieve_uninstall_state(TENANT, &app.id)
        .await
        .expect("read completed uninstall state");
    assert_eq!(stage, "complete");
    assert_eq!(status, "ARCHIVED");

    // The environment row still exists (forward-only: no DELETE) and is archived,
    // so it can no longer receive a promotion.
    let environments = repository
        .list_app_environments(TENANT, &app.id, 1, 100)
        .await
        .expect("list environments");
    let archived = environments
        .items
        .iter()
        .find(|item| item.id == environment.id)
        .expect("the environment row survives the uninstall");
    assert_eq!(archived.env_status, "ARCHIVED");

    // A stale retry of an earlier stage must not regress the completed marker.
    repository
        .mark_uninstall_stage(TENANT, &app.id, "pending_identity_disable")
        .await
        .expect("a stale retry is accepted as a no-op");
    let (stage, status) = repository
        .retrieve_uninstall_state(TENANT, &app.id)
        .await
        .expect("read after the stale retry");
    assert_eq!(stage, "complete", "a completed uninstall is forward-only");
    assert_eq!(status, "ARCHIVED");

    // ARCHIVED is terminal for the state machine: re-activating is refused.
    let revived = repository.set_app_status(TENANT, &app.id, 1).await;
    assert!(
        revived.is_err(),
        "an archived application must not be re-activated"
    );

    let stored: (String, String) =
        sqlx::query_as("SELECT app_status, uninstall_status FROM deploy_app WHERE uuid = $1")
            .bind(&app.id)
            .fetch_one(&pool)
            .await
            .expect("read the row back");
    assert_eq!(stored, ("ARCHIVED".to_owned(), "complete".to_owned()));
}
