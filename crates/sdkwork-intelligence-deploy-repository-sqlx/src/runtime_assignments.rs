use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sdkwork_deploy_web_port::RuntimeAssignmentReceipt;
use sdkwork_intelligence_deploy_service::runtime_publication::{
    DeployRuntimeAssignmentMutationPort, PersistRuntimeAssignmentCommand,
    RuntimeAssignmentPublishStatus, RuntimeAssignmentState, RuntimeObservationEvidence,
    RuntimeObservationPersistenceResult, RuntimeObservationState,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::support::{new_uuid, next_id, store_error};
use crate::DeployRepository;

const MAXIMUM_RUNTIME_GENERATION: u64 = 9_007_199_254_740_991;

fn normalize_timestamp(value: &str, field: &str) -> DeployServiceResult<String> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map(|timestamp| timestamp.to_rfc3339_opts(SecondsFormat::Millis, true))
        .map_err(|error| {
            DeployServiceError::Internal(format!("invalid {field} RFC 3339 timestamp: {error}"))
        })
}

struct SqlxRuntimeAssignmentMutation {
    transaction: Transaction<'static, Postgres>,
    id_generator: SnowflakeIdGenerator,
    target_id: i64,
    tenant_id: i64,
    latest: Option<RuntimeAssignmentState>,
    next_generation: u64,
}

