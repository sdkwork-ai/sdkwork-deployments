//! Remix source and remix-state persistence against a real PostgreSQL schema.
//!
//! Skipped without `SDKWORK_DATABASE_TEST_POSTGRES_URL`, like the other
//! PostgreSQL integration tests in this crate.
//!
//! Three things can only be proven here:
//!
//! * the migration's constraints — one source per application, exactly one
//!   pointer, and a sub-path that cannot escape the checkout;
//! * the reserved `remix` repository row is *replaced*, not duplicated, when a
//!   Git source is edited;
//! * readiness degrades when the referenced row changes underneath it (an
//!   artifact retired after the source was bound), which is the whole reason the
//!   verdict is derived rather than stored.

mod common;

use std::sync::Arc;

use sdkwork_deploy_contract::{
    AppKind, AppSourceKind, AppSourceReadiness, CreateAppRequest, DeployAppRequestContext,
    DeployServiceErrorKind, UpdateAppRemixRequest, ARTIFACT_STATUS_ACTIVE,
    ARTIFACT_STATUS_RETAINED,
};
use sdkwork_deploy_drive_port::MemoryDeployDrivePort;
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::repository::{
    UpdateAppSourceCommand, UpdateAppSourceGitCommand,
};
use sdkwork_intelligence_deploy_service::{
    DeployRepositoryPort, DeployService, GitRepositoryProbePort, GitRepositoryProbeResult,
};
use sqlx::PgPool;

/// A probe whose answer the test controls.
///
/// Injected rather than dialled: what these tests prove is the persistence and
/// the readiness transition (`PENDING` → `VALIDATED`/`INVALID`, and never a
/// fabricated `VALIDATED`), not the HTTP client — which has its own loopback
/// tests in the service crate.
struct StaticGitProbe {
    result: GitRepositoryProbeResult,
}

#[async_trait::async_trait]
impl GitRepositoryProbePort for StaticGitProbe {
    async fn probe(&self, _repo_url: &str) -> GitRepositoryProbeResult {
        self.result.clone()
    }
}

fn probing_service(
    repository: &DeployRepository,
    result: GitRepositoryProbeResult,
) -> DeployService {
    service(repository).with_git_repository_probe(std::sync::Arc::new(StaticGitProbe { result }))
}

const TENANT_ID: i64 = 7;
const ORGANIZATION_ID: i64 = 9;
const ACTOR_ID: i64 = 11;

const SESSION_REF_ID: i64 = 9401;
const ARTIFACT_ID: i64 = 9501;
const ARTIFACT_UUID: &str = "eeeeeeee-7777-4777-8777-eeeeeeeeeeee";
const SESSION_UUID: &str = "cccccccc-8888-4888-8888-cccccccccccc";

async fn repository() -> (DeployRepository, PgPool) {
    common::migrated_repository_with_pool("sdkwork-deploy-remix-source-test").await
}

fn context() -> DeployAppRequestContext {
    DeployAppRequestContext {
        tenant_id: TENANT_ID,
        actor_id: Some(ACTOR_ID),
        organization_id: Some(ORGANIZATION_ID),
        session_id: Some("remix-source-test".to_owned()),
        app_id: None,
        auth_token: None,
        access_token: None,
        permissions: Vec::new(),
    }
}

fn service(repository: &DeployRepository) -> DeployService {
    DeployService::new(
        Arc::new(repository.clone()),
        Arc::new(MemoryDeployDrivePort::default()),
    )
}

async fn create_app(repository: &DeployRepository, slug: &str) -> String {
    repository
        .create_app(
            TENANT_ID,
            Some(ORGANIZATION_ID),
            Some(ACTOR_ID),
            None,
            &CreateAppRequest {
                name: format!("remix-app-{slug}"),
                slug: Some(slug.to_owned()),
                app_kind: AppKind::ApiService,
                app_type: Some(2),
                runtime_config: None,
                metadata: None,
                description: None,
                default_environment: None,
                app_domain_label: None,
                app_domain_suffixes: None,
                source_specs: None,
                owner_type: None,
                idempotency_key: None,
            },
        )
        .await
        .expect("create app")
        .id
}

async fn app_internal_id(pool: &PgPool, app_id: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM deploy_app WHERE tenant_id = $1 AND uuid = $2")
        .bind(TENANT_ID)
        .bind(app_id)
        .fetch_one(pool)
        .await
        .expect("resolve app internal id")
}

