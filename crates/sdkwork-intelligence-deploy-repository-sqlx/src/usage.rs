//! Usage metering repository operations (TECH §4.6): append-only usage facts
//! with deduplication identity, traffic batch ingest from Web Server nodes,
//! and the entitlement projection read model.

use sdkwork_deploy_contract::{
    DeployServiceError, DeployServiceResult, TrafficUsageAppTotal, TrafficUsageDailyPoint,
    TrafficUsageStatistics, TrafficUsageStatisticsQuery, TrafficUsageTenantTotal,
    TrafficUsageTotal, UsageEventAttribution, UsageEventIngestItem, UsageEventPage,
    UsageEventQuery, UsageEventResponse, UsageIngestResult,
};
use sdkwork_intelligence_deploy_service::repository::InsertUsageEventCommand;
use sqlx::{postgres::PgRow, AssertSqlSafe, Row};

use crate::support::{
    datetime_from_row, is_unique_violation, new_uuid, next_id, now_rfc3339, pagination, store_error,
};
use crate::DeployRepository;

impl DeployRepository {
    /// Records one usage fact. Idempotent on the tenant deduplication key;
    /// duplicate delivery returns the existing fact.
    pub(super) async fn insert_usage_event_repo(
        &self,
        command: &InsertUsageEventCommand,
    ) -> DeployServiceResult<UsageEventResponse> {
        if let Some(existing) = self
            .find_usage_event_by_dedup_key_repo(command.tenant_id, &command.deduplication_key)
            .await?
        {
            return Ok(existing);
        }
        let event_id = next_id(self.id_generator())?;
        let event_uuid = new_uuid();
        let now = now_rfc3339();
        let attribution = command
            .attribution
            .clone()
            .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
        let result = sqlx::query(
            "INSERT INTO deploy_usage_event
                (id, uuid, tenant_id, organization_id, app_id, binding_id, period_start,
                 dimension, quantity, unit, source_target_uuid, source_window_id,
                 deduplication_key, attribution_json, observed_at, ingested_at, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, CAST($7 AS TIMESTAMPTZ), $8, $9, $10, $11, $12,
                     $13, $14, CAST($15 AS TIMESTAMPTZ), CAST($15 AS TIMESTAMPTZ),
                     CAST($15 AS TIMESTAMPTZ))
             ON CONFLICT (tenant_id, deduplication_key) DO NOTHING
             RETURNING uuid",
        )
        .bind(event_id)
        .bind(&event_uuid)
        .bind(command.tenant_id)
        .bind(command.organization_id)
        .bind(command.app_id)
        .bind(command.binding_id)
        .bind(&command.period_start)
        .bind(&command.dimension)
        .bind(command.quantity)
        .bind(&command.unit)
        .bind(command.source_target_uuid.as_deref())
        .bind(command.source_window_id.as_deref())
        .bind(&command.deduplication_key)
        .bind(&attribution)
        .bind(&now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            if is_unique_violation(&error) {
                DeployServiceError::conflict("usage event deduplication conflict")
            } else {
                store_error("insert deploy_usage_event", error)
            }
        })?;