impl DeployRepository {
    pub(super) async fn latest_runtime_assignment_repo(
        &self,
        target_uuid: &str,
    ) -> DeployServiceResult<Option<RuntimeAssignmentState>> {
        let row = sqlx::query(
            "SELECT a.uuid AS assignment_uuid, t.uuid AS target_uuid, a.tenant_id,
                    t.node_uuid, t.environment, a.trigger_app_revision_id,
                    a.generation, a.snapshot_uuid, a.snapshot_sha256,
                    a.desired_state_sha256,
                    CAST(a.runtime_set_json AS TEXT) AS runtime_set_json, a.publish_status,
                    a.remote_assignment_uuid, a.attempt_count, a.lease_owner
             FROM deploy_runtime_assignment a
             INNER JOIN deploy_web_node_target t ON t.id = a.node_target_id
             WHERE t.uuid = $1 AND t.deleted_at IS NULL
             ORDER BY a.generation DESC
             LIMIT 1",
        )
        .bind(target_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load latest deploy runtime assignment", error))?;
        row.as_ref().map(map_assignment_row).transpose()
    }

    pub(super) async fn begin_runtime_assignment_mutation_repo(
        &self,
        target_uuid: &str,
        tenant_id: i64,
    ) -> DeployServiceResult<Box<dyn DeployRuntimeAssignmentMutationPort>> {
        let mut transaction = begin_runtime_assignment_transaction(&self.pool).await?;
        let target = sqlx::query(
            "SELECT id, tenant_id FROM deploy_web_node_target
             WHERE uuid = $1 AND tenant_id = $2 AND status = 'ACTIVE' AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(target_uuid)
        .bind(tenant_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("resolve deploy Web Node target", error))?
        .ok_or_else(|| DeployServiceError::not_found("Web Node target not found"))?;
        let target_id: i64 = target
            .try_get("id")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        let latest = latest_runtime_assignment_for_target(&mut transaction, target_id).await?;
        let next_generation = latest
            .as_ref()
            .map_or(1, |assignment| assignment.generation.saturating_add(1));
        Ok(Box::new(SqlxRuntimeAssignmentMutation {
            transaction,
            id_generator: self.id_generator.clone(),
            target_id,
            tenant_id,
            latest,
            next_generation,
        }))
    }

    pub(super) async fn claim_due_runtime_assignments_repo(
        &self,
        maximum_items: i64,
        now: &str,
        lease_owner: &str,
        lease_expires_at: &str,
        maximum_attempts: i32,
    ) -> DeployServiceResult<Vec<RuntimeAssignmentState>> {
        let now = normalize_timestamp(now, "runtime assignment claim time")?;
        let lease_expires_at =
            normalize_timestamp(lease_expires_at, "runtime assignment lease expiry")?;
        let mut transaction = begin_runtime_assignment_transaction(&self.pool).await?;
        let rows = sqlx::query(
            "WITH candidates AS (
                    SELECT a.id
                    FROM deploy_runtime_assignment a
                    INNER JOIN deploy_web_node_target t ON t.id = a.node_target_id
                    WHERE (
                        (a.publish_status IN ('PENDING', 'FAILED')
                         AND a.attempt_count < $2
                         AND (a.next_attempt_at IS NULL
                              OR a.next_attempt_at <= CAST($1 AS TIMESTAMPTZ)))
                        OR (a.publish_status = 'PUBLISHING'
                            AND a.attempt_count < $2
                            AND a.lease_expires_at <= CAST($1 AS TIMESTAMPTZ))
                    )
                      AND t.status = 'ACTIVE' AND t.deleted_at IS NULL
                    ORDER BY COALESCE(a.next_attempt_at, a.lease_expires_at, a.created_at),
                             a.created_at, a.id
                    FOR UPDATE OF a SKIP LOCKED
                    LIMIT $3
                 ), claimed AS (
                    UPDATE deploy_runtime_assignment a
                    SET publish_status = 'PUBLISHING', attempt_count = attempt_count + 1,
                        next_attempt_at = NULL, lease_owner = $4,
                        lease_expires_at = CAST($5 AS TIMESTAMPTZ),
                        updated_at = CAST($1 AS TIMESTAMPTZ), version = version + 1
                    FROM candidates c
                    WHERE a.id = c.id
                    RETURNING a.*
                 )
                 SELECT a.uuid AS assignment_uuid, t.uuid AS target_uuid, a.tenant_id,
                         t.node_uuid, t.environment, a.trigger_app_revision_id,
                         a.generation, a.snapshot_uuid, a.snapshot_sha256,
                         a.desired_state_sha256,
                         CAST(a.runtime_set_json AS TEXT) AS runtime_set_json, a.publish_status,
                         a.remote_assignment_uuid, a.attempt_count, a.lease_owner
                 FROM claimed a
                 INNER JOIN deploy_web_node_target t ON t.id = a.node_target_id
                 ORDER BY a.created_at, a.id",
        )
        .bind(&now)
        .bind(maximum_attempts)
        .bind(maximum_items)
        .bind(lease_owner)
        .bind(&lease_expires_at)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| store_error("claim PostgreSQL runtime assignments", error))?;
        let assignments = rows
            .iter()
            .map(map_assignment_row)
            .collect::<DeployServiceResult<Vec<_>>>()?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit deploy runtime assignment claims", error))?;
        Ok(assignments)
    }

