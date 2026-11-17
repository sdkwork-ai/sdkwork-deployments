//! Remix source persistence: the application's single source row, the reserved
//! `remix` source repository, and `deploy_app.remix_enabled`.
//!
//! # Readiness is computed here, from the rows as they are now
//!
//! [`DeployRepository::retrieve_app_source_repo`] joins `deploy_app_source` to
//! whichever row its pointer column names and hands the *current* facts to
//! `sdkwork_deploy_core::evaluate_app_source_readiness`. A repository that has
//! since been soft-deleted, an artifact that was retired, a path that no longer
//! satisfies the rules — all of them degrade the verdict without any writer having
//! to remember to update a status column.
//!
//! # Why the write is one transaction
//!
//! A Git source is two rows (`deploy_source_repository` + `deploy_app_source`); an
//! archive source is a binding plus an ownership check against `deploy_artifact`.
//! A half-written pair would leave `deploy_app_source.git_repository_id` naming a
//! repository the operator never confirmed, so both are written inside a single
//! transaction and the read-back happens after commit.

use sdkwork_deploy_contract::{
    AppSourceArchiveResponse, AppSourceGitResponse, AppSourceKind, AppSourceResponse,
    DeployServiceError, DeployServiceResult, ARTIFACT_STATUS_ACTIVE,
};
use sdkwork_deploy_core::{
    evaluate_app_source_readiness, AppSourceArchiveFacts, AppSourceFacts, AppSourceGitFacts,
    AppSourceReadinessReason, GitRepositoryValidationStatus, GitValidationErrorCode,
    REMIX_SOURCE_REPOSITORY_KEY, SOURCE_REPOSITORY_STATUS_PENDING,
    SOURCE_REPOSITORY_STATUS_VALIDATED,
};
use sdkwork_intelligence_deploy_service::repository::{
    GitRepositoryValidationWrite, UpdateAppSourceCommand, UpdateAppSourceGitCommand,
};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

use crate::support::{
    new_uuid, next_id, now_rfc3339, optional_datetime_from_row, required_datetime,
    resolve_app_internal_id, store_error,
};
use crate::DeployRepository;

/// Every column the remix source read needs.
///
/// One join rather than three queries: readiness is a statement about the pointer
/// and the row it names *together*, and reading them in separate statements would
/// let a concurrent write make the verdict describe a state that never existed.
const APP_SOURCE_SELECT: &str = "
    a.uuid AS app_uuid,
    a.remix_enabled,
    s.uuid AS source_uuid,
    s.source_kind,
    s.source_ref,
    s.sub_path,
    s.updated_at AS source_updated_at,
    s.version AS source_version,
    r.uuid AS repo_uuid,
    r.repo_provider,
    r.repo_url,
    r.default_branch,
    r.clone_mode,
    r.credential_secret_ref,
    r.repo_status,
    r.last_validated_at,
    r.last_error_code,
    COALESCE(r.deleted_at IS NOT NULL, FALSE) AS repo_deleted,
    ar.uuid AS artifact_uuid,
    ar.package_type AS artifact_package_type,
    ar.file_name AS artifact_file_name,
    ar.content_type AS artifact_content_type,
    ar.content_length AS artifact_content_length,
    ar.checksum_sha256 AS artifact_checksum_sha256,
    ar.drive_node_id AS artifact_drive_node_id,
    ar.drive_space_id AS artifact_drive_space_id,
    ar.status AS artifact_status,
    ar.created_at AS artifact_created_at,
    u.uuid AS upload_session_uuid";

/// The Git facts readiness needs, owned by the mapper.
///
/// Owned rather than borrowed from the row because [`evaluate_app_source_readiness`]
/// takes borrowed facts and the returned verdict must outlive no borrow: leaking
/// the row's strings to satisfy a lifetime would trade a compile-time detail for a
/// permanent memory leak on every read.
struct OwnedGitFacts {
    repo_provider: String,
    repo_url: String,
    repo_status: String,
    repo_deleted: bool,
    source_ref: Option<String>,
    sub_path: Option<String>,
}

