//! App template marketplace catalog storage.
//!
//! Three tables (`deploy_app_template_category`, `deploy_app_template`,
//! `deploy_app_template_version`) behind the marketplace port. One invariant
//! lives here rather than in the service because only SQL can make it
//! race-proof:
//!
//! **Tenancy.** Every statement filters `tenant_id`, with the admin surface
//! passing `Option<i64>` where `NULL` is the explicit cross-tenant scope
//! (`COALESCE($n, tenant) = tenant` keeps one predicate for both shapes).
//! The marketplace browse surface is the only ownerless read, and it is
//! still fenced to one tenant plus `PUBLIC` + `PUBLISHED`.
//!
//! Acquisition, payment and entitlement are owned by the sdkwork-order order
//! center and sdkwork-payment: `deploy_app_template.install_count` is a
//! commerce-fed counter maintained by the order center, and this store only
//! reads it for marketplace ranking and response mapping.

use sdkwork_deploy_contract::{
    AppTemplatePage, AppTemplateResponse, AppTemplateSummaryPage, AppTemplateSummaryResponse,
    AppTemplateVersionPage, AppTemplateVersionResponse, CreateAppTemplateRequest,
    CreateAppTemplateVersionRequest, CreateTemplateCategoryRequest, DeployServiceError,
    DeployServiceResult, ListAppTemplatesAdminQuery, ListAppTemplatesQuery,
    ListMarketplaceTemplatesQuery, TemplateCategoryPage, TemplateCategoryResponse,
    UpdateAppTemplateAdminRequest, UpdateAppTemplateRequest, UpdateTemplateCategoryRequest,
    MARKETPLACE_SORT_NEWEST, MARKETPLACE_SORT_POPULAR, TEMPLATE_CATEGORY_STATUS_ACTIVE,
    TEMPLATE_PRICING_FREE, TEMPLATE_PRICING_PAID, TEMPLATE_STATUS_DISABLED,
    TEMPLATE_STATUS_PUBLISHED, TEMPLATE_TYPE_APP, TEMPLATE_TYPE_PPT, TEMPLATE_TYPE_VIDEO,
    TEMPLATE_VERSION_STATUS_DRAFT, TEMPLATE_VERSION_STATUS_PUBLISHED,
};
use sqlx::{postgres::PgRow, AssertSqlSafe, Row};

use crate::support::{
    datetime_from_row, new_uuid, next_id, optional_datetime_from_row, pagination,
    resolve_app_internal_id, store_error, string_list_from_row, string_list_to_json,
};
use crate::DeployRepository;

const CATEGORY_SELECT: &str = "c.uuid, pc.uuid AS parent_uuid, c.category_key, c.display_name,
    c.description, c.sort_order, c.status, c.created_at, c.updated_at, c.version";

const TEMPLATE_SELECT: &str =
    "t.uuid, t.template_type, t.template_key, t.display_name, t.summary, t.description,
    t.app_uuid, c.uuid AS category_uuid, t.author_user_id, t.icon_media_ref, t.cover_media_ref,
    t.visibility, t.pricing_model, t.price_minor, t.currency, t.status, t.review_note,
    t.is_featured, t.install_count, t.view_count, t.latest_version_uuid,
    t.created_at, t.updated_at, t.version";

const TEMPLATE_SUMMARY_SELECT: &str =
    "t.uuid, t.template_type, t.template_key, t.display_name, t.summary,
    c.uuid AS category_uuid, t.icon_media_ref, t.cover_media_ref, t.visibility, t.pricing_model,
    t.price_minor, t.currency, t.status, t.is_featured, t.install_count, t.view_count,
    t.latest_version_uuid, t.updated_at, t.version";

const VERSION_SELECT: &str = "v.uuid, t.uuid AS template_uuid, v.template_version, v.changelog,
    v.artifact_uuid, v.source_app_version, v.platform_targets_json, v.package_size_bytes,
    v.checksum_sha256, v.status, v.published_at, v.created_at, v.updated_at, v.version";

/// `($n = 0 OR x.tenant_id = $n)` scoped to one bound parameter: the admin
/// surface binds `0` for the cross-tenant scope, app-side callers never use
/// this predicate (they bind a hard tenant equality).
fn optional_tenant_predicate(column: &str, parameter: usize) -> String {
    format!("({column} = ${parameter} OR ${parameter} = 0)")
}

fn map_category_row(row: &PgRow) -> Result<TemplateCategoryResponse, sqlx::Error> {
    Ok(TemplateCategoryResponse {
        id: row.try_get("uuid")?,
        parent_id: row.try_get::<Option<String>, _>("parent_uuid")?,
        category_key: row.try_get("category_key")?,
        display_name: row.try_get("display_name")?,
        description: row.try_get::<Option<String>, _>("description")?,
        sort_order: row.try_get::<i32, _>("sort_order")?,
        status: row.try_get("status")?,
        created_at: datetime_from_row(row, "created_at")?,
        updated_at: datetime_from_row(row, "updated_at")?,
        version: row.try_get::<i64, _>("version")?.to_string(),
    })
}