    pub(super) async fn mark_runtime_assignment_published_repo(
        &self,
        assignment_uuid: &str,
        lease_owner: &str,
        receipt: &RuntimeAssignmentReceipt,
        published_at: &str,
    ) -> DeployServiceResult<()> {
        let published_at =
            normalize_timestamp(published_at, "runtime assignment publication time")?;
        let result = sqlx::query(
            "UPDATE deploy_runtime_assignment
             SET publish_status = 'PUBLISHED', remote_assignment_uuid = $3,
                 published_at = CAST($4 AS TIMESTAMPTZ), next_attempt_at = NULL,
                 lease_owner = NULL, lease_expires_at = NULL, last_error_code = NULL,
                 updated_at = CAST($4 AS TIMESTAMPTZ), version = version + 1
             WHERE uuid = $1 AND lease_owner = $2 AND snapshot_uuid = $5
               AND snapshot_sha256 = $6 AND generation = $7
               AND publish_status = 'PUBLISHING'",
        )
        .bind(assignment_uuid)
        .bind(lease_owner)
        .bind(&receipt.assignment_uuid)
        .bind(&published_at)
        .bind(&receipt.snapshot_uuid)
        .bind(&receipt.snapshot_sha256)
        .bind(receipt.generation.parse::<i64>().unwrap_or_default())
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("mark deploy runtime assignment published", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::conflict(
                "runtime assignment publication state changed concurrently",
            ));
        }
        Ok(())
    }

    pub(super) async fn mark_runtime_assignment_failed_repo(
        &self,
        assignment_uuid: &str,
        lease_owner: &str,
        error_code: &str,
        next_attempt_at: Option<&str>,
        updated_at: &str,
    ) -> DeployServiceResult<()> {
        let next_attempt_at = next_attempt_at
            .map(|value| normalize_timestamp(value, "runtime assignment next attempt time"))
            .transpose()?;
        let updated_at = normalize_timestamp(updated_at, "runtime assignment failure time")?;
        let result = sqlx::query(
            "UPDATE deploy_runtime_assignment
             SET publish_status = 'FAILED',
                 next_attempt_at = CAST($3 AS TIMESTAMPTZ), lease_owner = NULL,
                 lease_expires_at = NULL, last_error_code = $4,
                 updated_at = CAST($5 AS TIMESTAMPTZ), version = version + 1
             WHERE uuid = $1 AND lease_owner = $2 AND publish_status = 'PUBLISHING'",
        )
        .bind(assignment_uuid)
        .bind(lease_owner)
        .bind(next_attempt_at.as_deref())
        .bind(error_code)
        .bind(&updated_at)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("mark deploy runtime assignment failed", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::conflict(
                "runtime assignment publication state changed concurrently",
            ));
        }
        Ok(())
    }

    pub(super) async fn list_runtime_assignments_requiring_observation_repo(
        &self,
        maximum_items: i64,
    ) -> DeployServiceResult<Vec<RuntimeAssignmentState>> {
        let rows = sqlx::query(
            "SELECT a.uuid AS assignment_uuid, t.uuid AS target_uuid, a.tenant_id,
                    t.node_uuid, t.environment, a.trigger_app_revision_id,
                    a.generation, a.snapshot_uuid, a.snapshot_sha256,
                    a.desired_state_sha256,
                    CAST(a.runtime_set_json AS TEXT) AS runtime_set_json, a.publish_status,
                    a.remote_assignment_uuid, a.attempt_count, a.lease_owner
             FROM deploy_runtime_assignment a
             INNER JOIN deploy_web_node_target t ON t.id = a.node_target_id
             WHERE a.publish_status = 'PUBLISHED' AND a.remote_assignment_uuid IS NOT NULL
               AND t.deleted_at IS NULL
               AND NOT EXISTS (
                   SELECT 1 FROM deploy_app_target_observation o
                   WHERE o.runtime_assignment_id = a.id
                     AND o.state IN ('ACTIVE', 'REJECTED')
               )
             ORDER BY a.published_at, a.id
             LIMIT $1",
        )
        .bind(maximum_items)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list runtime assignments requiring observation", error))?;
        rows.iter().map(map_assignment_row).collect()
    }

    pub(super) async fn list_active_runtime_assignments_after_repo(
        &self,
        after_target_uuid: Option<&str>,
        maximum_items: i64,
    ) -> DeployServiceResult<Vec<RuntimeAssignmentState>> {
        let rows = sqlx::query(
            "SELECT a.uuid AS assignment_uuid, t.uuid AS target_uuid, a.tenant_id,
                    t.node_uuid, t.environment, a.trigger_app_revision_id,
                    a.generation, a.snapshot_uuid, a.snapshot_sha256,
                    a.desired_state_sha256,
                    CAST(a.runtime_set_json AS TEXT) AS runtime_set_json, a.publish_status,
                    a.remote_assignment_uuid, a.attempt_count, a.lease_owner
             FROM deploy_web_node_target t
             INNER JOIN deploy_runtime_assignment a ON a.node_target_id = t.id
             WHERE t.uuid > $1
               AND t.status = 'ACTIVE' AND t.deleted_at IS NULL
               AND a.publish_status = 'PUBLISHED'
               AND NOT EXISTS (
                   SELECT 1 FROM deploy_runtime_assignment newer
                   WHERE newer.node_target_id = a.node_target_id
                     AND newer.generation > a.generation
               )
               AND EXISTS (
                   SELECT 1 FROM deploy_app_target_observation observation
                   WHERE observation.runtime_assignment_id = a.id
                     AND observation.state = 'ACTIVE'
               )
             ORDER BY t.uuid
             LIMIT $2",
        )
        .bind(after_target_uuid.unwrap_or_default())
        .bind(maximum_items)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list active runtime assignments for renewal", error))?;
        rows.iter().map(map_assignment_row).collect()
    }

    pub(super) async fn persist_runtime_observation_repo(
        &self,
        assignment_uuid: &str,
        observation: &RuntimeObservationEvidence,
        ingested_at: &str,
    ) -> DeployServiceResult<RuntimeObservationPersistenceResult> {
        let observed_at =
            normalize_timestamp(&observation.observed_at, "runtime observation source time")?;
        let ingested_at = normalize_timestamp(ingested_at, "runtime observation ingestion time")?;
        let mut transaction = begin_runtime_assignment_transaction(&self.pool).await?;
        let row = sqlx::query(
            "SELECT a.id AS runtime_assignment_id, a.node_target_id,
                    r.app_id, a.uuid AS assignment_uuid, t.uuid AS target_uuid, a.tenant_id,
                    t.node_uuid, t.environment, a.trigger_app_revision_id,
                    a.generation, a.snapshot_uuid, a.snapshot_sha256,
                    a.desired_state_sha256,
                    CAST(a.runtime_set_json AS TEXT) AS runtime_set_json, a.publish_status,
                    a.remote_assignment_uuid, a.attempt_count, a.lease_owner
             FROM deploy_runtime_assignment a
             INNER JOIN deploy_web_node_target t ON t.id = a.node_target_id
             LEFT JOIN deploy_app_revision r ON r.id = a.trigger_app_revision_id
             WHERE a.uuid = $1
             FOR UPDATE OF a",
        )
        .bind(assignment_uuid)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock runtime assignment observation", error))?
        .ok_or_else(|| DeployServiceError::not_found("runtime assignment not found"))?;
        let assignment = map_assignment_row(&row)?;
        observation.validate_for(&assignment)?;
        let runtime_assignment_id: i64 = row
            .try_get("runtime_assignment_id")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        let node_target_id: i64 = row
            .try_get("node_target_id")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        let app_id: Option<i64> = row
            .try_get("app_id")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;

        if let Some(existing) = sqlx::query(
            "SELECT runtime_assignment_id, state
             FROM deploy_app_target_observation
             WHERE remote_observation_uuid = $1",
        )
        .bind(&observation.observation_uuid)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("load idempotent runtime observation", error))?
        {
            let existing_assignment_id: i64 = existing
                .try_get("runtime_assignment_id")
                .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
            let existing_state: String = existing
                .try_get("state")
                .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
            if existing_assignment_id != runtime_assignment_id
                || existing_state != observation.state.as_str()
            {
                return Err(DeployServiceError::conflict(
                    "Web runtime observation UUID was already ingested with another identity",
                ));
            }
            transaction
                .commit()
                .await
                .map_err(|error| store_error("commit idempotent runtime observation", error))?;
            return Ok(RuntimeObservationPersistenceResult::default());
        }

        if sqlx::query(
            "SELECT id FROM deploy_app_target_observation
             WHERE runtime_assignment_id = $1 AND state = $2",
        )
        .bind(runtime_assignment_id)
        .bind(observation.state.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("load runtime observation state", error))?
        .is_some()
        {
            return Err(DeployServiceError::conflict(
                "runtime observation state was already ingested with another UUID",
            ));
        }

        let id = next_id(&self.id_generator)?;
        let uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_app_target_observation (
                id, uuid, tenant_id, app_id, site_revision_id, node_target_id,
                runtime_assignment_id, remote_observation_uuid, remote_assignment_uuid,
                generation, snapshot_uuid, snapshot_sha256, environment, state,
                node_version, reason_code, detail, observed_at, ingested_at, created_at
             ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                CAST($18 AS TIMESTAMPTZ),CAST($19 AS TIMESTAMPTZ),CAST($19 AS TIMESTAMPTZ)
             )",
        )
        .bind(id)
        .bind(uuid)
        .bind(observation.tenant_id)
        .bind(app_id)
        .bind(assignment.trigger_app_revision_id)
        .bind(node_target_id)
        .bind(runtime_assignment_id)
        .bind(&observation.observation_uuid)
        .bind(&observation.assignment_uuid)
        .bind(observation.generation as i64)
        .bind(&observation.snapshot_uuid)
        .bind(&observation.snapshot_sha256)
        .bind(observation.environment.as_str())
        .bind(observation.state.as_str())
        .bind(&observation.node_version)
        .bind(&observation.reason_code)
        .bind(&observation.detail)
        .bind(&observed_at)
        .bind(&ingested_at)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert runtime observation evidence", error))?;

        let revision_activated = if observation.state == RuntimeObservationState::Active {
            activate_site_revision_if_converged(
                &mut transaction,
                observation.tenant_id,
                app_id,
                assignment.trigger_app_revision_id,
                &ingested_at,
            )
            .await?
        } else {
            false
        };
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit runtime observation evidence", error))?;
        Ok(RuntimeObservationPersistenceResult {
            inserted: true,
            revision_activated,
        })
    }
}

