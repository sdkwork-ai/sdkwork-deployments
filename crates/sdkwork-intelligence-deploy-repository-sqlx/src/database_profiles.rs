//! Application database structure contract repository operations: profiles
//! (engine/catalog/schema contract) and their versioned migration definitions.

use sdkwork_deploy_contract::{
    AppDatabaseMigrationPage, AppDatabaseMigrationResponse, AppDatabaseProfilePage,
    AppDatabaseProfileResponse, CreateAppDatabaseMigrationRequest, CreateAppDatabaseProfileRequest,
    DeployServiceError, DeployServiceResult, UpdateAppDatabaseProfileRequest,
};
use sqlx::Row;

use crate::support::{
    new_uuid, next_id, now_rfc3339, optional_datetime, pagination, required_datetime, store_error,
};
use crate::DeployRepository;

impl DeployRepository {
    pub(super) async fn create_app_database_profile_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        request: &CreateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let profile_id = next_id(self.id_generator())?;
        let profile_uuid = new_uuid();
        let now = now_rfc3339();
        let actor = actor_id.unwrap_or(0);
        sqlx::query(
            "INSERT INTO deploy_app_database_profile
                (id, uuid, tenant_id, organization_id, app_id, profile_key, db_engine,
                 catalog_name, schema_version, baseline_version, migration_strategy,
                 profile_status, created_by, updated_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'DRAFT', $12, $12,
                     CAST($13 AS TIMESTAMPTZ), CAST($13 AS TIMESTAMPTZ))",
        )
        .bind(profile_id)
        .bind(&profile_uuid)
        .bind(tenant_id)
        .bind(0_i64)
        .bind(app_internal_id)
        .bind(&request.profile_key)
        .bind(&request.db_engine)
        .bind(&request.catalog_name)
        .bind(request.schema_version.as_deref())
        .bind(request.baseline_version.as_deref())
        .bind(request.migration_strategy.as_deref().unwrap_or("VERSIONED"))
        .bind(actor)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_app_database_profile", error))?;
        self.retrieve_app_database_profile_internal_repo(tenant_id, app_internal_id, &profile_uuid)
            .await
    }

    pub(super) async fn list_app_database_profiles_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDatabaseProfilePage> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_app_database_profile
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_app_database_profile", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT p.uuid, p.tenant_id, a.uuid AS app_uuid, p.profile_key, p.db_engine,
                    p.catalog_name, p.schema_version, p.baseline_version, p.migration_strategy,
                    p.profile_status, p.created_at, p.updated_at, p.version,
                    (SELECT COUNT(*) FROM deploy_app_database_migration m
                     WHERE m.profile_id = p.id AND m.deleted_at IS NULL) AS migration_count
             FROM deploy_app_database_profile p
             JOIN deploy_app a ON a.id = p.app_id
             WHERE p.tenant_id = $1 AND p.app_id = $2 AND p.deleted_at IS NULL
             ORDER BY p.updated_at DESC, p.id DESC LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_database_profile", error))?;

        let items = rows
            .iter()
            .map(map_database_profile_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AppDatabaseProfilePage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_app_database_profile_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        profile_id: &str,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        self.retrieve_app_database_profile_internal_repo(tenant_id, app_internal_id, profile_id)
            .await
    }

    async fn retrieve_app_database_profile_internal_repo(
        &self,
        tenant_id: i64,
        app_internal_id: i64,
        profile_id: &str,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        let row = sqlx::query(
            "SELECT p.uuid, p.tenant_id, a.uuid AS app_uuid, p.profile_key, p.db_engine,
                    p.catalog_name, p.schema_version, p.baseline_version, p.migration_strategy,
                    p.profile_status, p.created_at, p.updated_at, p.version,
                    (SELECT COUNT(*) FROM deploy_app_database_migration m
                     WHERE m.profile_id = p.id AND m.deleted_at IS NULL) AS migration_count
             FROM deploy_app_database_profile p
             JOIN deploy_app a ON a.id = p.app_id
             WHERE p.tenant_id = $1 AND p.app_id = $2 AND p.uuid = $3 AND p.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_app_database_profile", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("database profile not found"));
        };
        map_database_profile_row(&row)
    }

    pub(super) async fn update_app_database_profile_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        profile_id: &str,
        request: &UpdateAppDatabaseProfileRequest,
    ) -> DeployServiceResult<AppDatabaseProfileResponse> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let row = sqlx::query(
            "SELECT id, uuid FROM deploy_app_database_profile
             WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_app_database_profile", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("database profile not found"));
        };
        let internal_id: i64 = row.try_get("id").unwrap_or(0);
        let profile_uuid: String = row.try_get("uuid").unwrap_or_default();
        sqlx::query(
            "UPDATE deploy_app_database_profile SET
                schema_version = COALESCE($1, schema_version),
                baseline_version = COALESCE($2, baseline_version),
                migration_strategy = COALESCE($3, migration_strategy),
                profile_status = COALESCE($4, profile_status),
                updated_by = $5, updated_at = NOW(), version = version + 1
             WHERE id = $6 AND deleted_at IS NULL",
        )
        .bind(request.schema_version.as_deref())
        .bind(request.baseline_version.as_deref())
        .bind(request.migration_strategy.as_deref())
        .bind(request.profile_status.as_deref())
        .bind(actor_id.unwrap_or(0))
        .bind(internal_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("update deploy_app_database_profile", error))?;
        self.retrieve_app_database_profile_internal_repo(tenant_id, app_internal_id, &profile_uuid)
            .await
    }

    pub(super) async fn create_app_database_migration_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        profile_id: &str,
        request: &CreateAppDatabaseMigrationRequest,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let profile_row = sqlx::query(
            "SELECT id FROM deploy_app_database_profile
             WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve database profile for migration", error))?;
        let Some(profile_row) = profile_row else {
            return Err(DeployServiceError::not_found("database profile not found"));
        };
        let profile_internal_id: i64 = profile_row.try_get("id").unwrap_or(0);
        let migration_id = next_id(self.id_generator())?;
        let migration_uuid = new_uuid();
        let now = now_rfc3339();
        let actor = actor_id.unwrap_or(0);
        sqlx::query(
            "INSERT INTO deploy_app_database_migration
                (id, uuid, tenant_id, organization_id, profile_id, migration_version,
                 migration_name, checksum_sha256, script_ref, migration_status,
                 created_by, updated_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'PENDING', $10, $10,
                     CAST($11 AS TIMESTAMPTZ), CAST($11 AS TIMESTAMPTZ))",
        )
        .bind(migration_id)
        .bind(&migration_uuid)
        .bind(tenant_id)
        .bind(0_i64)
        .bind(profile_internal_id)
        .bind(&request.migration_version)
        .bind(&request.migration_name)
        .bind(&request.checksum_sha256)
        .bind(request.script_ref.as_deref())
        .bind(actor)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_app_database_migration", error))?;
        self.retrieve_app_database_migration_internal_repo(
            tenant_id,
            app_internal_id,
            &migration_uuid,
        )
        .await
    }

    pub(super) async fn list_app_database_migrations_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        profile_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppDatabaseMigrationPage> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let profile_internal_id = resolve_database_profile_internal_id(
            &self.pool,
            tenant_id,
            app_internal_id,
            profile_id,
        )
        .await?;
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_app_database_migration
             WHERE tenant_id = $1 AND profile_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(profile_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_app_database_migration", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT m.uuid, m.tenant_id, p.uuid AS profile_uuid, m.migration_version,
                    m.migration_name, m.checksum_sha256, m.script_ref, m.migration_status,
                    m.applied_at, m.created_at, m.updated_at, m.version
             FROM deploy_app_database_migration m
             JOIN deploy_app_database_profile p ON p.id = m.profile_id
             WHERE m.tenant_id = $1 AND m.profile_id = $2 AND m.deleted_at IS NULL
             ORDER BY m.migration_version ASC, m.id ASC LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(profile_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_database_migration", error))?;

        let items = rows
            .iter()
            .map(map_database_migration_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AppDatabaseMigrationPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_app_database_migration_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        profile_id: &str,
        migration_id: &str,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        let app_internal_id =
            crate::support::resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let _ = resolve_database_profile_internal_id(
            &self.pool,
            tenant_id,
            app_internal_id,
            profile_id,
        )
        .await?;
        self.retrieve_app_database_migration_internal_repo(tenant_id, app_internal_id, migration_id)
            .await
    }

    async fn retrieve_app_database_migration_internal_repo(
        &self,
        tenant_id: i64,
        app_internal_id: i64,
        migration_id: &str,
    ) -> DeployServiceResult<AppDatabaseMigrationResponse> {
        let row = sqlx::query(
            "SELECT m.uuid, m.tenant_id, p.uuid AS profile_uuid, m.migration_version,
                    m.migration_name, m.checksum_sha256, m.script_ref, m.migration_status,
                    m.applied_at, m.created_at, m.updated_at, m.version
             FROM deploy_app_database_migration m
             JOIN deploy_app_database_profile p ON p.id = m.profile_id
             JOIN deploy_app a ON a.id = p.app_id
             WHERE m.tenant_id = $1 AND a.id = $2 AND m.uuid = $3 AND m.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(migration_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_app_database_migration", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found(
                "database migration not found",
            ));
        };
        map_database_migration_row(&row)
    }
}

