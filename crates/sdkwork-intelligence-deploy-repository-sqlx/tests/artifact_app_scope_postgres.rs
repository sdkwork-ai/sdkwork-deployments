//! `(tenant_id, app_id)` isolation of the artifact / upload-session data plane.
//!
//! Skipped without `SDKWORK_DATABASE_TEST_POSTGRES_URL`; it is a real-database
//! test because the defect it guards is a missing WHERE clause, and a missing
//! WHERE clause is invisible to every pure function above the repository.
//!
//! The scenario is the one `APPLICATION_IDENTITY_SPEC.md` §4.3 exists for: one
//! tenant runs two applications, each with its own access token. Application A's
//! token must not enumerate, read, retire, or replay-mint anything attributed to
//! application B — and the tenant-wide (unscoped) view must keep working, so the
//! fix is a narrowing rather than a replacement of the tenant boundary.

mod common;

use sdkwork_deploy_contract::{DeployServiceErrorKind, ARTIFACT_STATUS_ACTIVE};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;

const TENANT_ID: i64 = 7;
const APP_A_ID: i64 = 9101;
const APP_B_ID: i64 = 9102;
const APP_A_UUID: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
const APP_B_UUID: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
const SESSION_A_REF: i64 = 9201;
const SESSION_B_REF: i64 = 9202;
const SESSION_A_UUID: &str = "cccccccc-3333-4333-8333-cccccccccccc";
const SESSION_B_UUID: &str = "dddddddd-4444-4444-8444-dddddddddddd";
const ARTIFACT_A_ID: i64 = 9301;
const ARTIFACT_B_ID: i64 = 9302;
const ARTIFACT_A_UUID: &str = "eeeeeeee-5555-4555-8555-eeeeeeeeeeee";
const ARTIFACT_B_UUID: &str = "ffffffff-6666-4666-8666-ffffffffffff";