#[async_trait]
impl DeployRuntimeAssignmentMutationPort for SqlxRuntimeAssignmentMutation {
    fn latest_runtime_assignment(&self) -> Option<&RuntimeAssignmentState> {
        self.latest.as_ref()
    }

    fn next_generation(&self) -> u64 {
        self.next_generation
    }

    async fn commit_without_change(self: Box<Self>) -> DeployServiceResult<()> {
        self.transaction
            .commit()
            .await
            .map_err(|error| store_error("commit deploy runtime assignment read", error))
    }

    async fn persist_runtime_assignment(
        self: Box<Self>,
        command: PersistRuntimeAssignmentCommand,
    ) -> DeployServiceResult<RuntimeAssignmentState> {
        let SqlxRuntimeAssignmentMutation {
            mut transaction,
            id_generator,
            target_id,
            tenant_id,
            latest: _,
            next_generation,
        } = *self;
        if next_generation > MAXIMUM_RUNTIME_GENERATION {
            return Err(DeployServiceError::conflict(
                "runtime assignment generation is exhausted",
            ));
        }
        if command
            .runtime_set
            .get("generation")
            .and_then(serde_json::Value::as_u64)
            != Some(next_generation)
        {
            return Err(DeployServiceError::conflict(
                "runtime assignment generation does not match its transaction reservation",
            ));
        }
        let id = next_id(&id_generator)?;
        let runtime_set_json = serde_json::to_string(&command.runtime_set)
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        let created_at =
            normalize_timestamp(&command.created_at, "runtime assignment creation time")?;
        sqlx::query(
            "INSERT INTO deploy_runtime_assignment (
                id, uuid, tenant_id, node_target_id, generation, snapshot_uuid,
                snapshot_sha256, desired_state_sha256, runtime_set_json, runtime_set_bytes,
                publish_status, attempt_count, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, CAST($9 AS JSONB), $10,
                 'PENDING', 0, CAST($11 AS TIMESTAMPTZ), CAST($11 AS TIMESTAMPTZ), 1
             )",
        )
        .bind(id)
        .bind(&command.assignment_uuid)
        .bind(tenant_id)
        .bind(target_id)
        .bind(next_generation as i64)
        .bind(&command.snapshot_uuid)
        .bind(&command.snapshot_sha256)
        .bind(&command.desired_state_sha256)
        .bind(&runtime_set_json)
        .bind(command.runtime_set_bytes as i64)
        .bind(&created_at)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy runtime assignment", error))?;
        sqlx::query(
            "UPDATE deploy_runtime_assignment
             SET publish_status = 'SUPERSEDED', lease_owner = NULL,
                 lease_expires_at = NULL, updated_at = CAST($1 AS TIMESTAMPTZ),
                 version = version + 1
             WHERE node_target_id = $2 AND generation < $3
               AND publish_status <> 'SUPERSEDED'",
        )
        .bind(&created_at)
        .bind(target_id)
        .bind(next_generation as i64)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("supersede older deploy runtime assignments", error))?;
        let assignment = runtime_assignment_by_uuid(&mut transaction, &command.assignment_uuid)
            .await?
            .ok_or_else(|| {
                DeployServiceError::Internal(
                    "persisted runtime assignment could not be reloaded".to_owned(),
                )
            })?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit deploy runtime assignment", error))?;
        Ok(assignment)
    }
}