fn map_template_row(row: &PgRow) -> Result<AppTemplateResponse, sqlx::Error> {
    Ok(AppTemplateResponse {
        id: row.try_get("uuid")?,
        template_type: row.try_get("template_type")?,
        template_key: row.try_get("template_key")?,
        display_name: row.try_get("display_name")?,
        summary: row.try_get("summary")?,
        description: row.try_get("description")?,
        app_uuid: row.try_get("app_uuid")?,
        category_uuid: row.try_get("category_uuid")?,
        author_user_id: row.try_get::<i64, _>("author_user_id")?.to_string(),
        icon_media_ref: row.try_get::<Option<String>, _>("icon_media_ref")?,
        cover_media_ref: row.try_get::<Option<String>, _>("cover_media_ref")?,
        visibility: row.try_get("visibility")?,
        pricing_model: row.try_get("pricing_model")?,
        price_minor: row.try_get::<i64, _>("price_minor")?.to_string(),
        currency: row.try_get("currency")?,
        status: row.try_get("status")?,
        review_note: row.try_get::<Option<String>, _>("review_note")?,
        is_featured: row.try_get("is_featured")?,
        install_count: row.try_get::<i64, _>("install_count")?.to_string(),
        view_count: row.try_get::<i64, _>("view_count")?.to_string(),
        latest_version_uuid: row.try_get::<Option<String>, _>("latest_version_uuid")?,
        created_at: datetime_from_row(row, "created_at")?,
        updated_at: datetime_from_row(row, "updated_at")?,
        version: row.try_get::<i64, _>("version")?.to_string(),
    })
}

fn map_template_summary_row(row: &PgRow) -> Result<AppTemplateSummaryResponse, sqlx::Error> {
    Ok(AppTemplateSummaryResponse {
        id: row.try_get("uuid")?,
        template_type: row.try_get("template_type")?,
        template_key: row.try_get("template_key")?,
        display_name: row.try_get("display_name")?,
        summary: row.try_get("summary")?,
        category_uuid: row.try_get("category_uuid")?,
        icon_media_ref: row.try_get::<Option<String>, _>("icon_media_ref")?,
        cover_media_ref: row.try_get::<Option<String>, _>("cover_media_ref")?,
        visibility: row.try_get("visibility")?,
        pricing_model: row.try_get("pricing_model")?,
        price_minor: row.try_get::<i64, _>("price_minor")?.to_string(),
        currency: row.try_get("currency")?,
        status: row.try_get("status")?,
        is_featured: row.try_get("is_featured")?,
        install_count: row.try_get::<i64, _>("install_count")?.to_string(),
        view_count: row.try_get::<i64, _>("view_count")?.to_string(),
        latest_version_uuid: row.try_get::<Option<String>, _>("latest_version_uuid")?,
        updated_at: datetime_from_row(row, "updated_at")?,
        version: row.try_get::<i64, _>("version")?.to_string(),
    })
}

fn map_version_row(row: &PgRow) -> Result<AppTemplateVersionResponse, sqlx::Error> {
    let targets = string_list_from_row(row, "platform_targets_json")?.unwrap_or_default();
    Ok(AppTemplateVersionResponse {
        id: row.try_get("uuid")?,
        template_uuid: row.try_get("template_uuid")?,
        template_version: row.try_get("template_version")?,
        changelog: row.try_get("changelog")?,
        artifact_uuid: row.try_get::<Option<String>, _>("artifact_uuid")?,
        source_app_version: row.try_get::<Option<String>, _>("source_app_version")?,
        platform_targets: targets,
        package_size_bytes: row.try_get::<i64, _>("package_size_bytes")?.to_string(),
        checksum_sha256: row.try_get::<Option<String>, _>("checksum_sha256")?,
        status: row.try_get("status")?,
        published_at: optional_datetime_from_row(row, "published_at")?,
        created_at: datetime_from_row(row, "created_at")?,
        updated_at: datetime_from_row(row, "updated_at")?,
        version: row.try_get::<i64, _>("version")?.to_string(),
    })
}

/// Parses an inbound minor-unit price. The wire is a string (API_SPEC §13.6);
/// the column is `BIGINT`. `None` on a create means "let the pricing model
/// decide" (FREE → 0), which is why the default lives with the caller.
fn parse_price_minor(value: Option<&str>) -> DeployServiceResult<i64> {
    match value {
        None => Ok(0),
        Some(raw) => raw
            .trim()
            .parse::<i64>()
            .map_err(|_| DeployServiceError::validation("priceMinor must be a decimal integer")),
    }
}

fn parse_package_size(value: Option<&str>) -> DeployServiceResult<i64> {
    match value {
        None => Ok(0),
        Some(raw) => raw.trim().parse::<i64>().map_err(|_| {
            DeployServiceError::validation("packageSizeBytes must be a decimal integer")
        }),
    }
}

