//! Channel release repository: provider publish state per environment.
//!
//! A channel release is append-only history with one mutable status column. The
//! transition is guarded by the contract state machine *and* by an optimistic
//! version check, so a concurrent publisher cannot overwrite a newer provider
//! observation with an older one.
use sdkwork_deploy_contract::{
    validate_channel_release_transition, ChannelAccessMode, ChannelKind, ChannelReleaseResponse,
    ChannelReleaseStatus, CreateChannelReleaseRequest, DeployServiceError, DeployServiceResult,
};
use sdkwork_intelligence_deploy_service::ChannelReleaseStore;
use sqlx::{postgres::PgRow, Row};

use crate::support::{
    new_uuid, optional_datetime, required_datetime, resolve_app_internal_id, store_error,
};
use crate::DeployRepository;

const CHANNEL_RELEASE_SELECT: &str = "r.id AS uuid, a.uuid AS app_uuid, r.env_key, r.channel,
    r.access_mode, r.provider_version, r.template_id, r.commit_id, r.audit_id, r.release_status,
    r.gray_percentage, r.rollback_of_release_id, r.error_code, r.submitted_at, r.released_at,
    r.created_at, r.updated_at, r.version";

fn parse_status(value: &str) -> DeployServiceResult<ChannelReleaseStatus> {
    ChannelReleaseStatus::from_wire(value)
        .ok_or_else(|| DeployServiceError::Internal(format!("unknown channel status {value}")))
}

fn parse_channel(value: &str) -> DeployServiceResult<ChannelKind> {
    ChannelKind::from_wire(value)
        .ok_or_else(|| DeployServiceError::Internal(format!("unknown channel {value}")))
}

fn parse_access_mode(value: &str) -> DeployServiceResult<ChannelAccessMode> {
    ChannelAccessMode::from_wire(value)
        .ok_or_else(|| DeployServiceError::Internal(format!("unknown access mode {value}")))
}

fn map_channel_release_row(row: &PgRow) -> DeployServiceResult<ChannelReleaseResponse> {
    Ok(ChannelReleaseResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        environment: row.try_get("env_key").unwrap_or_default(),
        channel: parse_channel(&row.try_get::<String, _>("channel").unwrap_or_default())?,
        access_mode: parse_access_mode(
            &row.try_get::<String, _>("access_mode").unwrap_or_default(),
        )?,
        provider_version: row.try_get("provider_version").ok().flatten(),
        template_id: row.try_get("template_id").ok().flatten(),
        commit_id: row.try_get("commit_id").ok().flatten(),
        audit_id: row.try_get("audit_id").ok().flatten(),
        release_status: parse_status(
            &row.try_get::<String, _>("release_status")
                .unwrap_or_default(),
        )?,
        gray_percentage: u16::try_from(row.try_get::<i16, _>("gray_percentage").unwrap_or(100))
            .unwrap_or(100)
            .min(100),
        rollback_of_release_id: row.try_get("rollback_of_release_id").ok().flatten(),
        error_code: row.try_get("error_code").ok().flatten(),
        submitted_at: optional_datetime(row, "submitted_at")?,
        released_at: optional_datetime(row, "released_at")?,
        created_at: required_datetime(row, "created_at")?,
        updated_at: required_datetime(row, "updated_at")?,
        version: row.try_get::<i64, _>("version").unwrap_or(1).to_string(),
    })
}

