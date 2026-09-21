//! Usage metering repository operations (TECH §4.6): append-only usage facts
//! with deduplication identity, traffic batch ingest from Web Server nodes,
//! and the entitlement projection read model.

use sdkwork_deploy_contract::{
    DeployServiceError, DeployServiceResult, UsageEventAttribution, UsageEventIngestItem,
    UsageEventPage, UsageEventQuery, UsageEventResponse, UsageIngestResult,
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