impl OwnedGitFacts {
    fn from_row(row: &PgRow) -> Self {
        Self {
            repo_provider: row.try_get("repo_provider").unwrap_or_default(),
            repo_url: row.try_get("repo_url").unwrap_or_default(),
            repo_status: row.try_get("repo_status").unwrap_or_default(),
            repo_deleted: row.try_get("repo_deleted").unwrap_or(false),
            source_ref: row.try_get("source_ref").ok().flatten(),
            sub_path: row.try_get("sub_path").ok().flatten(),
        }
    }

    fn as_facts(&self) -> AppSourceGitFacts<'_> {
        AppSourceGitFacts {
            repo_provider: &self.repo_provider,
            repo_url: &self.repo_url,
            repo_status: &self.repo_status,
            repo_deleted: self.repo_deleted,
            source_ref: self.source_ref.as_deref(),
            sub_path: self.sub_path.as_deref(),
        }
    }
}

impl DeployRepository {
    pub(super) async fn retrieve_app_source_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<AppSourceResponse> {
        let query = format!(
            "SELECT {APP_SOURCE_SELECT}
             FROM deploy_app a
             LEFT JOIN deploy_app_source s
                    ON s.app_id = a.id AND s.deleted_at IS NULL
             LEFT JOIN deploy_source_repository r ON r.id = s.git_repository_id
             LEFT JOIN deploy_artifact ar ON ar.id = s.archive_artifact_id
             LEFT JOIN deploy_upload_session_ref u ON u.id = ar.upload_session_ref_id
             WHERE a.tenant_id = $1 AND a.uuid = $2 AND a.deleted_at IS NULL"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(app_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve deploy_app_source", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("app not found"));
        };
        map_app_source_row(&row)
    }

    pub(super) async fn update_app_source_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        command: &UpdateAppSourceCommand,
    ) -> DeployServiceResult<AppSourceResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin deploy_app_source transaction", error))?;

        let (git_repository_id, archive_artifact_id, source_ref, sub_path) = match command.kind {
            AppSourceKind::Git => {
                let Some(git) = command.git.as_ref() else {
                    return Err(DeployServiceError::Internal(
                        "git source command is missing the git half".to_owned(),
                    ));
                };
                let repository_id = upsert_remix_repository(
                    &mut transaction,
                    next_id(self.id_generator())?,
                    tenant_id,
                    app_internal_id,
                    actor_id,
                    git,
                )
                .await?;
                (
                    Some(repository_id),
                    None,
                    git.repo_ref.clone(),
                    git.sub_path.clone(),
                )
            }
            AppSourceKind::Archive => {
                let Some(artifact_uuid) = command.archive_artifact_id.as_deref() else {
                    return Err(DeployServiceError::Internal(
                        "archive source command is missing the artifact id".to_owned(),
                    ));
                };
                let artifact_id = resolve_active_source_artifact(
                    &mut transaction,
                    tenant_id,
                    app_internal_id,
                    artifact_uuid,
                )
                .await?;
                (None, Some(artifact_id), None, None)
            }
        };

        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO deploy_app_source
                (id, uuid, tenant_id, organization_id, app_id, source_kind,
                 git_repository_id, archive_artifact_id, source_ref, sub_path,
                 created_by, updated_by, created_at, updated_at, version)
             VALUES ($1, $2, $3, 0, $4, $5, $6, $7, $8, $9, $10, $10,
                     CAST($11 AS TIMESTAMPTZ), CAST($11 AS TIMESTAMPTZ), 1)
             ON CONFLICT (app_id) WHERE deleted_at IS NULL DO UPDATE SET
                 source_kind = EXCLUDED.source_kind,
                 git_repository_id = EXCLUDED.git_repository_id,
                 archive_artifact_id = EXCLUDED.archive_artifact_id,
                 source_ref = EXCLUDED.source_ref,
                 sub_path = EXCLUDED.sub_path,
                 updated_by = EXCLUDED.updated_by,
                 updated_at = EXCLUDED.updated_at,
                 version = deploy_app_source.version + 1",
        )
        .bind(next_id(self.id_generator())?)
        .bind(new_uuid())
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(command.kind.as_str())
        .bind(git_repository_id)
        .bind(archive_artifact_id)
        .bind(source_ref.as_deref())
        .bind(sub_path.as_deref())
        .bind(actor_id)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("upsert deploy_app_source", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit deploy_app_source transaction", error))?;

        self.retrieve_app_source_repo(tenant_id, app_id).await
    }

    pub(super) async fn delete_app_source_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
    ) -> DeployServiceResult<()> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin deploy_app_source deletion", error))?;

        let now = now_rfc3339();
        let deleted = sqlx::query(
            "UPDATE deploy_app_source
             SET deleted_at = CAST($3 AS TIMESTAMPTZ),
                 updated_at = CAST($3 AS TIMESTAMPTZ),
                 updated_by = $4,
                 version = version + 1
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(&now)
        .bind(actor_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("retire deploy_app_source", error))?;

        if deleted.rows_affected() == 0 {
            return Err(DeployServiceError::not_found(
                "the application has no remix source",
            ));
        }

        // Removing the source must not leave remix advertising a download that has
        // nothing to resolve. Done in the same transaction so no reader ever sees
        // "remix enabled, no source".
        sqlx::query(
            "UPDATE deploy_app
             SET remix_enabled = FALSE,
                 updated_at = CAST($3 AS TIMESTAMPTZ),
                 updated_by = $4,
                 version = version + 1
             WHERE tenant_id = $1 AND id = $2 AND remix_enabled",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(&now)
        .bind(actor_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("disable remix after source deletion", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit deploy_app_source deletion", error))?;
        Ok(())
    }

    pub(super) async fn update_app_remix_enabled_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        enabled: bool,
    ) -> DeployServiceResult<()> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let now = now_rfc3339();
        sqlx::query(
            "UPDATE deploy_app
             SET remix_enabled = $3,
                 updated_at = CAST($4 AS TIMESTAMPTZ),
                 updated_by = $5,
                 version = version + 1
             WHERE tenant_id = $1 AND id = $2 AND remix_enabled IS DISTINCT FROM $3",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(enabled)
        .bind(&now)
        .bind(actor_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("update deploy_app.remix_enabled", error))?;
        Ok(())
    }

    pub(super) async fn record_git_repository_validation_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        write: &GitRepositoryValidationWrite,
    ) -> DeployServiceResult<AppSourceResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        record_git_repository_validation_row(
            &self.pool,
            tenant_id,
            app_internal_id,
            actor_id,
            write,
        )
        .await?;
        self.retrieve_app_source_repo(tenant_id, app_id).await
    }
}

