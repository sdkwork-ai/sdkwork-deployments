//! Regression coverage for `apps.releases.create`
//! (`POST /app/v3/api/apps/{appId}/releases`).
//!
//! `create_app_release_repo` resolved the package scope with
//! `SELECT p.build_number ... FROM deploy_package p`. `deploy_package` has no
//! `build_number` column — the number lives on the build the package was
//! produced by (`deploy_package.build_id -> deploy_build.build_number`). The
//! read went through `.unwrap_or(0)`, so the query never even failed loudly in
//! a way a test could see: it either mistyped and errored, or silently wrote a
//! bogus `0` into `deploy_release.build_number`, where nothing downstream could
//! tell it apart from a real build #0.
//!
//! These tests pin the resolved value to a non-zero build number so a
//! regression to `unwrap_or(0)` fails, not merely a regression to a SQL typo.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`.

mod common;

use sdkwork_deploy_contract::{CreateAppReleaseRequest, ReleaseStatus};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::PgPool;

/// Seeds one app, one platform target, one build (#42) and one VALIDATED package
/// for it. Returns `(app_uuid, target_uuid, package_uuid)`.
async fn seed_release_scope(pool: &PgPool) -> (String, String, String) {
    let app_uuid = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let target_uuid = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let package_uuid = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";

    sqlx::raw_sql(
        "INSERT INTO deploy_app (id, uuid, tenant_id, organization_id, name, slug, app_kind,
                                 app_status, type, runtime_config, metadata, data_scope,
                                 default_environment, created_at, updated_at, version)
         VALUES (5001, 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', 7, 9, 'Release App', 'release-app',
                 'MOBILE', 'ACTIVE', 1, '{}', '{}', 1, 'production', NOW(), NOW(), 1);

         INSERT INTO deploy_app_platform_target (id, uuid, tenant_id, organization_id, app_id,
                                                 target_key, platform, target_status,
                                                 created_at, updated_at, version)
         VALUES (5002, 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', 7, 9, 5001,
                 'android-universal', 'ANDROID', 'ACTIVE', NOW(), NOW(), 1);

         INSERT INTO deploy_build (id, uuid, tenant_id, organization_id, app_id,
                                   platform_target_id, build_number, build_status,
                                   created_at, updated_at, version)
         VALUES (5003, 'dddddddd-dddd-4ddd-8ddd-dddddddddddd', 7, 9, 5001, 5002, 42,
                 'SUCCEEDED', NOW(), NOW(), 1);

         INSERT INTO deploy_package (id, uuid, tenant_id, organization_id, app_id,
                                     platform_target_id, build_id, package_format,
                                     semantic_version, package_size_bytes, checksum_sha256,
                                     manifest_sha256, package_status, created_at, updated_at,
                                     version)
         VALUES (5004, 'cccccccc-cccc-4ccc-8ccc-cccccccccccc', 7, 9, 5001, 5002, 5003,
                 'APK', '1.4.2', 1024, 'checksum', 'manifest', 'VALIDATED', NOW(), NOW(), 1);",
    )
    .execute(pool)
    .await
    .expect("seed release scope");

    (
        app_uuid.to_owned(),
        target_uuid.to_owned(),
        package_uuid.to_owned(),
    )
}

fn release_request(target_uuid: &str, package_uuid: &str) -> CreateAppReleaseRequest {
    CreateAppReleaseRequest {
        platform_target_id: target_uuid.to_owned(),
        package_id: package_uuid.to_owned(),
        // Empty: the version is derived from the package, which is the path
        // that also has to read `build_number`.
        semantic_version: String::new(),
        release_notes: None,
        release_status: Some(ReleaseStatus::Draft),
        idempotency_key: "release-1".to_owned(),
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn create_app_release_resolves_the_build_number_from_the_package_build() {
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-release-buildnumber-test").await;
    let (app_uuid, target_uuid, package_uuid) = seed_release_scope(&pool).await;

    let release = repository
        .create_app_release(7, Some(1), &release_request(&target_uuid, &package_uuid))
        .await
        .expect("creating a release from a VALIDATED package must not fail");

    assert_eq!(release.app_id, app_uuid, "the release is app-qualified");
    assert_eq!(
        release.platform_target_id, target_uuid,
        "the release keeps the package platform target"
    );
    assert_eq!(
        release.package_id, package_uuid,
        "the release pins the package"
    );
    assert_eq!(
        release.semantic_version, "1.4.2",
        "the version is derived from the package when the request omits it"
    );
    // The regression assertion: `p.build_number` never existed on
    // `deploy_package`, so the buggy projection either errored outright or
    // silently produced 0. A real build #42 must come through.
    assert_eq!(
        release.build_number, 42,
        "the build number is resolved from deploy_build through deploy_package.build_id"
    );

    // And it is what actually landed in the table, not just the response shape.
    use sqlx::Row;
    let stored: i64 = sqlx::query("SELECT build_number FROM deploy_release WHERE uuid = $1")
        .bind(&release.id)
        .fetch_one(&pool)
        .await
        .expect("read stored release")
        .try_get("build_number")
        .expect("build_number");
    assert_eq!(stored, 42, "deploy_release.build_number is the build's number");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn create_app_release_is_idempotent_on_the_request_key() {
    let (repository, pool) =
        common::migrated_repository_with_pool("sdkwork-deploy-release-idempotency-test").await;
    let (_app_uuid, target_uuid, package_uuid) = seed_release_scope(&pool).await;

    let first = repository
        .create_app_release(7, Some(1), &release_request(&target_uuid, &package_uuid))
        .await
        .expect("first create");
    let replay = repository
        .create_app_release(7, Some(1), &release_request(&target_uuid, &package_uuid))
        .await
        .expect("replayed create returns the original release");

    assert_eq!(replay.id, first.id, "the replay returns the original release");
    assert_eq!(replay.build_number, 42, "the replay keeps the build number");

    use sqlx::Row;
    let count: i64 = sqlx::query("SELECT COUNT(*) AS total FROM deploy_release")
        .fetch_one(&pool)
        .await
        .expect("count releases")
        .try_get("total")
        .expect("total");
    assert_eq!(count, 1, "the replay did not insert a second release");
}
