use sqlx::Row;

use sdkwork_deploy_contract::{
    CreateDeployUploadSessionRequest, DeployServiceError, DeployServiceResult,
    DeployUploadSessionResponse,
};

use crate::support::{
    datetime_from_row, new_uuid, next_id, now_rfc3339, resolve_app_internal_id,
    resolve_scope_app_internal_id, sha256_hex, store_error,
};
use crate::DeployRepository;

/// The `(tenant_id, app_id)` half of an upload-session statement
/// (`APPLICATION_IDENTITY_SPEC.md` §4.3).
///
/// `parameter` carries the internal id of the deploy application the caller's
/// credential proves, bound as `NULL` when the credential names no application.
/// A bound scope restricts the statement to sessions attributed to that
/// application, so one application can neither read, complete, cancel, nor
/// replay-mint another's upload — which would otherwise let it consume the
/// session's Drive node and mint an artifact in the victim application's name.
fn upload_session_app_scope_predicate(parameter: usize) -> String {
    format!("(${parameter}::BIGINT IS NULL OR app_id = ${parameter})")
}

impl DeployRepository {
    pub(crate) async fn create_upload_session_ref_repo(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        request: &CreateDeployUploadSessionRequest,
        drive: &DeployUploadSessionResponse,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        // The credential-proved application wins over the body's `appId`: the
        // service layer has already refused a body naming a different
        // application, and preferring the scope keeps a caller that omits the
        // field from registering a session it could then not read back.
        let bound_app_uuid = app_scope.or(request.app_id.as_deref());
        let app_internal_id = match bound_app_uuid {
            Some(app_uuid) => Some(resolve_app_internal_id(&self.pool, tenant_id, app_uuid).await?),
            None => None,
        };
        let id = next_id(&self.id_generator)?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO deploy_upload_session_ref
             (id, uuid, tenant_id, app_id, drive_upload_session_id, drive_upload_item_id,
              drive_space_id, drive_node_id, package_type, file_name, content_type,
              content_length, checksum, status, idempotency_key, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, CAST($16 AS TIMESTAMPTZ), CAST($17 AS TIMESTAMPTZ))",
        )
        .bind(id)
        .bind(&uuid)
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(&drive.drive_upload_session_id)
        .bind(drive.drive_upload_item_id.as_deref())
        .bind(drive.drive_space_id.as_deref())
        .bind(drive.drive_node_id.as_deref())
        .bind(request.package_type)
        .bind(&request.file_name)
        .bind(&request.content_type)
        .bind(request.content_length)
        .bind(request.checksum.as_deref())
        .bind(drive.status)
        .bind(sha256_hex(request.idempotency_key.trim()))
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("create deploy upload session ref", error))?;

        Ok(DeployUploadSessionResponse {
            id: uuid,
            app_id: bound_app_uuid.map(str::to_owned),
            package_type: request.package_type,
            file_name: request.file_name.clone(),
            content_type: request.content_type.clone(),
            content_length: request.content_length,
            checksum: request.checksum.clone(),
            status: drive.status,
            drive_upload_session_id: drive.drive_upload_session_id.clone(),
            drive_upload_item_id: drive.drive_upload_item_id.clone(),
            drive_space_id: drive.drive_space_id.clone(),
            drive_node_id: drive.drive_node_id.clone(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub(crate) async fn retrieve_upload_session_ref_repo(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        upload_session_id: &str,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let app_internal_id = resolve_scope_app_internal_id(&self.pool, tenant_id, app_scope).await?;
        let query = format!(
            "SELECT uuid, app_id, package_type, file_name, content_type, content_length,
                    checksum, status, drive_upload_session_id, drive_upload_item_id,
                    drive_space_id, drive_node_id, created_at, updated_at
             FROM deploy_upload_session_ref
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
               AND {}",
            upload_session_app_scope_predicate(3)
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(upload_session_id)
            .bind(app_internal_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve deploy upload session ref", error))?
            .ok_or_else(|| DeployServiceError::not_found("upload session not found"))?;

        let app_uuid = match row.try_get::<Option<i64>, _>("app_id").ok().flatten() {
            Some(app_id) => {
                Some(crate::support::resolve_app_uuid(&self.pool, tenant_id, app_id).await?)
            }
            None => None,
        };

        map_upload_session_row(&row, app_uuid).map_err(|error| {
            DeployServiceError::Internal(format!("map deploy upload session ref: {error}"))
        })
    }

    pub(crate) async fn find_upload_session_by_idempotency_key_repo(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        idempotency_key: &str,
    ) -> DeployServiceResult<Option<DeployUploadSessionResponse>> {
        let app_internal_id = resolve_scope_app_internal_id(&self.pool, tenant_id, app_scope).await?;
        let query = format!(
            "SELECT uuid, app_id, package_type, file_name, content_type, content_length,
                    checksum, status, drive_upload_session_id, drive_upload_item_id,
                    drive_space_id, drive_node_id, created_at, updated_at
             FROM deploy_upload_session_ref
             WHERE tenant_id = $1 AND idempotency_key = $2 AND deleted_at IS NULL
               AND {}",
            upload_session_app_scope_predicate(3)
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(sha256_hex(idempotency_key.trim()))
            .bind(app_internal_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("find deploy upload session ref", error))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let app_uuid = match row.try_get::<Option<i64>, _>("app_id").ok().flatten() {
            Some(app_id) => {
                Some(crate::support::resolve_app_uuid(&self.pool, tenant_id, app_id).await?)
            }
            None => None,
        };

        map_upload_session_row(&row, app_uuid)
            .map(Some)
            .map_err(|error| {
                DeployServiceError::Internal(format!("map deploy upload session ref: {error}"))
            })
    }

    pub(crate) async fn update_upload_session_status_repo(
        &self,
        tenant_id: i64,
        app_scope: Option<&str>,
        upload_session_id: &str,
        status: i32,
        drive_node_id: Option<&str>,
    ) -> DeployServiceResult<DeployUploadSessionResponse> {
        let app_internal_id = resolve_scope_app_internal_id(&self.pool, tenant_id, app_scope).await?;
        let now = now_rfc3339();
        let query = format!(
            "UPDATE deploy_upload_session_ref
             SET status = $1,
                 drive_node_id = COALESCE($2, drive_node_id),
                 updated_at = CAST($3 AS TIMESTAMPTZ)
             WHERE tenant_id = $4 AND uuid = $5 AND deleted_at IS NULL
               AND {}",
            upload_session_app_scope_predicate(6)
        );
        sqlx::query(sqlx::AssertSqlSafe(&*query))
            .bind(status)
            .bind(drive_node_id)
            .bind(&now)
            .bind(tenant_id)
            .bind(upload_session_id)
            .bind(app_internal_id)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update deploy upload session ref", error))?;
        self.retrieve_upload_session_ref_repo(tenant_id, app_scope, upload_session_id)
            .await
    }
}

fn map_upload_session_row(
    row: &sqlx::postgres::PgRow,
    app_id: Option<String>,
) -> Result<DeployUploadSessionResponse, sqlx::Error> {
    Ok(DeployUploadSessionResponse {
        id: row.try_get("uuid")?,
        app_id,
        package_type: row.try_get("package_type")?,
        file_name: row.try_get("file_name")?,
        content_type: row.try_get("content_type")?,
        content_length: row.try_get("content_length")?,
        checksum: row.try_get("checksum").ok(),
        status: row.try_get("status")?,
        drive_upload_session_id: row.try_get("drive_upload_session_id")?,
        drive_upload_item_id: row.try_get("drive_upload_item_id").ok(),
        drive_space_id: row.try_get("drive_space_id").ok(),
        drive_node_id: row.try_get("drive_node_id").ok(),
        created_at: datetime_from_row(row, "created_at")?,
        updated_at: datetime_from_row(row, "updated_at")?,
    })
}

#[cfg(test)]
mod app_scope_tests {
    use super::upload_session_app_scope_predicate;

    /// The session statements must carry the app predicate with their own
    /// placeholder. Without it, one application's token can read, complete, or
    /// cancel another application's upload and mint an artifact under the
    /// victim's `app_id`.
    #[test]
    fn upload_session_predicate_binds_the_app_scope_parameter() {
        for parameter in [3usize, 6] {
            let predicate = upload_session_app_scope_predicate(parameter);
            assert!(
                predicate.contains(&format!("app_id = ${parameter}")),
                "scope parameter {parameter} must constrain the app column: {predicate}"
            );
            assert!(predicate.contains(&format!("${parameter}::BIGINT IS NULL")));
            assert!(!predicate.contains("OR TRUE"));
        }
    }
}