async fn begin_runtime_assignment_transaction(
    pool: &PgPool,
) -> DeployServiceResult<Transaction<'static, Postgres>> {
    pool.begin()
        .await
        .map_err(|error| store_error("begin deploy runtime assignment transaction", error))
}

async fn latest_runtime_assignment_for_target(
    transaction: &mut Transaction<'static, Postgres>,
    target_id: i64,
) -> DeployServiceResult<Option<RuntimeAssignmentState>> {
    let row = sqlx::query(
        "SELECT a.uuid AS assignment_uuid, t.uuid AS target_uuid, a.tenant_id,
                t.node_uuid, t.environment, a.trigger_app_revision_id,
                a.generation, a.snapshot_uuid, a.snapshot_sha256,
                a.desired_state_sha256,
                CAST(a.runtime_set_json AS TEXT) AS runtime_set_json, a.publish_status,
                a.remote_assignment_uuid, a.attempt_count, a.lease_owner
         FROM deploy_runtime_assignment a
         INNER JOIN deploy_web_node_target t ON t.id = a.node_target_id
         WHERE a.node_target_id = $1
         ORDER BY a.generation DESC
         LIMIT 1",
    )
    .bind(target_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| store_error("load locked deploy runtime assignment", error))?;
    row.as_ref().map(map_assignment_row).transpose()
}

