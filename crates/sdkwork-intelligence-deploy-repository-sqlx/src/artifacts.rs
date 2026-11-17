use sdkwork_deploy_contract::{
    is_deploy_package_artifact_type, ArtifactPage, ArtifactResponse, CreateArtifactRequest,
    DeployServiceError, DeployServiceResult, ARTIFACT_STATUS_ACTIVE, ARTIFACT_STATUS_RETAINED,
    UPLOAD_SESSION_STATUS_COMPLETED,
};
use sqlx::{postgres::PgRow, AssertSqlSafe, PgPool, Row};

use crate::support::{
    datetime_from_row, new_uuid, next_id, now_rfc3339, pagination, resolve_app_internal_id,
    resolve_app_uuid, resolve_scope_app_internal_id, store_error,
};
use crate::DeployRepository;

const ARTIFACT_SELECT: &str = "a.uuid, a.app_id, a.package_type, a.file_name, a.content_type,
    a.content_length, a.checksum_sha256, a.drive_node_id, a.status, a.created_at,
    u.uuid AS upload_session_uuid";

/// The `(tenant_id, app_id)` half of an artifact statement
/// (`APPLICATION_IDENTITY_SPEC.md` §4.3).
///
/// `parameter` is the positional placeholder carrying the internal id of the
/// deploy application the caller's credential proves, bound as `NULL` when the
/// credential names no application. A bound scope restricts the statement to
/// rows attributed to that application, so one application can neither
/// enumerate, read, nor retire another's artifacts. `NULL` keeps the
/// pre-existing tenant-wide behaviour for callers whose credential carries no
/// application identity (for example a user auth token without an `app_id`
/// claim).
///
/// The column is nullable in the baseline (`deploy_artifact.app_id BIGINT
/// NULL`), so an un-attributed legacy row is *not* another application's data;
/// it stays unreachable for an application-scoped caller and visible only to
/// the tenant-wide reader. Backfilling attribution is a schema/data task and is
/// recorded as a follow-up rather than done here.
fn artifact_app_scope_predicate(parameter: usize) -> String {
    format!("(${parameter}::BIGINT IS NULL OR a.app_id = ${parameter})")
}

impl DeployRepository {
    pub(super) async fn list_artifacts_repo(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ArtifactPage> {
        let (_page, page_size, offset) = pagination(page, page_size);
        let app_internal_id =
            resolve_scope_app_internal_id(&self.pool, tenant_id, app_scope).await?;

        let count_query = format!(
            "SELECT COUNT(*) AS total FROM deploy_artifact a
             WHERE a.tenant_id = $1 AND a.status = $2 AND {}",
            artifact_app_scope_predicate(3)
        );
        let count_row = sqlx::query(AssertSqlSafe(&*count_query))
            .bind(tenant_id)
            .bind(ARTIFACT_STATUS_ACTIVE)
            .bind(app_internal_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count deploy_artifact", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let query = format!(
            "SELECT {ARTIFACT_SELECT}
             FROM deploy_artifact a
             JOIN deploy_upload_session_ref u ON u.id = a.upload_session_ref_id
             WHERE a.tenant_id = $1 AND a.status = $2 AND {}
             ORDER BY a.created_at DESC, a.id DESC LIMIT $4 OFFSET $5",
            artifact_app_scope_predicate(3)
        );
        let rows = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(ARTIFACT_STATUS_ACTIVE)
            .bind(app_internal_id)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list deploy_artifact", error))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(
                map_artifact_row(&self.pool, tenant_id, row)
                    .await
                    .map_err(|error| {
                        DeployServiceError::Internal(format!("map deploy_artifact row: {error}"))
                    })?,
            );
        }

