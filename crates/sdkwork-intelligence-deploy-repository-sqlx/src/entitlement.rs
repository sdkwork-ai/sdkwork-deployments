//! Entitlement consumption enforcement and backend build fleet read models:
//! tenant usage aggregates, Commerce-backed projection listing, build queue
//! administration, and runner health (TECH §4.6 / §8).

use sdkwork_deploy_contract::{
    BuildQueueItemResponse, BuildQueuePage, DeployServiceError, DeployServiceResult,
    EntitlementProjectionPage, EntitlementProjectionResponse, RunnerHealthPage,
    RunnerHealthResponse,
};
use sqlx::{AssertSqlSafe, Row};

use crate::support::{optional_datetime, pagination, required_datetime, store_error};
use crate::DeployRepository;

impl DeployRepository {
    /// Current tenant usage for one entitlement dimension. All aggregates are
    /// tenant-scoped and bounded; they are read-only evidence for enforcement.
    pub(super) async fn entitlement_usage_repo(
        &self,
        tenant_id: i64,
        dimension: &str,
    ) -> DeployServiceResult<i64> {
        // Traffic dimensions aggregate the tenant daily rollup (SaaS
        // billing); the daily tables carry no deleted_at column, so they
        // are handled before the generic live-table match.
        if matches!(
            dimension,
            "traffic.requests" | "traffic.ingress_bytes" | "traffic.egress_bytes"
        ) {
            let row = sqlx::query(
                "SELECT COALESCE(SUM(quantity), 0) AS usage_value
                 FROM deploy_tenant_usage_daily
                 WHERE tenant_id = $1 AND dimension = $2 AND finalization_status = 'PENDING'",
            )
            .bind(tenant_id)
            .bind(dimension)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("traffic entitlement usage aggregate", error))?;
            let value: i64 = row.try_get("usage_value").unwrap_or(0);
            return Ok(value);
        }
        let (aggregate_sql, table, status_filter) = match dimension {
            "active_apps" => ("COUNT(*)", "deploy_app", "AND app_status <> 'ARCHIVED'"),
            "platform_targets" => (
                "COUNT(*)",
                "deploy_app_platform_target",
                "AND target_status <> 'ARCHIVED'",
            ),
            "build_concurrency" => (
                "COUNT(*)",
                "deploy_build",
                "AND build_status NOT IN ('SUCCEEDED','FAILED','CANCELLED','TIMED_OUT')",
            ),
            "package_storage_bytes" => {
                ("COALESCE(SUM(package_size_bytes), 0)", "deploy_package", "")
            }
            "release_count" => (
                "COUNT(*)",
                "deploy_release",
                "AND release_status IS NOT NULL",
            ),
            "deployment_count" => (
                "COUNT(*)",
                "deploy_deployment",
                "AND deployment_status <> 'CANCELLED'",
            ),
            "channel_count" => (
                "COUNT(*)",
                "deploy_release_channel",
                "AND channel_status <> 'ARCHIVED'",
            ),
            _ => {
                return Err(DeployServiceError::validation(format!(
                    "unknown entitlement dimension {dimension}"
                )))
            }
        };
        let query = format!(
            "SELECT {aggregate_sql} AS usage_value
             FROM {table}
             WHERE tenant_id = $1 AND deleted_at IS NULL {status_filter}"
        );
        let row = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("entitlement usage aggregate", error))?;
        let value: i64 = row.try_get("usage_value").unwrap_or(0);
        Ok(value)
    }

    /// Backend platform management surface: list Commerce-backed entitlement
    /// projections across tenants (bounded by optional tenant scope).
    pub(super) async fn list_entitlement_projections_repo(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<EntitlementProjectionPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        // The tenant predicate and the page window must never share a placeholder
        // number. PostgreSQL resolves a repeated `$n` by type rather than by
        // intent, so a shared `$1` silently becomes `LIMIT <tenant_id> OFFSET
        // <page_size>`: for tenant 7 with page_size 20 that reads rows 21..28 of
        // the tenant, so the tenant-scoped page comes back empty (or wrong)
        // while the independently counted total still reports rows. No error is
        // raised, which is what made this survive review.
        let (filter, paging, bind) = match tenant_id {
            Some(_tenant_id) => ("WHERE tenant_id = $1", "LIMIT $2 OFFSET $3", true),
            None => ("", "LIMIT $1 OFFSET $2", false),
        };
        let count_query =
            format!("SELECT COUNT(*) AS total FROM deploy_tenant_entitlement_projection {filter}");
        let mut count = sqlx::query(AssertSqlSafe(&*count_query));
        if bind {
            count = count.bind(tenant_id.unwrap_or(0));
        }
        let count_row = count
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count entitlement projections", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let list_query = format!(
            "SELECT uuid, tenant_id, source_system, source_subscription_uuid, source_revision,
                    plan_key, entitlements_json, effective_at, expires_at, projection_status,
                    created_at, updated_at
             FROM deploy_tenant_entitlement_projection {filter}
             ORDER BY updated_at DESC, id DESC {paging}"
        );
        let mut list = sqlx::query(AssertSqlSafe(&*list_query));
        if bind {
            list = list
                .bind(tenant_id.unwrap_or(0))
                .bind(page_size)
                .bind(offset);
        } else {
            list = list.bind(page_size).bind(offset);
        }
        let rows = list
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list entitlement projections", error))?;

        let items = rows
            .iter()
            .map(map_entitlement_projection_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EntitlementProjectionPage {
            items,
            total,
            page,
            page_size,
        })
    }

    /// Backend fleet administration: queued/preparing builds waiting for or
    /// being claimed by runners, oldest first.
    pub(super) async fn list_build_queue_repo(
        &self,
        tenant_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildQueuePage> {
        let (page, page_size, offset) = pagination(page, page_size);
        // See `list_entitlement_projections_repo`: the tenant predicate and the
        // page window must not share a placeholder number.
        let (filter, paging, bind) = match tenant_id {
            Some(_tenant_id) => (
                "WHERE b.tenant_id = $1 AND b.build_status IN ('QUEUED','PREPARING') AND b.deleted_at IS NULL",
                "LIMIT $2 OFFSET $3",
                true,
            ),
            None => (
                "WHERE b.build_status IN ('QUEUED','PREPARING') AND b.deleted_at IS NULL",
                "LIMIT $1 OFFSET $2",
                false,
            ),
        };
        let count_query = format!("SELECT COUNT(*) AS total FROM deploy_build b {filter}");
        let mut count = sqlx::query(AssertSqlSafe(&*count_query));
        if bind {
            count = count.bind(tenant_id.unwrap_or(0));
        }
        let count_row = count
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count build queue", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let list_query = format!(
            "SELECT b.uuid, a.uuid AS app_uuid, t.uuid AS target_uuid, b.build_number,
                    b.build_status, b.runner_node_uuid, b.created_at, b.updated_at
             FROM deploy_build b
             JOIN deploy_app a ON a.id = b.app_id
             JOIN deploy_app_platform_target t ON t.id = b.platform_target_id
             {filter}
             ORDER BY b.created_at ASC, b.id ASC {paging}"
        );
        let mut list = sqlx::query(AssertSqlSafe(&*list_query));
        if bind {
            list = list
                .bind(tenant_id.unwrap_or(0))
                .bind(page_size)
                .bind(offset);
        } else {
            list = list.bind(page_size).bind(offset);
        }
        let rows = list
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list build queue", error))?;

        let items = rows
            .iter()
            .map(|row| {
                Ok(BuildQueueItemResponse {
                    id: row.try_get("uuid").unwrap_or_default(),
                    app_id: row.try_get("app_uuid").unwrap_or_default(),
                    platform_target_id: row.try_get("target_uuid").unwrap_or_default(),
                    build_number: row.try_get("build_number").unwrap_or(0),
                    build_status: row.try_get("build_status").unwrap_or_default(),
                    runner_node_uuid: row.try_get("runner_node_uuid").ok(),
                    created_at: required_datetime(row, "created_at")?,
                    updated_at: required_datetime(row, "updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, DeployServiceError>>()?;
        Ok(BuildQueuePage {
            items,
            total,
            page,
            page_size,
        })
    }

    /// Backend fleet administration: runner liveness and workload summary
    /// grouped by runner node.
    pub(super) async fn list_runner_health_repo(
        &self,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<RunnerHealthPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(DISTINCT runner_node_uuid) AS total
             FROM deploy_build WHERE runner_node_uuid IS NOT NULL",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count runners", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT runner_node_uuid,
                    MAX(runner_version) AS runner_version,
                    MAX(updated_at) AS last_seen_at,
                    COUNT(*) FILTER (WHERE build_status = 'SUCCEEDED') AS builds_completed,
                    COUNT(*) FILTER (WHERE build_status IN ('FAILED','TIMED_OUT')) AS builds_failed,
                    COUNT(*) FILTER (WHERE build_status NOT IN
                        ('SUCCEEDED','FAILED','CANCELLED','TIMED_OUT')) AS active_builds
             FROM deploy_build
             WHERE runner_node_uuid IS NOT NULL
             GROUP BY runner_node_uuid
             ORDER BY last_seen_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list runner health", error))?;

        let items = rows
            .iter()
            .map(|row| {
                Ok(RunnerHealthResponse {
                    runner_node_uuid: row.try_get("runner_node_uuid").unwrap_or_default(),
                    runner_version: row.try_get("runner_version").ok(),
                    last_seen_at: required_datetime(row, "last_seen_at")?,
                    builds_completed: row.try_get("builds_completed").unwrap_or(0),
                    builds_failed: row.try_get("builds_failed").unwrap_or(0),
                    active_builds: row.try_get("active_builds").unwrap_or(0),
                })
            })
            .collect::<Result<Vec<_>, DeployServiceError>>()?;
        Ok(RunnerHealthPage {
            items,
            total,
            page,
            page_size,
        })
    }
}

fn map_entitlement_projection_row(
    row: &sqlx::postgres::PgRow,
) -> Result<EntitlementProjectionResponse, DeployServiceError> {
    let effective_at = required_datetime(row, "effective_at")?;
    let expires_at = optional_datetime(row, "expires_at")?;
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    Ok(EntitlementProjectionResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        tenant_id: row.try_get("tenant_id").unwrap_or(0),
        source_system: row.try_get("source_system").unwrap_or_default(),
        source_subscription_uuid: row.try_get("source_subscription_uuid").unwrap_or_default(),
        source_revision: row.try_get("source_revision").ok(),
        plan_key: row.try_get("plan_key").ok(),
        entitlements: row.try_get("entitlements_json").unwrap_or_default(),
        effective_at,
        expires_at,
        projection_status: row.try_get("projection_status").unwrap_or_default(),
        created_at,
        updated_at,
    })
}
