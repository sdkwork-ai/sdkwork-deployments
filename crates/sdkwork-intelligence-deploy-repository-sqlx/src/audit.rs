use sdkwork_deploy_contract::{
    AuditLogPage, AuditLogQuery, AuditLogResponse, DeployServiceError, DeployServiceResult,
};
use sdkwork_intelligence_deploy_service::repository::InsertAuditLogCommand;
use sqlx::{postgres::PgRow, AssertSqlSafe, Row};

use crate::support::{datetime_from_row, new_uuid, next_id, now_rfc3339, pagination, store_error};
use crate::DeployRepository;

impl DeployRepository {
    pub(super) async fn list_audit_logs_repo(
        &self,
        tenant_id: Option<i64>,
        query: &AuditLogQuery,
        cursor: Option<&str>,
    ) -> DeployServiceResult<AuditLogPage> {
        let Some(tenant_id) = tenant_id else {
            // 审计日志不允许无租户上下文的全库枚举：调用方必须先解析租户。
            return Err(DeployServiceError::forbidden(
                "tenant context is required for audit log listing",
            ));
        };

        // keyset 模式（PAGINATION_SPEC §6）：审计日志是典型 growing 表，游标
        // 分页 O(page size) 内存、无深 OFFSET、无全表 COUNT。
        if let Some(cursor) = cursor {
            if !(1..=200).contains(&query.page_size) {
                return Err(DeployServiceError::validation(
                    "page_size must be between 1 and 200",
                ));
            }
            let (cursor_created_at, cursor_id) = crate::support::decode_keyset_cursor(cursor)
                .ok_or_else(|| DeployServiceError::validation("cursor is invalid"))?;
            // The cursor instant is declared as TEXT on the wire, so the cast is
            // what makes the row comparison legal: without it PostgreSQL
            // resolves the operator against the parameter's declared type and
            // reports `timestamp with time zone < text`.
            let rows = sqlx::query(
                "SELECT id, uuid, action, target_type, created_at
                 FROM deploy_audit_log
                 WHERE tenant_id = $1
                   AND (created_at, id) < (CAST($2 AS TIMESTAMPTZ), $3)
                 ORDER BY created_at DESC, id DESC LIMIT $4",
            )
            .bind(tenant_id)
            .bind(&cursor_created_at)
            .bind(cursor_id)
            .bind(i64::from(query.page_size) + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list deploy_audit_log cursor", error))?;
            let has_more = rows.len() > query.page_size as usize;
            let page_rows = rows
                .into_iter()
                .take(query.page_size as usize)
                .collect::<Vec<_>>();
            let mut items = Vec::with_capacity(page_rows.len());
            for row in &page_rows {
                items.push(map_audit_log_row(row).map_err(|error| {
                    DeployServiceError::Internal(format!("map deploy_audit_log row: {error}"))
                })?);
            }
            let next_cursor = has_more
                .then(|| {
                    let last = page_rows.last().expect("non-empty page when has_more");
                    // `created_at` is TIMESTAMPTZ. Decoding it as `String` asks
                    // sqlx for TEXT and is rejected at the wire level, so read it
                    // through the shared helper that takes a `DateTime<Utc>` and
                    // renders RFC3339 — the shape the cursor payload expects.
                    let created_at = datetime_from_row(last, "created_at").map_err(|error| {
                        store_error("map deploy_audit_log cursor instant", error)
                    })?;
                    let id: i64 = last
                        .try_get("id")
                        .map_err(|error| store_error("map deploy_audit_log cursor id", error))?;
                    Ok::<_, DeployServiceError>(crate::support::encode_keyset_cursor(
                        &created_at,
                        id,
                    ))
                })
                .transpose()?;
            return Ok(AuditLogPage {
                items,
                total: 0,
                page: 0,
                page_size: query.page_size,
                next_cursor,
                has_more: Some(has_more),
            });
        }

        let (page, page_size, offset) = pagination(query.page, query.page_size);

        // OpenAPI 声明的过滤器逐项落地（PAGINATION_SPEC §4：声明即实现）。
        // 所有动态值经 bind 参数注入，WHERE 片段仅由固定子句拼接。
        let mut conditions = vec!["tenant_id = $1".to_string()];
        let mut bind_index = 2;
        if query.target_type.as_deref().is_some_and(|v| !v.is_empty()) {
            conditions.push(format!("target_type = ${bind_index}"));
            bind_index += 1;
        }
        if query.action.as_deref().is_some_and(|v| !v.is_empty()) {
            conditions.push(format!("action = ${bind_index}"));
            bind_index += 1;
        }
        if query.operator_id.is_some() {
            conditions.push(format!("operator_id = ${bind_index}"));
            bind_index += 1;
        }
        if query.start_date.as_deref().is_some_and(|v| !v.is_empty()) {
            conditions.push(format!("created_at >= CAST(${bind_index} AS TIMESTAMPTZ)"));
            bind_index += 1;
        }
        if query.end_date.as_deref().is_some_and(|v| !v.is_empty()) {
            conditions.push(format!("created_at <= CAST(${bind_index} AS TIMESTAMPTZ)"));
            bind_index += 1;
        }
        let where_clause = conditions.join(" AND ");

        let count_sql =
            format!("SELECT COUNT(*) AS total FROM deploy_audit_log WHERE {where_clause}");
        let mut count_query = sqlx::query(AssertSqlSafe(count_sql.as_str())).bind(tenant_id);
        if let Some(target_type) = query.target_type.as_deref().filter(|v| !v.is_empty()) {
            count_query = count_query.bind(target_type);
        }
        if let Some(action) = query.action.as_deref().filter(|v| !v.is_empty()) {
            count_query = count_query.bind(action);
        }
        if let Some(operator_id) = query.operator_id {
            count_query = count_query.bind(operator_id);
        }
        if let Some(start_date) = query.start_date.as_deref().filter(|v| !v.is_empty()) {
            count_query = count_query.bind(start_date);
        }
        if let Some(end_date) = query.end_date.as_deref().filter(|v| !v.is_empty()) {
            count_query = count_query.bind(end_date);
        }
        let count_row = count_query
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count deploy_audit_log", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map deploy_audit_log count", error))?;