/// Writes the reserved `remix` repository row and answers its internal id.
///
/// The row is what carries the provider, URL, credential reference and validation
/// state the readiness rule reads. A write lands in `PENDING`, never in
/// `VALIDATED`: the service validated the *declaration* (provider, scheme, ref,
/// sub-path), and this layer has no way to know the remote exists. Only
/// `apps.source.validate` records a verdict.
///
/// A repeated identical PUT keeps the recorded verdict, which is what makes the
/// operation idempotent; changing the repository *identity* — provider, URL or
/// credential reference — invalidates it, because a verdict describes one
/// repository. `cloneMode` is not part of the identity: it changes how a client
/// downloads, not which repository exists.
///
/// `default_branch` is left at `main` rather than being overwritten with the
/// selected ref: this row records *where* the repository is, and the ref is a
/// property of the source selection (`deploy_app_source.source_ref`), which is
/// what the response reports as `repoRef`.
async fn upsert_remix_repository(
    transaction: &mut Transaction<'static, Postgres>,
    repository_id: i64,
    tenant_id: i64,
    app_internal_id: i64,
    actor_id: Option<i64>,
    git: &UpdateAppSourceGitCommand,
) -> DeployServiceResult<i64> {
    let now = now_rfc3339();
    let row = sqlx::query(
        "INSERT INTO deploy_source_repository
            (id, uuid, tenant_id, organization_id, app_id, repo_key, repo_provider,
             repo_url, default_branch, clone_mode, credential_secret_ref, repo_status,
             last_validated_at, last_error_code, created_by, updated_by, created_at,
             updated_at, version)
         VALUES ($1, $2, $3, 0, $4, $5, $6, $7, 'main', $8, $9, $10, NULL, NULL,
                 $11, $11, CAST($12 AS TIMESTAMPTZ), CAST($12 AS TIMESTAMPTZ), 1)
         ON CONFLICT (app_id, repo_key) WHERE deleted_at IS NULL DO UPDATE SET
             repo_provider = EXCLUDED.repo_provider,
             repo_url = EXCLUDED.repo_url,
             clone_mode = EXCLUDED.clone_mode,
             credential_secret_ref = EXCLUDED.credential_secret_ref,
             repo_status = CASE
                 WHEN deploy_source_repository.repo_provider = EXCLUDED.repo_provider
                  AND deploy_source_repository.repo_url = EXCLUDED.repo_url
                  AND deploy_source_repository.credential_secret_ref
                      IS NOT DISTINCT FROM EXCLUDED.credential_secret_ref
                 THEN deploy_source_repository.repo_status
                 ELSE EXCLUDED.repo_status
             END,
             last_validated_at = CASE
                 WHEN deploy_source_repository.repo_provider = EXCLUDED.repo_provider
                  AND deploy_source_repository.repo_url = EXCLUDED.repo_url
                  AND deploy_source_repository.credential_secret_ref
                      IS NOT DISTINCT FROM EXCLUDED.credential_secret_ref
                 THEN deploy_source_repository.last_validated_at
                 ELSE EXCLUDED.last_validated_at
             END,
             last_error_code = CASE
                 WHEN deploy_source_repository.repo_provider = EXCLUDED.repo_provider
                  AND deploy_source_repository.repo_url = EXCLUDED.repo_url
                  AND deploy_source_repository.credential_secret_ref
                      IS NOT DISTINCT FROM EXCLUDED.credential_secret_ref
                 THEN deploy_source_repository.last_error_code
                 ELSE EXCLUDED.last_error_code
             END,
             updated_by = EXCLUDED.updated_by,
             updated_at = EXCLUDED.updated_at,
             version = deploy_source_repository.version + 1
         RETURNING id",
    )
    .bind(repository_id)
    .bind(new_uuid())
    .bind(tenant_id)
    .bind(app_internal_id)
    .bind(REMIX_SOURCE_REPOSITORY_KEY)
    .bind(&git.repo_provider)
    .bind(&git.repo_url)
    .bind(&git.clone_mode)
    .bind(git.credential_secret_ref.as_deref())
    .bind(SOURCE_REPOSITORY_STATUS_PENDING)
    .bind(actor_id)
    .bind(&now)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| store_error("upsert remix deploy_source_repository", error))?;
    row.try_get("id")
        .map_err(|error| DeployServiceError::Internal(format!("read remix repository id: {error}")))
}