        let Some(row) = result else {
            return self
                .find_usage_event_by_dedup_key_repo(command.tenant_id, &command.deduplication_key)
                .await?
                .ok_or_else(|| {
                    DeployServiceError::Internal(
                        "usage event disappeared after concurrent insertion".into(),
                    )
                });
        };
        let inserted_uuid: String = row.try_get("uuid").map_err(|error| {
            DeployServiceError::Internal(format!("read usage event uuid: {error}"))
        })?;
        self.retrieve_usage_event_repo(command.tenant_id, &inserted_uuid)
            .await
    }

    /// Batch-ingest traffic usage events from a Web Server node. Every event
    /// is attributed: the binding uuid resolves to the binding internal id,
    /// the app id and the owning tenant (the node may not know the tenant
    /// for website-runtime-served traffic); site uuid resolves the site when
    /// no binding is present. Events without any deploy reference keep their
    /// submitted tenant id (0 = unmanaged, platform-attributed).
    pub(super) async fn insert_usage_events_batch_repo(
        &self,
        events: &[UsageEventIngestItem],
    ) -> DeployServiceResult<UsageIngestResult> {
        let mut result = UsageIngestResult::default();
        for event in events {
            if event.dimension.is_empty()
                || event.quantity < 0
                || event.deduplication_key.is_empty()
            {
                result.rejected += 1;
                continue;
            }
            let (tenant_id, app_id, binding_id, app_uuid) =
                self.resolve_usage_attribution(event).await?;
            // Enrich the app attribution when the node could not attribute
            // it (website-runtime-served traffic): the site's owning app is
            // known to the control plane.
            let mut attribution = event.attribution.clone();
            if attribution.app_id.is_none() {
                if let Some(app_uuid) = app_uuid {
                    attribution.app_id = Some(app_uuid);
                }
            }
            let command = InsertUsageEventCommand {
                tenant_id,
                organization_id: event.organization_id,
                app_id,
                binding_id,
                period_start: event.period_start.clone(),
                dimension: event.dimension.clone(),
                quantity: event.quantity,
                unit: event.unit.clone(),
                source_target_uuid: event.binding_uuid.clone(),
                source_window_id: None,
                deduplication_key: event.deduplication_key.clone(),
                attribution: Some(
                    serde_json::to_value(&attribution)
                        .unwrap_or_else(|_| serde_json::Value::Object(Default::default())),
                ),
            };
            // Pre-check the dedup key so replayed batches are reported as
            // duplicates instead of ingested (the insert itself is still
            // idempotent under concurrency).
            match self
                .find_usage_event_by_dedup_key_repo(tenant_id, &command.deduplication_key)
                .await
            {
                Ok(Some(_)) => result.duplicates += 1,
                Ok(None) => match self.insert_usage_event_repo(&command).await {
                    Ok(_) => result.ingested += 1,
                    Err(DeployServiceError::Conflict(_)) => result.duplicates += 1,
                    Err(_) => result.rejected += 1,
                },
                Err(_) => result.rejected += 1,
            }
        }
        Ok(result)
    }

    /// Resolve a submitted event's tenant/app/binding internal ids and the
    /// owning app uuid. The binding uuid is authoritative (it carries the
    /// owning app and tenant); app uuid is a secondary resolver.
    async fn resolve_usage_attribution(
        &self,
        event: &UsageEventIngestItem,
    ) -> DeployServiceResult<(i64, Option<i64>, Option<i64>, Option<String>)> {
        if let Some(binding_uuid) = event.binding_uuid.as_deref() {
            let row = sqlx::query(
                "SELECT b.tenant_id, b.app_id, b.id AS binding_id,
                        (SELECT app.uuid FROM deploy_app app
                         WHERE app.id = b.app_id AND app.deleted_at IS NULL
                         LIMIT 1) AS app_uuid
                 FROM deploy_app_binding b
                 WHERE b.uuid = $1 AND b.deleted_at IS NULL
                 LIMIT 1",
            )
            .bind(binding_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve usage binding attribution", error))?;
            if let Some(row) = row {
                let tenant_id: i64 = row.try_get("tenant_id").map_err(|error| {
                    DeployServiceError::Internal(format!("read usage binding tenant: {error}"))
                })?;
                let app_id: Option<i64> = row.try_get("app_id").ok();
                let binding_id: Option<i64> = row.try_get("binding_id").ok();
                let app_uuid: Option<String> = row.try_get("app_uuid").ok();
                return Ok((tenant_id, app_id, binding_id, app_uuid));
            }
        }
        if let Some(app_uuid) = event.app_uuid.as_deref() {
            let row = sqlx::query(
                "SELECT tenant_id, id,
                        (SELECT app.uuid FROM deploy_app app
                         WHERE app.id = deploy_app.id AND app.deleted_at IS NULL
                         LIMIT 1) AS app_uuid
                 FROM deploy_app
                 WHERE uuid = $1 AND deleted_at IS NULL LIMIT 1",
            )
            .bind(app_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve usage app attribution", error))?;
            if let Some(row) = row {
                let tenant_id: i64 = row.try_get("tenant_id").map_err(|error| {
                    DeployServiceError::Internal(format!("read usage site tenant: {error}"))
                })?;
                let app_id: Option<i64> = row.try_get("id").ok();
                let app_uuid: Option<String> = row.try_get("app_uuid").ok();
                return Ok((tenant_id, app_id, None, app_uuid));
            }
        }
        Ok((event.tenant_id, None, None, None))
    }

    pub(super) async fn find_usage_event_by_dedup_key_repo(
        &self,
        tenant_id: i64,
        deduplication_key: &str,
    ) -> DeployServiceResult<Option<UsageEventResponse>> {
        let row = sqlx::query(
            "SELECT u.uuid, u.tenant_id, s.uuid AS app_uuid, b.uuid AS binding_uuid,
                    u.period_start, u.dimension, u.quantity, u.unit,
                    u.source_target_uuid, u.source_window_id, u.deduplication_key,
                    u.attribution_json, u.observed_at, u.created_at
             FROM deploy_usage_event u
             LEFT JOIN deploy_app s ON s.id = u.app_id
             LEFT JOIN deploy_app_binding b ON b.id = u.binding_id
             WHERE u.tenant_id = $1 AND u.deduplication_key = $2
             ORDER BY u.created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(deduplication_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("find deploy_usage_event by dedup key", error))?;

        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(map_usage_event_row(&row)?))
    }

    pub(super) async fn retrieve_usage_event_repo(
        &self,
        tenant_id: i64,
        event_id: &str,
    ) -> DeployServiceResult<UsageEventResponse> {
        let row = sqlx::query(
            "SELECT u.uuid, u.tenant_id, s.uuid AS app_uuid, b.uuid AS binding_uuid,
                    u.period_start, u.dimension, u.quantity, u.unit,
                    u.source_target_uuid, u.source_window_id, u.deduplication_key,
                    u.attribution_json, u.observed_at, u.created_at
             FROM deploy_usage_event u
             LEFT JOIN deploy_app s ON s.id = u.app_id
             LEFT JOIN deploy_app_binding b ON b.id = u.binding_id
             WHERE u.tenant_id = $1 AND u.uuid = $2",
        )
        .bind(tenant_id)
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_usage_event", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("usage event not found"));
        };
        map_usage_event_row(&row)
    }

    pub(super) async fn list_usage_events_repo(
        &self,
        tenant_id: i64,
        query: &UsageEventQuery,
    ) -> DeployServiceResult<UsageEventPage> {
        let (page, page_size, offset) = pagination(query.page, query.page_size);
        let binding_id = query.binding_id.as_deref().unwrap_or("");
        let dimension = query.dimension.as_deref().unwrap_or("");
        let hostname = query.hostname.as_deref().unwrap_or("");
        let server_ip = query.server_ip.as_deref().unwrap_or("");
        let app_id = query.app_id.as_deref().unwrap_or("");
        let since = query.since.as_deref().unwrap_or("");
        let until = query.until.as_deref().unwrap_or("");
        let predicate = "u.tenant_id = $1
            AND ($2 = '' OR b.uuid = $2)
            AND ($3 = '' OR u.dimension = $3)
            AND ($4 = '' OR u.attribution_json->>'hostname' = $4)
            AND ($5 = '' OR u.attribution_json->>'serverIp' = $5)
            AND ($6 = '' OR u.attribution_json->>'appId' = $6)
            AND ($7 = '' OR u.period_start >= CAST($7 AS TIMESTAMPTZ))
            AND ($8 = '' OR u.period_start < CAST($8 AS TIMESTAMPTZ))";
        let count_sql = format!(
            "SELECT COUNT(*) AS total FROM deploy_usage_event u
             LEFT JOIN deploy_app_binding b ON b.id = u.binding_id
             WHERE {predicate}"
        );
        let count_row = sqlx::query(AssertSqlSafe(&*count_sql))
            .bind(tenant_id)
            .bind(binding_id)
            .bind(dimension)
            .bind(hostname)
            .bind(server_ip)
            .bind(app_id)
            .bind(since)
            .bind(until)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count deploy_usage_event", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let list_sql = format!(
            "SELECT u.uuid, u.tenant_id, s.uuid AS app_uuid, b.uuid AS binding_uuid,
                    u.period_start, u.dimension, u.quantity, u.unit,
                    u.source_target_uuid, u.source_window_id, u.deduplication_key,
                    u.attribution_json, u.observed_at, u.created_at
             FROM deploy_usage_event u
             LEFT JOIN deploy_app s ON s.id = u.app_id
             LEFT JOIN deploy_app_binding b ON b.id = u.binding_id
             WHERE {predicate}
             ORDER BY u.period_start DESC, u.id DESC LIMIT $9 OFFSET $10"
        );
        let rows = sqlx::query(AssertSqlSafe(&*list_sql))
            .bind(tenant_id)
            .bind(binding_id)
            .bind(dimension)
            .bind(hostname)
            .bind(server_ip)
            .bind(app_id)
            .bind(since)
            .bind(until)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list deploy_usage_event", error))?;

        let items = rows
            .iter()
            .map(map_usage_event_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(UsageEventPage {
            items,
            total,
            page,
            page_size,
        })
    }
    /// Aggregate traffic usage over a closed date window.
    ///
    /// `tenant_id: None` reads **every** tenant and additionally fills the
    /// per-tenant breakdown (the platform-wide view the Web Server operations
    /// surface needs); `Some(id)` reads exactly one tenant and leaves that
    /// breakdown empty, because repeating the caller's own totals once per
    /// dimension is noise rather than information.
    ///
    /// Every view aggregates `deploy_usage_event` — the append-only facts —
    /// rather than the daily rollups, so the totals, the daily series, and the
    /// per-app breakdown cannot disagree with each other merely because the
    /// reconciliation job has not run since the last window closed.
    ///
    /// The window is **half-open** (`date_from <= day < date_to`) and anchored
    /// to UTC explicitly. Relying on the session `TimeZone` here would make the
    /// same query mean different windows on differently configured servers,
    /// and a closed-both-ends window would double-count the boundary day in
    /// every consecutive pair.
    pub(super) async fn usage_statistics_repo(
        &self,
        tenant_id: Option<i64>,
        query: &TrafficUsageStatisticsQuery,
    ) -> DeployServiceResult<TrafficUsageStatistics> {
        let top_apps = query.top_apps.clamp(1, MAX_TRAFFIC_STATISTICS_APPS);
        let dimension = query.dimension.as_deref().unwrap_or("");
        let predicate = "($1::bigint IS NULL OR tenant_id = $1)
            AND ($2 = '' OR dimension = $2)
            AND period_start >= ($3::date)::timestamp AT TIME ZONE 'UTC'
            AND period_start < ($4::date)::timestamp AT TIME ZONE 'UTC'";

        // Every query of one read shares a transaction that carries a server-side
        // statement timeout. Two reasons, and both are about the *kernel* rather
        // than this read:
        //
        // * This pool is the process-shared Deploy pool the control plane and the
        //   edge's own lookups draw from, so an unbounded aggregate on a busy
        //   installation would hold a connection they need. The bound is set on
        //   the server because dropping the client future — a cancelled request —
        //   would leave the backend running and the connection still held.
        // * One snapshot for the four aggregates keeps them mutually consistent:
        //   facts are append-only, but a batch ingested mid-read would otherwise
        //   land in the daily series and not in the totals.
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin traffic usage statistics", error))?;
        sqlx::query(AssertSqlSafe(&*format!(
            "SET LOCAL statement_timeout = {TRAFFIC_STATISTICS_STATEMENT_TIMEOUT_MS}"
        )))
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("bound the traffic usage statistics read", error))?;

        let totals_sql = format!(
            "SELECT dimension, COALESCE(SUM(quantity), 0) AS quantity, MIN(unit) AS unit
             FROM deploy_usage_event
             WHERE {predicate}
             GROUP BY dimension
             ORDER BY dimension"
        );
        let totals_rows = sqlx::query(AssertSqlSafe(&*totals_sql))
            .bind(tenant_id)
            .bind(dimension)
            .bind(&query.date_from)
            .bind(&query.date_to)
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| store_error("aggregate deploy_usage_event totals", error))?;
        let totals = totals_rows
            .iter()
            .map(|row| {
                Ok(TrafficUsageTotal {
                    dimension: row
                        .try_get("dimension")
                        .map_err(|error| read_error("dimension", error))?,
                    quantity: row
                        .try_get("quantity")
                        .map_err(|error| read_error("quantity", error))?,
                    unit: row
                        .try_get("unit")
                        .unwrap_or_else(|_| String::new()),
                })
            })
            .collect::<Result<Vec<_>, DeployServiceError>>()?;

        let daily_sql = format!(
            "SELECT to_char(period_start AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS usage_date,
                    dimension,
                    COALESCE(SUM(quantity), 0) AS quantity
             FROM deploy_usage_event
             WHERE {predicate}
             GROUP BY usage_date, dimension
             ORDER BY usage_date, dimension"
        );
        let daily_rows = sqlx::query(AssertSqlSafe(&*daily_sql))
            .bind(tenant_id)
            .bind(dimension)
            .bind(&query.date_from)
            .bind(&query.date_to)
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| store_error("aggregate deploy_usage_event daily series", error))?;
        let daily = daily_rows
            .iter()
            .map(|row| {
                Ok(TrafficUsageDailyPoint {
                    usage_date: row
                        .try_get("usage_date")
                        .map_err(|error| read_error("usage_date", error))?,
                    dimension: row
                        .try_get("dimension")
                        .map_err(|error| read_error("dimension", error))?,
                    quantity: row
                        .try_get("quantity")
                        .map_err(|error| read_error("quantity", error))?,
                })
            })
            .collect::<Result<Vec<_>, DeployServiceError>>()?;

        // The bound applies to *attributed* apps only; the unattributed bucket
        // is unioned back unconditionally. Bounding the whole set instead would
        // let a busy drift bucket push a real app out of the list, and the
        // breakdown would then no longer sum back to the totals.
        let apps_sql = format!(
            "WITH facts AS (
                 SELECT app_id, dimension, quantity, unit
                 FROM deploy_usage_event
                 WHERE {predicate}
             ), ranked AS (
                 SELECT app_id, COALESCE(SUM(quantity), 0) AS total
                 FROM facts
                 WHERE app_id IS NOT NULL
                 GROUP BY app_id
                 ORDER BY total DESC, app_id
                 LIMIT $5
             )
             SELECT a.uuid AS app_uuid, a.slug AS app_slug, f.dimension AS dimension,
                    COALESCE(SUM(f.quantity), 0) AS quantity, MIN(f.unit) AS unit
             FROM facts f
             JOIN ranked r ON r.app_id = f.app_id
             LEFT JOIN deploy_app a ON a.id = f.app_id
             GROUP BY a.uuid, a.slug, f.dimension
             UNION ALL
             SELECT NULL::varchar AS app_uuid, NULL::varchar AS app_slug,
                    f.dimension AS dimension,
                    COALESCE(SUM(f.quantity), 0) AS quantity, MIN(f.unit) AS unit
             FROM facts f
             WHERE f.app_id IS NULL
             GROUP BY f.dimension
             ORDER BY app_slug NULLS LAST, dimension"
        );
        let apps_rows = sqlx::query(AssertSqlSafe(&*apps_sql))
            .bind(tenant_id)
            .bind(dimension)
            .bind(&query.date_from)
            .bind(&query.date_to)
            .bind(top_apps)
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| store_error("aggregate deploy_usage_event app breakdown", error))?;
        let apps = apps_rows
            .iter()
            .map(|row| {
                Ok(TrafficUsageAppTotal {
                    app_uuid: row.try_get("app_uuid").ok(),
                    app_slug: row.try_get("app_slug").ok(),
                    dimension: row
                        .try_get("dimension")
                        .map_err(|error| read_error("dimension", error))?,
                    quantity: row
                        .try_get("quantity")
                        .map_err(|error| read_error("quantity", error))?,
                    unit: row
                        .try_get("unit")
                        .unwrap_or_else(|_| String::new()),
                })
            })
            .collect::<Result<Vec<_>, DeployServiceError>>()?;

        // A tenant-scoped read would repeat the caller's own totals once per
        // dimension, so the breakdown is only meaningful platform-wide.
        let tenants = if tenant_id.is_none() {
            let tenants_sql = format!(
                "SELECT tenant_id, dimension, COALESCE(SUM(quantity), 0) AS quantity,
                        MIN(unit) AS unit
                 FROM deploy_usage_event
                 WHERE {predicate}
                 GROUP BY tenant_id, dimension
                 ORDER BY tenant_id, dimension"
            );
            let tenant_rows = sqlx::query(AssertSqlSafe(&*tenants_sql))
                .bind(tenant_id)
                .bind(dimension)
                .bind(&query.date_from)
                .bind(&query.date_to)
                .fetch_all(&mut *transaction)
                .await
                .map_err(|error| store_error("aggregate deploy_usage_event tenant breakdown", error))?;
            tenant_rows
                .iter()
                .map(|row| {
                    Ok(TrafficUsageTenantTotal {
                        tenant_id: row
                            .try_get("tenant_id")
                            .map_err(|error| read_error("tenant_id", error))?,
                        dimension: row
                            .try_get("dimension")
                            .map_err(|error| read_error("dimension", error))?,
                        quantity: row
                            .try_get("quantity")
                            .map_err(|error| read_error("quantity", error))?,
                        unit: row
                            .try_get("unit")
                            .unwrap_or_else(|_| String::new()),
                    })
                })
                .collect::<Result<Vec<_>, DeployServiceError>>()?
        } else {
            Vec::new()
        };

        // Read-only, so committing and rolling back are equivalent on the data;
        // committing returns the connection to the pool without a round trip.
        transaction
            .commit()
            .await
            .map_err(|error| store_error("close traffic usage statistics", error))?;

        Ok(TrafficUsageStatistics {
            date_from: query.date_from.clone(),
            date_to: query.date_to.clone(),
            totals,
            daily,
            apps,
            tenants,
            platform_scope: tenant_id.is_none(),
        })
    }
}

