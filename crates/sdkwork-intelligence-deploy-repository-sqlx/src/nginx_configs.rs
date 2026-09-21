use sdkwork_deploy_contract::{
    CreateNginxConfigRequest, DeployServiceError, DeployServiceResult, ListNginxConfigsQuery,
    NginxConfigPage, NginxConfigResponse, NginxReloadResponse, NginxStatusResponse,
    NginxValidateResponse, UpdateNginxConfigRequest,
};
use sqlx::{postgres::PgArguments, postgres::PgRow, AssertSqlSafe, PgPool, Postgres, Row};

use crate::nginx_orchestrator::{parse_sdkwork_deploy_binding, publish_nginx_config};
use crate::nginx_security::{
    nginx_reload_enabled, run_nginx_reload_command, validate_nginx_config_content,
};
use crate::support::{
    bool_from_row, json_from_row, new_uuid, next_id, now_rfc3339, pagination,
    resolve_app_internal_id, resolve_app_uuid, sha256_hex, store_error,
};
use crate::DeployRepository;

impl DeployRepository {
    pub(super) async fn list_nginx_configs_repo(
        &self,
        tenant_id: Option<i64>,
        query: &ListNginxConfigsQuery,
    ) -> DeployServiceResult<NginxConfigPage> {
        let (page, page_size, offset) = pagination(query.page, query.page_size);
        let mut count_sql =
            String::from("SELECT COUNT(*) AS total FROM deploy_nginx_config WHERE 1=1");
        let mut list_sql = String::from(
            "SELECT uuid, app_id, config_name, config_type, is_active, status
             FROM deploy_nginx_config WHERE 1=1",
        );
        let mut binds: Vec<BindValue> = Vec::new();

        if let Some(tenant_id) = tenant_id {
            let index = binds.len() + 1;
            let clause = format!(" AND tenant_id = ${index}");
            count_sql.push_str(&clause);
            list_sql.push_str(&clause);
            binds.push(BindValue::I64(tenant_id));
        }
        if let Some(app_uuid) = query.app_id.as_deref() {
            if let Some(tenant_id) = tenant_id {
                let app_internal_id =
                    resolve_app_internal_id(&self.pool, tenant_id, app_uuid).await?;
                let index = binds.len() + 1;
                let clause = format!(" AND app_id = ${index}");
                count_sql.push_str(&clause);
                list_sql.push_str(&clause);
                binds.push(BindValue::I64(app_internal_id));
            }
        }
        if let Some(config_type) = query.config_type {
            let index = binds.len() + 1;
            let clause = format!(" AND config_type = ${index}");
            count_sql.push_str(&clause);
            list_sql.push_str(&clause);
            binds.push(BindValue::I32(config_type));
        }
        if let Some(is_active) = query.is_active {
            let index = binds.len() + 1;
            let clause = format!(" AND is_active = ${index}");
            count_sql.push_str(&clause);
            list_sql.push_str(&clause);
            binds.push(BindValue::Bool(is_active));
        }

        let limit_index = binds.len() + 1;
        let offset_index = binds.len() + 2;
        list_sql.push_str(&format!(
            " ORDER BY updated_at DESC, id DESC LIMIT ${limit_index} OFFSET ${offset_index}"
        ));

        let count_row = apply_binds(sqlx::query(AssertSqlSafe(&*count_sql)), &binds)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count deploy_nginx_config", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let mut list_query = apply_binds(sqlx::query(AssertSqlSafe(&*list_sql)), &binds);
        list_query = list_query.bind(page_size).bind(offset);
        let rows = list_query
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list deploy_nginx_config", error))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(
                map_nginx_config_row(&self.pool, tenant_id.unwrap_or(0), row)
                    .await
                    .map_err(|error| {
                        DeployServiceError::Internal(format!(
                            "map deploy_nginx_config row: {error}"
                        ))
                    })?,
            );
        }

        Ok(NginxConfigPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn create_nginx_config_repo(
        &self,
        tenant_id: i64,
        request: &CreateNginxConfigRequest,
    ) -> DeployServiceResult<NginxConfigResponse> {
        let app_internal_id =
            resolve_app_internal_id(&self.pool, tenant_id, &request.app_id).await?;
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        let config_hash = sha256_hex(&request.config_content);

        sqlx::query(
            "INSERT INTO deploy_nginx_config (
                id, uuid, tenant_id, app_id, config_type, config_name, config_content, config_hash,
                is_active, status, metadata, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, 0, 0, '{}', CAST($9 AS TIMESTAMPTZ), CAST($9 AS TIMESTAMPTZ), 0
             )",
        )
        .bind(id)
        .bind(&uuid)
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(request.config_type)
        .bind(&request.config_name)
        .bind(&request.config_content)
        .bind(&config_hash)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_nginx_config", error))?;

        self.retrieve_nginx_config_repo(Some(tenant_id), &uuid)
            .await
    }

    pub(super) async fn retrieve_nginx_config_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> DeployServiceResult<NginxConfigResponse> {
        let row = if let Some(tenant_id) = tenant_id {
            sqlx::query(
                "SELECT uuid, tenant_id, app_id, config_name, config_type, is_active, status
                 FROM deploy_nginx_config WHERE tenant_id = $1 AND uuid = $2",
            )
            .bind(tenant_id)
            .bind(config_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve deploy_nginx_config", error))?
        } else {
            sqlx::query(
                "SELECT uuid, tenant_id, app_id, config_name, config_type, is_active, status
                 FROM deploy_nginx_config WHERE uuid = $1",
            )
            .bind(config_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve deploy_nginx_config", error))?
        }
        .ok_or_else(|| DeployServiceError::not_found("nginx config not found"))?;

        let row_tenant_id: i64 = row
            .try_get("tenant_id")
            .map_err(|error| store_error("retrieve deploy_nginx_config tenant_id", error))?;
        map_nginx_config_row(&self.pool, row_tenant_id, &row)
            .await
            .map_err(|error| DeployServiceError::Internal(error.to_string()))
    }

    pub(super) async fn update_nginx_config_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> DeployServiceResult<NginxConfigResponse> {
        let existing = self
            .retrieve_nginx_config_repo(tenant_id, config_id)
            .await?;
        let row = if let Some(tenant_id) = tenant_id {
            sqlx::query(
                "SELECT config_name, config_content FROM deploy_nginx_config
                 WHERE tenant_id = $1 AND uuid = $2",
            )
            .bind(tenant_id)
            .bind(config_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("load deploy_nginx_config for update", error))?
        } else {
            sqlx::query(
                "SELECT config_name, config_content FROM deploy_nginx_config WHERE uuid = $1",
            )
            .bind(config_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("load deploy_nginx_config for update", error))?
        }
        .ok_or_else(|| DeployServiceError::not_found("nginx config not found"))?;

        let config_name = request
            .config_name
            .as_ref()
            .cloned()
            .or_else(|| row.try_get("config_name").ok())
            .unwrap_or(existing.config_name);
        let config_content = request
            .config_content
            .as_ref()
            .cloned()
            .or_else(|| row.try_get("config_content").ok())
            .unwrap_or_default();
        let config_hash = sha256_hex(&config_content);
        let now = now_rfc3339();

        let result = if let Some(tenant_id) = tenant_id {
            sqlx::query(
                "UPDATE deploy_nginx_config
                 SET config_name = $3, config_content = $4, config_hash = $5, updated_at = CAST($6 AS TIMESTAMPTZ), version = version + 1
                 WHERE tenant_id = $1 AND uuid = $2",
            )
            .bind(tenant_id)
            .bind(config_id)
            .bind(&config_name)
            .bind(&config_content)
            .bind(&config_hash)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update deploy_nginx_config", error))?
        } else {
            sqlx::query(
                "UPDATE deploy_nginx_config
                 SET config_name = $2, config_content = $3, config_hash = $4, updated_at = CAST($5 AS TIMESTAMPTZ), version = version + 1
                 WHERE uuid = $1",
            )
            .bind(config_id)
            .bind(&config_name)
            .bind(&config_content)
            .bind(&config_hash)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update deploy_nginx_config", error))?
        };

        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found("nginx config not found"));
        }

        self.retrieve_nginx_config_repo(tenant_id, config_id).await
    }

    pub(super) async fn validate_nginx_config_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> DeployServiceResult<NginxValidateResponse> {
        let row = if let Some(tenant_id) = tenant_id {
            sqlx::query(
                "SELECT config_content FROM deploy_nginx_config WHERE tenant_id = $1 AND uuid = $2",
            )
            .bind(tenant_id)
            .bind(config_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("validate deploy_nginx_config lookup", error))?
        } else {
            sqlx::query("SELECT config_content FROM deploy_nginx_config WHERE uuid = $1")
                .bind(config_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error("validate deploy_nginx_config lookup", error))?
        }
        .ok_or_else(|| DeployServiceError::not_found("nginx config not found"))?;

        let content: String = row
            .try_get("config_content")
            .map_err(|error| store_error("validate deploy_nginx_config content", error))?;
        match validate_nginx_config_content(&content) {
            Ok(()) => Ok(NginxValidateResponse {
                valid: true,
                message: None,
            }),
            Err(message) => Ok(NginxValidateResponse {
                valid: false,
                message: Some(message),
            }),
        }
    }

    pub(super) async fn deploy_nginx_config_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> DeployServiceResult<NginxConfigResponse> {
        let (config_content, _site_uuid, site_runtime_config, primary_domain) = self
            .load_nginx_publish_context(tenant_id, config_id)
            .await?;
        let binding = parse_sdkwork_deploy_binding(&site_runtime_config, primary_domain.as_deref());
        publish_nginx_config(&config_content, binding.as_ref(), primary_domain.as_deref())
            .map_err(DeployServiceError::validation)?;

        let existing = self
            .retrieve_nginx_config_repo(tenant_id, config_id)
            .await?;
        let app_internal_id =
            resolve_app_internal_id(&self.pool, tenant_id.unwrap_or(0), &existing.app_id).await?;
        let now = now_rfc3339();

        sqlx::query(
            "UPDATE deploy_nginx_config SET is_active = FALSE, updated_at = CAST($2 AS TIMESTAMPTZ), version = version + 1
             WHERE app_id = $1 AND is_active",
        )
        .bind(app_internal_id)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("deactivate deploy_nginx_config", error))?;

        let result = if let Some(tenant_id) = tenant_id {
            sqlx::query(
                "UPDATE deploy_nginx_config
                 SET is_active = TRUE, status = 1, deployed_at = CAST($3 AS TIMESTAMPTZ), updated_at = CAST($3 AS TIMESTAMPTZ), version = version + 1
                 WHERE tenant_id = $1 AND uuid = $2",
            )
            .bind(tenant_id)
            .bind(config_id)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("deploy deploy_nginx_config", error))?
        } else {
            sqlx::query(
                "UPDATE deploy_nginx_config
                 SET is_active = TRUE, status = 1, deployed_at = CAST($2 AS TIMESTAMPTZ), updated_at = CAST($2 AS TIMESTAMPTZ), version = version + 1
                 WHERE uuid = $1",
            )
            .bind(config_id)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("deploy deploy_nginx_config", error))?
        };

        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found("nginx config not found"));
        }

        self.retrieve_nginx_config_repo(tenant_id, config_id).await
    }

    async fn load_nginx_publish_context(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> DeployServiceResult<(String, String, serde_json::Value, Option<String>)> {
        let row = if let Some(tenant_id) = tenant_id {
            sqlx::query(
                "SELECT nc.config_content, s.uuid AS app_uuid, s.runtime_config,
                        (
                            SELECT b.hostname_ascii
                            FROM deploy_app_binding b
                            WHERE b.app_id = s.id
                              AND b.tenant_id = s.tenant_id
                              AND b.deleted_at IS NULL
                            ORDER BY b.is_canonical DESC, b.created_at ASC
                            LIMIT 1
                        ) AS primary_domain
                 FROM deploy_nginx_config nc
                 INNER JOIN deploy_app s ON s.id = nc.app_id
                 WHERE nc.tenant_id = $1 AND nc.uuid = $2 AND s.deleted_at IS NULL",
            )
            .bind(tenant_id)
            .bind(config_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("load nginx publish context", error))?
        } else {
            sqlx::query(
                "SELECT nc.config_content, s.uuid AS app_uuid, s.runtime_config,
                        (
                            SELECT b.hostname_ascii
                            FROM deploy_app_binding b
                            WHERE b.app_id = s.id
                              AND b.tenant_id = s.tenant_id
                              AND b.deleted_at IS NULL
                            ORDER BY b.is_canonical DESC, b.created_at ASC
                            LIMIT 1
                        ) AS primary_domain
                 FROM deploy_nginx_config nc
                 INNER JOIN deploy_app s ON s.id = nc.app_id
                 WHERE nc.uuid = $1 AND s.deleted_at IS NULL",
            )
            .bind(config_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("load nginx publish context", error))?
        }
        .ok_or_else(|| DeployServiceError::not_found("nginx config not found"))?;

        let config_content: String = row
            .try_get("config_content")
            .map_err(|error| store_error("load nginx publish content", error))?;
        let app_uuid: String = row
            .try_get("app_uuid")
            .map_err(|error| store_error("load nginx publish app uuid", error))?;
        let runtime_config = json_from_row(&row, "runtime_config")
            .map_err(|error| store_error("load nginx publish runtime_config", error))?
            .unwrap_or_else(|| serde_json::json!({}));
        let primary_domain: Option<String> = row.try_get("primary_domain").ok();

        Ok((config_content, app_uuid, runtime_config, primary_domain))
    }

    pub(super) async fn reload_nginx_repo(&self) -> DeployServiceResult<NginxReloadResponse> {
        if !nginx_reload_enabled() {
            return Ok(NginxReloadResponse { reloaded: false });
        }

        run_nginx_reload_command().map_err(DeployServiceError::validation)?;

        Ok(NginxReloadResponse { reloaded: true })
    }

    pub(super) async fn retrieve_nginx_status_repo(
        &self,
        tenant_id: Option<i64>,
    ) -> DeployServiceResult<NginxStatusResponse> {
        let active_configs = if let Some(tenant_id) = tenant_id {
            let row = sqlx::query(
                "SELECT COUNT(*) AS total FROM deploy_nginx_config
                 WHERE tenant_id = $1 AND is_active AND status = 1",
            )
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count active deploy_nginx_config", error))?;
            row.try_get::<i64, _>("total").unwrap_or(0)
        } else {
            let row = sqlx::query(
                "SELECT COUNT(*) AS total FROM deploy_nginx_config WHERE is_active AND status = 1",
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count active deploy_nginx_config", error))?;
            row.try_get::<i64, _>("total").unwrap_or(0)
        };

        Ok(NginxStatusResponse {
            running: active_configs > 0,
            active_configs,
        })
    }
}

enum BindValue {
    I64(i64),
    I32(i32),
    Bool(bool),
}

fn apply_binds<'q>(
    mut query: sqlx::query::Query<'q, Postgres, PgArguments>,
    binds: &[BindValue],
) -> sqlx::query::Query<'q, Postgres, PgArguments> {
    for value in binds {
        query = match value {
            BindValue::I64(value) => query.bind(*value),
            BindValue::I32(value) => query.bind(*value),
            BindValue::Bool(value) => query.bind(*value),
        };
    }
    query
}

async fn map_nginx_config_row(
    pool: &PgPool,
    tenant_id: i64,
    row: &PgRow,
) -> Result<NginxConfigResponse, sqlx::Error> {
    let app_internal_id: i64 = row.try_get("app_id")?;
    let app_uuid = resolve_app_uuid(pool, tenant_id, app_internal_id)
        .await
        .map_err(|error| sqlx::Error::Decode(error.to_string().into()))?;

    Ok(NginxConfigResponse {
        id: row.try_get("uuid")?,
        app_id: app_uuid,
        config_name: row.try_get("config_name")?,
        config_type: row.try_get("config_type")?,
        is_active: bool_from_row(row, "is_active")?,
        status: row.try_get("status")?,
    })
}