async fn resolve_database_profile_internal_id(
    pool: &sqlx::PgPool,
    tenant_id: i64,
    app_internal_id: i64,
    profile_id: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_app_database_profile
         WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(app_internal_id)
    .bind(profile_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve deploy_app_database_profile id", error))?;

    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("database profile not found"))
}

fn map_database_profile_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AppDatabaseProfileResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    let version: i64 = row.try_get("version").unwrap_or(1);
    Ok(AppDatabaseProfileResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        profile_key: row.try_get("profile_key").unwrap_or_default(),
        db_engine: row.try_get("db_engine").unwrap_or_default(),
        catalog_name: row.try_get("catalog_name").unwrap_or_default(),
        schema_version: row.try_get("schema_version").ok(),
        baseline_version: row.try_get("baseline_version").ok(),
        migration_strategy: row.try_get("migration_strategy").unwrap_or_default(),
        profile_status: row.try_get("profile_status").unwrap_or_default(),
        migration_count: row.try_get("migration_count").unwrap_or(0),
        created_at,
        updated_at,
        version: version.to_string(),
    })
}

fn map_database_migration_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AppDatabaseMigrationResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    let applied_at = optional_datetime(row, "applied_at")?;
    let version: i64 = row.try_get("version").unwrap_or(1);
    Ok(AppDatabaseMigrationResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        profile_id: row.try_get("profile_uuid").unwrap_or_default(),
        migration_version: row.try_get("migration_version").unwrap_or_default(),
        migration_name: row.try_get("migration_name").unwrap_or_default(),
        checksum_sha256: row.try_get("checksum_sha256").unwrap_or_default(),
        script_ref: row.try_get("script_ref").ok(),
        migration_status: row.try_get("migration_status").unwrap_or_default(),
        applied_at,
        created_at,
        updated_at,
        version: version.to_string(),
    })
}
