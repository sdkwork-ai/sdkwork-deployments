//! Build execution and deployment package repository operations
//! (REQ-2026-0002). Build numbers are strictly monotonic per
//! (App, platform target); claim transitions are transactional.

use sdkwork_deploy_contract::{
    BuildPage, BuildResponse, BuildStatus, CreateBuildRequest, DeployServiceError,
    DeployServiceResult, PackagePage, PackageResponse, PackageStatus, RegisterPackageRequest,
    UpdateBuildStateRequest,
};
use sqlx::{postgres::PgRow, AssertSqlSafe, Row};

use crate::support::{
    new_uuid, next_id, now_rfc3339, optional_datetime, pagination, required_datetime,
    resolve_app_internal_id, resolve_build_internal_id, resolve_package_internal_id,
    resolve_platform_target_internal_id, store_error,
};
use crate::DeployRepository;

const BUILD_SELECT: &str = "b.uuid, b.app_id, b.platform_target_id, b.template_id,
    b.build_number, b.source_repository_id, b.source_ref, b.source_snapshot_json,
    b.build_status, b.log_ref, b.produced_package_id, b.quality_gate_json,
    b.runner_node_uuid, b.runner_version, b.error_code, b.started_at, b.finished_at,
    b.duration_ms, b.created_at, b.updated_at, b.version";

fn map_build_row(row: &PgRow) -> Result<BuildResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    Ok(BuildResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        platform_target_id: row.try_get("target_uuid").unwrap_or_default(),
        template_id: row.try_get("template_uuid").ok(),
        build_number: row.try_get("build_number").unwrap_or(0),
        source_repository_id: row.try_get("repo_uuid").ok(),
        source_ref: row.try_get("source_ref").ok(),
        source_snapshot: row
            .try_get::<Option<serde_json::Value>, _>("source_snapshot_json")
            .ok()
            .flatten(),
        build_status: row.try_get("build_status").unwrap_or_default(),
        log_ref: row.try_get("log_ref").ok(),
        produced_package_id: row.try_get("package_uuid").ok(),
        quality_gate: row
            .try_get::<Option<serde_json::Value>, _>("quality_gate_json")
            .ok()
            .flatten(),
        runner_node_uuid: row.try_get("runner_node_uuid").ok(),
        error_code: row.try_get("error_code").ok(),
        started_at: optional_datetime(row, "started_at")?,
        finished_at: optional_datetime(row, "finished_at")?,
        duration_ms: row.try_get("duration_ms").ok(),
        created_at,
        updated_at,
        version: row.try_get::<i64, _>("version").unwrap_or(1).to_string(),
    })
}