async fn seed_two_applications(
    repository: &DeployRepository,
    pool: &sqlx::PgPool,
) -> (
    sdkwork_deploy_contract::ArtifactPage,
    sdkwork_deploy_contract::ArtifactPage,
) {
    for (id, uuid, name) in [
        (APP_A_ID, APP_A_UUID, "App A"),
        (APP_B_ID, APP_B_UUID, "App B"),
    ] {
        sqlx::query(
            "INSERT INTO deploy_app
                (id, uuid, tenant_id, organization_id, name, slug, app_kind, app_status,
                 owner_type, created_by, created_at, updated_at, version)
             VALUES ($1, $2, $3, 0, $4, $5, 'STATIC_WEB', 'ACTIVE', 'TENANT', 11, NOW(), NOW(), 1)",
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

    for (ref_id, session_uuid, app_id, idempotency_key) in [
        (SESSION_A_REF, SESSION_A_UUID, APP_A_ID, "idem-a"),
        (SESSION_B_REF, SESSION_B_UUID, APP_B_ID, "idem-b"),
    ] {
        sqlx::query(
            "INSERT INTO deploy_upload_session_ref
                (id, uuid, tenant_id, app_id, drive_upload_session_id, drive_space_id,
                 drive_node_id, package_type, file_name, content_type, content_length,
                 checksum, status, idempotency_key, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, 'space-1', $6, 1, 'app.zip', 'application/zip',
                     1024, 'checksum', 0, $7, NOW(), NOW())",
        )
        .bind(ref_id)
        .bind(session_uuid)
        .bind(TENANT_ID)
        .bind(app_id)
        .bind(format!("drive-{ref_id}"))
        .bind(format!("node-{ref_id}"))
        .bind(sdkwork_utils_rust::sha256_hash(idempotency_key.as_bytes()))
        .execute(pool)
        .await
        .expect("insert deploy_upload_session_ref fixture");
    }

    for (id, uuid, app_id, ref_id) in [
        (ARTIFACT_A_ID, ARTIFACT_A_UUID, APP_A_ID, SESSION_A_REF),
        (ARTIFACT_B_ID, ARTIFACT_B_UUID, APP_B_ID, SESSION_B_REF),
    ] {
        sqlx::query(
            "INSERT INTO deploy_artifact
                (id, uuid, tenant_id, app_id, upload_session_ref_id, package_type, file_name,
                 content_type, content_length, checksum_sha256, drive_node_id, drive_path,
                 status, created_at, updated_at, version)
             VALUES ($1, $2, $3, $4, $5, 1, 'app.zip', 'application/zip', 1024,
                     'checksum', $6, $7, $8, NOW(), NOW(), 1)",
        )
        .bind(id)
        .bind(uuid)
        .bind(TENANT_ID)
        .bind(app_id)
        .bind(ref_id)
        .bind(format!("node-{ref_id}"))
        .bind(format!("drive://node/node-{ref_id}"))
        .bind(ARTIFACT_STATUS_ACTIVE)
        .execute(pool)
        .await
        .expect("insert deploy_artifact fixture");
    }

    let scoped = repository
        .list_artifacts(TENANT_ID, Some(APP_A_UUID), 1, 20)
        .await
        .expect("application-scoped artifact listing");
    let tenant_wide = repository
        .list_artifacts(TENANT_ID, None, 1, 20)
        .await
        .expect("tenant-wide artifact listing");
    (scoped, tenant_wide)
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL (PostgreSQL integration profile)"]
async fn application_a_cannot_read_application_b_artifacts_or_upload_sessions() {
    let (repository, pool) = common::migrated_repository_with_pool(
        "artifact_app_scope_postgres::cross_application_read",
    )
    .await;

    let (scoped, tenant_wide) = seed_two_applications(&repository, &pool).await;

    // ---- artifacts.list -----------------------------------------------------
    assert_eq!(
        scoped.total,
        1,
        "application A's listing must contain exactly its own artifact, got {:?}",
        scoped
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(scoped.items[0].id, ARTIFACT_A_UUID);
    assert!(
        !scoped.items.iter().any(|item| item.id == ARTIFACT_B_UUID),
        "application B's artifact leaked into application A's listing"
    );
    // The tenant boundary is retained: an unscoped caller still sees both, which
    // is what the console/back-office path relies on.
    assert_eq!(
        tenant_wide.total, 2,
        "the tenant-wide listing must be unchanged"
    );

    // ---- artifacts.retrieve -------------------------------------------------
    let denied = repository
        .retrieve_artifact(TENANT_ID, Some(APP_A_UUID), ARTIFACT_B_UUID)
        .await
        .expect_err("application A must not read application B's artifact");
    assert_eq!(denied.kind(), DeployServiceErrorKind::NotFound);
    let own = repository
        .retrieve_artifact(TENANT_ID, Some(APP_A_UUID), ARTIFACT_A_UUID)
        .await
        .expect("application A must still read its own artifact");
    assert_eq!(own.id, ARTIFACT_A_UUID);

    // ---- artifacts.delete (retain) -----------------------------------------
    let denied = repository
        .retain_artifact(TENANT_ID, Some(APP_A_UUID), ARTIFACT_B_UUID)
        .await
        .expect_err("application A must not retire application B's artifact");
    assert_eq!(denied.kind(), DeployServiceErrorKind::NotFound);
    let still_active: i32 = sqlx::query_scalar("SELECT status FROM deploy_artifact WHERE id = $1")
        .bind(ARTIFACT_B_ID)
        .fetch_one(&pool)
        .await
        .expect("read application B artifact status");
    assert_eq!(
        still_active, ARTIFACT_STATUS_ACTIVE,
        "application B's artifact must be untouched by application A's request"
    );

    // ---- upload_sessions.retrieve / complete / cancel ----------------------
    let denied = repository
        .retrieve_upload_session_ref(TENANT_ID, Some(APP_A_UUID), SESSION_B_UUID)
        .await
        .expect_err("application A must not read application B's upload session");
    assert_eq!(denied.kind(), DeployServiceErrorKind::NotFound);
    let denied = repository
        .update_upload_session_status(TENANT_ID, Some(APP_A_UUID), SESSION_B_UUID, 3, None)
        .await
        .expect_err("application A must not advance application B's upload session");
    assert_eq!(denied.kind(), DeployServiceErrorKind::NotFound);
    let own_session = repository
        .retrieve_upload_session_ref(TENANT_ID, Some(APP_A_UUID), SESSION_A_UUID)
        .await
        .expect("application A must still read its own upload session");
    assert_eq!(own_session.app_id.as_deref(), Some(APP_A_UUID));

    // ---- idempotency replay ------------------------------------------------
    let replayed = repository
        .find_upload_session_by_idempotency_key(TENANT_ID, Some(APP_A_UUID), "idem-b")
        .await
        .expect("idempotency lookup must not fail");
    assert!(
        replayed.is_none(),
        "application A must not observe application B's session through a shared idempotency key"
    );

    // ---- artifact minting from another application's session ---------------
    let denied = repository
        .create_artifact_from_upload_session(
            TENANT_ID,
            Some(APP_A_UUID),
            SESSION_B_UUID,
            "checksum",
        )
        .await
        .expect_err("application A must not mint an artifact from application B's session");
    assert_eq!(denied.kind(), DeployServiceErrorKind::NotFound);
}
