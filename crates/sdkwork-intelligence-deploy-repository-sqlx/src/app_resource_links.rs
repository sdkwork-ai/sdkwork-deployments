//! `deploy_application_resource_link` persistence: the account bindings an
//! application environment publishes through.
//!
//! The table existed in the baseline with zero readers and zero writers. This
//! module is the only writer, and it writes the three things the schema models:
//!
//! * the **reference** — the provider-side identity (WeChat AppID, merchant
//!   account number) a runtime resolves;
//! * the **address** — `(app, environment, kind[, reference])`, which is what the
//!   unique indexes actually constrain;
//! * the **default flag** — one payment account per `(environment, provider)`,
//!   which `uk_deploy_application_resource_link_default` enforces and this module
//!   satisfies by *moving* the default rather than by failing.
//!
//! # Why the upsert locks the application row
//!
//! "Find the row, then insert or update it" is two statements, and two concurrent
//! requests for the same address can both find nothing and both insert — the
//! second one then fails on `uk_single`/`uk_ref` with a `23505` the API can only
//! mask. Taking `FOR UPDATE` on the owning `deploy_app` row first serializes the
//! writers for one application, which is the same granularity `deploy_env_variable`
//! uses for its capacity check, and it also orders the default move against the
//! write that caused it.

use sdkwork_deploy_contract::{
    AppPublishEnvironment, AppResourceKind, AppResourceLinkPage, AppResourceLinkResponse,
    AppResourceLinkStatus, DeployServiceError, DeployServiceResult,
};
use sdkwork_intelligence_deploy_service::{
    DeleteAppResourceLinkCommand, ListAppResourceLinksCommand, UpsertAppResourceLinkCommand,
};
use sqlx::postgres::PgRow;
use sqlx::{AssertSqlSafe, Row};

use crate::support::{json_from_row, new_uuid, now_rfc3339, resolve_app_internal_id, store_error};
use crate::DeployRepository;

/// The tenant/state predicate every read and write shares.
const LIVE: &str = "deleted_at IS NULL";

const LINK_COLUMNS: &str = "id, env_key, resource_kind, resource_ref, provider_code, is_default, \
     binding_status, metadata, created_at, updated_at, version";

impl DeployRepository {
    pub(super) async fn list_app_resource_links_repo(
        &self,
        command: ListAppResourceLinksCommand,
    ) -> DeployServiceResult<AppResourceLinkPage> {
        let ListAppResourceLinksCommand {
            tenant_id,
            app_uuid,
            environment,
            resource_kind,
            page,
            page_size,
        } = command;
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, &app_uuid).await?;
        let (page, page_size, offset) = crate::support::pagination(page, page_size);
        // The filters are normalized to '' so one statement serves all four
        // combinations; the bound values stay typed, so an unknown environment
        // simply matches nothing instead of silently listing everything.
        let environment = environment.unwrap_or_default();
        let kind = resource_kind.map(|kind| kind.as_str()).unwrap_or("");

