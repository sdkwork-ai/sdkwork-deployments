//! Template-artifact clone contract on the migrated schema.
//!
//! Skipped without `SDKWORK_DATABASE_TEST_POSTGRES_URL`; it is a real-database
//! test because the clone inserts rows into two tables under a partial unique
//! idempotency index, and only PostgreSQL can prove the replay lands on the same
//! artifact rather than a second one.

mod common;

use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;

const TENANT_ID: i64 = 7;
const SOURCE_APP_ID: i64 = 9101;
const TARGET_APP_ID: i64 = 9102;
const SOURCE_APP_UUID: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
const TARGET_APP_UUID: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
const SESSION_REF: i64 = 9201;
const SESSION_UUID: &str = "cccccccc-3333-4333-8333-cccccccccccc";
const ARTIFACT_ID: i64 = 9301;
const ARTIFACT_UUID: &str = "eeeeeeee-5555-4555-8555-eeeeeeeeeeee";

async fn seed(repository: &DeployRepository, pool: &sqlx::PgPool) {
    for (id, uuid, name) in [
        (SOURCE_APP_ID, SOURCE_APP_UUID, "Template source"),
        (TARGET_APP_ID, TARGET_APP_UUID, "Installed app"),
    ] {
        sqlx::query(
            "INSERT INTO deploy_app
                (id, uuid, tenant_id, organization_id, name, slug, app_kind, app_status,
                 owner_type, created_by, created_at, updated_at, version)
             VALUES ($1, $2, $3, 0, $4, $5, 'API_SERVICE', 'ACTIVE', 'TENANT', 11, NOW(), NOW(), 1)",
        )
        .bind(id)
        .bind(uuid)
        .bind(TENANT_ID)
        .bind(name)
        .bind(name.to_lowercase().replace(' ', "-"))
        .execute(pool)
        .await
        .expect("insert deploy_app fixture");
    }

    sqlx::query(
        "INSERT INTO deploy_upload_session_ref
            (id, uuid, tenant_id, app_id, drive_upload_session_id, drive_space_id,
             drive_node_id, package_type, file_name, content_type, content_length,
             checksum, status, idempotency_key, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'drive-session-1', 'space-1', 'node-1', 1,
                 'template.zip', 'application/zip', 1024, 'checksum-1', 3, 'seed-session',
                 NOW(), NOW())",
    )
    .bind(SESSION_REF)
    .bind(SESSION_UUID)
    .bind(TENANT_ID)
    .bind(SOURCE_APP_ID)
    .execute(pool)
    .await
    .expect("insert upload session fixture");

    sqlx::query(
        "INSERT INTO deploy_artifact
            (id, uuid, tenant_id, app_id, upload_session_ref_id, package_type, file_name,
             content_type, content_length, checksum_sha256, drive_node_id, drive_space_id,
             drive_path, status, created_at, updated_at, version)
         VALUES ($1, $2, $3, $4, $5, 1, 'template.zip', 'application/zip', 1024,
                 'checksum-1', 'node-1', 'space-1', 'drive://node/node-1', 1,
                 NOW(), NOW(), 1)",
    )
    .bind(ARTIFACT_ID)
    .bind(ARTIFACT_UUID)
    .bind(TENANT_ID)
    .bind(SOURCE_APP_ID)
    .bind(SESSION_REF)
    .execute(pool)
    .await
    .expect("insert artifact fixture");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL (PostgreSQL integration profile)"]
async fn cloning_a_template_artifact_reattributes_it_and_is_idempotent() {
    let (repository, pool) =
        common::migrated_repository_with_pool("template_install::artifact_clone").await;
    seed(&repository, &pool).await;

    let first = repository
        .clone_marketplace_artifact_for_app(TENANT_ID, ARTIFACT_UUID, TARGET_APP_UUID)
        .await
        .expect("first clone");
    assert_ne!(first, ARTIFACT_UUID, "the clone must be a new artifact row");

    let (target_app_id, target_node, target_checksum, target_status): (i64, String, String, i32) =
        sqlx::query_as(
            "SELECT app_id, drive_node_id, checksum_sha256, status
             FROM deploy_artifact WHERE uuid = $1",
        )
        .bind(&first)
        .fetch_one(&pool)
        .await
        .expect("read cloned artifact");
    assert_eq!(target_app_id, TARGET_APP_ID);
    assert_eq!(target_node, "node-1");
    assert_eq!(target_checksum, "checksum-1");
    assert_eq!(target_status, 1);

    let replay = repository
        .clone_marketplace_artifact_for_app(TENANT_ID, ARTIFACT_UUID, TARGET_APP_UUID)
        .await
        .expect("replay clone");
    assert_eq!(replay, first, "a replay must reuse the cloned artifact");
    let artifact_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_artifact WHERE tenant_id = $1 AND app_id = $2",
    )
    .bind(TENANT_ID)
    .bind(TARGET_APP_ID)
    .fetch_one(&pool)
    .await
    .expect("count target artifacts");
    assert_eq!(artifact_count, 1, "a replay must not create a second artifact");
}
