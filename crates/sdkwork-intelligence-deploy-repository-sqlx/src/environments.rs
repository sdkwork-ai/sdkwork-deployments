//! Application environment repository operations: environment lifecycle
//! (create/update/list/retrieve) and the immutable promotion history that
//! moves releases through the promotion chain (P0 product gap).

use sdkwork_deploy_contract::{
    AppEnvironmentPage, AppEnvironmentResponse, CreateAppEnvironmentRequest, DeployServiceError,
    DeployServiceResult, EnvironmentPromotionPage, EnvironmentPromotionResponse,
    PromoteEnvironmentRequest, UpdateAppEnvironmentRequest,
};
use sqlx::Row;

use crate::support::{new_uuid, next_id, now_rfc3339, pagination, required_datetime, store_error};
use crate::DeployRepository;

impl DeployRepository {
    pub(super) async fn create_app_environment_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        request: &CreateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let environment_id = next_id(self.id_generator())?;
        let environment_uuid = new_uuid();
        let now = now_rfc3339();
        let actor = actor_id.unwrap_or(0);
        sqlx::query(
            "INSERT INTO deploy_app_environment
                (id, uuid, tenant_id, organization_id, app_id, env_key, env_name, env_level,
                 approval_required, env_status, created_by, updated_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'DRAFT', $10, $10,
                     CAST($11 AS TIMESTAMPTZ), CAST($11 AS TIMESTAMPTZ))",
        )
        .bind(environment_id)
        .bind(&environment_uuid)
        .bind(tenant_id)
        .bind(0_i64)
        .bind(app_internal_id)
        .bind(&request.env_key)
        .bind(&request.env_name)
        .bind(&request.env_level)
        .bind(request.approval_required)
        .bind(actor)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_app_environment", error))?;
        self.retrieve_app_environment_internal_repo(tenant_id, app_internal_id, &environment_uuid)
            .await
    }

    pub(super) async fn list_app_environments_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppEnvironmentPage> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_app_environment
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_app_environment", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT e.uuid, e.tenant_id, a.uuid AS app_uuid, e.env_key, e.env_name, e.env_level,
                    e.approval_required, e.current_release_id, r.semantic_version,
                    e.env_status, e.created_at, e.updated_at, e.version
             FROM deploy_app_environment e
             JOIN deploy_app a ON a.id = e.app_id
             LEFT JOIN deploy_release r ON r.id = e.current_release_id
             WHERE e.tenant_id = $1 AND e.app_id = $2 AND e.deleted_at IS NULL
             ORDER BY e.created_at ASC, e.id ASC LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_environment", error))?;

        let items = rows
            .iter()
            .map(map_app_environment_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AppEnvironmentPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_app_environment_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        environment_id: &str,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        self.retrieve_app_environment_internal_repo(tenant_id, app_internal_id, environment_id)
            .await
    }

    async fn retrieve_app_environment_internal_repo(
        &self,
        tenant_id: i64,
        app_internal_id: i64,
        environment_id: &str,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        let row = sqlx::query(
            "SELECT e.uuid, e.tenant_id, a.uuid AS app_uuid, e.env_key, e.env_name, e.env_level,
                    e.approval_required, e.current_release_id, r.semantic_version,
                    e.env_status, e.created_at, e.updated_at, e.version
             FROM deploy_app_environment e
             JOIN deploy_app a ON a.id = e.app_id
             LEFT JOIN deploy_release r ON r.id = e.current_release_id
             WHERE e.tenant_id = $1 AND e.app_id = $2 AND e.uuid = $3 AND e.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(environment_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_app_environment", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("environment not found"));
        };
        map_app_environment_row(&row)
    }

    pub(super) async fn update_app_environment_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        environment_id: &str,
        request: &UpdateAppEnvironmentRequest,
    ) -> DeployServiceResult<AppEnvironmentResponse> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let environment_internal_id =
            resolve_environment_internal_id(&self.pool, tenant_id, app_internal_id, environment_id)
                .await?;
        sqlx::query(
            "UPDATE deploy_app_environment SET
                env_name = COALESCE($1, env_name),
                approval_required = COALESCE($2, approval_required),
                env_status = COALESCE($3, env_status),
                updated_by = $4, updated_at = NOW(), version = version + 1
             WHERE id = $5 AND deleted_at IS NULL",
        )
        .bind(request.env_name.as_deref())
        .bind(request.approval_required)
        .bind(request.env_status.as_deref())
        .bind(actor_id.unwrap_or(0))
        .bind(environment_internal_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("update deploy_app_environment", error))?;
        self.retrieve_app_environment_internal_repo(tenant_id, app_internal_id, environment_id)
            .await
    }

    /// Promotes a release into the environment: sets the current release
    /// pointer and appends the immutable promotion history in one
    /// transaction. When `from_environment_id` is provided, the release must
    /// be the source environment's current release (chain enforcement).
    pub(super) async fn promote_environment_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        environment_id: &str,
        request: &PromoteEnvironmentRequest,
    ) -> DeployServiceResult<EnvironmentPromotionResponse> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let environment_internal_id =
            resolve_environment_internal_id(&self.pool, tenant_id, app_internal_id, environment_id)
                .await?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin environment promotion", error))?;

        // The release must belong to the app.
        let release_internal_id = sqlx::query(
            "SELECT id, uuid FROM deploy_release
             WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3
               AND release_status IS NOT NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(&request.release_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("resolve release for promotion", error))?
        .and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("release not found"))?;

        // Chain enforcement: the source environment must currently point at
        // the release being promoted.
        let mut from_environment_internal_id: Option<i64> = None;
        if let Some(from_environment_id) = request.from_environment_id.as_deref() {
            let from_row = sqlx::query(
                "SELECT id, current_release_id FROM deploy_app_environment
                 WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
            )
            .bind(tenant_id)
            .bind(app_internal_id)
            .bind(from_environment_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("resolve source environment", error))?;
            let Some(from_row) = from_row else {
                return Err(DeployServiceError::not_found(
                    "source environment not found",
                ));
            };
            let from_id: i64 = from_row.try_get("id").unwrap_or(0);
            let from_current: Option<i64> = from_row.try_get("current_release_id").ok();
            if from_current != Some(release_internal_id) {
                return Err(DeployServiceError::conflict(format!(
                    "release {} is not the current release of the source environment",
                    request.release_id
                )));
            }
            from_environment_internal_id = Some(from_id);
        }

        // Reject promoting a release the environment already points at.
        let current_row =
            sqlx::query("SELECT current_release_id FROM deploy_app_environment WHERE id = $1")
                .bind(environment_internal_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|error| store_error("read environment current release", error))?;
        let current_release: Option<i64> = current_row
            .and_then(|row| row.try_get("current_release_id").ok())
            .flatten();
        if current_release == Some(release_internal_id) {
            return Err(DeployServiceError::conflict(
                "release is already current in this environment",
            ));
        }

        sqlx::query(
            "UPDATE deploy_app_environment
                SET current_release_id = $1, updated_by = $2, updated_at = NOW(),
                    version = version + 1
             WHERE id = $3 AND deleted_at IS NULL",
        )
        .bind(release_internal_id)
        .bind(actor_id.unwrap_or(0))
        .bind(environment_internal_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("update environment current release", error))?;

        let promotion_id = next_id(self.id_generator())?;
        let promotion_uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_environment_promotion
                (id, uuid, tenant_id, organization_id, app_id, environment_id, release_id,
                 from_environment_id, promoted_by, note, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())",
        )
        .bind(promotion_id)
        .bind(&promotion_uuid)
        .bind(tenant_id)
        .bind(0_i64)
        .bind(app_internal_id)
        .bind(environment_internal_id)
        .bind(release_internal_id)
        .bind(from_environment_internal_id)
        .bind(actor_id)
        .bind(request.note.as_deref())
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy_environment_promotion", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit environment promotion", error))?;
        self.retrieve_environment_promotion_internal_repo(tenant_id, &promotion_uuid)
            .await
    }

    pub(super) async fn list_environment_promotions_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        environment_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<EnvironmentPromotionPage> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let environment_internal_id =
            resolve_environment_internal_id(&self.pool, tenant_id, app_internal_id, environment_id)
                .await?;
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_environment_promotion
             WHERE tenant_id = $1 AND environment_id = $2",
        )
        .bind(tenant_id)
        .bind(environment_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count environment promotions", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT p.uuid, p.tenant_id, a.uuid AS app_uuid, e.uuid AS environment_uuid,
                    e.env_key, r.uuid AS release_uuid, r.semantic_version,
                    fe.uuid AS from_environment_uuid, fe.env_key AS from_environment_key,
                    p.promoted_by, p.note, p.created_at
             FROM deploy_environment_promotion p
             JOIN deploy_app a ON a.id = p.app_id
             JOIN deploy_app_environment e ON e.id = p.environment_id
             JOIN deploy_release r ON r.id = p.release_id
             LEFT JOIN deploy_app_environment fe ON fe.id = p.from_environment_id
             WHERE p.tenant_id = $1 AND p.environment_id = $2
             ORDER BY p.created_at DESC, p.id DESC LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(environment_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list environment promotions", error))?;

        let items = rows
            .iter()
            .map(map_environment_promotion_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EnvironmentPromotionPage {
            items,
            total,
            page,
            page_size,
        })
    }

    async fn retrieve_environment_promotion_internal_repo(
        &self,
        tenant_id: i64,
        promotion_id: &str,
    ) -> DeployServiceResult<EnvironmentPromotionResponse> {
        let row = sqlx::query(
            "SELECT p.uuid, p.tenant_id, a.uuid AS app_uuid, e.uuid AS environment_uuid,
                    e.env_key, r.uuid AS release_uuid, r.semantic_version,
                    fe.uuid AS from_environment_uuid, fe.env_key AS from_environment_key,
                    p.promoted_by, p.note, p.created_at
             FROM deploy_environment_promotion p
             JOIN deploy_app a ON a.id = p.app_id
             JOIN deploy_app_environment e ON e.id = p.environment_id
             JOIN deploy_release r ON r.id = p.release_id
             LEFT JOIN deploy_app_environment fe ON fe.id = p.from_environment_id
             WHERE p.tenant_id = $1 AND p.uuid = $2",
        )
        .bind(tenant_id)
        .bind(promotion_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve environment promotion", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("promotion not found"));
        };
        map_environment_promotion_row(&row)
    }
}