/// Seeds one completed upload session and its immutable artifact for `app_id`.
///
/// Written with raw SQL on purpose: the upload flow itself is covered by
/// `artifact_app_scope_postgres.rs`, and what this file needs is the *row* an
/// archive source points at.
async fn seed_artifact(pool: &PgPool, app_id: &str, artifact_uuid: &str) {
    let app_internal_id = app_internal_id(pool, app_id).await;
    sqlx::query(
        "INSERT INTO deploy_upload_session_ref
            (id, uuid, tenant_id, app_id, drive_upload_session_id, drive_space_id,
             drive_node_id, package_type, file_name, content_type, content_length,
             checksum, status, idempotency_key, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'drive-session-1', 'space-1', 'node-1', 1,
                 'source.zip', 'application/zip', 4096, 'checksum', 1, 'remix-idem-1',
                 NOW(), NOW())",
    )
    .bind(SESSION_REF_ID)
    .bind(SESSION_UUID)
    .bind(TENANT_ID)
    .bind(app_internal_id)
    .execute(pool)
    .await
    .expect("insert upload session fixture");

    sqlx::query(
        "INSERT INTO deploy_artifact
            (id, uuid, tenant_id, app_id, upload_session_ref_id, package_type, file_name,
             content_type, content_length, checksum_sha256, drive_node_id, drive_space_id,
             drive_path, status, created_at, updated_at, version)
         VALUES ($1, $2, $3, $4, $5, 1, 'source.zip', 'application/zip', 4096,
                 REPEAT('a', 64), 'node-1', 'space-1', 'drive://node/node-1', $6,
                 NOW(), NOW(), 1)",
    )
    .bind(ARTIFACT_ID)
    .bind(artifact_uuid)
    .bind(TENANT_ID)
    .bind(app_internal_id)
    .bind(SESSION_REF_ID)
    .bind(ARTIFACT_STATUS_ACTIVE)
    .execute(pool)
    .await
    .expect("insert artifact fixture");
}

fn git_command(sub_path: Option<&str>) -> UpdateAppSourceCommand {
    UpdateAppSourceCommand {
        kind: AppSourceKind::Git,
        git: Some(UpdateAppSourceGitCommand {
            repo_provider: "GITHUB".to_owned(),
            repo_url: "https://github.com/sdkwork/app.git".to_owned(),
            repo_ref: Some("release/1.4".to_owned()),
            sub_path: sub_path.map(str::to_owned),
            credential_secret_ref: None,
            clone_mode: "SHALLOW".to_owned(),
        }),
        archive_artifact_id: None,
    }
}