async fn runtime_assignment_by_uuid(
    transaction: &mut Transaction<'static, Postgres>,
    assignment_uuid: &str,
) -> DeployServiceResult<Option<RuntimeAssignmentState>> {
    let row = sqlx::query(
        "SELECT a.uuid AS assignment_uuid, t.uuid AS target_uuid, a.tenant_id,
                t.node_uuid, t.environment, a.trigger_app_revision_id,
                a.generation, a.snapshot_uuid, a.snapshot_sha256,
                a.desired_state_sha256,
                CAST(a.runtime_set_json AS TEXT) AS runtime_set_json, a.publish_status,
                a.remote_assignment_uuid, a.attempt_count, a.lease_owner
         FROM deploy_runtime_assignment a
         INNER JOIN deploy_web_node_target t ON t.id = a.node_target_id
         WHERE a.uuid = $1",
    )
    .bind(assignment_uuid)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| store_error("reload deploy runtime assignment", error))?;
    row.as_ref().map(map_assignment_row).transpose()
}

async fn activate_site_revision_if_converged(
    transaction: &mut Transaction<'static, Postgres>,
    tenant_id: i64,
    app_id: Option<i64>,
    site_revision_id: Option<i64>,
    activated_at: &str,
) -> DeployServiceResult<bool> {
    let (Some(app_id), Some(site_revision_id)) = (app_id, site_revision_id) else {
        return Ok(false);
    };
    let counts = sqlx::query(
        "SELECT COUNT(*) AS assignment_count,
                SUM(CASE WHEN a.publish_status = 'PUBLISHED' AND EXISTS (
                    SELECT 1 FROM deploy_app_target_observation o
                    WHERE o.runtime_assignment_id = a.id AND o.state = 'ACTIVE'
                ) THEN 1 ELSE 0 END) AS active_count
         FROM deploy_runtime_assignment a
         WHERE a.tenant_id = $1 AND a.trigger_app_revision_id = $2",
    )
    .bind(tenant_id)
    .bind(site_revision_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| store_error("evaluate runtime observation quorum", error))?;
    let assignment_count: i64 = counts
        .try_get("assignment_count")
        .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
    let active_count: i64 = counts
        .try_get::<Option<i64>, _>("active_count")
        .map_err(|error| DeployServiceError::Internal(error.to_string()))?
        .unwrap_or(0);
    if assignment_count == 0 || active_count != assignment_count {
        return Ok(false);
    }

    let updated = sqlx::query(
        "UPDATE deploy_app
         SET current_revision_id = $3, updated_at = CAST($4 AS TIMESTAMPTZ), version = version + 1
         WHERE id = $1 AND tenant_id = $2 AND desired_revision_id = $3
           AND (current_revision_id IS NULL OR current_revision_id <> $3)",
    )
    .bind(app_id)
    .bind(tenant_id)
    .bind(site_revision_id)
    .bind(activated_at)
    .execute(&mut **transaction)
    .await
    .map_err(|error| store_error("activate converged app revision", error))?;
    Ok(updated.rows_affected() == 1)
}