impl DeployRepository {
    pub(super) async fn create_build_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateBuildRequest,
    ) -> DeployServiceResult<BuildResponse> {
        // Idempotency: return the prior build when the key matches.
        if let Some(existing) = self
            .find_build_by_idempotency_key_repo(tenant_id, &request.idempotency_key)
            .await?
        {
            return Ok(existing);
        }

        let app_internal_id = sqlx::query(
            "SELECT app_id FROM deploy_app_platform_target
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(&request.platform_target_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve build app id", error))?
        .and_then(|row| row.try_get::<i64, _>("app_id").ok())
        .ok_or_else(|| DeployServiceError::not_found("platform target not found"))?;

        let target_internal_id = resolve_platform_target_internal_id(
            &self.pool,
            tenant_id,
            app_internal_id,
            &request.platform_target_id,
        )
        .await?;

        let repo_internal_id = match request.source_repository_id.as_deref() {
            Some(repo_id) => Some(
                sqlx::query(
                    "SELECT id FROM deploy_source_repository
                     WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
                )
                .bind(tenant_id)
                .bind(app_internal_id)
                .bind(repo_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error("resolve source repository id", error))?
                .and_then(|row| row.try_get::<i64, _>("id").ok())
                .ok_or_else(|| DeployServiceError::not_found("source repository not found"))?,
            ),
            None => None,
        };

        let template_internal_id = match request.template_id.as_deref() {
            Some(template_id) => Some(
                sqlx::query(
                    "SELECT id FROM deploy_build_template
                     WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
                )
                .bind(tenant_id)
                .bind(template_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error("resolve build template id", error))?
                .and_then(|row| row.try_get::<i64, _>("id").ok())
                .ok_or_else(|| DeployServiceError::not_found("build template not found"))?,
            ),
            None => None,
        };

        // Monotonic build_number: reserve the next value and insert; the
        // unique (app_id, platform_target_id, build_number) index fences
        // concurrent reservations.
        let build_id = next_id(self.id_generator())?;
        let build_uuid = new_uuid();
        let source_ref = request
            .source_ref
            .clone()
            .unwrap_or_else(|| "HEAD".to_owned());
        let semantic_version = request.semantic_version.clone();

        let build_number_row = sqlx::query(
            "SELECT COALESCE(MAX(build_number), 0) + 1 AS next_number
             FROM deploy_build
             WHERE app_id = $1 AND platform_target_id = $2",
        )
        .bind(app_internal_id)
        .bind(target_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("reserve build number", error))?;
        let build_number: i64 = build_number_row.try_get("next_number").unwrap_or(1);

        let result = sqlx::query(
            "INSERT INTO deploy_build
                (id, uuid, tenant_id, organization_id, app_id, platform_target_id,
                 template_id, build_number, source_repository_id, source_ref,
                 source_snapshot_json, build_status, idempotency_key, created_by,
                 updated_by, created_at, updated_at, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, '{}', $11, $12, $13, $13, NOW(), NOW(), 1)
             ON CONFLICT (tenant_id, app_id, idempotency_key)
             WHERE idempotency_key IS NOT NULL DO NOTHING
             RETURNING uuid",
        )
        .bind(build_id)
        .bind(&build_uuid)
        .bind(tenant_id)
        .bind(0)
        .bind(app_internal_id)
        .bind(target_internal_id)
        .bind(template_internal_id)
        .bind(build_number)
        .bind(repo_internal_id)
        .bind(&source_ref)
        .bind(BuildStatus::Queued.as_str())
        .bind(&request.idempotency_key)
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            if crate::support::is_unique_violation(&error) {
                DeployServiceError::conflict(
                    "build number reservation or idempotency conflict; retry the request",
                )
            } else {
                store_error("insert deploy_build", error)
            }
        })?;

        let Some(row) = result else {
            // A concurrent reservation won the number; the caller retries.
            return Err(DeployServiceError::conflict(
                "build number reservation conflict; retry the request",
            ));
        };
        let inserted_uuid: String = row
            .try_get("uuid")
            .map_err(|error| DeployServiceError::Internal(format!("read build uuid: {error}")))?;
        let _ = semantic_version;
        let app_uuid: String = sqlx::query("SELECT uuid FROM deploy_app WHERE id = $1")
            .bind(app_internal_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve build app uuid", error))?
            .and_then(|row| row.try_get::<String, _>("uuid").ok())
            .ok_or_else(|| DeployServiceError::not_found("app not found"))?;
        self.retrieve_build_repo(tenant_id, &app_uuid, &inserted_uuid)
            .await
    }

    pub(super) async fn find_build_by_idempotency_key_repo(
        &self,
        tenant_id: i64,
        idempotency_key: &str,
    ) -> DeployServiceResult<Option<BuildResponse>> {
        let query = format!(
            "SELECT {BUILD_SELECT}, a.uuid AS app_uuid, t.uuid AS target_uuid,
                    bt.uuid AS template_uuid, r.uuid AS repo_uuid, p.uuid AS package_uuid
             FROM deploy_build b
             JOIN deploy_app a ON a.id = b.app_id
             JOIN deploy_app_platform_target t ON t.id = b.platform_target_id
             LEFT JOIN deploy_build_template bt ON bt.id = b.template_id
             LEFT JOIN deploy_source_repository r ON r.id = b.source_repository_id
             LEFT JOIN deploy_package p ON p.id = b.produced_package_id
             WHERE b.tenant_id = $1 AND b.idempotency_key = $2 AND b.deleted_at IS NULL
             ORDER BY b.created_at DESC LIMIT 1"
        );
        let row = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(idempotency_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("find deploy_build by idempotency key", error))?;

        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(map_build_row(&row)?))
    }

    pub(super) async fn list_builds_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildPage> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_build
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_build", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let query = format!(
            "SELECT {BUILD_SELECT}, a.uuid AS app_uuid, t.uuid AS target_uuid,
                    bt.uuid AS template_uuid, r.uuid AS repo_uuid, p.uuid AS package_uuid
             FROM deploy_build b
             JOIN deploy_app a ON a.id = b.app_id
             JOIN deploy_app_platform_target t ON t.id = b.platform_target_id
             LEFT JOIN deploy_build_template bt ON bt.id = b.template_id
             LEFT JOIN deploy_source_repository r ON r.id = b.source_repository_id
             LEFT JOIN deploy_package p ON p.id = b.produced_package_id
             WHERE b.tenant_id = $1 AND b.app_id = $2 AND b.deleted_at IS NULL
             ORDER BY b.created_at DESC, b.id DESC LIMIT $3 OFFSET $4"
        );
        let rows = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(app_internal_id)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list deploy_build", error))?;

        let items = rows
            .iter()
            .map(map_build_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(BuildPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_build_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        build_id: &str,
    ) -> DeployServiceResult<BuildResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        resolve_build_internal_id(&self.pool, tenant_id, app_internal_id, build_id).await?;

        let query = format!(
            "SELECT {BUILD_SELECT}, a.uuid AS app_uuid, t.uuid AS target_uuid,
                    bt.uuid AS template_uuid, r.uuid AS repo_uuid, p.uuid AS package_uuid
             FROM deploy_build b
             JOIN deploy_app a ON a.id = b.app_id
             JOIN deploy_app_platform_target t ON t.id = b.platform_target_id
             LEFT JOIN deploy_build_template bt ON bt.id = b.template_id
             LEFT JOIN deploy_source_repository r ON r.id = b.source_repository_id
             LEFT JOIN deploy_package p ON p.id = b.produced_package_id
             WHERE b.tenant_id = $1 AND b.app_id = $2 AND b.uuid = $3 AND b.deleted_at IS NULL"
        );
        let row = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(app_internal_id)
            .bind(build_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve deploy_build", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("build not found"));
        };
        map_build_row(&row)
    }

    /// Runner-reported state transition (typed executor contract). Terminal
    /// states are final; active states must be a forward step of the state
    /// machine; the runner identity must match the current claim.
    pub(super) async fn update_build_state_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        build_id: &str,
        request: &UpdateBuildStateRequest,
    ) -> DeployServiceResult<BuildResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let build_internal_id =
            resolve_build_internal_id(&self.pool, tenant_id, app_internal_id, build_id).await?;

        let current = sqlx::query(
            "SELECT build_status, runner_node_uuid, started_at, finished_at FROM deploy_build
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(build_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("read deploy_build state", error))?;
        let current_status: String = current.try_get("build_status").unwrap_or_default();
        let current_runner: Option<String> = current.try_get("runner_node_uuid").ok();
        // Both instants are TIMESTAMPTZ. Reading them as `String` failed on the
        // wire and `.ok()` turned that failure into `None`, which made the
        // terminal-state guard below unreachable: a finished build could be
        // driven through the state machine again, and `duration_ms` was never
        // computed. `started_at` also has to be selected — it is what the
        // duration is measured from.
        let finished_at = optional_datetime(&current, "finished_at")?;
        let started_at = optional_datetime(&current, "started_at")?;

        if finished_at.is_some() {
            return Err(DeployServiceError::conflict(
                "build is already in a terminal state",
            ));
        }
        if let Some(current_runner) = current_runner {
            if !current_runner.is_empty() && current_runner != request.runner_node_uuid {
                return Err(DeployServiceError::conflict(format!(
                    "build is claimed by runner {current_runner}"
                )));
            }
        }

        let next_status = request.build_status.as_str();
        if BuildStatus::Failed.as_str() == next_status
            || BuildStatus::Cancelled.as_str() == next_status
            || BuildStatus::TimedOut.as_str() == next_status
            || BuildStatus::Succeeded.as_str() == next_status
        {
            // terminal transition: accept from any active state
        } else if current_status == BuildStatus::Queued.as_str()
            && matches!(
                request.build_status,
                BuildStatus::Preparing | BuildStatus::Cancelled
            )
        {
            // queued -> preparing/cancelled
        } else if current_status == BuildStatus::Preparing.as_str()
            && matches!(
                request.build_status,
                BuildStatus::Compiling | BuildStatus::Cancelled
            )
        {
            // preparing -> compiling/cancelled
        } else if current_status == BuildStatus::Compiling.as_str()
            && matches!(
                request.build_status,
                BuildStatus::Testing | BuildStatus::Packaging | BuildStatus::Cancelled
            )
        {
            // compiling -> testing/packaging/cancelled
        } else if current_status == BuildStatus::Testing.as_str()
            && matches!(
                request.build_status,
                BuildStatus::Packaging | BuildStatus::Cancelled
            )
        {
            // testing -> packaging/cancelled
        } else if current_status == BuildStatus::Packaging.as_str()
            && matches!(
                request.build_status,
                BuildStatus::Succeeded | BuildStatus::Failed
            )
        {
            // packaging -> succeeded/failed
        } else {
            return Err(DeployServiceError::conflict(format!(
                "invalid build state transition {current_status} -> {next_status}"
            )));
        }

        let now = now_rfc3339();
        let (finished_value, duration_ms) = if request.build_status.is_terminal() {
            let duration = started_at
                .as_deref()
                .and_then(|started| parse_duration_ms(started, &now));
            (Some(now.clone()), duration)
        } else {
            (None, None)
        };

        sqlx::query(
            "UPDATE deploy_build SET
                build_status = $3,
                runner_node_uuid = $4,
                runner_version = COALESCE($5, runner_version),
                log_ref = COALESCE($6, log_ref),
                source_snapshot_json = COALESCE($7, source_snapshot_json),
                quality_gate_json = COALESCE($8, quality_gate_json),
                error_code = $9,
                -- The runner reports these as RFC3339 strings, so the cast is
                -- what lets them meet the TIMESTAMPTZ column: COALESCE of a
                -- text parameter and a timestamp column is rejected outright
                -- instead of being coerced.
                started_at = COALESCE(CAST($10 AS TIMESTAMPTZ), started_at),
                finished_at = COALESCE(CAST($11 AS TIMESTAMPTZ), finished_at),
                duration_ms = COALESCE($12, duration_ms),
                updated_by = $13, updated_at = NOW(), version = version + 1
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(build_internal_id)
        .bind(build_id)
        .bind(next_status)
        .bind(&request.runner_node_uuid)
        .bind(request.runner_version.as_deref())
        .bind(request.log_ref.as_deref())
        .bind(request.source_snapshot.as_ref())
        .bind(request.quality_gate.as_ref())
        .bind(request.error_code.as_deref())
        .bind(request.started_at.as_deref())
        .bind(finished_value.as_deref())
        .bind(duration_ms)
        .bind(0_i64)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("update deploy_build state", error))?;

        self.retrieve_build_repo(tenant_id, app_id, build_id).await
    }

    /// Claim the oldest QUEUED build for a runner (bounded keyset scan).
    /// The claim is the `PREPARING` transition recorded with the runner
    /// identity; a crash leaves the row claimable after expiry handled by
    /// the caller's durable scan.
    pub(super) async fn claim_next_build_repo(
        &self,
        tenant_id: i64,
        runner_node_uuid: &str,
        runner_version: &str,
    ) -> DeployServiceResult<Option<BuildResponse>> {
        let candidate = sqlx::query(
            "SELECT b.id, b.uuid, a.uuid AS app_uuid FROM deploy_build b
             JOIN deploy_app a ON a.id = b.app_id
             WHERE b.tenant_id = $1 AND b.build_status = $2 AND b.deleted_at IS NULL
             ORDER BY b.created_at ASC, b.id ASC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(BuildStatus::Queued.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("claim deploy_build", error))?;

        let Some(candidate) = candidate else {
            return Ok(None);
        };
        let build_internal_id: i64 = candidate.try_get("id").unwrap_or(0);
        let build_uuid: String = candidate.try_get("uuid").unwrap_or_default();
        let app_uuid: String = candidate.try_get("app_uuid").unwrap_or_default();

        let claimed = sqlx::query(
            "UPDATE deploy_build SET build_status = $3, runner_node_uuid = $4,
                runner_version = $5, started_at = NOW(), updated_at = NOW(),
                version = version + 1
             WHERE id = $1 AND build_status = $2 AND deleted_at IS NULL
             RETURNING uuid",
        )
        .bind(build_internal_id)
        .bind(BuildStatus::Queued.as_str())
        .bind(BuildStatus::Preparing.as_str())
        .bind(runner_node_uuid)
        .bind(runner_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("claim deploy_build transition", error))?;

        let Some(_) = claimed else {
            // Another runner won the claim; the queue scan continues.
            return Ok(None);
        };
        let build = self
            .retrieve_build_repo(tenant_id, &app_uuid, &build_uuid)
            .await?;
        Ok(Some(build))
    }

    // -- packages ------------------------------------------------------------

    pub(super) async fn register_package_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &RegisterPackageRequest,
    ) -> DeployServiceResult<PackageResponse> {
        // A build may produce at most one package; registration is idempotent
        // per build.
        if let Some(existing) = self
            .find_package_by_build_repo(tenant_id, &request.build_id)
            .await?
        {
            return Ok(existing);
        }

        let (app_internal_id, target_internal_id) = self
            .resolve_build_scope_repo(tenant_id, &request.build_id, &request.platform_target_id)
            .await?;
        let build_internal_id =
            resolve_build_internal_id(&self.pool, tenant_id, app_internal_id, &request.build_id)
                .await?;

        let signing_internal_id = match request.signing_identity_id.as_deref() {
            Some(identity_id) => Some(
                sqlx::query(
                    "SELECT id FROM deploy_signing_identity
                     WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
                )
                .bind(tenant_id)
                .bind(identity_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error("resolve signing identity id", error))?
                .and_then(|row| row.try_get::<i64, _>("id").ok())
                .ok_or_else(|| DeployServiceError::not_found("signing identity not found"))?,
            ),
            None => None,
        };

        let package_id = next_id(self.id_generator())?;
        let package_uuid = new_uuid();
        let drive_ref = serde_json::json!({
            "nodeId": request.drive_node_id,
            "spaceId": request.drive_space_id,
        });
        let architectures = request.architectures.clone().unwrap_or_default();
        let arch_json = serde_json::to_value(&architectures).map_err(|error| {
            DeployServiceError::Internal(format!("serialize package architectures: {error}"))
        })?;
        let bundle_identity = request
            .bundle_identity
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let validation_report = request
            .validation_report
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        sqlx::query(
            "INSERT INTO deploy_package
                (id, uuid, tenant_id, organization_id, app_id, platform_target_id, build_id,
                 package_format, semantic_version, package_size_bytes, checksum_sha256,
                 manifest_sha256, drive_ref_json, signing_identity_id, min_platform_version,
                 arch_json, bundle_identity_json, package_status, validation_report_json,
                 created_by, updated_by, created_at, updated_at, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                     $16, $17, $18, $19, $20, $20, NOW(), NOW(), 1)
             ON CONFLICT (app_id, platform_target_id, semantic_version, build_id) DO NOTHING
             RETURNING uuid",
        )
        .bind(package_id)
        .bind(&package_uuid)
        .bind(tenant_id)
        .bind(0)
        .bind(app_internal_id)
        .bind(target_internal_id)
        .bind(build_internal_id)
        .bind(request.package_format.as_str())
        .bind(&request.semantic_version)
        .bind(request.package_size_bytes)
        .bind(&request.checksum_sha256)
        .bind(&request.manifest_sha256)
        .bind(drive_ref)
        .bind(signing_internal_id)
        .bind(request.min_platform_version.as_deref())
        .bind(arch_json)
        .bind(bundle_identity)
        .bind(PackageStatus::Validated.as_str())
        .bind(validation_report)
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_package", error))?;

        // Link the produced package onto the build row.
        sqlx::query(
            "UPDATE deploy_build SET produced_package_id = $2, updated_at = NOW(),
                version = version + 1
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(build_internal_id)
        .bind(package_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("link deploy_build produced package", error))?;

        let result = self
            .retrieve_package_any_app_repo(tenant_id, &package_uuid)
            .await?;
        Ok(result)
    }

    pub(super) async fn find_package_by_build_repo(
        &self,
        tenant_id: i64,
        build_id: &str,
    ) -> DeployServiceResult<Option<PackageResponse>> {
        let row = sqlx::query(
            "SELECT p.uuid, a.uuid AS app_uuid, t.uuid AS target_uuid, b.uuid AS build_uuid,
                    p.package_format, p.semantic_version, p.package_size_bytes,
                    p.checksum_sha256, p.manifest_sha256, p.drive_ref_json,
                    si.uuid AS signing_uuid, p.min_platform_version, p.arch_json,
                    p.package_status, p.created_at, p.updated_at, p.version
             FROM deploy_package p
             JOIN deploy_app a ON a.id = p.app_id
             JOIN deploy_app_platform_target t ON t.id = p.platform_target_id
             JOIN deploy_build b ON b.id = p.build_id
             LEFT JOIN deploy_signing_identity si ON si.id = p.signing_identity_id
             WHERE p.tenant_id = $1 AND p.build_id = (
                 SELECT id FROM deploy_build WHERE tenant_id = $1 AND uuid = $2
             ) AND p.deleted_at IS NULL
             ORDER BY p.created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(build_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("find deploy_package by build", error))?;

        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(map_package_row(&row)?))
    }

    pub(super) async fn list_packages_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<PackagePage> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_package
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_package", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT p.uuid, a.uuid AS app_uuid, t.uuid AS target_uuid, b.uuid AS build_uuid,
                    p.package_format, p.semantic_version, p.package_size_bytes,
                    p.checksum_sha256, p.manifest_sha256, p.drive_ref_json,
                    si.uuid AS signing_uuid, p.min_platform_version, p.arch_json,
                    p.package_status, p.created_at, p.updated_at, p.version
             FROM deploy_package p
             JOIN deploy_app a ON a.id = p.app_id
             JOIN deploy_app_platform_target t ON t.id = p.platform_target_id
             JOIN deploy_build b ON b.id = p.build_id
             LEFT JOIN deploy_signing_identity si ON si.id = p.signing_identity_id
             WHERE p.tenant_id = $1 AND p.app_id = $2 AND p.deleted_at IS NULL
             ORDER BY p.created_at DESC, p.id DESC LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_package", error))?;

        let items = rows
            .iter()
            .map(map_package_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PackagePage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_package_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        package_id: &str,
    ) -> DeployServiceResult<PackageResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        resolve_package_internal_id(&self.pool, tenant_id, app_internal_id, package_id).await?;
        let row = sqlx::query(
            "SELECT p.uuid, a.uuid AS app_uuid, t.uuid AS target_uuid, b.uuid AS build_uuid,
                    p.package_format, p.semantic_version, p.package_size_bytes,
                    p.checksum_sha256, p.manifest_sha256, p.drive_ref_json,
                    si.uuid AS signing_uuid, p.min_platform_version, p.arch_json,
                    p.package_status, p.created_at, p.updated_at, p.version
             FROM deploy_package p
             JOIN deploy_app a ON a.id = p.app_id
             JOIN deploy_app_platform_target t ON t.id = p.platform_target_id
             JOIN deploy_build b ON b.id = p.build_id
             LEFT JOIN deploy_signing_identity si ON si.id = p.signing_identity_id
             WHERE p.tenant_id = $1 AND p.app_id = $2 AND p.uuid = $3 AND p.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(package_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_package", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("package not found"));
        };
        map_package_row(&row)
    }

    pub(super) async fn retrieve_package_any_app_repo(
        &self,
        tenant_id: i64,
        package_id: &str,
    ) -> DeployServiceResult<PackageResponse> {
        let row = sqlx::query(
            "SELECT p.uuid, a.uuid AS app_uuid, t.uuid AS target_uuid, b.uuid AS build_uuid,
                    p.package_format, p.semantic_version, p.package_size_bytes,
                    p.checksum_sha256, p.manifest_sha256, p.drive_ref_json,
                    si.uuid AS signing_uuid, p.min_platform_version, p.arch_json,
                    p.package_status, p.created_at, p.updated_at, p.version
             FROM deploy_package p
             JOIN deploy_app a ON a.id = p.app_id
             JOIN deploy_app_platform_target t ON t.id = p.platform_target_id
             JOIN deploy_build b ON b.id = p.build_id
             LEFT JOIN deploy_signing_identity si ON si.id = p.signing_identity_id
             WHERE p.tenant_id = $1 AND p.uuid = $2 AND p.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(package_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_package any app", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("package not found"));
        };
        map_package_row(&row)
    }

    pub(super) async fn resolve_build_platform_repo(
        &self,
        tenant_id: i64,
        build_id: &str,
    ) -> DeployServiceResult<(String, String, String)> {
        let row = sqlx::query(
            "SELECT a.uuid AS app_uuid, t.uuid AS target_uuid, t.platform
             FROM deploy_build b
             JOIN deploy_app a ON a.id = b.app_id
             JOIN deploy_app_platform_target t ON t.id = b.platform_target_id
             WHERE b.tenant_id = $1 AND b.uuid = $2 AND b.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(build_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_build platform", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("build not found"));
        };
        Ok((
            row.try_get("app_uuid").unwrap_or_default(),
            row.try_get("target_uuid").unwrap_or_default(),
            row.try_get("platform").unwrap_or_default(),
        ))
    }

    /// Resolves the (app, platform target) scope of a build and verifies the
    /// requested target matches the build's own target.
    async fn resolve_build_scope_repo(
        &self,
        tenant_id: i64,
        build_id: &str,
        platform_target_id: &str,
    ) -> DeployServiceResult<(i64, i64)> {
        let row = sqlx::query(
            "SELECT b.app_id, b.platform_target_id FROM deploy_build b
             WHERE b.tenant_id = $1 AND b.uuid = $2 AND b.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(build_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_build scope", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("build not found"));
        };
        let app_internal_id: i64 = row.try_get("app_id").unwrap_or(0);
        let build_target_id: i64 = row.try_get("platform_target_id").unwrap_or(0);
        let requested_target = resolve_platform_target_internal_id(
            &self.pool,
            tenant_id,
            app_internal_id,
            platform_target_id,
        )
        .await?;
        if requested_target != build_target_id {
            return Err(DeployServiceError::validation(
                "platform target does not match the build's platform target",
            ));
        }
        Ok((app_internal_id, build_target_id))
    }
}

fn map_package_row(row: &PgRow) -> Result<PackageResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    let drive_node_id = row
        .try_get::<Option<serde_json::Value>, _>("drive_ref_json")
        .ok()
        .flatten()
        .and_then(|value| {
            value
                .get("nodeId")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        });
    Ok(PackageResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        platform_target_id: row.try_get("target_uuid").unwrap_or_default(),
        build_id: row.try_get("build_uuid").unwrap_or_default(),
        package_format: row.try_get("package_format").unwrap_or_default(),
        semantic_version: row.try_get("semantic_version").unwrap_or_default(),
        package_size_bytes: row.try_get("package_size_bytes").unwrap_or(0),
        checksum_sha256: row.try_get("checksum_sha256").unwrap_or_default(),
        manifest_sha256: row.try_get("manifest_sha256").unwrap_or_default(),
        drive_node_id,
        signing_identity_id: row.try_get("signing_uuid").ok(),
        min_platform_version: row.try_get("min_platform_version").ok(),
        architectures: row
            .try_get::<Option<serde_json::Value>, _>("arch_json")
            .ok()
            .flatten()
            .and_then(|value| serde_json::from_value(value).ok()),
        package_status: row.try_get("package_status").unwrap_or_default(),
        created_at,
        updated_at,
        version: row.try_get::<i64, _>("version").unwrap_or(1).to_string(),
    })
}

fn parse_duration_ms(started_at: &str, finished_at: &str) -> Option<i64> {
    let start = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    let finish = chrono::DateTime::parse_from_rfc3339(finished_at).ok()?;
    let duration = finish.signed_duration_since(start);
    Some(duration.num_milliseconds().max(0))
}