async fn resolve_environment_internal_id(
    pool: &sqlx::PgPool,
    tenant_id: i64,
    app_internal_id: i64,
    environment_id: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_app_environment
         WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(app_internal_id)
    .bind(environment_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve deploy_app_environment id", error))?;
    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("environment not found"))
}

fn map_app_environment_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AppEnvironmentResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    let version: i64 = row.try_get("version").unwrap_or(1);
    Ok(AppEnvironmentResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        env_key: row.try_get("env_key").unwrap_or_default(),
        env_name: row.try_get("env_name").unwrap_or_default(),
        env_level: row.try_get("env_level").unwrap_or_default(),
        approval_required: row.try_get("approval_required").unwrap_or(false),
        current_release_id: row.try_get("current_release_id").ok(),
        current_release_version: row.try_get("semantic_version").ok(),
        env_status: row.try_get("env_status").unwrap_or_default(),
        created_at,
        updated_at,
        version: version.to_string(),
    })
}

fn map_environment_promotion_row(
    row: &sqlx::postgres::PgRow,
) -> Result<EnvironmentPromotionResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    Ok(EnvironmentPromotionResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        environment_id: row.try_get("environment_uuid").unwrap_or_default(),
        environment_key: row.try_get("env_key").unwrap_or_default(),
        release_id: row.try_get("release_uuid").unwrap_or_default(),
        release_version: row.try_get("semantic_version").unwrap_or_default(),
        from_environment_id: row.try_get("from_environment_uuid").ok(),
        from_environment_key: row.try_get("from_environment_key").ok(),
        promoted_by: row.try_get("promoted_by").ok(),
        note: row.try_get("note").ok(),
        created_at,
    })
}
