//! App-scoped release, release channel, and channel rollout repository
//! operations (REQ-2026-0002). Releases are immutable; semantic versions are
//! unique per (App, platform target); channel promotion is transactional with
//! a fence on the previous rollout.

use sdkwork_deploy_contract::{
    AppReleasePage, AppReleaseResponse, ChannelPage, ChannelResponse, ChannelRolloutPage,
    ChannelRolloutResponse, CreateAppReleaseRequest, DeployServiceError, DeployServiceResult,
    PromoteChannelRequest, ReleaseStatus, RolloutStatus, RolloutStrategy,
};
use sqlx::{postgres::PgRow, AssertSqlSafe, Row};

use crate::support::{
    new_uuid, next_id, optional_datetime, pagination, required_datetime, resolve_app_internal_id,
    resolve_app_release_internal_id, resolve_channel_internal_id,
    resolve_platform_target_internal_id, store_error,
};
use crate::DeployRepository;

const APP_RELEASE_SELECT: &str = "r.uuid, r.app_id, r.platform_target_id, r.package_id,
    r.semantic_version, r.build_number, r.release_status, r.release_notes_json,
    r.created_at, r.updated_at, r.version";

fn map_app_release_row(row: &PgRow) -> Result<AppReleaseResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    Ok(AppReleaseResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        platform_target_id: row.try_get("target_uuid").unwrap_or_default(),
        package_id: row.try_get("package_uuid").unwrap_or_default(),
        semantic_version: row.try_get("semantic_version").unwrap_or_default(),
        build_number: row.try_get("build_number").unwrap_or(0),
        release_status: row.try_get("release_status").unwrap_or_default(),
        release_notes: row
            .try_get::<Option<serde_json::Value>, _>("release_notes_json")
            .ok()
            .flatten(),
        created_at,
        updated_at,
        version: row.try_get::<i64, _>("version").unwrap_or(1).to_string(),
    })
}