/// Aggregated traffic usage for the Web Server operations surface
/// (inherent method so a read-only consumer that shares this database reuses
/// one entry point instead of reaching into the tables itself).
///
/// `tenant_id: None` means every tenant. The Web Server's admin surface is the
/// only caller: its `app-console` half reads the deployments app API instead,
/// which is tenant-scoped by the session and therefore already answers "my own
/// traffic" without a platform-wide read being reachable from it.
impl DeployRepository {
    pub async fn traffic_usage_statistics_lookup(
        &self,
        tenant_id: Option<i64>,
        query: &TrafficUsageStatisticsQuery,
    ) -> DeployServiceResult<TrafficUsageStatistics> {
        self.usage_statistics_repo(tenant_id, query).await
    }
}

/// Upper bound on the per-app breakdown a single statistics read may return.
///
/// The read is a UI drill-down, not an export: without a server-side bound a
/// platform-wide read over a wide window would group by every app on the
/// installation in one response.
pub const MAX_TRAFFIC_STATISTICS_APPS: i64 = 200;

/// Wall-clock bound on the server-side work of one statistics read, in
/// milliseconds.
///
/// Deliberately generous relative to what the aggregate should cost on an
/// indexed window: the point is not to tune the query but to make sure a
/// pathological one — an unindexed platform-wide scan, a lock, a plan that
/// degrades as the fact table grows — returns an error and releases its
/// connection back to the pool the control plane shares, instead of holding it
/// while the installation grows.
pub const TRAFFIC_STATISTICS_STATEMENT_TIMEOUT_MS: u64 = 5_000;