        let limit_index = bind_index;
        let offset_index = bind_index + 1;
        // `id` is selected because the keyset cursor is `(created_at, id)` and
        // this page hands one out; one extra row is fetched so the page can
        // prove a further window exists without a second round trip.
        let list_sql = format!(
            "SELECT id, uuid, action, target_type, created_at
             FROM deploy_audit_log
             WHERE {where_clause}
             ORDER BY created_at DESC, id DESC LIMIT ${limit_index} OFFSET ${offset_index}"
        );
        let mut list_query = sqlx::query(AssertSqlSafe(list_sql.as_str())).bind(tenant_id);
        if let Some(target_type) = query.target_type.as_deref().filter(|v| !v.is_empty()) {
            list_query = list_query.bind(target_type);
        }
        if let Some(action) = query.action.as_deref().filter(|v| !v.is_empty()) {
            list_query = list_query.bind(action);
        }
        if let Some(operator_id) = query.operator_id {
            list_query = list_query.bind(operator_id);
        }
        if let Some(start_date) = query.start_date.as_deref().filter(|v| !v.is_empty()) {
            list_query = list_query.bind(start_date);
        }
        if let Some(end_date) = query.end_date.as_deref().filter(|v| !v.is_empty()) {
            list_query = list_query.bind(end_date);
        }
        let rows = list_query
            .bind(page_size + 1)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list deploy_audit_log", error))?;

        let has_more = rows.len() > page_size as usize;
        let page_rows = rows
            .into_iter()
            .take(page_size as usize)
            .collect::<Vec<_>>();
        let mut items = Vec::with_capacity(page_rows.len());
        for row in &page_rows {
            items.push(map_audit_log_row(row).map_err(|error| {
                DeployServiceError::Internal(format!("map deploy_audit_log row: {error}"))
            })?);
        }

        // An offset page still hands out the keyset continuation. Without it a
        // client could never enter cursor mode — it would have no first cursor —
        // and every page after the first would have to walk a deeper OFFSET,
        // which is exactly what PAGINATION_SPEC §6 rules out for a growing table
        // like the audit log.
        let next_cursor = has_more
            .then(|| {
                let last = page_rows.last().expect("non-empty page when has_more");
                let created_at = datetime_from_row(last, "created_at")
                    .map_err(|error| store_error("map deploy_audit_log cursor instant", error))?;
                let id: i64 = last
                    .try_get("id")
                    .map_err(|error| store_error("map deploy_audit_log cursor id", error))?;
                Ok::<_, DeployServiceError>(crate::support::encode_keyset_cursor(&created_at, id))
            })
            .transpose()?;

        Ok(AuditLogPage {
            items,
            total,
            page,
            page_size,
            next_cursor,
            has_more: Some(has_more),
        })
    }

    pub(super) async fn insert_audit_log_repo(
        &self,
        command: &InsertAuditLogCommand,
    ) -> DeployServiceResult<()> {
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();

        sqlx::query(
            "INSERT INTO deploy_audit_log (
                id, uuid, tenant_id, organization_id, operator_id, action, target_type,
                target_id, target_uuid, metadata, created_at
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, '{}', CAST($10 AS TIMESTAMPTZ)
             )",
        )
        .bind(id)
        .bind(&uuid)
        .bind(command.tenant_id)
        .bind(command.organization_id)
        .bind(command.operator_id)
        .bind(&command.action)
        .bind(&command.target_type)
        .bind(command.target_id)
        .bind(command.target_uuid.as_deref())
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_audit_log", error))?;

        Ok(())
    }
}

fn map_audit_log_row(row: &PgRow) -> Result<AuditLogResponse, sqlx::Error> {
    Ok(AuditLogResponse {
        id: row.try_get("uuid")?,
        action: row.try_get("action")?,
        resource: row.try_get("target_type")?,
        created_at: datetime_from_row(row, "created_at")?,
    })
}