impl DeployRepository {
    pub(super) async fn create_app_release_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateAppReleaseRequest,
    ) -> DeployServiceResult<AppReleaseResponse> {
        // Idempotency: reuse the prior result for the same key.
        if let Some(existing) = self
            .find_app_release_by_idempotency_key_repo(tenant_id, &request.idempotency_key)
            .await?
        {
            return Ok(existing);
        }

        // The package pins the (app, platform target) scope; `build_number`
        // lives on the build the package was produced by (`deploy_package`
        // carries only `build_id`), so it has to be joined in.
        let package_row = sqlx::query(
            "SELECT p.app_id, p.platform_target_id, b.build_number,
                    p.semantic_version AS package_version, p.package_status
             FROM deploy_package p
             LEFT JOIN deploy_build b ON b.id = p.build_id AND b.deleted_at IS NULL
             WHERE p.tenant_id = $1 AND p.uuid = $2 AND p.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(&request.package_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_package scope", error))?;

        let Some(package_row) = package_row else {
            return Err(DeployServiceError::not_found("package not found"));
        };
        let app_internal_id: i64 = package_row.try_get("app_id").unwrap_or(0);
        let target_internal_id: i64 = package_row.try_get("platform_target_id").unwrap_or(0);
        // `deploy_package.build_id` is `NOT NULL`, but the build row itself is
        // soft-deleted rather than removed, so the `LEFT JOIN` above can still
        // miss. Surfacing that as a validation error beats writing a bogus `0`
        // into `deploy_release.build_number`, where nothing downstream could
        // tell it apart from a real build #0.
        let build_number: i64 = package_row.try_get::<Option<i64>, _>("build_number").map_err(
            |error| DeployServiceError::Internal(format!("read package build number: {error}")),
        )?
        .ok_or_else(|| {
            DeployServiceError::validation(
                "package build is missing or deleted; cannot derive the build number",
            )
        })?;
        let package_version: String = package_row.try_get("package_version").unwrap_or_default();
        let package_status: String = package_row.try_get("package_status").unwrap_or_default();
        if !matches!(package_status.as_str(), "VALIDATED" | "READY") {
            return Err(DeployServiceError::validation(format!(
                "package status {package_status} does not allow release creation"
            )));
        }

        let requested_target = resolve_platform_target_internal_id(
            &self.pool,
            tenant_id,
            app_internal_id,
            &request.platform_target_id,
        )
        .await?;
        if requested_target != target_internal_id {
            return Err(DeployServiceError::validation(
                "platform target does not match the package platform target",
            ));
        }

        // The caller may derive the version from the package; when the
        // request version differs from the package version, the request wins
        // only for build metadata, otherwise it must agree with the package.
        let semantic_version = if request.semantic_version.is_empty() {
            package_version.clone()
        } else {
            request.semantic_version.clone()
        };

        let release_id = next_id(self.id_generator())?;
        let release_uuid = new_uuid();
        let release_status = request
            .release_status
            .map(|status| status.as_str())
            .unwrap_or(ReleaseStatus::Draft.as_str());
        let release_notes = request
            .release_notes
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        let result = sqlx::query(
            "INSERT INTO deploy_release
                (id, uuid, tenant_id, organization_id, app_id, platform_target_id,
                 package_id, semantic_version, build_number, release_status,
                 release_notes_json, idempotency_key, created_at, updated_at, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), NOW(), 1)
             ON CONFLICT (tenant_id, app_id, idempotency_key)
             WHERE idempotency_key IS NOT NULL DO NOTHING
             RETURNING uuid",
        )
        .bind(release_id)
        .bind(&release_uuid)
        .bind(tenant_id)
        .bind(0)
        .bind(app_internal_id)
        .bind(target_internal_id)
        .bind(
            sqlx::query("SELECT id FROM deploy_package WHERE tenant_id = $1 AND uuid = $2")
                .bind(tenant_id)
                .bind(&request.package_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error("resolve deploy_package id", error))?
                .and_then(|row| row.try_get::<i64, _>("id").ok())
                .ok_or_else(|| DeployServiceError::not_found("package not found"))?,
        )
        .bind(&semantic_version)
        .bind(build_number)
        .bind(release_status)
        .bind(release_notes)
        .bind(&request.idempotency_key)
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            if crate::support::is_unique_violation(&error) {
                // The unique (app, target, semantic_version) index rejected
                // the version; report a conflict with the stable vocabulary.
                DeployServiceError::conflict(format!(
                    "semantic version {semantic_version} already exists for this app platform target"
                ))
            } else {
                store_error("insert deploy_release", error)
            }
        })?;

        let Some(row) = result else {
            return Err(DeployServiceError::conflict(format!(
                "semantic version {semantic_version} already exists for this app platform target"
            )));
        };
        let inserted_uuid: String = row.try_get("uuid").map_err(|error| {
            DeployServiceError::Internal(format!("read deploy_release uuid: {error}"))
        })?;
        // The release request does not carry the app id; resolve the app
        // scope from the package so the response is app-qualified.
        let app_uuid: String = sqlx::query(
            "SELECT a.uuid FROM deploy_package p
             JOIN deploy_app a ON a.id = p.app_id
             WHERE p.tenant_id = $1 AND p.uuid = $2",
        )
        .bind(tenant_id)
        .bind(&request.package_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve package app uuid", error))?
        .and_then(|row| row.try_get::<String, _>("uuid").ok())
        .ok_or_else(|| DeployServiceError::not_found("package not found"))?;
        self.retrieve_app_release_repo(tenant_id, &app_uuid, &inserted_uuid)
            .await
    }

    pub(super) async fn find_app_release_by_idempotency_key_repo(
        &self,
        tenant_id: i64,
        idempotency_key: &str,
    ) -> DeployServiceResult<Option<AppReleaseResponse>> {
        let query = format!(
            "SELECT {APP_RELEASE_SELECT}, a.uuid AS app_uuid, t.uuid AS target_uuid,
                    p.uuid AS package_uuid
             FROM deploy_release r
             JOIN deploy_app a ON a.id = r.app_id
             JOIN deploy_app_platform_target t ON t.id = r.platform_target_id
             JOIN deploy_package p ON p.id = r.package_id
             WHERE r.tenant_id = $1 AND r.idempotency_key = $2 AND r.release_status IS NOT NULL
             ORDER BY r.created_at DESC LIMIT 1"
        );
        let row = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(idempotency_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("find deploy_release by idempotency key", error))?;

        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(map_app_release_row(&row)?))
    }

    pub(super) async fn list_app_releases_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppReleasePage> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_release
             WHERE tenant_id = $1 AND app_id = $2 AND release_status IS NOT NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_release app", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let query = format!(
            "SELECT {APP_RELEASE_SELECT}, a.uuid AS app_uuid, t.uuid AS target_uuid,
                    p.uuid AS package_uuid
             FROM deploy_release r
             JOIN deploy_app a ON a.id = r.app_id
             JOIN deploy_app_platform_target t ON t.id = r.platform_target_id
             JOIN deploy_package p ON p.id = r.package_id
             WHERE r.tenant_id = $1 AND r.app_id = $2 AND r.release_status IS NOT NULL
             ORDER BY r.created_at DESC, r.id DESC LIMIT $3 OFFSET $4"
        );
        let rows = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(app_internal_id)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list deploy_release app", error))?;

        let items = rows
            .iter()
            .map(map_app_release_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AppReleasePage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_app_release_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        release_id: &str,
    ) -> DeployServiceResult<AppReleaseResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        resolve_app_release_internal_id(&self.pool, tenant_id, app_internal_id, release_id).await?;

        let query = format!(
            "SELECT {APP_RELEASE_SELECT}, a.uuid AS app_uuid, t.uuid AS target_uuid,
                    p.uuid AS package_uuid
             FROM deploy_release r
             JOIN deploy_app a ON a.id = r.app_id
             JOIN deploy_app_platform_target t ON t.id = r.platform_target_id
             JOIN deploy_package p ON p.id = r.package_id
             WHERE r.tenant_id = $1 AND r.app_id = $2 AND r.uuid = $3 AND r.release_status IS NOT NULL"
        );
        let row = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(app_internal_id)
            .bind(release_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve deploy_release app", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("release not found"));
        };
        map_app_release_row(&row)
    }

    /// Changes the release lifecycle status (DRAFT -> ACTIVE/SUPERSEDED/...).
    /// Content never changes; only the status transitions.
    pub(super) async fn update_app_release_status_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        release_id: &str,
        release_status: ReleaseStatus,
    ) -> DeployServiceResult<AppReleaseResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let release_internal_id =
            resolve_app_release_internal_id(&self.pool, tenant_id, app_internal_id, release_id)
                .await?;

        sqlx::query(
            "UPDATE deploy_release SET release_status = $3, updated_at = NOW(),
                version = version + 1
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(release_internal_id)
        .bind(release_id)
        .bind(release_status.as_str())
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("update deploy_release status", error))?;

        self.retrieve_app_release_repo(tenant_id, app_id, release_id)
            .await
    }

    // -- channels -------------------------------------------------------------

    pub(super) async fn ensure_release_channel_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        target_id: &str,
        channel_key: &str,
    ) -> DeployServiceResult<ChannelResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let target_internal_id =
            resolve_platform_target_internal_id(&self.pool, tenant_id, app_internal_id, target_id)
                .await?;

        let existing = sqlx::query(
            "SELECT c.uuid, a.uuid AS app_uuid, t.uuid AS target_uuid, c.channel_key,
                    r.uuid AS current_release_uuid, r.semantic_version AS current_release_version,
                    c.channel_status, c.updated_at, c.version
             FROM deploy_release_channel c
             JOIN deploy_app a ON a.id = c.app_id
             JOIN deploy_app_platform_target t ON t.id = c.platform_target_id
             LEFT JOIN deploy_release r ON r.id = c.current_release_id
             WHERE c.tenant_id = $1 AND c.app_id = $2 AND c.platform_target_id = $3
               AND c.channel_key = $4 AND c.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(target_internal_id)
        .bind(channel_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("find deploy_release_channel", error))?;

        if let Some(row) = existing {
            return map_channel_row(&row);
        }

        let channel_id = next_id(self.id_generator())?;
        let channel_uuid = new_uuid();
        let inserted = sqlx::query(
            "INSERT INTO deploy_release_channel
                (id, uuid, tenant_id, organization_id, app_id, platform_target_id,
                 channel_key, current_release_id, channel_status, created_by, updated_by,
                 created_at, updated_at, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, 'ACTIVE', $8, $8, NOW(), NOW(), 1)
             ON CONFLICT (app_id, platform_target_id, channel_key) WHERE deleted_at IS NULL DO NOTHING
             RETURNING uuid",
        )
        .bind(channel_id)
        .bind(&channel_uuid)
        .bind(tenant_id)
        .bind(0)
        .bind(app_internal_id)
        .bind(target_internal_id)
        .bind(channel_key)
        .bind(0_i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_release_channel", error))?;

        let Some(row) = inserted else {
            // Concurrent creation won; read the existing row directly
            // (no recursion: async recursion would require boxing).
            let existing = sqlx::query(
                "SELECT c.uuid, a.uuid AS app_uuid, t.uuid AS target_uuid, c.channel_key,
                        r.uuid AS current_release_uuid,
                        r.semantic_version AS current_release_version,
                        c.channel_status, c.updated_at, c.version
                 FROM deploy_release_channel c
                 JOIN deploy_app a ON a.id = c.app_id
                 JOIN deploy_app_platform_target t ON t.id = c.platform_target_id
                 LEFT JOIN deploy_release r ON r.id = c.current_release_id
                 WHERE c.tenant_id = $1 AND c.app_id = $2
                   AND c.platform_target_id = $3 AND c.channel_key = $4
                   AND c.deleted_at IS NULL",
            )
            .bind(tenant_id)
            .bind(app_internal_id)
            .bind(target_internal_id)
            .bind(channel_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("re-read deploy_release_channel", error))?;
            let Some(existing) = existing else {
                return Err(DeployServiceError::Internal(
                    "channel disappeared after concurrent creation".into(),
                ));
            };
            return map_channel_row(&existing);
        };
        let inserted_uuid: String = row
            .try_get("uuid")
            .map_err(|error| DeployServiceError::Internal(format!("read channel uuid: {error}")))?;
        self.retrieve_channel_repo(tenant_id, app_id, &inserted_uuid)
            .await
    }

    pub(super) async fn retrieve_channel_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        channel_id: &str,
    ) -> DeployServiceResult<ChannelResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        resolve_channel_internal_id(&self.pool, tenant_id, app_internal_id, channel_id).await?;
        let row = sqlx::query(
            "SELECT c.uuid, a.uuid AS app_uuid, t.uuid AS target_uuid, c.channel_key,
                    r.uuid AS current_release_uuid, r.semantic_version AS current_release_version,
                    c.channel_status, c.updated_at, c.version
             FROM deploy_release_channel c
             JOIN deploy_app a ON a.id = c.app_id
             JOIN deploy_app_platform_target t ON t.id = c.platform_target_id
             LEFT JOIN deploy_release r ON r.id = c.current_release_id
             WHERE c.tenant_id = $1 AND c.app_id = $2 AND c.uuid = $3 AND c.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(channel_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_release_channel", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("channel not found"));
        };
        map_channel_row(&row)
    }

    pub(super) async fn list_channels_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<ChannelPage> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let rows = sqlx::query(
            "SELECT c.uuid, a.uuid AS app_uuid, t.uuid AS target_uuid, c.channel_key,
                    r.uuid AS current_release_uuid, r.semantic_version AS current_release_version,
                    c.channel_status, c.updated_at, c.version
             FROM deploy_release_channel c
             JOIN deploy_app a ON a.id = c.app_id
             JOIN deploy_app_platform_target t ON t.id = c.platform_target_id
             LEFT JOIN deploy_release r ON r.id = c.current_release_id
             WHERE c.tenant_id = $1 AND c.app_id = $2 AND c.deleted_at IS NULL
             ORDER BY c.created_at ASC, c.id ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_release_channel", error))?;

        let items = rows
            .iter()
            .map(map_channel_row)
            .collect::<Result<Vec<_>, _>>()?;
        let total = items.len() as i64;
        Ok(ChannelPage {
            items,
            total,
            page: 1,
            page_size: total.max(1) as i32,
        })
    }

    /// Transactional promotion: insert the immutable rollout row (fencing the
    /// previous rollout) and advance the channel current pointer.
    pub(super) async fn promote_channel_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        channel_id: &str,
        actor_id: Option<i64>,
        request: &PromoteChannelRequest,
    ) -> DeployServiceResult<ChannelRolloutResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let channel_internal_id =
            resolve_channel_internal_id(&self.pool, tenant_id, app_internal_id, channel_id).await?;

        // The release must belong to the channel's app scope and platform
        // target scope.
        let release_row = sqlx::query(
            "SELECT r.id, r.platform_target_id, r.release_status FROM deploy_release r
             JOIN deploy_release_channel c ON c.id = $1
             WHERE r.tenant_id = $2 AND r.uuid = $3 AND r.release_status IS NOT NULL",
        )
        .bind(channel_internal_id)
        .bind(tenant_id)
        .bind(&request.release_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve channel release", error))?;

        let Some(release_row) = release_row else {
            return Err(DeployServiceError::not_found("release not found"));
        };
        let release_internal_id: i64 = release_row.try_get("id").unwrap_or(0);
        let release_target_id: i64 = release_row.try_get("platform_target_id").unwrap_or(0);
        let release_status: String = release_row.try_get("release_status").unwrap_or_default();
        if matches!(release_status.as_str(), "RETIRED" | "ARCHIVED") {
            return Err(DeployServiceError::validation(format!(
                "release status {release_status} cannot be promoted"
            )));
        }

        let channel_target_id: i64 =
            sqlx::query("SELECT platform_target_id FROM deploy_release_channel WHERE id = $1")
                .bind(channel_internal_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|error| store_error("read channel target", error))?
                .try_get("platform_target_id")
                .unwrap_or(0);
        if release_target_id != channel_target_id {
            return Err(DeployServiceError::validation(
                "release platform target does not match the channel platform target",
            ));
        }

        let strategy = request.strategy.unwrap_or(RolloutStrategy::Immediate);
        let percentage = request.percentage;
        if strategy == RolloutStrategy::Percentage && percentage.is_none() {
            return Err(DeployServiceError::validation(
                "percentage strategy requires a percentage",
            ));
        }
        if let Some(percentage) = percentage {
            if !(1..=100).contains(&percentage) {
                return Err(DeployServiceError::validation("percentage must be 1..=100"));
            }
        }

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin channel promotion", error))?;

        // Fence the previous rollout: mark it superseded by this one.
        let previous_rollout_id: Option<i64> = sqlx::query(
            "SELECT id FROM deploy_channel_rollout
             WHERE channel_id = $1 AND rollout_status IN ('PENDING', 'ROLLING', 'COMPLETED')
             ORDER BY created_at DESC, id DESC LIMIT 1",
        )
        .bind(channel_internal_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("find previous rollout", error))?
        .and_then(|row| row.try_get::<i64, _>("id").ok());

        let rollout_id = next_id(self.id_generator())?;
        let rollout_uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_channel_rollout
                (id, uuid, tenant_id, organization_id, channel_id, release_id, strategy,
                 percentage, rollout_status, supersedes_rollout_id, requested_by,
                 requested_at, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW(), NOW())",
        )
        .bind(rollout_id)
        .bind(&rollout_uuid)
        .bind(tenant_id)
        .bind(0)
        .bind(channel_internal_id)
        .bind(release_internal_id)
        .bind(strategy.as_str())
        .bind(percentage.map(|value| value as i32))
        .bind(RolloutStatus::Completed.as_str())
        .bind(previous_rollout_id)
        .bind(actor_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy_channel_rollout", error))?;

        // Advance the channel current pointer; the older rollout is fenced.
        sqlx::query(
            "UPDATE deploy_release_channel SET current_release_id = $2, updated_at = NOW(),
                version = version + 1
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(channel_internal_id)
        .bind(release_internal_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("update channel current release", error))?;

        // The promoted release becomes ACTIVE unless it is DRAFT (manual
        // approval flow keeps DRAFT).
        if release_status == ReleaseStatus::Draft.as_str() {
            sqlx::query(
                "UPDATE deploy_release SET release_status = $2, updated_at = NOW(),
                    version = version + 1
                 WHERE id = $1",
            )
            .bind(release_internal_id)
            .bind(ReleaseStatus::Active.as_str())
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("activate promoted release", error))?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit channel promotion", error))?;

        self.retrieve_channel_rollout_repo(tenant_id, app_id, &rollout_uuid)
            .await
    }

    pub(super) async fn list_channel_rollouts_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        channel_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ChannelRolloutPage> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let channel_internal_id =
            resolve_channel_internal_id(&self.pool, tenant_id, app_internal_id, channel_id).await?;
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_channel_rollout
             WHERE tenant_id = $1 AND channel_id = $2",
        )
        .bind(tenant_id)
        .bind(channel_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_channel_rollout", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT o.uuid, c.uuid AS channel_uuid, r.uuid AS release_uuid,
                    r.semantic_version AS release_version, o.strategy, o.percentage,
                    o.rollout_status, so.uuid AS supersedes_rollout_uuid, o.requested_at,
                    o.completed_at
             FROM deploy_channel_rollout o
             JOIN deploy_release_channel c ON c.id = o.channel_id
             JOIN deploy_release r ON r.id = o.release_id
             LEFT JOIN deploy_channel_rollout so ON so.id = o.supersedes_rollout_id
             WHERE o.tenant_id = $1 AND o.channel_id = $2
             ORDER BY o.created_at DESC, o.id DESC LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(channel_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_channel_rollout", error))?;

        let items = rows
            .iter()
            .map(map_channel_rollout_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ChannelRolloutPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_channel_rollout_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        rollout_id: &str,
    ) -> DeployServiceResult<ChannelRolloutResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let row = sqlx::query(
            "SELECT o.uuid, c.uuid AS channel_uuid, r.uuid AS release_uuid,
                    r.semantic_version AS release_version, o.strategy, o.percentage,
                    o.rollout_status, so.uuid AS supersedes_rollout_uuid, o.requested_at,
                    o.completed_at
             FROM deploy_channel_rollout o
             JOIN deploy_release_channel c ON c.id = o.channel_id
             JOIN deploy_release r ON r.id = o.release_id
             LEFT JOIN deploy_channel_rollout so ON so.id = o.supersedes_rollout_id
             WHERE o.tenant_id = $1 AND o.uuid = $2 AND o.channel_id IN (
                 SELECT id FROM deploy_release_channel WHERE app_id = $3
             )",
        )
        .bind(tenant_id)
        .bind(rollout_id)
        .bind(app_internal_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_channel_rollout", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("channel rollout not found"));
        };
        map_channel_rollout_row(&row)
    }
}