        let predicate = format!(
            "tenant_id = $1 AND app_id = $2 AND {LIVE}
             AND ($3 = '' OR env_key = $3)
             AND ($4 = '' OR resource_kind = $4)"
        );
        let count_row = sqlx::query(AssertSqlSafe(format!(
            "SELECT COUNT(*) AS total FROM deploy_application_resource_link WHERE {predicate}"
        )))
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(&environment)
        .bind(kind)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_application_resource_link", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map deploy_application_resource_link total", error))?;

        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {LINK_COLUMNS}
             FROM deploy_application_resource_link
             WHERE {predicate}
             ORDER BY env_key ASC, resource_kind ASC, provider_code ASC NULLS FIRST,
                      resource_ref ASC, id ASC
             LIMIT $5 OFFSET $6"
        )))
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(&environment)
        .bind(kind)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_application_resource_link", error))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_link_row(row, &app_uuid)?);
        }
        Ok(AppResourceLinkPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn upsert_app_resource_link_repo(
        &self,
        command: UpsertAppResourceLinkCommand,
    ) -> DeployServiceResult<AppResourceLinkResponse> {
        let UpsertAppResourceLinkCommand {
            tenant_id,
            organization_id,
            actor_id,
            app_uuid,
            binding,
        } = command;
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, &app_uuid).await?;
        let now = now_rfc3339();

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin resource link transaction", error))?;

        // Serializes writers for this application (see the module docs). The
        // UPDATE-with-no-change is the cheapest row lock that does not depend on
        // the row's version staying put.
        let locked = sqlx::query(
            "UPDATE deploy_app SET version = version
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_app for resource link write", error))?;
        if locked.rows_affected() != 1 {
            return Err(DeployServiceError::not_found("app not found"));
        }

        // A payment default is a *slot*, not a flag: making this account the
        // default vacates the previous holder in the same transaction, because
        // `uk_deploy_application_resource_link_default` permits one default per
        // (app, environment, provider). Doing it as two statements inside one
        // transaction keeps the intermediate state invisible.
        if binding.resource_kind == AppResourceKind::PaymentAccount && binding.is_default {
            sqlx::query(
                "UPDATE deploy_application_resource_link
                 SET is_default = FALSE, updated_by = $5,
                     updated_at = CAST($6 AS TIMESTAMPTZ), version = version + 1
                 WHERE tenant_id = $1 AND app_id = $2 AND env_key = $3
                   AND resource_kind = 'payment_account' AND provider_code = $4
                   AND is_default AND deleted_at IS NULL",
            )
            .bind(tenant_id)
            .bind(app_internal_id)
            .bind(binding.environment)
            .bind(binding.provider_code.as_deref().unwrap_or_default())
            .bind(actor_id)
            .bind(&now)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("vacate previous resource link default", error))?;
        }

        // The address of the write. A single-instance kind is addressed by
        // (environment, kind) *without* the reference, so switching the
        // environment's official account replaces the row instead of colliding on
        // `uk_single`.
        let existing = if binding.resource_kind.is_single_instance() {
            sqlx::query(
                "SELECT id FROM deploy_application_resource_link
                 WHERE tenant_id = $1 AND app_id = $2 AND env_key = $3
                   AND resource_kind = $4 AND deleted_at IS NULL
                 FOR UPDATE",
            )
            .bind(tenant_id)
            .bind(app_internal_id)
            .bind(binding.environment)
            .bind(binding.resource_kind.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("lock resource link by kind", error))?
        } else {
            sqlx::query(
                "SELECT id FROM deploy_application_resource_link
                 WHERE tenant_id = $1 AND app_id = $2 AND env_key = $3
                   AND resource_kind = $4 AND resource_ref = $5 AND deleted_at IS NULL
                 FOR UPDATE",
            )
            .bind(tenant_id)
            .bind(app_internal_id)
            .bind(binding.environment)
            .bind(binding.resource_kind.as_str())
            .bind(&binding.resource_ref)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("lock resource link by reference", error))?
        };

        let row = match existing {
            Some(row) => {
                let id: String = row
                    .try_get("id")
                    .map_err(|error| store_error("read resource link id", error))?;
                sqlx::query(AssertSqlSafe(format!(
                    "UPDATE deploy_application_resource_link
                     SET resource_ref = $3, provider_code = $4, is_default = $5,
                         binding_status = $6, metadata = $7, updated_by = $8,
                         updated_at = CAST($9 AS TIMESTAMPTZ), version = version + 1
                     WHERE tenant_id = $1 AND id = $2
                     RETURNING {LINK_COLUMNS}"
                )))
                .bind(tenant_id)
                .bind(&id)
                .bind(&binding.resource_ref)
                .bind(binding.provider_code.as_deref())
                .bind(binding.is_default)
                .bind(binding.binding_status.as_str())
                .bind(&binding.metadata)
                .bind(actor_id)
                .bind(&now)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| store_error("update deploy_application_resource_link", error))?
            }
            None => {
                // The table's `id` is the binding's public uuid (`VARCHAR(36)`),
                // not a minted snowflake: see the table comment in
                // `database/migrations/postgres/0012_deploy_application_resource_link_reconcile.up.sql`
                // for why the module's `BIGINT id + uuid` pair was not applied
                // here. One identifier, on the wire and in the row.
                let uuid = new_uuid();
                sqlx::query(AssertSqlSafe(format!(
                    "INSERT INTO deploy_application_resource_link (
                        id, tenant_id, organization_id, app_id, env_key, resource_kind,
                        resource_ref, provider_code, is_default, binding_status, metadata,
                        created_by, updated_by, created_at, updated_at, version
                     ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12,
                        CAST($13 AS TIMESTAMPTZ), CAST($13 AS TIMESTAMPTZ), 1
                     )
                     RETURNING {LINK_COLUMNS}"
                )))
                .bind(&uuid)
                .bind(tenant_id)
                .bind(organization_id)
                .bind(app_internal_id)
                .bind(binding.environment)
                .bind(binding.resource_kind.as_str())
                .bind(&binding.resource_ref)
                .bind(binding.provider_code.as_deref())
                .bind(binding.is_default)
                .bind(binding.binding_status.as_str())
                .bind(&binding.metadata)
                .bind(actor_id)
                .bind(&now)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| {
                    if crate::support::is_unique_violation(&error) {
                        DeployServiceError::conflict(
                            "this environment already binds an account with that reference",
                        )
                    } else {
                        store_error("insert deploy_application_resource_link", error)
                    }
                })?
            }
        };

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit resource link transaction", error))?;

        map_link_row(&row, &app_uuid)
    }

    pub(super) async fn delete_app_resource_link_repo(
        &self,
        command: DeleteAppResourceLinkCommand,
    ) -> DeployServiceResult<()> {
        let DeleteAppResourceLinkCommand {
            tenant_id,
            app_uuid,
            resource_link_uuid,
        } = command;
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, &app_uuid).await?;
        let now = now_rfc3339();
        // Soft delete: the row keeps its audit trail. `uk_ref`/`uk_single` are
        // partial on `deleted_at IS NULL`, so a retired binding stops constraining
        // the address immediately.
        let result = sqlx::query(
            "UPDATE deploy_application_resource_link
             SET deleted_at = CAST($4 AS TIMESTAMPTZ), updated_at = CAST($4 AS TIMESTAMPTZ),
                 version = version + 1
             WHERE tenant_id = $1 AND app_id = $2 AND id = $3 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(resource_link_uuid)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("delete deploy_application_resource_link", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found("resource link not found"));
        }
        Ok(())
    }
}