        Ok(ArtifactPage { items, total })
    }

    pub(super) async fn retrieve_artifact_repo(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        artifact_id: &str,
    ) -> DeployServiceResult<ArtifactResponse> {
        let app_internal_id =
            resolve_scope_app_internal_id(&self.pool, tenant_id, app_scope).await?;
        let query = format!(
            "SELECT {ARTIFACT_SELECT}
             FROM deploy_artifact a
             JOIN deploy_upload_session_ref u ON u.id = a.upload_session_ref_id
             WHERE a.tenant_id = $1 AND a.uuid = $2 AND a.status = $3 AND {}",
            artifact_app_scope_predicate(4)
        );
        let row = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(artifact_id)
            .bind(ARTIFACT_STATUS_ACTIVE)
            .bind(app_internal_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve deploy_artifact", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("artifact not found"));
        };
        map_artifact_row(&self.pool, tenant_id, &row)
            .await
            .map_err(|error| DeployServiceError::Internal(error.to_string()))
    }

    pub(super) async fn create_artifact_from_drive_repo(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        request: &CreateArtifactRequest,
    ) -> DeployServiceResult<ArtifactResponse> {
        if let Some(existing) = self
            .find_upload_session_by_idempotency_key_repo(
                tenant_id,
                app_scope,
                &request.idempotency_key,
            )
            .await?
        {
            return self
                .create_artifact_from_upload_session_repo(
                    tenant_id,
                    app_scope,
                    &existing.id,
                    request.checksum_sha256.as_deref().unwrap_or(""),
                )
                .await;
        }

        // The credential-proved application wins over the body's `appId`: the
        // service layer has already refused a body naming a different
        // application, and this ordering keeps a caller that omits the field
        // from registering an un-attributed row it could then not read.
        let bound_app_uuid = app_scope.or(request.app_id.as_deref());
        let app_internal_id = match bound_app_uuid {
            Some(app_id) => Some(resolve_app_internal_id(&self.pool, tenant_id, app_id).await?),
            None => None,
        };
        let reference_id = next_id(self.id_generator())?;
        let reference_uuid = new_uuid();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO deploy_upload_session_ref
             (id, uuid, tenant_id, app_id, drive_upload_session_id, drive_upload_item_id,
              drive_space_id, drive_node_id, package_type, file_name, content_type,
              content_length, checksum, status, idempotency_key, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, CAST($16 AS TIMESTAMPTZ), CAST($16 AS TIMESTAMPTZ))",
        )
        .bind(reference_id)
        .bind(&reference_uuid)
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(request.drive_upload_session_id.trim())
        .bind(request.drive_upload_item_id.as_deref())
        .bind(request.drive_space_id.trim())
        .bind(request.drive_node_id.trim())
        .bind(request.package_type)
        .bind(request.file_name.trim())
        .bind(request.content_type.trim())
        .bind(request.content_length)
        .bind(request.checksum_sha256.as_deref())
        .bind(UPLOAD_SESSION_STATUS_COMPLETED)
        .bind(request.idempotency_key.trim())
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("register Drive artifact reference", error))?;

        self.create_artifact_from_upload_session_repo(
            tenant_id,
            app_scope,
            &reference_uuid,
            request.checksum_sha256.as_deref().unwrap_or(""),
        )
        .await
    }

    pub(super) async fn retain_artifact_repo(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        artifact_id: &str,
    ) -> DeployServiceResult<()> {
        let app_internal_id =
            resolve_scope_app_internal_id(&self.pool, tenant_id, app_scope).await?;
        let now = now_rfc3339();
        let query = format!(
            "UPDATE deploy_artifact a
             SET status = $3, updated_at = CAST($4 AS TIMESTAMPTZ), version = version + 1
             WHERE a.tenant_id = $1 AND a.uuid = $2 AND a.status = $5 AND {}",
            artifact_app_scope_predicate(6)
        );
        let result = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(artifact_id)
            .bind(ARTIFACT_STATUS_RETAINED)
            .bind(&now)
            .bind(ARTIFACT_STATUS_ACTIVE)
            .bind(app_internal_id)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("retain deploy_artifact", error))?;

        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found("artifact not found"));
        }
        Ok(())
    }

    pub(super) async fn create_artifact_from_upload_session_repo(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        upload_session_id: &str,
        checksum_sha256: &str,
    ) -> DeployServiceResult<ArtifactResponse> {
        let app_internal_id =
            resolve_scope_app_internal_id(&self.pool, tenant_id, app_scope).await?;
        let session_query = format!(
            "SELECT s.id, s.uuid, s.app_id, s.package_type, s.file_name, s.content_type,
                    s.content_length, s.status, s.drive_node_id, s.drive_space_id
             FROM deploy_upload_session_ref s
             WHERE s.tenant_id = $1 AND s.uuid = $2 AND s.deleted_at IS NULL
               AND ($3::BIGINT IS NULL OR s.app_id = $3)"
        );
        let session_row = sqlx::query(AssertSqlSafe(&*session_query))
            .bind(tenant_id)
            .bind(upload_session_id)
            .bind(app_internal_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("load upload session for artifact", error))?
            .ok_or_else(|| DeployServiceError::not_found("upload session not found"))?;

        let session_internal_id: i64 = session_row
            .try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("upload session id: {error}")))?;
        let package_type: i32 = session_row.try_get("package_type").unwrap_or(1);
        let status: i32 = session_row.try_get("status").unwrap_or(0);

        if !is_deploy_package_artifact_type(package_type) {
            return Err(DeployServiceError::validation(
                "upload session package type does not produce a deploy artifact",
            ));
        }
        if status != UPLOAD_SESSION_STATUS_COMPLETED {
            return Err(DeployServiceError::validation(
                "upload session must be completed before creating an artifact",
            ));
        }

        let existing =
            sqlx::query("SELECT uuid FROM deploy_artifact WHERE upload_session_ref_id = $1")
                .bind(session_internal_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error("find deploy_artifact by upload session", error))?;

        if let Some(row) = existing {
            let artifact_uuid: String = row
                .try_get("uuid")
                .map_err(|error| DeployServiceError::Internal(format!("artifact uuid: {error}")))?;
            return self
                .retrieve_artifact_repo(tenant_id, app_scope, &artifact_uuid)
                .await;
        }

        let drive_node_id: Option<String> = session_row.try_get("drive_node_id").ok();
        let drive_node_id = drive_node_id.ok_or_else(|| {
            DeployServiceError::validation("completed upload session has no drive node")
        })?;
        let drive_space_id: Option<String> = session_row.try_get("drive_space_id").ok();
        // The artifact inherits the session's attribution: that is the application
        // the upload was registered for, already narrowed by the scope predicate
        // above, so it cannot name a third application here.
        let session_app_internal_id: Option<i64> = session_row.try_get("app_id").ok();
        let file_name: String = session_row.try_get("file_name").unwrap_or_default();
        let content_type: String = session_row.try_get("content_type").unwrap_or_default();
        let content_length: i64 = session_row.try_get("content_length").unwrap_or(0);
        let drive_path = format!("drive://node/{drive_node_id}");

        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();

        sqlx::query(
            "INSERT INTO deploy_artifact (
                id, uuid, tenant_id, app_id, upload_session_ref_id, package_type,
                file_name, content_type, content_length, checksum_sha256, drive_node_id,
                drive_space_id, drive_path, status, metadata, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, '{}', CAST($15 AS TIMESTAMPTZ), CAST($15 AS TIMESTAMPTZ), 0
             )",
        )
        .bind(id)
        .bind(&uuid)
        .bind(tenant_id)
        .bind(session_app_internal_id)
        .bind(session_internal_id)
        .bind(package_type)
        .bind(&file_name)
        .bind(&content_type)
        .bind(content_length)
        .bind(checksum_sha256)
        .bind(&drive_node_id)
        .bind(drive_space_id.as_deref())
        .bind(&drive_path)
        .bind(ARTIFACT_STATUS_ACTIVE)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_artifact", error))?;

        self.retrieve_artifact_repo(tenant_id, app_scope, &uuid)
            .await
    }
}