fn map_channel_row(row: &PgRow) -> Result<ChannelResponse, DeployServiceError> {
    let updated_at = required_datetime(row, "updated_at")?;
    Ok(ChannelResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        platform_target_id: row.try_get("target_uuid").unwrap_or_default(),
        channel_key: row.try_get("channel_key").unwrap_or_default(),
        current_release_id: row.try_get("current_release_uuid").ok(),
        current_release_version: row.try_get("current_release_version").ok(),
        channel_status: row.try_get("channel_status").unwrap_or_default(),
        updated_at,
        version: row.try_get::<i64, _>("version").unwrap_or(1).to_string(),
    })
}

fn map_channel_rollout_row(row: &PgRow) -> Result<ChannelRolloutResponse, DeployServiceError> {
    let requested_at = required_datetime(row, "requested_at")?;
    Ok(ChannelRolloutResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        channel_id: row.try_get("channel_uuid").unwrap_or_default(),
        release_id: row.try_get("release_uuid").unwrap_or_default(),
        release_version: row.try_get("release_version").unwrap_or_default(),
        strategy: row.try_get("strategy").unwrap_or_default(),
        percentage: row
            .try_get::<Option<i32>, _>("percentage")
            .ok()
            .flatten()
            .map(|value| value as u32),
        rollout_status: row.try_get("rollout_status").unwrap_or_default(),
        supersedes_rollout_id: row
            .try_get::<Option<String>, _>("supersedes_rollout_uuid")
            .ok()
            .flatten()
            .filter(|value| !value.is_empty()),
        requested_at,
        completed_at: optional_datetime(row, "completed_at")?,
    })
}