/// Records one validation verdict on the reserved `remix` repository row.
///
/// `repo_status`, `last_validated_at` and `last_error_code` are one fact and are
/// written together: the timestamp is set exactly for `VALIDATED` and cleared
/// otherwise, so no reader sees a validated timestamp on an invalid row (or the
/// reverse).
async fn record_git_repository_validation_row(
    pool: &sqlx::PgPool,
    tenant_id: i64,
    app_internal_id: i64,
    actor_id: Option<i64>,
    write: &GitRepositoryValidationWrite,
) -> DeployServiceResult<()> {
    let now = now_rfc3339();
    let validated_at =
        (write.repo_status == GitRepositoryValidationStatus::Validated).then(|| now.clone());
    let updated = sqlx::query(
        "UPDATE deploy_source_repository r
            SET repo_status = $3,
                last_validated_at = CAST($4 AS TIMESTAMPTZ),
                last_error_code = $5,
                updated_by = $6,
                updated_at = CAST($7 AS TIMESTAMPTZ),
                version = r.version + 1
           FROM deploy_app_source s
          WHERE s.git_repository_id = r.id
            AND s.tenant_id = $1
            AND s.app_id = $2
            AND s.deleted_at IS NULL
            AND r.tenant_id = $1
            AND r.repo_key = $8
            AND r.deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(app_internal_id)
    .bind(write.repo_status.as_str())
    .bind(validated_at.as_deref())
    .bind(write.last_error_code.map(GitValidationErrorCode::as_str))
    .bind(actor_id)
    .bind(&now)
    .bind(REMIX_SOURCE_REPOSITORY_KEY)
    .execute(pool)
    .await
    .map_err(|error| store_error("record Git repository validation", error))?;
    if updated.rows_affected() == 0 {
        return Err(DeployServiceError::not_found(
            "the application has no Git source to validate",
        ));
    }
    Ok(())
}

/// Resolves the artifact named by an archive source and proves it is usable.
///
/// Two refusals, both before the binding is written: the artifact must belong to
/// this application (a tenant-level or foreign artifact would let one application
/// remix another's upload) and it must still be active (a retained artifact is not
/// downloadable, so binding it would create a source that is `MISSING` the moment
/// it is created).
async fn resolve_active_source_artifact(
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    app_internal_id: i64,
    artifact_uuid: &str,
) -> DeployServiceResult<i64> {
    let row = sqlx::query(
        "SELECT id, status FROM deploy_artifact
         WHERE tenant_id = $1 AND uuid = $2 AND app_id = $3",
    )
    .bind(tenant_id)
    .bind(artifact_uuid)
    .bind(app_internal_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| store_error("resolve deploy_artifact for remix source", error))?;
    let Some(row) = row else {
        return Err(DeployServiceError::not_found("artifact not found"));
    };
    let status: i32 = row
        .try_get("status")
        .map_err(|error| DeployServiceError::Internal(format!("read artifact status: {error}")))?;
    if status != ARTIFACT_STATUS_ACTIVE {
        return Err(DeployServiceError::validation(
            "artifact is not active and cannot be used as a remix source",
        ));
    }
    row.try_get("id")
        .map_err(|error| DeployServiceError::Internal(format!("read artifact id: {error}")))
}

fn map_app_source_row(row: &PgRow) -> DeployServiceResult<AppSourceResponse> {
    let app_id: String = row.try_get("app_uuid").unwrap_or_default();
    let remix_enabled: bool = row.try_get("remix_enabled").unwrap_or(false);
    let source_kind = row
        .try_get::<Option<String>, _>("source_kind")
        .ok()
        .flatten()
        .and_then(|value| AppSourceKind::parse(&value));

    let git = match source_kind {
        Some(AppSourceKind::Git) => Some(git_response(row)?),
        _ => None,
    };
    let archive = match source_kind {
        Some(AppSourceKind::Archive) => Some(archive_response(row)?),
        _ => None,
    };

    let owned_git = (source_kind == Some(AppSourceKind::Git)).then(|| OwnedGitFacts::from_row(row));
    let artifact_status: Option<i32> = row.try_get("artifact_status").ok().flatten();
    let facts = source_kind.map(|kind| AppSourceFacts {
        kind,
        git: owned_git.as_ref().map(OwnedGitFacts::as_facts),
        archive: (kind == AppSourceKind::Archive).then_some(AppSourceArchiveFacts {
            artifact_active: artifact_status == Some(ARTIFACT_STATUS_ACTIVE),
        }),
    });
    let outcome = evaluate_app_source_readiness(facts.as_ref());

    Ok(AppSourceResponse {
        app_id,
        remix_enabled,
        source_kind,
        readiness: outcome.readiness,
        readiness_reason: outcome
            .reason
            .map(|reason: AppSourceReadinessReason| reason.as_str().to_owned()),
        git,
        archive,
        updated_at: optional_datetime_from_row(row, "source_updated_at").map_err(|error| {
            DeployServiceError::Internal(format!("read source updated_at: {error}"))
        })?,
        version: row
            .try_get::<Option<i64>, _>("source_version")
            .ok()
            .flatten()
            .map(|value| value.to_string()),
    })
}

/// The Git half of the response.
///
/// `repo_ref` is the *effective* ref: the recorded `source_ref` when one was
/// selected, otherwise the repository row's `default_branch`. Returning the
/// fallback here is what lets the remix client use `repoRef` unconditionally.
///
/// The validation facts (`repoStatus`, `lastValidatedAt`, `lastErrorCode`) are
/// read from the row and explained through the core vocabulary, so the console
/// gets the same sentence the audit trail recorded instead of carrying its own
/// copy of every code.
fn git_response(row: &PgRow) -> DeployServiceResult<AppSourceGitResponse> {
    let source_ref: Option<String> = row.try_get("source_ref").ok().flatten();
    let default_branch: String = row.try_get("default_branch").unwrap_or_default();
    let repo_status: String = row.try_get("repo_status").unwrap_or_default();
    let last_error_code: Option<String> = row.try_get("last_error_code").ok().flatten();
    let parsed_error = last_error_code
        .as_deref()
        .and_then(GitValidationErrorCode::parse);
    Ok(AppSourceGitResponse {
        repo_provider: row.try_get("repo_provider").unwrap_or_default(),
        repo_url: row.try_get("repo_url").unwrap_or_default(),
        repo_ref: source_ref.unwrap_or(default_branch),
        sub_path: row.try_get("sub_path").ok().flatten(),
        clone_mode: row.try_get("clone_mode").unwrap_or_default(),
        credential_secret_ref: row.try_get("credential_secret_ref").ok().flatten(),
        repo_status: repo_status.clone(),
        last_validated_at: optional_datetime_from_row(row, "last_validated_at").map_err(
            |error| {
                DeployServiceError::Internal(format!("read repository last_validated_at: {error}"))
            },
        )?,
        last_error_code,
        last_error_description: parsed_error.map(|code| code.description().to_owned()),
        credentials_required: repo_status == SOURCE_REPOSITORY_STATUS_VALIDATED
            && parsed_error == Some(GitValidationErrorCode::CredentialsRequired),
    })
}

fn archive_response(row: &PgRow) -> DeployServiceResult<AppSourceArchiveResponse> {
    Ok(AppSourceArchiveResponse {
        artifact_id: row.try_get("artifact_uuid").unwrap_or_default(),
        upload_session_id: row.try_get("upload_session_uuid").unwrap_or_default(),
        package_type: row.try_get("artifact_package_type").unwrap_or_default(),
        file_name: row.try_get("artifact_file_name").unwrap_or_default(),
        content_type: row.try_get("artifact_content_type").unwrap_or_default(),
        content_length: row.try_get("artifact_content_length").unwrap_or_default(),
        checksum_sha256: row.try_get("artifact_checksum_sha256").ok().flatten(),
        drive_node_id: row.try_get("artifact_drive_node_id").unwrap_or_default(),
        drive_space_id: row.try_get("artifact_drive_space_id").ok().flatten(),
        created_at: required_datetime(row, "artifact_created_at")?,
    })
}
