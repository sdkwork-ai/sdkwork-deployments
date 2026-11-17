//! `AppResponse.deployApplicationId` on every app read: `apps.list`,
//! `apps.retrieve`, `apps.create` and `apps.update`.
//!
//! `AppResponse.id` is `deploy_app.uuid` — the opaque address each app endpoint
//! resolves against. The **IAM installation anchor** is the row's `BIGINT`
//! `deploy_app.id`: `apps.identityProvision` forwards that decimal id to IAM as
//! `deployApplicationId`, IAM stores it in
//! `iam_tenant_application.deploy_application_id` under a unique index, and the
//! minted installation's runtime `app_id` / token `aud` are the same number.
//!
//! Until this field existed a caller could create an application but never read
//! the anchor back, so an automated registration tool had to guess the mapping and
//! fail closed. These tests pin the two properties that fix makes valuable:
//!
//! 1. the value in the response **is** the stored row's `deploy_app.id` (not the
//!    uuid, not a derived hash), on all four reads, and
//! 2. it is a JSON **string**, because a number would be rounded past 2^53 and
//!    then address a different application.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; ignored by default like the
//! other PostgreSQL integration tests in this crate.

mod common;

use sdkwork_deploy_contract::{
    AppKind, AppResponse, CreateAppRequest, ListAppsQuery, UpdateAppRequest,
};
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

const TENANT_ID: i64 = 7;
const ORGANIZATION_ID: i64 = 9;
const ACTOR_ID: i64 = 11;

/// The decimal id of the seeded row. Deliberately **not** a snowflake: a fixture
/// value this small is unmistakeable for a uuid or a hash if it ever leaks into
/// the wrong field.
const SEEDED_APP_ID: i64 = 5001;
const SEEDED_APP_UUID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

async fn seed_app(pool: &PgPool) {
    sqlx::query(
        "INSERT INTO deploy_app (id, uuid, tenant_id, organization_id, name, slug, app_kind,
                                 app_status, type, runtime_config, metadata, data_scope,
                                 default_environment, created_at, updated_at, version)
         VALUES ($1, $2, $3, $4, 'Anchored App', 'anchored-app', 'STATIC_WEB', 'ACTIVE', 1,
                 '{}', '{}', 1, 'production', NOW(), NOW(), 1)",
    )
    .bind(SEEDED_APP_ID)
    .bind(SEEDED_APP_UUID)
    .bind(TENANT_ID)
    .bind(ORGANIZATION_ID)
    .execute(pool)
    .await
    .expect("seed deploy_app");
}

fn wire(response: &AppResponse) -> serde_json::Value {
    serde_json::to_value(response).expect("AppResponse serializes")
}

fn every_reachable_app() -> ListAppsQuery {
    ListAppsQuery {
        page: 1,
        page_size: 20,
        keyword: None,
        scope: None,
    }
}

async fn stored_id(pool: &PgPool, app_uuid: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM deploy_app WHERE uuid = $1")
        .bind(app_uuid)
        .fetch_one(pool)
        .await
        .expect("read deploy_app.id for the row the response names")
}

/// `apps.list` / `apps.retrieve` / `apps.update` read the same row and must all
/// report its decimal id — the response is not allowed to invent or drop the
/// anchor on any one of them.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn list_retrieve_and_update_report_the_rows_decimal_id_beside_the_uuid() {
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-app-anchor-read-test").await;
    seed_app(&pool).await;

    // -- retrieve -----------------------------------------------------------
    let retrieved = repository
        .retrieve_app(TENANT_ID, SEEDED_APP_UUID)
        .await
        .expect("retrieve the seeded app");
    assert_eq!(
        retrieved.id, SEEDED_APP_UUID,
        "`id` stays the uuid the route addresses",
    );
    assert_eq!(
        retrieved.deploy_application_id, SEEDED_APP_ID,
        "`deployApplicationId` is the decimal deploy_app.id",
    );
    assert_eq!(
        stored_id(&pool, &retrieved.id).await,
        retrieved.deploy_application_id,
        "the reported anchor is the id of the row the response names",
    );

    // -- list ---------------------------------------------------------------
    let page = repository
        .list_apps(
            TENANT_ID,
            Some(ACTOR_ID),
            Some(ORGANIZATION_ID),
            &every_reachable_app(),
        )
        .await
        .expect("list apps");
    assert_eq!(page.items.len(), 1, "the fixture holds exactly one app");

    let listed = &page.items[0];
    assert_eq!(listed.id, SEEDED_APP_UUID);
    assert_eq!(
        listed.deploy_application_id, SEEDED_APP_ID,
        "the list row carries the same anchor as retrieve",
    );

    // -- wire shape ---------------------------------------------------------
    let listed_json = wire(listed);
    assert!(
        listed_json["deployApplicationId"].is_string(),
        "int64 wire fields MUST be strings (API_SPEC §13.6), was {}",
        listed_json["deployApplicationId"],
    );
    assert_eq!(
        listed_json["deployApplicationId"], "5001",
        "the wire value is the decimal id as text, not a number and not the uuid",
    );

    // -- update -------------------------------------------------------------
    // All `UpdateAppRequest` members are optional on the wire, so the patch is
    // built the way a client builds it.
    let patch: UpdateAppRequest =
        serde_json::from_str(r#"{"name":"Renamed App"}"#).expect("patch parses");
    let updated = repository
        .update_app(TENANT_ID, Some(ACTOR_ID), SEEDED_APP_UUID, &patch)
        .await
        .expect("update the seeded app");
    assert_eq!(updated.name, "Renamed App", "the patch was applied");
    assert_eq!(
        updated.deploy_application_id, SEEDED_APP_ID,
        "update answers the same anchor; rewriting a name must not move it",
    );
}

/// `apps.create` is the call the automated registration tool makes first, and the
/// response is the only place it can learn the anchor from. The id has to be the
/// row the create actually wrote, and it has to arrive as a string: a snowflake is
/// far past 2^53, so a numeric wire form would silently round.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn create_reports_the_decimal_anchor_of_the_row_it_wrote() {
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-app-anchor-create-test").await;

    let created = repository
        .create_app(
            TENANT_ID,
            Some(ORGANIZATION_ID),
            Some(ACTOR_ID),
            Some("anchor-create"),
            &CreateAppRequest {
                name: "Anchor App".to_string(),
                slug: None,
                app_kind: AppKind::ApiService,
                app_type: None,
                runtime_config: None,
                metadata: None,
                description: None,
                default_environment: None,
                app_domain_label: None,
                app_domain_suffixes: None,
                owner_type: None,
                source_specs: None,
                idempotency_key: Some("anchor-create".to_string()),
            },
        )
        .await
        .expect("create app");

    let stored = stored_id(&pool, &created.id).await;
    assert_eq!(
        created.deploy_application_id, stored,
        "the create response reports the id of the row it inserted",
    );
    assert!(
        created.deploy_application_id > 0,
        "a `0` here would mean the anchor was defaulted rather than read",
    );

    let json = wire(&created);
    assert!(
        json["deployApplicationId"].is_string(),
        "created ids are snowflakes (past 2^53) and MUST cross the wire as strings, was {}",
        json["deployApplicationId"],
    );
    assert_eq!(
        json["deployApplicationId"],
        created.deploy_application_id.to_string(),
        "the string form is the exact decimal id, digit for digit",
    );
}