fn archive_command(artifact_uuid: &str) -> UpdateAppSourceCommand {
    UpdateAppSourceCommand {
        kind: AppSourceKind::Archive,
        git: None,
        archive_artifact_id: Some(artifact_uuid.to_owned()),
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn an_application_without_a_source_reports_source_not_configured() {
    let (repository, _pool) = repository().await;
    let app_id = create_app(&repository, "no-source").await;

    let source = repository
        .retrieve_app_source(TENANT_ID, &app_id)
        .await
        .expect("reading an unconfigured source is not an error");
    assert_eq!(source.readiness, AppSourceReadiness::Missing);
    assert_eq!(
        source.readiness_reason.as_deref(),
        Some("SOURCE_NOT_CONFIGURED")
    );
    assert!(!source.remix_enabled);
    assert!(source.source_kind.is_none());
    assert!(source.git.is_none());
    assert!(source.archive.is_none());
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_git_source_is_pending_until_validation_records_a_verdict() {
    let (repository, _pool) = repository().await;
    let app_id = create_app(&repository, "git-source").await;

    let source = repository
        .update_app_source(
            TENANT_ID,
            Some(ACTOR_ID),
            &app_id,
            &git_command(Some("apps/web")),
        )
        .await
        .expect("set git source");

    // A write is a declaration, not a reachability claim.
    assert_eq!(source.source_kind, Some(AppSourceKind::Git));
    let git = source.git.clone().expect("git half");
    assert_eq!(git.repo_provider, "GITHUB");
    assert_eq!(git.repo_url, "https://github.com/sdkwork/app.git");
    assert_eq!(git.repo_ref, "release/1.4");
    assert_eq!(git.sub_path.as_deref(), Some("apps/web"));
    assert_eq!(git.repo_status, "PENDING");
    assert!(git.last_validated_at.is_none());
    assert!(git.last_error_code.is_none());
    assert!(!git.credentials_required);
    assert!(source.archive.is_none());
    assert_eq!(source.readiness, AppSourceReadiness::Missing);
    assert_eq!(
        source.readiness_reason.as_deref(),
        Some("GIT_REPOSITORY_PENDING")
    );

    // 2xx moves PENDING → VALIDATED, and only then is the source READY.
    let service = probing_service(&repository, GitRepositoryProbeResult::Status(200));
    let validated = service
        .validate_app_source(&context(), &app_id)
        .await
        .expect("validate reachable repository");
    assert_eq!(validated.readiness, AppSourceReadiness::Ready);
    assert!(validated.readiness_reason.is_none());
    let git = validated.git.expect("git half");
    assert_eq!(git.repo_status, "VALIDATED");
    assert!(
        git.last_validated_at.is_some(),
        "VALIDATED must stamp last_validated_at"
    );
    assert!(git.last_error_code.is_none());
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn validation_of_an_absent_repository_records_invalid_and_the_reason() {
    let (repository, _pool) = repository().await;
    let app_id = create_app(&repository, "git-invalid").await;
    repository
        .update_app_source(TENANT_ID, Some(ACTOR_ID), &app_id, &git_command(None))
        .await
        .expect("set git source");

    let service = probing_service(&repository, GitRepositoryProbeResult::Status(404));
    let invalid = service
        .validate_app_source(&context(), &app_id)
        .await
        .expect("the attempt itself succeeds");
    assert_eq!(invalid.readiness, AppSourceReadiness::Missing);
    assert_eq!(
        invalid.readiness_reason.as_deref(),
        Some("GIT_REPOSITORY_INVALID")
    );
    let git = invalid.git.expect("git half");
    assert_eq!(git.repo_status, "INVALID");
    assert_eq!(
        git.last_error_code.as_deref(),
        Some("GIT_VALIDATION_REPOSITORY_NOT_FOUND")
    );
    assert!(
        git.last_validated_at.is_none(),
        "an invalid row must not carry a validation timestamp"
    );
    assert!(
        git.last_error_description
            .as_deref()
            .is_some_and(|description| !description.is_empty()),
        "the response must explain the failure"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_credentials_challenge_is_recorded_as_reachable_but_private() {
    let (repository, _pool) = repository().await;
    let app_id = create_app(&repository, "git-private").await;
    repository
        .update_app_source(TENANT_ID, Some(ACTOR_ID), &app_id, &git_command(None))
        .await
        .expect("set git source");

    let service = probing_service(&repository, GitRepositoryProbeResult::Status(401));
    let source = service
        .validate_app_source(&context(), &app_id)
        .await
        .expect("validate private repository");
    assert_eq!(
        source.readiness,
        AppSourceReadiness::Ready,
        "a 401 proves the repository exists, so the source is ready"
    );
    let git = source.git.expect("git half");
    assert_eq!(git.repo_status, "VALIDATED");
    assert!(
        git.credentials_required,
        "the clone will need credentialSecretRef"
    );
    assert_eq!(
        git.last_error_code.as_deref(),
        Some("GIT_VALIDATION_CREDENTIALS_REQUIRED")
    );
    assert!(
        git.last_validated_at.is_some(),
        "a reachable repository is validated even when it demands credentials"
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn an_ssh_remote_stays_pending_and_needs_manual_confirmation() {
    let (repository, _pool) = repository().await;
    let app_id = create_app(&repository, "git-ssh").await;
    let mut command = git_command(None);
    command.git.as_mut().expect("git half").repo_url = "git@github.com:sdkwork/app.git".to_owned();
    repository
        .update_app_source(TENANT_ID, Some(ACTOR_ID), &app_id, &command)
        .await
        .expect("set ssh git source");

    // Even with a probe installed: an SSH remote must never be reported as
    // validated, because nothing here can authenticate to it.
    let service = probing_service(&repository, GitRepositoryProbeResult::Status(200));
    let source = service
        .validate_app_source(&context(), &app_id)
        .await
        .expect("validate ssh repository");
    assert_eq!(source.readiness, AppSourceReadiness::Missing);
    assert_eq!(
        source.readiness_reason.as_deref(),
        Some("GIT_REPOSITORY_PENDING")
    );
    let git = source.git.expect("git half");
    assert_eq!(git.repo_status, "PENDING");
    assert_eq!(
        git.last_error_code.as_deref(),
        Some("GIT_VALIDATION_UNSUPPORTED_SCHEME")
    );
    assert!(git.last_validated_at.is_none());
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn an_identical_put_keeps_the_verdict_and_a_changed_url_invalidates_it() {
    let (repository, _pool) = repository().await;
    let app_id = create_app(&repository, "git-idempotent-put").await;
    repository
        .update_app_source(TENANT_ID, Some(ACTOR_ID), &app_id, &git_command(None))
        .await
        .expect("set git source");
    let service = probing_service(&repository, GitRepositoryProbeResult::Status(200));
    service
        .validate_app_source(&context(), &app_id)
        .await
        .expect("validate");

    let replay = repository
        .update_app_source(TENANT_ID, Some(ACTOR_ID), &app_id, &git_command(None))
        .await
        .expect("replay the same PUT");
    assert_eq!(
        replay.git.expect("git half").repo_status,
        "VALIDATED",
        "an identical PUT must not throw away a verdict it still describes"
    );

    let mut moved = git_command(None);
    moved.git.as_mut().expect("git half").repo_url =
        "https://gitlab.com/sdkwork/app.git".to_owned();
    let changed = repository
        .update_app_source(TENANT_ID, Some(ACTOR_ID), &app_id, &moved)
        .await
        .expect("replace with another repository");
    let git = changed.git.expect("git half");
    assert_eq!(git.repo_status, "PENDING");
    assert!(git.last_validated_at.is_none());
    assert_eq!(changed.readiness, AppSourceReadiness::Missing);
    assert_eq!(
        changed.readiness_reason.as_deref(),
        Some("GIT_REPOSITORY_PENDING")
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn validating_an_archive_source_is_a_validation_error() {
    let (repository, pool) = repository().await;
    let service = service(&repository);
    let app_id = create_app(&repository, "validate-archive").await;
    seed_artifact(&pool, &app_id, ARTIFACT_UUID).await;
    repository
        .update_app_source(
            TENANT_ID,
            Some(ACTOR_ID),
            &app_id,
            &archive_command(ARTIFACT_UUID),
        )
        .await
        .expect("set archive source");

    let refused = service
        .validate_app_source(&context(), &app_id)
        .await
        .expect_err("an archive has no repository to probe");
    assert_eq!(refused.kind(), DeployServiceErrorKind::Validation);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn editing_a_git_source_reuses_the_reserved_repository_row() {
    let (repository, pool) = repository().await;
    let app_id = create_app(&repository, "git-replace").await;

    repository
        .update_app_source(TENANT_ID, Some(ACTOR_ID), &app_id, &git_command(None))
        .await
        .expect("first git source");
    repository
        .update_app_source(
            TENANT_ID,
            Some(ACTOR_ID),
            &app_id,
            &git_command(Some("apps/h5")),
        )
        .await
        .expect("replace git source");

    let repository_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_source_repository r
         JOIN deploy_app a ON a.id = r.app_id
         WHERE a.uuid = $1 AND r.repo_key = 'remix' AND r.deleted_at IS NULL",
    )
    .bind(&app_id)
    .fetch_one(&pool)
    .await
    .expect("count repository rows");
    assert_eq!(
        repository_rows, 1,
        "the reserved remix repository must be replaced, not duplicated"
    );

    let source_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deploy_app_source s
         JOIN deploy_app a ON a.id = s.app_id
         WHERE a.uuid = $1 AND s.deleted_at IS NULL",
    )
    .bind(&app_id)
    .fetch_one(&pool)
    .await
    .expect("count source rows");
    assert_eq!(source_rows, 1, "an application has exactly one source");
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn an_archive_source_binds_an_active_artifact_and_is_ready() {
    let (repository, pool) = repository().await;
    let app_id = create_app(&repository, "archive-source").await;
    seed_artifact(&pool, &app_id, ARTIFACT_UUID).await;

    let source = repository
        .update_app_source(
            TENANT_ID,
            Some(ACTOR_ID),
            &app_id,
            &archive_command(ARTIFACT_UUID),
        )
        .await
        .expect("set archive source");

    assert_eq!(source.readiness, AppSourceReadiness::Ready);
    assert_eq!(source.source_kind, Some(AppSourceKind::Archive));
    let archive = source.archive.expect("archive half");
    assert_eq!(archive.artifact_id, ARTIFACT_UUID);
    assert_eq!(archive.upload_session_id, SESSION_UUID);
    assert_eq!(archive.file_name, "source.zip");
    assert_eq!(archive.content_length, 4096);
    assert_eq!(archive.drive_node_id, "node-1");
    assert!(source.git.is_none());
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn another_applications_artifact_cannot_become_the_source() {
    let (repository, pool) = repository().await;
    let owner = create_app(&repository, "artifact-owner").await;
    let other = create_app(&repository, "artifact-other").await;
    seed_artifact(&pool, &owner, ARTIFACT_UUID).await;

    let error = repository
        .update_app_source(
            TENANT_ID,
            Some(ACTOR_ID),
            &other,
            &archive_command(ARTIFACT_UUID),
        )
        .await
        .expect_err("a foreign artifact must be refused");
    assert_eq!(error.kind(), DeployServiceErrorKind::NotFound);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn enabling_remix_requires_a_ready_source() {
    let (repository, pool) = repository().await;
    let service = service(&repository);
    let app_id = create_app(&repository, "remix-gate").await;

    let refused = service
        .update_app_remix(
            &context(),
            &app_id,
            &UpdateAppRemixRequest { enabled: true },
        )
        .await
        .expect_err("no source means remix cannot be enabled");
    assert_eq!(refused.kind(), DeployServiceErrorKind::Conflict);
    assert!(
        refused.to_string().contains("SOURCE_NOT_CONFIGURED"),
        "the refusal must name the readiness reason: {refused}"
    );

    seed_artifact(&pool, &app_id, ARTIFACT_UUID).await;
    repository
        .update_app_source(
            TENANT_ID,
            Some(ACTOR_ID),
            &app_id,
            &archive_command(ARTIFACT_UUID),
        )
        .await
        .expect("set archive source");

    let enabled = service
        .update_app_remix(
            &context(),
            &app_id,
            &UpdateAppRemixRequest { enabled: true },
        )
        .await
        .expect("a ready source enables remix");
    assert!(enabled.remix_enabled);
    assert!(enabled.download.is_some());
    assert_eq!(
        enabled.download.expect("download instructions").mode,
        AppSourceKind::Archive
    );
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn a_retired_artifact_degrades_readiness_and_remix_can_still_be_disabled() {
    let (repository, pool) = repository().await;
    let service = service(&repository);
    let app_id = create_app(&repository, "remix-retired").await;
    seed_artifact(&pool, &app_id, ARTIFACT_UUID).await;
    repository
        .update_app_source(
            TENANT_ID,
            Some(ACTOR_ID),
            &app_id,
            &archive_command(ARTIFACT_UUID),
        )
        .await
        .expect("set archive source");
    service
        .update_app_remix(
            &context(),
            &app_id,
            &UpdateAppRemixRequest { enabled: true },
        )
        .await
        .expect("enable remix");

    // Nothing rewrote the source: the artifact was retired out from under it.
    sqlx::query("UPDATE deploy_artifact SET status = $2 WHERE uuid = $1")
        .bind(ARTIFACT_UUID)
        .bind(ARTIFACT_STATUS_RETAINED)
        .execute(&pool)
        .await
        .expect("retire artifact");

    let remix = service
        .retrieve_app_remix(&context(), &app_id)
        .await
        .expect("read remix state");
    assert!(remix.remix_enabled, "the switch itself was not rewritten");
    assert_eq!(remix.readiness, AppSourceReadiness::Missing);
    assert_eq!(
        remix.readiness_reason.as_deref(),
        Some("ARCHIVE_UNAVAILABLE")
    );
    assert!(
        remix.download.is_none(),
        "an unusable source must not offer a download"
    );

    let disabled = service
        .update_app_remix(
            &context(),
            &app_id,
            &UpdateAppRemixRequest { enabled: false },
        )
        .await
        .expect("disabling is always allowed");
    assert!(!disabled.remix_enabled);
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]
async fn deleting_the_source_turns_remix_off() {
    let (repository, pool) = repository().await;
    let service = service(&repository);
    let app_id = create_app(&repository, "remix-delete").await;
    seed_artifact(&pool, &app_id, ARTIFACT_UUID).await;
    repository
        .update_app_source(
            TENANT_ID,
            Some(ACTOR_ID),
            &app_id,
            &archive_command(ARTIFACT_UUID),
        )
        .await
        .expect("set archive source");
    service
        .update_app_remix(
            &context(),
            &app_id,
            &UpdateAppRemixRequest { enabled: true },
        )
        .await
        .expect("enable remix");

    service
        .delete_app_source(&context(), &app_id)
        .await
        .expect("delete source");

    let source = repository
        .retrieve_app_source(TENANT_ID, &app_id)
        .await
        .expect("read source after deletion");
    assert_eq!(source.readiness, AppSourceReadiness::Missing);
    assert_eq!(
        source.readiness_reason.as_deref(),
        Some("SOURCE_NOT_CONFIGURED")
    );
    assert!(
        !source.remix_enabled,
        "deleting the source must clear the remix switch in the same transaction"
    );

    let second_delete = service.delete_app_source(&context(), &app_id).await;
    assert_eq!(
        second_delete.expect_err("already deleted").kind(),
        DeployServiceErrorKind::NotFound
    );
}