impl DeployRepository {
    /// Creates a `DRAFT` channel release for one application environment.
    pub async fn create_channel_release_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_uuid: &str,
        request: &CreateChannelReleaseRequest,
    ) -> DeployServiceResult<ChannelReleaseResponse> {
        let app_id = resolve_app_internal_id(&self.pool, tenant_id, app_uuid).await?;
        let link_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM deploy_application_resource_link \
             WHERE tenant_id = $1 AND app_id = $2 AND id = $3 AND env_key = $4 \
               AND deleted_at IS NULL)",
        )
        .bind(tenant_id)
        .bind(app_id)
        .bind(&request.resource_link_id)
        .bind(&request.environment)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("check channel resource link", error))?;
        if !link_exists {
            return Err(DeployServiceError::not_found(
                "application resource link not found for this environment",
            ));
        }
        let uuid = new_uuid();
        let payload = request
            .ext_json
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        sqlx::query(
            "INSERT INTO deploy_channel_release \
                (id, tenant_id, organization_id, app_id, env_key, resource_link_id, channel, \
                 access_mode, provider_version, template_id, release_status, gray_percentage, \
                 provider_payload_json, created_by, updated_by, created_at, updated_at) \
             VALUES ($1, $2, 0, $3, $4, $5, $6, $7, $8, $9, 'DRAFT', 100, $10, $11, $11, \
                     NOW(), NOW())",
        )
        .bind(&uuid)
        .bind(tenant_id)
        .bind(app_id)
        .bind(request.environment.trim())
        .bind(&request.resource_link_id)
        .bind(request.channel.as_str())
        .bind(request.access_mode.as_str())
        .bind(&request.provider_version)
        .bind(&request.template_id)
        .bind(payload)
        .bind(actor_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("create channel release", error))?;
        self.find_channel_release_repo(tenant_id, &uuid).await
    }

    /// Loads one channel release by id.
    pub async fn find_channel_release_repo(
        &self,
        tenant_id: i64,
        release_uuid: &str,
    ) -> DeployServiceResult<ChannelReleaseResponse> {
        let query = format!(
            "SELECT {CHANNEL_RELEASE_SELECT} FROM deploy_channel_release r \
             JOIN deploy_app a ON a.id = r.app_id \
             WHERE r.tenant_id = $1 AND r.id = $2"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
            .bind(tenant_id)
            .bind(release_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("load channel release", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("channel release not found"));
        };
        map_channel_release_row(&row)
    }

    /// Lists channel releases for one application, newest first.
    pub async fn list_channel_releases_repo(
        &self,
        tenant_id: i64,
        app_uuid: &str,
        env_key: Option<&str>,
    ) -> DeployServiceResult<Vec<ChannelReleaseResponse>> {
        let app_id = resolve_app_internal_id(&self.pool, tenant_id, app_uuid).await?;
        let query = format!(
            "SELECT {CHANNEL_RELEASE_SELECT} FROM deploy_channel_release r \
             JOIN deploy_app a ON a.id = r.app_id \
             WHERE r.tenant_id = $1 AND r.app_id = $2 AND ($3::text IS NULL OR r.env_key = $3) \
             ORDER BY r.created_at DESC, r.id DESC"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
            .bind(tenant_id)
            .bind(app_id)
            .bind(env_key)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list channel releases", error))?;
        rows.iter().map(map_channel_release_row).collect()
    }

    /// Moves one channel release to the next provider state.
    ///
    /// The contract state machine decides whether the move is possible; the
    /// optimistic `version` predicate decides whether the caller is looking at
    /// the row it thinks it is.
    #[allow(clippy::too_many_arguments)]
    pub async fn transition_channel_release_repo(
        &self,
        tenant_id: i64,
        release_uuid: &str,
        expected_version: i64,
        next_status: ChannelReleaseStatus,
        commit_id: Option<&str>,
        audit_id: Option<&str>,
        error_code: Option<&str>,
    ) -> DeployServiceResult<ChannelReleaseResponse> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin channel release transition", error))?;
        let row = sqlx::query(
            "SELECT release_status, version FROM deploy_channel_release \
             WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(release_uuid)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock channel release", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("channel release not found"));
        };
        let current_status = parse_status(
            &row.try_get::<String, _>("release_status")
                .unwrap_or_default(),
        )?;
        let current_version: i64 = row.try_get("version").unwrap_or(0);
        if current_version != expected_version {
            return Err(DeployServiceError::conflict(
                "channel release version changed; reload before retrying",
            ));
        }
        validate_channel_release_transition(current_status, next_status)
            .map_err(DeployServiceError::conflict)?;
        let next = next_status.as_str();
        sqlx::query(
            "UPDATE deploy_channel_release SET \
                release_status = $3, \
                commit_id = COALESCE($4, commit_id), \
                audit_id = COALESCE($5, audit_id), \
                error_code = COALESCE($6, error_code), \
                submitted_at = CASE WHEN $3 = 'AUDIT_SUBMITTED' THEN NOW() ELSE submitted_at END, \
                released_at = CASE WHEN $3 = 'RELEASED' THEN NOW() ELSE released_at END, \
                updated_at = NOW(), \
                version = version + 1 \
             WHERE tenant_id = $1 AND id = $2 AND version = $7",
        )
        .bind(tenant_id)
        .bind(release_uuid)
        .bind(next)
        .bind(commit_id)
        .bind(audit_id)
        .bind(error_code)
        .bind(expected_version)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("update channel release", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit channel release transition", error))?;
        self.find_channel_release_repo(tenant_id, release_uuid)
            .await
    }
}

#[async_trait::async_trait]
impl ChannelReleaseStore for DeployRepository {
    async fn load_channel_release_status(
        &self,
        tenant_id: i64,
        release_id: &str,
    ) -> DeployServiceResult<(ChannelReleaseStatus, i64)> {
        let release = self
            .find_channel_release_repo(tenant_id, release_id)
            .await?;
        let version = release.version.parse::<i64>().map_err(|_| {
            DeployServiceError::Internal("channel release version is not numeric".to_string())
        })?;
        Ok((release.release_status, version))
    }

    async fn transition_channel_release(
        &self,
        tenant_id: i64,
        release_id: &str,
        expected_version: i64,
        next: ChannelReleaseStatus,
        commit_id: Option<&str>,
        audit_id: Option<&str>,
        detail: Option<&str>,
    ) -> DeployServiceResult<()> {
        self.transition_channel_release_repo(
            tenant_id,
            release_id,
            expected_version,
            next,
            commit_id,
            audit_id,
            detail,
        )
        .await?;
        Ok(())
    }

    async fn set_channel_release_gray_percentage(
        &self,
        tenant_id: i64,
        release_id: &str,
        gray_percentage: u16,
    ) -> DeployServiceResult<()> {
        let percentage = i16::try_from(gray_percentage).unwrap_or(100).clamp(0, 100);
        let result = sqlx::query(
            "UPDATE deploy_channel_release SET \
                gray_percentage = $3, \
                updated_at = NOW(), \
                version = version + 1 \
             WHERE tenant_id = $1 AND id = $2 AND release_status = 'RELEASED'",
        )
        .bind(tenant_id)
        .bind(release_id)
        .bind(percentage)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("set channel release gray percentage", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::conflict(
                "channel release is not released; a gray rollout requires RELEASED",
            ));
        }
        Ok(())
    }
}