fn require_text(value: &str, field: &str, max_len: usize) -> DeployServiceResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DeployServiceError::validation(&format!(
            "{field} is required"
        )));
    }
    if trimmed.chars().count() > max_len {
        return Err(DeployServiceError::validation(&format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(trimmed.to_owned())
}

impl DeployRepository {
    // -- categories --------------------------------------------------------------

    pub(super) async fn list_template_categories_repo(
        &self,
        tenant_id: Option<i64>,
        include_disabled: bool,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<TemplateCategoryPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let status_filter = if include_disabled {
            ""
        } else {
            TEMPLATE_CATEGORY_STATUS_ACTIVE
        };
        let predicate = format!(
            "{} AND c.deleted_at IS NULL
            AND ($2 = '' OR c.status = $2)",
            optional_tenant_predicate("c.tenant_id", 1)
        );
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_app_template_category c WHERE {predicate}"
        )))
        .bind(tenant_id.unwrap_or(0))
        .bind(status_filter)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_app_template_category", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {CATEGORY_SELECT} FROM deploy_app_template_category c
             LEFT JOIN deploy_app_template_category pc ON pc.id = c.parent_id
             WHERE {predicate}
             ORDER BY c.sort_order ASC, c.id ASC LIMIT $3 OFFSET $4"
        )))
        .bind(tenant_id.unwrap_or(0))
        .bind(status_filter)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_template_category", error))?;
        let items = rows
            .iter()
            .map(map_category_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                DeployServiceError::Internal(format!("map deploy_app_template_category: {error}"))
            })?;
        Ok(TemplateCategoryPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn create_template_category_repo(
        &self,
        tenant_id: i64,
        _organization_id: Option<i64>,
        actor_id: Option<i64>,
        request: &CreateTemplateCategoryRequest,
    ) -> DeployServiceResult<TemplateCategoryResponse> {
        let category_key = require_text(&request.category_key, "categoryKey", 64)?;
        let display_name = require_text(&request.display_name, "displayName", 128)?;
        let parent_id = match request.parent_id.as_deref() {
            None | Some("") => None,
            Some(parent_uuid) => Some(
                self.resolve_category_internal_id(tenant_id, parent_uuid)
                    .await?,
            ),
        };
        let category_id = next_id(self.id_generator())?;
        let category_uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_app_template_category (
                id, uuid, tenant_id, organization_id, parent_id, category_key, display_name,
                description, sort_order, status, created_by, updated_by
            ) VALUES ($1, $2, $3, COALESCE($4, 0), $5, $6, $7, $8, COALESCE($9, 0), 'ACTIVE', $10, $10)",
        )
        .bind(category_id)
        .bind(&category_uuid)
        .bind(tenant_id)
        .bind(_organization_id)
        .bind(parent_id)
        .bind(&category_key)
        .bind(&display_name)
        .bind(request.description.as_deref().map(str::trim).filter(|value| !value.is_empty()))
        .bind(request.sort_order)
        .bind(actor_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_app_template_category", error))?;
        self.retrieve_template_category_repo(Some(tenant_id), &category_uuid)
            .await
    }

    pub(super) async fn retrieve_template_category_repo(
        &self,
        tenant_id: Option<i64>,
        category_uuid: &str,
    ) -> DeployServiceResult<TemplateCategoryResponse> {
        let predicate = format!(
            "c.uuid = $2 AND c.deleted_at IS NULL AND {}",
            optional_tenant_predicate("c.tenant_id", 1)
        );
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {CATEGORY_SELECT} FROM deploy_app_template_category c
             LEFT JOIN deploy_app_template_category pc ON pc.id = c.parent_id
             WHERE {predicate}"
        )))
        .bind(tenant_id.unwrap_or(0))
        .bind(category_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_app_template_category", error))?;
        row.as_ref()
            .map(map_category_row)
            .transpose()
            .map_err(|error| DeployServiceError::Internal(format!("map category: {error}")))?
            .ok_or_else(|| DeployServiceError::not_found("template category not found"))
    }

    pub(super) async fn update_template_category_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        category_uuid: &str,
        request: &UpdateTemplateCategoryRequest,
    ) -> DeployServiceResult<TemplateCategoryResponse> {
        let parent_id = match request.parent_id.as_deref() {
            None | Some("") => None,
            Some(parent_uuid) => {
                let parent_internal = self
                    .resolve_category_internal_id(tenant_id, parent_uuid)
                    .await?;
                if parent_internal
                    == self
                        .resolve_category_internal_id(tenant_id, category_uuid)
                        .await?
                {
                    return Err(DeployServiceError::validation(
                        "a category cannot be its own parent",
                    ));
                }
                Some(parent_internal)
            }
        };
        let result = sqlx::query(
            "UPDATE deploy_app_template_category SET
                parent_id = COALESCE($3, parent_id),
                display_name = COALESCE($4, display_name),
                description = COALESCE($5, description),
                sort_order = COALESCE($6, sort_order),
                status = COALESCE($7, status),
                updated_by = COALESCE($8, updated_by),
                updated_at = NOW(),
                version = version + 1
            WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(category_uuid)
        .bind(parent_id)
        .bind(
            request
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )
        .bind(
            request
                .description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )
        .bind(request.sort_order)
        .bind(request.status.as_deref())
        .bind(actor_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("update deploy_app_template_category", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found("template category not found"));
        }
        self.retrieve_template_category_repo(Some(tenant_id), category_uuid)
            .await
    }

    pub(super) async fn delete_template_category_repo(
        &self,
        tenant_id: i64,
        category_uuid: &str,
    ) -> DeployServiceResult<()> {
        let category_id = self
            .resolve_category_internal_id(tenant_id, category_uuid)
            .await?;
        let children: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deploy_app_template_category
             WHERE parent_id = $1 AND deleted_at IS NULL",
        )
        .bind(category_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count child categories", error))?;
        if children > 0 {
            return Err(DeployServiceError::conflict(
                "category still has child categories",
            ));
        }
        let templates: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deploy_app_template
             WHERE category_id = $1 AND deleted_at IS NULL",
        )
        .bind(category_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count category templates", error))?;
        if templates > 0 {
            return Err(DeployServiceError::conflict(
                "category still has app templates",
            ));
        }
        sqlx::query(
            "UPDATE deploy_app_template_category SET deleted_at = NOW()
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(category_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("delete deploy_app_template_category", error))?;
        Ok(())
    }

    async fn resolve_category_internal_id(
        &self,
        tenant_id: i64,
        category_uuid: &str,
    ) -> DeployServiceResult<i64> {
        let row = sqlx::query(
            "SELECT id, status FROM deploy_app_template_category
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(category_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_app_template_category id", error))?;
        let row =
            row.ok_or_else(|| DeployServiceError::validation("template category not found"))?;
        let status: String = row
            .try_get("status")
            .map_err(|error| DeployServiceError::Internal(format!("read status: {error}")))?;
        if status != TEMPLATE_CATEGORY_STATUS_ACTIVE {
            return Err(DeployServiceError::validation(
                "template category is disabled",
            ));
        }
        row.try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("read id: {error}")))
    }

    // -- marketplace browse ------------------------------------------------------

    pub(super) async fn list_marketplace_templates_repo(
        &self,
        tenant_id: i64,
        query: &ListMarketplaceTemplatesQuery,
    ) -> DeployServiceResult<AppTemplateSummaryPage> {
        let (page, page_size, offset) = pagination(query.page, query.page_size);
        let keyword = query
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{}%", value.to_ascii_lowercase()))
            .unwrap_or_default();
        let pricing = query.pricing_model.as_deref().unwrap_or("");
        let sort = query.sort.as_deref().unwrap_or(MARKETPLACE_SORT_NEWEST);
        // The marketplace sort is a closed vocabulary, so the ORDER BY is chosen
        // in Rust instead of interpolated from caller input.
        let order_by = match sort {
            MARKETPLACE_SORT_POPULAR => "t.install_count DESC, t.updated_at DESC, t.id DESC",
            _ => "t.is_featured DESC, t.updated_at DESC, t.id DESC",
        };
        let template_type = query.template_type.as_deref().unwrap_or("");
        let predicate = format!(
            "t.tenant_id = $1 AND t.visibility = 'PUBLIC' AND t.status = 'PUBLISHED'
            AND t.deleted_at IS NULL
            AND ($2 = '' OR c.uuid = $2)
            AND ($3 = '' OR LOWER(t.display_name) LIKE $3 OR LOWER(t.summary) LIKE $3)
            AND ($4 = '' OR t.pricing_model = $4)
            AND ($5 = '' OR t.template_type = $5)"
        );
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_app_template t
             JOIN deploy_app_template_category c ON c.id = t.category_id
             WHERE {predicate}"
        )))
        .bind(tenant_id)
        .bind(query.category_uuid.as_deref().unwrap_or(""))
        .bind(&keyword)
        .bind(pricing)
        .bind(template_type)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count marketplace templates", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {TEMPLATE_SUMMARY_SELECT} FROM deploy_app_template t
             JOIN deploy_app_template_category c ON c.id = t.category_id
             WHERE {predicate}
             ORDER BY {order_by} LIMIT $6 OFFSET $7"
        )))
        .bind(tenant_id)
        .bind(query.category_uuid.as_deref().unwrap_or(""))
        .bind(&keyword)
        .bind(pricing)
        .bind(template_type)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list marketplace templates", error))?;
        let items = rows
            .iter()
            .map(map_template_summary_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                DeployServiceError::Internal(format!("map marketplace template: {error}"))
            })?;
        Ok(AppTemplateSummaryPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_marketplace_template_repo(
        &self,
        tenant_id: i64,
        template_uuid: &str,
    ) -> DeployServiceResult<AppTemplateResponse> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {TEMPLATE_SELECT} FROM deploy_app_template t
             JOIN deploy_app_template_category c ON c.id = t.category_id
             WHERE t.tenant_id = $1 AND t.uuid = $2 AND t.visibility = 'PUBLIC'
             AND t.status = 'PUBLISHED' AND t.deleted_at IS NULL"
        )))
        .bind(tenant_id)
        .bind(template_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve marketplace template", error))?;
        row.as_ref()
            .map(map_template_row)
            .transpose()
            .map_err(|error| DeployServiceError::Internal(format!("map template: {error}")))?
            .ok_or_else(|| DeployServiceError::not_found("template not found"))
    }

    // -- author templates --------------------------------------------------------

    pub(super) async fn list_app_templates_repo(
        &self,
        tenant_id: i64,
        author_user_id: Option<i64>,
        query: &ListAppTemplatesQuery,
    ) -> DeployServiceResult<AppTemplatePage> {
        let (page, page_size, offset) = pagination(query.page, query.page_size);
        let keyword = query
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{}%", value.to_ascii_lowercase()))
            .unwrap_or_default();
        let status = query.status.as_deref().unwrap_or("");
        // The author list is the caller's own inventory; a caller with no user
        // subject sees nothing rather than the whole tenant (the inverse of the
        // zone owner gate, where NULL-owner rows are the platform's).
        let predicate = format!(
            "t.tenant_id = $1 AND t.deleted_at IS NULL
            AND ($2 <= 0 OR t.author_user_id = $2)
            AND ($3 = '' OR t.status = $3)
            AND ($4 = '' OR LOWER(t.display_name) LIKE $4 OR LOWER(t.summary) LIKE $4)"
        );
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_app_template t WHERE {predicate}"
        )))
        .bind(tenant_id)
        .bind(author_user_id.unwrap_or(0))
        .bind(status)
        .bind(&keyword)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_app_template", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {TEMPLATE_SELECT} FROM deploy_app_template t
             JOIN deploy_app_template_category c ON c.id = t.category_id
             WHERE {predicate}
             ORDER BY t.updated_at DESC, t.id DESC LIMIT $5 OFFSET $6"
        )))
        .bind(tenant_id)
        .bind(author_user_id.unwrap_or(0))
        .bind(status)
        .bind(&keyword)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_template", error))?;
        let items = rows
            .iter()
            .map(map_template_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DeployServiceError::Internal(format!("map template: {error}")))?;
        Ok(AppTemplatePage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn list_app_templates_admin_repo(
        &self,
        tenant_id: Option<i64>,
        query: &ListAppTemplatesAdminQuery,
    ) -> DeployServiceResult<AppTemplatePage> {
        let (page, page_size, offset) = pagination(query.page, query.page_size);
        let keyword = query
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{}%", value.to_ascii_lowercase()))
            .unwrap_or_default();
        let status = query.status.as_deref().unwrap_or("");
        let visibility = query.visibility.as_deref().unwrap_or("");
        let predicate = format!(
            "{} AND t.deleted_at IS NULL
            AND ($2 = '' OR t.status = $2)
            AND ($3 = '' OR c.uuid = $3)
            AND ($4 = '' OR t.visibility = $4)
            AND ($5 = '' OR LOWER(t.display_name) LIKE $5 OR LOWER(t.summary) LIKE $5)
            AND ($6 = '' OR t.template_type = $6)",
            optional_tenant_predicate("t.tenant_id", 1)
        );
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_app_template t
             JOIN deploy_app_template_category c ON c.id = t.category_id
             WHERE {predicate}"
        )))
        .bind(tenant_id.unwrap_or(0))
        .bind(status)
        .bind(query.category_uuid.as_deref().unwrap_or(""))
        .bind(visibility)
        .bind(&keyword)
        .bind(query.template_type.as_deref().unwrap_or(""))
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count admin templates", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {TEMPLATE_SELECT} FROM deploy_app_template t
             JOIN deploy_app_template_category c ON c.id = t.category_id
             WHERE {predicate}
             ORDER BY t.updated_at DESC, t.id DESC LIMIT $7 OFFSET $8"
        )))
        .bind(tenant_id.unwrap_or(0))
        .bind(status)
        .bind(query.category_uuid.as_deref().unwrap_or(""))
        .bind(visibility)
        .bind(&keyword)
        .bind(query.template_type.as_deref().unwrap_or(""))
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list admin templates", error))?;
        let items = rows
            .iter()
            .map(map_template_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DeployServiceError::Internal(format!("map template: {error}")))?;
        Ok(AppTemplatePage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_app_template_repo(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        template_uuid: &str,
    ) -> DeployServiceResult<AppTemplateResponse> {
        // `$3 <= 0` reads "no ownership gate": the marketplace and admin reads
        // pass NULL, the author read passes its subject.
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {TEMPLATE_SELECT} FROM deploy_app_template t
             JOIN deploy_app_template_category c ON c.id = t.category_id
             WHERE t.uuid = $1 AND t.deleted_at IS NULL
             AND (t.tenant_id = $2 OR $2 = 0)
             AND (t.author_user_id = $3 OR $3 <= 0)"
        )))
        .bind(template_uuid)
        .bind(tenant_id.unwrap_or(0))
        .bind(author_user_id.unwrap_or(0))
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_app_template", error))?;
        row.as_ref()
            .map(map_template_row)
            .transpose()
            .map_err(|error| DeployServiceError::Internal(format!("map template: {error}")))?
            .ok_or_else(|| DeployServiceError::not_found("template not found"))
    }

    pub(super) async fn create_app_template_repo(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        request: &CreateAppTemplateRequest,
    ) -> DeployServiceResult<AppTemplateResponse> {
        let template_key = require_text(&request.template_key, "templateKey", 64)?;
        let display_name = require_text(&request.display_name, "displayName", 200)?;
        let summary = require_text(&request.summary, "summary", 512)?;
        let template_type = request
            .template_type
            .as_deref()
            .unwrap_or(TEMPLATE_TYPE_APP);
        if template_type != TEMPLATE_TYPE_APP
            && template_type != TEMPLATE_TYPE_PPT
            && template_type != TEMPLATE_TYPE_VIDEO
        {
            return Err(DeployServiceError::validation("templateType is invalid"));
        }
        // The author must own the source app: the resolve helper is
        // tenant-scoped and soft-delete aware, so a foreign or deleted
        // `app_uuid` is refused before any row is written.
        resolve_app_internal_id(&self.pool, tenant_id, &request.app_uuid).await?;
        let category_id = self
            .resolve_category_internal_id(tenant_id, &request.category_uuid)
            .await?;
        let visibility = request.visibility.as_deref().unwrap_or("PRIVATE");
        if visibility != "PUBLIC" && visibility != "PRIVATE" {
            return Err(DeployServiceError::validation("visibility is invalid"));
        }
        let pricing_model = request
            .pricing_model
            .as_deref()
            .unwrap_or(TEMPLATE_PRICING_FREE);
        let price_minor = match pricing_model {
            TEMPLATE_PRICING_PAID => {
                let price = parse_price_minor(request.price_minor.as_deref())?;
                if price <= 0 {
                    return Err(DeployServiceError::validation(
                        "paid templates require a positive priceMinor",
                    ));
                }
                price
            }
            TEMPLATE_PRICING_FREE => 0,
            _ => return Err(DeployServiceError::validation("pricingModel is invalid")),
        };
        let currency = request
            .currency
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("CNY")
            .to_owned();
        let author_user_id = actor_id.filter(|id| *id > 0).ok_or_else(|| {
            DeployServiceError::forbidden("author identity is required to publish a template")
        })?;

        let template_id = next_id(self.id_generator())?;
        let template_uuid = new_uuid();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin create deploy_app_template", error))?;
        sqlx::query(
            "INSERT INTO deploy_app_template (
                id, uuid, tenant_id, organization_id, category_id, author_user_id, app_uuid,
                template_type, template_key, display_name, summary, description, icon_media_ref,
                cover_media_ref, visibility, pricing_model, price_minor, currency, status,
                created_by, updated_by
            ) VALUES ($1, $2, $3, COALESCE($4, 0), $5, $6, $7, $8, $9, $10, $11, $12, $13,
                $14, $15, $16, $17, $18, 'DRAFT', $19, $19)",
        )
        .bind(template_id)
        .bind(&template_uuid)
        .bind(tenant_id)
        .bind(organization_id)
        .bind(category_id)
        .bind(author_user_id)
        .bind(&request.app_uuid)
        .bind(template_type)
        .bind(&template_key)
        .bind(&display_name)
        .bind(&summary)
        .bind(request.description.as_deref().unwrap_or(""))
        .bind(request.icon_media_ref.as_deref())
        .bind(request.cover_media_ref.as_deref())
        .bind(visibility)
        .bind(pricing_model)
        .bind(price_minor)
        .bind(&currency)
        .bind(actor_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy_app_template", error))?;
        if let Some(initial) = request.initial_version.as_ref() {
            self.insert_template_version(
                &mut transaction,
                tenant_id,
                organization_id,
                template_id,
                actor_id,
                initial,
                TEMPLATE_VERSION_STATUS_DRAFT,
            )
            .await?;
        }
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit create deploy_app_template", error))?;
        self.retrieve_app_template_repo(Some(tenant_id), None, &template_uuid)
            .await
    }

    pub(super) async fn update_app_template_repo(
        &self,
        tenant_id: i64,
        author_user_id: i64,
        template_uuid: &str,
        request: &UpdateAppTemplateRequest,
    ) -> DeployServiceResult<AppTemplateResponse> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin update deploy_app_template", error))?;
        let row = sqlx::query(
            "SELECT status, pricing_model, price_minor FROM deploy_app_template
             WHERE tenant_id = $1 AND uuid = $2 AND author_user_id = $3 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(template_uuid)
        .bind(author_user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_app_template", error))?;
        let row = row.ok_or_else(|| DeployServiceError::not_found("template not found"))?;
        let status: String = row
            .try_get("status")
            .map_err(|error| DeployServiceError::Internal(format!("read status: {error}")))?;
        if status == TEMPLATE_STATUS_DISABLED {
            return Err(DeployServiceError::conflict(
                "a disabled template cannot be edited",
            ));
        }
        let pricing_model =
            request
                .pricing_model
                .as_deref()
                .unwrap_or(row.try_get("pricing_model").map_err(|error| {
                    DeployServiceError::Internal(format!("read pricing model: {error}"))
                })?);
        let current_price: i64 = row
            .try_get("price_minor")
            .map_err(|error| DeployServiceError::Internal(format!("read price: {error}")))?;
        let price_minor = match pricing_model {
            TEMPLATE_PRICING_PAID => {
                let price = match request.price_minor.as_deref() {
                    None => current_price,
                    Some(raw) => parse_price_minor(Some(raw))?,
                };
                if price <= 0 {
                    return Err(DeployServiceError::validation(
                        "paid templates require a positive priceMinor",
                    ));
                }
                price
            }
            TEMPLATE_PRICING_FREE => 0,
            _ => return Err(DeployServiceError::validation("pricingModel is invalid")),
        };
        let category_id = match request.category_uuid.as_deref() {
            None => None,
            Some(category_uuid) => Some(
                self.resolve_category_internal_id(tenant_id, category_uuid)
                    .await?,
            ),
        };
        let visibility = request.visibility.as_deref();
        if let Some(value) = visibility {
            if value != "PUBLIC" && value != "PRIVATE" {
                return Err(DeployServiceError::validation("visibility is invalid"));
            }
        }
        sqlx::query(
            "UPDATE deploy_app_template SET
                display_name = COALESCE($4, display_name),
                summary = COALESCE($5, summary),
                description = COALESCE($6, description),
                category_id = COALESCE($7, category_id),
                visibility = COALESCE($8, visibility),
                pricing_model = $9,
                price_minor = $10,
                currency = COALESCE($11, currency),
                icon_media_ref = COALESCE($12, icon_media_ref),
                cover_media_ref = COALESCE($13, cover_media_ref),
                updated_by = $14,
                updated_at = NOW(),
                version = version + 1
            WHERE tenant_id = $1 AND uuid = $2 AND author_user_id = $3 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(template_uuid)
        .bind(author_user_id)
        .bind(
            request
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )
        .bind(
            request
                .summary
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )
        .bind(request.description.as_deref())
        .bind(category_id)
        .bind(visibility)
        .bind(pricing_model)
        .bind(price_minor)
        .bind(
            request
                .currency
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )
        .bind(request.icon_media_ref.as_deref())
        .bind(request.cover_media_ref.as_deref())
        .bind(author_user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("update deploy_app_template", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit update deploy_app_template", error))?;
        self.retrieve_app_template_repo(Some(tenant_id), None, template_uuid)
            .await
    }

    pub(super) async fn delete_app_template_repo(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        template_uuid: &str,
    ) -> DeployServiceResult<()> {
        let result = sqlx::query(
            "UPDATE deploy_app_template SET deleted_at = NOW()
             WHERE uuid = $1 AND deleted_at IS NULL
             AND (tenant_id = $2 OR $2 = 0)
             AND (author_user_id = $3 OR $3 <= 0)",
        )
        .bind(template_uuid)
        .bind(tenant_id.unwrap_or(0))
        .bind(author_user_id.unwrap_or(0))
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("delete deploy_app_template", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found("template not found"));
        }
        Ok(())
    }

    pub(super) async fn submit_app_template_repo(
        &self,
        tenant_id: i64,
        author_user_id: i64,
        template_uuid: &str,
    ) -> DeployServiceResult<AppTemplateResponse> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin submit deploy_app_template", error))?;
        let row = sqlx::query(
            "SELECT id, status FROM deploy_app_template
             WHERE tenant_id = $1 AND uuid = $2 AND author_user_id = $3 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(template_uuid)
        .bind(author_user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_app_template", error))?;
        let row = row.ok_or_else(|| DeployServiceError::not_found("template not found"))?;
        let template_id: i64 = row
            .try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("read id: {error}")))?;
        let status: String = row
            .try_get("status")
            .map_err(|error| DeployServiceError::Internal(format!("read status: {error}")))?;
        if status != "DRAFT" && status != "REJECTED" {
            return Err(DeployServiceError::conflict(&format!(
                "a template in status {status} cannot be submitted for review"
            )));
        }
        let versions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deploy_app_template_version
             WHERE template_id = $1 AND deleted_at IS NULL",
        )
        .bind(template_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error("count template versions", error))?;
        if versions == 0 {
            return Err(DeployServiceError::validation(
                "a template needs at least one version before review",
            ));
        }
        sqlx::query(
            "UPDATE deploy_app_template SET status = 'PENDING_REVIEW', updated_at = NOW(),
                updated_by = $2, version = version + 1
             WHERE id = $1",
        )
        .bind(template_id)
        .bind(author_user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("submit deploy_app_template", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit submit deploy_app_template", error))?;
        self.retrieve_app_template_repo(Some(tenant_id), None, template_uuid)
            .await
    }

    // -- versions ----------------------------------------------------------------

    pub(super) async fn list_app_template_versions_repo(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        template_uuid: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppTemplateVersionPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let predicate = format!(
            "t.uuid = $1 AND t.deleted_at IS NULL AND v.deleted_at IS NULL
            AND (t.tenant_id = $2 OR $2 = 0)
            AND (t.author_user_id = $3 OR $3 <= 0)"
        );
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_app_template_version v
             JOIN deploy_app_template t ON t.id = v.template_id
             WHERE {predicate}"
        )))
        .bind(template_uuid)
        .bind(tenant_id.unwrap_or(0))
        .bind(author_user_id.unwrap_or(0))
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_app_template_version", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {VERSION_SELECT} FROM deploy_app_template_version v
             JOIN deploy_app_template t ON t.id = v.template_id
             WHERE {predicate}
             ORDER BY v.created_at DESC, v.id DESC LIMIT $4 OFFSET $5"
        )))
        .bind(template_uuid)
        .bind(tenant_id.unwrap_or(0))
        .bind(author_user_id.unwrap_or(0))
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_template_version", error))?;
        let items = rows
            .iter()
            .map(map_version_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DeployServiceError::Internal(format!("map version: {error}")))?;
        Ok(AppTemplateVersionPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_app_template_version_repo(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        template_uuid: &str,
        version_uuid: &str,
    ) -> DeployServiceResult<AppTemplateVersionResponse> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {VERSION_SELECT} FROM deploy_app_template_version v
             JOIN deploy_app_template t ON t.id = v.template_id
             WHERE v.uuid = $1 AND t.uuid = $2 AND v.deleted_at IS NULL AND t.deleted_at IS NULL
             AND (t.tenant_id = $3 OR $3 = 0)
             AND (t.author_user_id = $4 OR $4 <= 0)"
        )))
        .bind(version_uuid)
        .bind(template_uuid)
        .bind(tenant_id.unwrap_or(0))
        .bind(author_user_id.unwrap_or(0))
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_app_template_version", error))?;
        row.as_ref()
            .map(map_version_row)
            .transpose()
            .map_err(|error| DeployServiceError::Internal(format!("map version: {error}")))?
            .ok_or_else(|| DeployServiceError::not_found("template version not found"))
    }

    pub(super) async fn create_app_template_version_repo(
        &self,
        tenant_id: i64,
        author_user_id: i64,
        template_uuid: &str,
        request: &CreateAppTemplateVersionRequest,
    ) -> DeployServiceResult<AppTemplateVersionResponse> {
        // Validate up front so a malformed version fails before the row lock;
        // the insert helper re-validates when it binds.
        require_text(&request.version, "version", 64)?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin create deploy_app_template_version", error))?;
        let row = sqlx::query(
            "SELECT id, status, organization_id FROM deploy_app_template
             WHERE tenant_id = $1 AND uuid = $2 AND author_user_id = $3 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(template_uuid)
        .bind(author_user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_app_template", error))?;
        let row = row.ok_or_else(|| DeployServiceError::not_found("template not found"))?;
        let template_id: i64 = row
            .try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("read id: {error}")))?;
        let organization_id: i64 = row
            .try_get("organization_id")
            .map_err(|error| DeployServiceError::Internal(format!("read organization: {error}")))?;
        let template_status: String = row
            .try_get("status")
            .map_err(|error| DeployServiceError::Internal(format!("read status: {error}")))?;
        if template_status == TEMPLATE_STATUS_DISABLED {
            return Err(DeployServiceError::conflict(
                "a disabled template cannot take new versions",
            ));
        }
        // A live listing publishes new versions as they land; a listing that is
        // still Draft/In Review/Rejected keeps them Draft until approval.
        let version_status = if template_status == TEMPLATE_STATUS_PUBLISHED {
            TEMPLATE_VERSION_STATUS_PUBLISHED
        } else {
            TEMPLATE_VERSION_STATUS_DRAFT
        };
        let version_uuid = self
            .insert_template_version(
                &mut transaction,
                tenant_id,
                Some(organization_id),
                template_id,
                Some(author_user_id),
                request,
                version_status,
            )
            .await?;
        if version_status == TEMPLATE_VERSION_STATUS_PUBLISHED {
            sqlx::query(
                "UPDATE deploy_app_template SET latest_version_uuid = $2, updated_at = NOW(),
                    updated_by = $3, version = version + 1
                 WHERE id = $1",
            )
            .bind(template_id)
            .bind(&version_uuid)
            .bind(author_user_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("advance deploy_app_template latest version", error))?;
        }
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit create deploy_app_template_version", error))?;
        self.retrieve_app_template_version_repo(Some(tenant_id), None, template_uuid, &version_uuid)
            .await
    }

    /// Shared version insert used by template creation (initial version) and
    /// `create_app_template_version`. Returns the new version's uuid.
    #[allow(clippy::too_many_arguments)]
    async fn insert_template_version(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: i64,
        organization_id: Option<i64>,
        template_id: i64,
        actor_id: Option<i64>,
        request: &CreateAppTemplateVersionRequest,
        status: &str,
    ) -> DeployServiceResult<String> {
        let template_version = require_text(&request.version, "version", 64)?;
        let package_size = parse_package_size(request.package_size_bytes.as_deref())?;
        let version_id = next_id(self.id_generator())?;
        let version_uuid = new_uuid();
        let published_at = if status == TEMPLATE_VERSION_STATUS_PUBLISHED {
            Some(chrono::Utc::now())
        } else {
            None
        };
        sqlx::query(
            "INSERT INTO deploy_app_template_version (
                id, uuid, tenant_id, organization_id, template_id, template_version, changelog,
                artifact_uuid, source_app_version, platform_targets_json, package_size_bytes,
                checksum_sha256, status, published_at, created_by, updated_by
            ) VALUES ($1, $2, $3, COALESCE($4, 0), $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15)",
        )
        .bind(version_id)
        .bind(&version_uuid)
        .bind(tenant_id)
        .bind(organization_id)
        .bind(template_id)
        .bind(&template_version)
        .bind(request.changelog.as_deref().unwrap_or(""))
        .bind(request.artifact_uuid.as_deref())
        .bind(request.source_app_version.as_deref())
        // The column is NOT NULL DEFAULT '[]'; an absent facet means an empty
        // target set, and binding NULL here would bypass the column default.
        .bind(
            string_list_to_json(request.platform_targets.as_ref())
                .unwrap_or_else(|| serde_json::json!([])),
        )
        .bind(package_size)
        .bind(request.checksum_sha256.as_deref())
        .bind(status)
        .bind(published_at)
        .bind(actor_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| store_error("insert deploy_app_template_version", error))?;
        Ok(version_uuid)
    }

    // -- admin moderation ----------------------------------------------------------

    pub(super) async fn review_app_template_repo(
        &self,
        tenant_id: Option<i64>,
        operator_id: Option<i64>,
        template_uuid: &str,
        request: &UpdateAppTemplateAdminRequest,
    ) -> DeployServiceResult<AppTemplateResponse> {
        if let Some(status) = request.status.as_deref() {
            if status != TEMPLATE_STATUS_PUBLISHED
                && status != "REJECTED"
                && status != TEMPLATE_STATUS_DISABLED
                && status != "PENDING_REVIEW"
            {
                return Err(DeployServiceError::validation(
                    "admin template status must be PUBLISHED, REJECTED, DISABLED or PENDING_REVIEW",
                ));
            }
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin review deploy_app_template", error))?;
        let row = sqlx::query(
            "SELECT id, status FROM deploy_app_template
             WHERE uuid = $1 AND deleted_at IS NULL AND (tenant_id = $2 OR $2 = 0)
             FOR UPDATE",
        )
        .bind(template_uuid)
        .bind(tenant_id.unwrap_or(0))
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_app_template", error))?;
        let row = row.ok_or_else(|| DeployServiceError::not_found("template not found"))?;
        let template_id: i64 = row
            .try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("read id: {error}")))?;
        let next_status = request.status.as_deref();
        sqlx::query(
            "UPDATE deploy_app_template SET
                status = COALESCE($2, status),
                review_note = COALESCE($3, review_note),
                is_featured = COALESCE($4, is_featured),
                visibility = COALESCE($5, visibility),
                updated_by = COALESCE($6, updated_by),
                updated_at = NOW(),
                version = version + 1
             WHERE id = $1",
        )
        .bind(template_id)
        .bind(next_status)
        .bind(
            request
                .review_note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )
        .bind(request.is_featured)
        .bind(request.visibility.as_deref())
        .bind(operator_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("review deploy_app_template", error))?;
        if next_status == Some(TEMPLATE_STATUS_PUBLISHED) {
            // Approval publishes every draft version: the author submitted the
            // listing as a whole, and partial approvals would silently strand
            // the newest artifact.
            sqlx::query(
                "UPDATE deploy_app_template_version SET status = $2,
                    published_at = COALESCE(published_at, NOW()),
                    updated_by = COALESCE($3, updated_by),
                    updated_at = NOW(), version = version + 1
                 WHERE template_id = $1 AND status = $4 AND deleted_at IS NULL",
            )
            .bind(template_id)
            .bind(TEMPLATE_VERSION_STATUS_PUBLISHED)
            .bind(operator_id)
            .bind(TEMPLATE_VERSION_STATUS_DRAFT)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("publish template versions", error))?;
            let latest: Option<String> = sqlx::query_scalar(
                "SELECT uuid FROM deploy_app_template_version
                 WHERE template_id = $1 AND status = $2 AND deleted_at IS NULL
                 ORDER BY created_at DESC, id DESC LIMIT 1",
            )
            .bind(template_id)
            .bind(TEMPLATE_VERSION_STATUS_PUBLISHED)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("resolve latest template version", error))?;
            if latest.is_none() {
                return Err(DeployServiceError::conflict(
                    "a template cannot be published without at least one version",
                ));
            }
            sqlx::query("UPDATE deploy_app_template SET latest_version_uuid = $2 WHERE id = $1")
                .bind(template_id)
                .bind(latest)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("pin latest template version", error))?;
        }
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit review deploy_app_template", error))?;
        self.retrieve_app_template_repo(tenant_id, None, template_uuid)
            .await
    }
}