async fn map_artifact_row(
    pool: &PgPool,
    tenant_id: i64,
    row: &PgRow,
) -> Result<ArtifactResponse, sqlx::Error> {
    let app_id = match row.try_get::<Option<i64>, _>("app_id").ok().flatten() {
        Some(app_internal_id) => Some(
            resolve_app_uuid(pool, tenant_id, app_internal_id)
                .await
                .map_err(|error| sqlx::Error::Decode(error.to_string().into()))?,
        ),
        None => None,
    };

    Ok(ArtifactResponse {
        id: row.try_get("uuid")?,
        app_id,
        package_type: row.try_get("package_type")?,
        file_name: row.try_get("file_name")?,
        content_type: row.try_get("content_type")?,
        content_length: row.try_get("content_length")?,
        checksum_sha256: row.try_get("checksum_sha256").ok(),
        drive_node_id: row.try_get("drive_node_id")?,
        upload_session_id: row.try_get("upload_session_uuid")?,
        status: row.try_get("status")?,
        created_at: datetime_from_row(row, "created_at")?,
    })
}

#[cfg(test)]
mod app_scope_tests {
    use super::artifact_app_scope_predicate;

    /// The predicate must name the app column *and* the bound placeholder, so an
    /// application-scoped read cannot degenerate into the tenant-wide one it
    /// replaces. This is the parameter-level half of "application A cannot read
    /// application B's artifacts"; the SQL-executing half lives in
    /// `tests/artifact_app_scope_postgres.rs` and needs a database.
    #[test]
    fn artifact_predicate_binds_the_app_scope_parameter() {
        let predicate = artifact_app_scope_predicate(4);
        assert!(
            predicate.contains("a.app_id = $4"),
            "the scope parameter must constrain the app column: {predicate}"
        );
        assert!(
            predicate.contains("$4::BIGINT IS NULL"),
            "an unscoped caller must be expressible without dropping the predicate: {predicate}"
        );
        assert!(
            !predicate.contains("OR TRUE"),
            "the predicate must never widen to every row: {predicate}"
        );
    }

    /// Each statement gives the scope its own placeholder, so a statement that
    /// already binds page size / offset cannot accidentally alias the scope onto
    /// another argument.
    #[test]
    fn artifact_predicate_placeholders_are_parameter_specific() {
        for parameter in [3usize, 4, 6] {
            let predicate = artifact_app_scope_predicate(parameter);
            assert!(predicate.contains(&format!("a.app_id = ${parameter}")));
            assert!(predicate.contains(&format!("${parameter}::BIGINT IS NULL")));
        }
    }
}