fn map_assignment_row(row: &PgRow) -> DeployServiceResult<RuntimeAssignmentState> {
    let generation: i64 = row
        .try_get("generation")
        .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
    let runtime_set_json: String = row
        .try_get("runtime_set_json")
        .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
    let publish_status: String = row
        .try_get("publish_status")
        .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
    Ok(RuntimeAssignmentState {
        assignment_uuid: row
            .try_get("assignment_uuid")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        target_uuid: row
            .try_get("target_uuid")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        tenant_id: row
            .try_get("tenant_id")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        node_uuid: row
            .try_get("node_uuid")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        environment: parse_runtime_environment(
            &row.try_get::<String, _>("environment")
                .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        )?,
        trigger_app_revision_id: row
            .try_get("trigger_app_revision_id")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        generation: generation.try_into().map_err(|_| {
            DeployServiceError::Internal("stored runtime generation is negative".to_owned())
        })?,
        snapshot_uuid: row
            .try_get("snapshot_uuid")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        snapshot_sha256: row
            .try_get("snapshot_sha256")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        desired_state_sha256: row
            .try_get("desired_state_sha256")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        runtime_set: serde_json::from_str(&runtime_set_json)
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        publish_status: parse_publish_status(&publish_status)?,
        remote_assignment_uuid: row
            .try_get("remote_assignment_uuid")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        attempt_count: row
            .try_get("attempt_count")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
        lease_owner: row
            .try_get("lease_owner")
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?,
    })
}

fn parse_publish_status(value: &str) -> DeployServiceResult<RuntimeAssignmentPublishStatus> {
    match value {
        "PENDING" => Ok(RuntimeAssignmentPublishStatus::Pending),
        "PUBLISHING" => Ok(RuntimeAssignmentPublishStatus::Publishing),
        "PUBLISHED" => Ok(RuntimeAssignmentPublishStatus::Published),
        "FAILED" => Ok(RuntimeAssignmentPublishStatus::Failed),
        "SUPERSEDED" => Ok(RuntimeAssignmentPublishStatus::Superseded),
        _ => Err(DeployServiceError::Internal(format!(
            "unknown runtime assignment publish status {value}"
        ))),
    }
}

fn parse_runtime_environment(
    value: &str,
) -> DeployServiceResult<sdkwork_deploy_runtime_compiler::RuntimeEnvironment> {
    sdkwork_deploy_runtime_compiler::RuntimeEnvironment::parse(value)
        .map_err(DeployServiceError::Internal)
}