fn read_error(column: &str, error: sqlx::Error) -> DeployServiceError {
    DeployServiceError::Internal(format!("read usage aggregate {column}: {error}"))
}

fn map_usage_event_row(row: &PgRow) -> Result<UsageEventResponse, DeployServiceError> {
    let attribution_json: serde_json::Value = row
        .try_get("attribution_json")
        .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
    let attribution: UsageEventAttribution =
        serde_json::from_value(attribution_json).unwrap_or_default();
    Ok(UsageEventResponse {
        id: row.try_get("uuid").map_err(|error| {
            DeployServiceError::Internal(format!("read usage event uuid: {error}"))
        })?,
        tenant_id: row.try_get("tenant_id").map_err(|error| {
            DeployServiceError::Internal(format!("read usage event tenant: {error}"))
        })?,
        app_id: row.try_get("app_uuid").ok(),
        binding_id: row.try_get("binding_uuid").ok(),
        period_start: datetime_from_row(row, "period_start").map_err(|error| {
            DeployServiceError::Internal(format!("read usage event period: {error}"))
        })?,
        dimension: row.try_get("dimension").map_err(|error| {
            DeployServiceError::Internal(format!("read usage event dimension: {error}"))
        })?,
        quantity: row.try_get("quantity").map_err(|error| {
            DeployServiceError::Internal(format!("read usage event quantity: {error}"))
        })?,
        unit: row.try_get("unit").map_err(|error| {
            DeployServiceError::Internal(format!("read usage event unit: {error}"))
        })?,
        source_target_uuid: row.try_get("source_target_uuid").ok(),
        source_window_id: row.try_get("source_window_id").ok(),
        deduplication_key: row.try_get("deduplication_key").map_err(|error| {
            DeployServiceError::Internal(format!("read usage event dedup key: {error}"))
        })?,
        attribution: Some(attribution),
        observed_at: datetime_from_row(row, "observed_at").map_err(|error| {
            DeployServiceError::Internal(format!("read usage event observed: {error}"))
        })?,
        created_at: datetime_from_row(row, "created_at").map_err(|error| {
            DeployServiceError::Internal(format!("read usage event created: {error}"))
        })?,
    })
}