fn map_link_row(row: &PgRow, app_uuid: &str) -> DeployServiceResult<AppResourceLinkResponse> {
    let env_key: String = row
        .try_get("env_key")
        .map_err(|error| DeployServiceError::Internal(format!("read env_key: {error}")))?;
    let environment = AppPublishEnvironment::parse(&env_key).ok_or_else(|| {
        DeployServiceError::Internal(format!(
            "deploy_application_resource_link.env_key holds unknown environment {env_key:?}; \
             chk_deploy_application_resource_link_environment should have refused it"
        ))
    })?;
    let kind: String = row
        .try_get("resource_kind")
        .map_err(|error| DeployServiceError::Internal(format!("read resource_kind: {error}")))?;
    let resource_kind = AppResourceKind::parse(&kind).ok_or_else(|| {
        DeployServiceError::Internal(format!(
            "deploy_application_resource_link.resource_kind holds unknown kind {kind:?}"
        ))
    })?;
    let status: String = row
        .try_get("binding_status")
        .map_err(|error| DeployServiceError::Internal(format!("read binding_status: {error}")))?;
    let binding_status = AppResourceLinkStatus::parse(&status).ok_or_else(|| {
        DeployServiceError::Internal(format!(
            "deploy_application_resource_link.binding_status holds unknown status {status:?}"
        ))
    })?;
    let version: i64 = row
        .try_get("version")
        .map_err(|error| DeployServiceError::Internal(format!("read version: {error}")))?;
    let metadata = json_from_row(row, "metadata")
        .map_err(|error| DeployServiceError::Internal(format!("read metadata: {error}")))?
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(AppResourceLinkResponse {
        id: row
            .try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("read id: {error}")))?,
        app_id: app_uuid.to_owned(),
        environment,
        resource_kind,
        resource_ref: row
            .try_get("resource_ref")
            .map_err(|error| DeployServiceError::Internal(format!("read resource_ref: {error}")))?,
        provider_code: row.try_get("provider_code").map_err(|error| {
            DeployServiceError::Internal(format!("read provider_code: {error}"))
        })?,
        is_default: row
            .try_get("is_default")
            .map_err(|error| DeployServiceError::Internal(format!("read is_default: {error}")))?,
        binding_status,
        metadata,
        created_at: crate::support::required_datetime(row, "created_at")?,
        updated_at: crate::support::required_datetime(row, "updated_at")?,
        version: version.to_string(),
    })
}
