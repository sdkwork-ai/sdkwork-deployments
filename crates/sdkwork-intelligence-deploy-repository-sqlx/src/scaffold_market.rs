//! App scaffold marketplace catalog storage.
//!
//! Three tables (`deploy_application_scaffold_category`, `deploy_application_scaffold`,
//! `deploy_application_scaffold_version`) behind the marketplace port. One invariant
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
//! center and sdkwork-payment: `deploy_application_scaffold.install_count` is a
//! commerce-fed counter maintained by the order center, and this store only
//! reads it for marketplace ranking and response mapping.

use sdkwork_deploy_contract::{
    ApplicationScaffoldPage, ApplicationScaffoldResponse, ApplicationScaffoldScreenshot,
    ApplicationScaffoldSummaryPage, ApplicationScaffoldSummaryResponse,
    ApplicationScaffoldVersionPage, ApplicationScaffoldVersionResponse,
    CreateApplicationScaffoldRequest, CreateApplicationScaffoldVersionRequest,
    CreateScaffoldCategoryRequest, DeployServiceError, DeployServiceResult,
    ListApplicationScaffoldsAdminQuery, ListApplicationScaffoldsQuery,
    ListMarketplaceScaffoldsQuery, ScaffoldCategoryPage, ScaffoldCategoryResponse,
    UpdateApplicationScaffoldAdminRequest, UpdateApplicationScaffoldRequest,
    UpdateScaffoldCategoryRequest, MARKETPLACE_SORT_NEWEST, MARKETPLACE_SORT_POPULAR,
    SCAFFOLD_CATEGORY_STATUS_ACTIVE, SCAFFOLD_PRICING_FREE, SCAFFOLD_PRICING_PAID,
    SCAFFOLD_STATUS_DISABLED, SCAFFOLD_STATUS_PUBLISHED, SCAFFOLD_TYPE_APP, SCAFFOLD_TYPE_PPT,
    SCAFFOLD_TYPE_VIDEO, SCAFFOLD_VERSION_STATUS_DRAFT, SCAFFOLD_VERSION_STATUS_PUBLISHED,
};
use sdkwork_intelligence_deploy_service::ScaffoldReadScope;
use std::collections::HashSet;

use sqlx::{postgres::PgRow, AssertSqlSafe, Row};

use crate::support::{
    datetime_from_row, new_uuid, next_id, optional_datetime_from_row, pagination,
    resolve_app_internal_id, store_error, string_list_from_row, string_list_to_json,
};
use crate::DeployRepository;

const CATEGORY_SELECT: &str = "c.uuid, pc.uuid AS parent_uuid, c.category_key, c.display_name,
    c.description, c.sort_order, c.status, c.created_at, c.updated_at, c.version";

const SCAFFOLD_SELECT: &str =
    "t.uuid, t.template_type, t.template_key, t.display_name, t.summary, t.description,
    t.app_uuid, c.uuid AS category_uuid, t.author_user_id, t.icon_media_ref, t.cover_media_ref,
    t.visibility, t.pricing_model, t.price_minor, t.currency, t.status, t.review_note,
    t.is_featured, t.install_count, t.view_count, t.latest_version_uuid, t.metadata,
    t.created_at, t.updated_at, t.version";

const SCAFFOLD_SUMMARY_SELECT: &str =
    "t.uuid, t.template_type, t.template_key, t.display_name, t.summary,
    c.uuid AS category_uuid, t.icon_media_ref, t.cover_media_ref, t.visibility, t.pricing_model,
    t.price_minor, t.currency, t.status, t.is_featured, t.install_count, t.view_count,
    t.latest_version_uuid, t.metadata, t.updated_at, t.version";

const VERSION_SELECT: &str = "v.uuid, t.uuid AS scaffold_uuid, v.template_version, v.changelog,
    v.artifact_uuid, v.source_app_version, v.platform_targets_json, v.package_size_bytes,
    v.checksum_sha256, v.status, v.published_at, v.created_at, v.updated_at, v.version";

/// The marketplace visibility gate, kept in one string so the shelf
/// (`list_marketplace_scaffolds`), the listing read
/// (`retrieve_marketplace_scaffold`) and the consumer reads of versions and
/// artifacts cannot drift apart: a listing is market-visible while it is
/// `PUBLIC`, approved (`PUBLISHED`) and not soft-deleted.
///
/// Every call site aliases the listing table as `t`; the consumer reads add
/// their own `v.status = 'PUBLISHED'` on top of it.
const MARKET_VISIBLE_SCAFFOLD: &str =
    "t.visibility = 'PUBLIC' AND t.status = 'PUBLISHED' AND t.deleted_at IS NULL";

/// `($n = 0 OR x.tenant_id = $n)` scoped to one bound parameter: the admin
/// surface binds `0` for the cross-tenant scope, app-side callers never use
/// this predicate (they bind a hard tenant equality).
fn optional_tenant_predicate(column: &str, parameter: usize) -> String {
    format!("({column} = ${parameter} OR ${parameter} = 0)")
}

fn map_category_row(row: &PgRow) -> Result<ScaffoldCategoryResponse, sqlx::Error> {
    Ok(ScaffoldCategoryResponse {
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

/// Broadest store total (App Store). The client applies the narrower
/// per-app-type shelf limits; the repository keeps one safe upper bound.
const MAX_SCAFFOLD_SCREENSHOTS: usize = 30;
const SCAFFOLD_CATEGORY_MAX_DEPTH: i32 = 5;

/// Normalize and validate the author-provided sample pages before they reach
/// JSONB. Empty media references and duplicate pages are refused because the
/// marketplace would otherwise render broken or repeated cards.
fn normalize_screenshots(
    screenshots: &[ApplicationScaffoldScreenshot],
) -> DeployServiceResult<Vec<ApplicationScaffoldScreenshot>> {
    if screenshots.len() > MAX_SCAFFOLD_SCREENSHOTS {
        return Err(DeployServiceError::validation(format!(
            "at most {MAX_SCAFFOLD_SCREENSHOTS} screenshots are allowed"
        )));
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(screenshots.len());
    for (index, screenshot) in screenshots.iter().enumerate() {
        let media_ref = screenshot.media_ref.trim();
        if media_ref.is_empty() || media_ref.len() > 512 {
            return Err(DeployServiceError::validation(
                "screenshot mediaRef must be between 1 and 512 characters",
            ));
        }
        if !seen.insert(media_ref.to_owned()) {
            return Err(DeployServiceError::validation(
                "a screenshot mediaRef is repeated",
            ));
        }
        let device = screenshot
            .device
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if device
            .as_ref()
            .is_some_and(|value| value.chars().count() > 32)
        {
            return Err(DeployServiceError::validation(
                "screenshot device must be at most 32 characters",
            ));
        }
        let caption = screenshot
            .caption
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if caption
            .as_ref()
            .is_some_and(|value| value.chars().count() > 160)
        {
            return Err(DeployServiceError::validation(
                "screenshot caption must be at most 160 characters",
            ));
        }
        normalized.push(ApplicationScaffoldScreenshot {
            media_ref: media_ref.to_owned(),
            device,
            caption,
            sort_order: if screenshot.sort_order >= 0 {
                screenshot.sort_order
            } else {
                i32::try_from(index).unwrap_or(0)
            },
        });
    }
    normalized.sort_by_key(|screenshot| screenshot.sort_order);
    Ok(normalized)
}

fn metadata_with_screenshots(
    mut metadata: serde_json::Value,
    screenshots: &[ApplicationScaffoldScreenshot],
) -> DeployServiceResult<serde_json::Value> {
    let object = metadata.as_object_mut().ok_or_else(|| {
        DeployServiceError::Internal(
            "deploy_application_scaffold.metadata is not a JSON object".to_owned(),
        )
    })?;
    object.insert(
        "screenshots".to_owned(),
        serde_json::to_value(screenshots).map_err(|error| {
            DeployServiceError::Internal(format!("encode scaffold screenshots: {error}"))
        })?,
    );
    Ok(metadata)
}

fn screenshots_from_metadata(
    metadata: &serde_json::Value,
) -> Result<Vec<ApplicationScaffoldScreenshot>, sqlx::Error> {
    let Some(value) = metadata.get("screenshots") else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(value.clone()).map_err(|error| {
        sqlx::Error::Decode(
            format!("deploy_application_scaffold.metadata.screenshots is malformed: {error}")
                .into(),
        )
    })
}

fn map_scaffold_row(row: &PgRow) -> Result<ApplicationScaffoldResponse, sqlx::Error> {
    let metadata: serde_json::Value = row.try_get("metadata")?;
    Ok(ApplicationScaffoldResponse {
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
        screenshots: screenshots_from_metadata(&metadata)?,
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

async fn load_git_source_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: i64,
    app_uuid: &str,
) -> DeployServiceResult<Option<serde_json::Value>> {
    let has_app_source: bool =
        sqlx::query_scalar("SELECT to_regclass('deploy_app_source') IS NOT NULL")
            .fetch_one(&mut **tx)
            .await
            .map_err(|error| store_error("probe deploy_app_source", error))?;
    let sql = if has_app_source {
        "SELECT r.repo_provider, r.repo_url, r.default_branch, r.clone_mode,
                r.credential_secret_ref, s.source_ref, s.sub_path
         FROM deploy_source_repository r
         JOIN deploy_app a ON a.id = r.app_id
         LEFT JOIN deploy_app_source s
           ON s.app_id = a.id AND s.tenant_id = a.tenant_id AND s.deleted_at IS NULL
         WHERE a.tenant_id = $1 AND a.uuid = $2
           AND a.deleted_at IS NULL AND r.deleted_at IS NULL
         ORDER BY r.id DESC
         LIMIT 1"
    } else {
        "SELECT r.repo_provider, r.repo_url, r.default_branch, r.clone_mode,
                r.credential_secret_ref,
                NULL::text AS source_ref, NULL::text AS sub_path
         FROM deploy_source_repository r
         JOIN deploy_app a ON a.id = r.app_id
         WHERE a.tenant_id = $1 AND a.uuid = $2
           AND a.deleted_at IS NULL AND r.deleted_at IS NULL
         ORDER BY r.id DESC
         LIMIT 1"
    };
    let row = sqlx::query(sql)
        .bind(tenant_id)
        .bind(app_uuid)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| store_error("load scaffold source snapshot", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(serde_json::json!({
        "git": {
            "repoProvider": row.try_get::<String, _>("repo_provider").unwrap_or_default(),
            "repoUrl": row.try_get::<String, _>("repo_url").unwrap_or_default(),
            "repoRef": row
                .try_get::<Option<String>, _>("source_ref")
                .ok()
                .flatten()
                .or_else(|| row.try_get::<String, _>("default_branch").ok())
                .unwrap_or_default(),
            "subPath": row.try_get::<Option<String>, _>("sub_path").ok().flatten(),
            "cloneMode": row.try_get::<String, _>("clone_mode").unwrap_or_default(),
            "credentialSecretRef": row.try_get::<Option<String>, _>("credential_secret_ref").ok().flatten(),
        }
    })))
}

fn map_scaffold_summary_row(
    row: &PgRow,
) -> Result<ApplicationScaffoldSummaryResponse, sqlx::Error> {
    let metadata: serde_json::Value = row.try_get("metadata")?;
    Ok(ApplicationScaffoldSummaryResponse {
        id: row.try_get("uuid")?,
        template_type: row.try_get("template_type")?,
        template_key: row.try_get("template_key")?,
        display_name: row.try_get("display_name")?,
        summary: row.try_get("summary")?,
        category_uuid: row.try_get("category_uuid")?,
        icon_media_ref: row.try_get::<Option<String>, _>("icon_media_ref")?,
        cover_media_ref: row.try_get::<Option<String>, _>("cover_media_ref")?,
        screenshots: screenshots_from_metadata(&metadata)?,
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

fn map_version_row(row: &PgRow) -> Result<ApplicationScaffoldVersionResponse, sqlx::Error> {
    let targets = string_list_from_row(row, "platform_targets_json")?.unwrap_or_default();
    Ok(ApplicationScaffoldVersionResponse {
        id: row.try_get("uuid")?,
        scaffold_uuid: row.try_get("scaffold_uuid")?,
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

    pub(super) async fn list_scaffold_categories_repo(
        &self,
        tenant_id: Option<i64>,
        include_disabled: bool,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ScaffoldCategoryPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let status_filter = if include_disabled {
            ""
        } else {
            SCAFFOLD_CATEGORY_STATUS_ACTIVE
        };
        let predicate = format!(
            "{} AND c.deleted_at IS NULL
            AND ($2 = '' OR c.status = $2)",
            optional_tenant_predicate("c.tenant_id", 1)
        );
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_application_scaffold_category c WHERE {predicate}"
        )))
        .bind(tenant_id.unwrap_or(0))
        .bind(status_filter)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_application_scaffold_category", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {CATEGORY_SELECT} FROM deploy_application_scaffold_category c
             LEFT JOIN deploy_application_scaffold_category pc ON pc.id = c.parent_id
             WHERE {predicate}
             ORDER BY c.sort_order ASC, c.id ASC LIMIT $3 OFFSET $4"
        )))
        .bind(tenant_id.unwrap_or(0))
        .bind(status_filter)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_application_scaffold_category", error))?;
        let items = rows
            .iter()
            .map(map_category_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                DeployServiceError::Internal(format!(
                    "map deploy_application_scaffold_category: {error}"
                ))
            })?;
        Ok(ScaffoldCategoryPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn create_scaffold_category_repo(
        &self,
        tenant_id: i64,
        _organization_id: Option<i64>,
        actor_id: Option<i64>,
        request: &CreateScaffoldCategoryRequest,
    ) -> DeployServiceResult<ScaffoldCategoryResponse> {
        let category_key = require_text(&request.category_key, "categoryKey", 64)?;
        let display_name = require_text(&request.display_name, "displayName", 128)?;
        let parent_id = match request.parent_id.as_deref() {
            None | Some("") => None,
            Some(parent_uuid) => Some(
                self.resolve_category_internal_id(tenant_id, parent_uuid)
                    .await?,
            ),
        };
        if let Some(parent_internal) = parent_id {
            let parent_depth = self
                .scaffold_category_depth(tenant_id, parent_internal)
                .await?;
            if parent_depth >= SCAFFOLD_CATEGORY_MAX_DEPTH {
                return Err(DeployServiceError::validation(
                    "scaffold category depth cannot exceed 5 levels",
                ));
            }
        }
        let category_id = next_id(self.id_generator())?;
        let category_uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_application_scaffold_category (
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
        .map_err(|error| store_error("insert deploy_application_scaffold_category", error))?;
        self.retrieve_scaffold_category_repo(Some(tenant_id), &category_uuid)
            .await
    }

    pub(super) async fn retrieve_scaffold_category_repo(
        &self,
        tenant_id: Option<i64>,
        category_uuid: &str,
    ) -> DeployServiceResult<ScaffoldCategoryResponse> {
        let predicate = format!(
            "c.uuid = $2 AND c.deleted_at IS NULL AND {}",
            optional_tenant_predicate("c.tenant_id", 1)
        );
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {CATEGORY_SELECT} FROM deploy_application_scaffold_category c
             LEFT JOIN deploy_application_scaffold_category pc ON pc.id = c.parent_id
             WHERE {predicate}"
        )))
        .bind(tenant_id.unwrap_or(0))
        .bind(category_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_application_scaffold_category", error))?;
        row.as_ref()
            .map(map_category_row)
            .transpose()
            .map_err(|error| DeployServiceError::Internal(format!("map category: {error}")))?
            .ok_or_else(|| DeployServiceError::not_found("scaffold category not found"))
    }

    pub(super) async fn update_scaffold_category_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        category_uuid: &str,
        request: &UpdateScaffoldCategoryRequest,
    ) -> DeployServiceResult<ScaffoldCategoryResponse> {
        let parent_id = match request.parent_id.as_deref() {
            None | Some("") => None,
            Some(parent_uuid) => {
                let parent_internal = self
                    .resolve_category_internal_id(tenant_id, parent_uuid)
                    .await?;
                let category_internal = self
                    .resolve_category_internal_id(tenant_id, category_uuid)
                    .await?;
                if parent_internal == category_internal {
                    return Err(DeployServiceError::validation(
                        "a category cannot be its own parent",
                    ));
                }
                if self
                    .scaffold_category_is_ancestor(tenant_id, parent_internal, category_internal)
                    .await?
                {
                    return Err(DeployServiceError::validation(
                        "a category cannot be moved below its own descendant",
                    ));
                }
                let parent_depth = self
                    .scaffold_category_depth(tenant_id, parent_internal)
                    .await?;
                let subtree_height = self
                    .scaffold_category_subtree_height(tenant_id, category_internal)
                    .await?;
                if parent_depth + subtree_height > SCAFFOLD_CATEGORY_MAX_DEPTH {
                    return Err(DeployServiceError::validation(
                        "scaffold category depth cannot exceed 5 levels",
                    ));
                }
                Some(parent_internal)
            }
        };
        let result = sqlx::query(
            "UPDATE deploy_application_scaffold_category SET
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
        .map_err(|error| store_error("update deploy_application_scaffold_category", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found("scaffold category not found"));
        }
        self.retrieve_scaffold_category_repo(Some(tenant_id), category_uuid)
            .await
    }

    pub(super) async fn delete_scaffold_category_repo(
        &self,
        tenant_id: i64,
        category_uuid: &str,
    ) -> DeployServiceResult<()> {
        let category_id = self
            .resolve_category_internal_id(tenant_id, category_uuid)
            .await?;
        let children: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deploy_application_scaffold_category
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
        let scaffolds: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deploy_application_scaffold
             WHERE category_id = $1 AND deleted_at IS NULL",
        )
        .bind(category_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count category scaffolds", error))?;
        if scaffolds > 0 {
            return Err(DeployServiceError::conflict(
                "category still has app scaffolds",
            ));
        }
        sqlx::query(
            "UPDATE deploy_application_scaffold_category SET deleted_at = NOW()
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(category_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("delete deploy_application_scaffold_category", error))?;
        Ok(())
    }

    async fn resolve_category_internal_id(
        &self,
        tenant_id: i64,
        category_uuid: &str,
    ) -> DeployServiceResult<i64> {
        let row = sqlx::query(
            "SELECT id, status FROM deploy_application_scaffold_category
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(category_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_application_scaffold_category id", error))?;
        let row =
            row.ok_or_else(|| DeployServiceError::validation("scaffold category not found"))?;
        let status: String = row
            .try_get("status")
            .map_err(|error| DeployServiceError::Internal(format!("read status: {error}")))?;
        if status != SCAFFOLD_CATEGORY_STATUS_ACTIVE {
            return Err(DeployServiceError::validation(
                "scaffold category is disabled",
            ));
        }
        row.try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("read id: {error}")))
    }

    /// Depth of a scaffold category (root = 1), bounded to avoid loops on
    /// legacy corrupt data.
    async fn scaffold_category_depth(
        &self,
        tenant_id: i64,
        category_id: i64,
    ) -> DeployServiceResult<i32> {
        let depth: Option<i32> = sqlx::query_scalar(
            "WITH RECURSIVE chain AS (
                SELECT id, parent_id, 1 AS depth
                  FROM deploy_application_scaffold_category
                 WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL
                UNION ALL
                SELECT c.id, c.parent_id, chain.depth + 1
                  FROM deploy_application_scaffold_category c
                  JOIN chain ON c.id = chain.parent_id
                 WHERE c.tenant_id = $1 AND c.deleted_at IS NULL AND chain.depth < 10
            )
            SELECT MAX(depth) FROM chain",
        )
        .bind(tenant_id)
        .bind(category_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("read scaffold category depth", error))?;
        depth.ok_or_else(|| DeployServiceError::validation("scaffold category not found"))
    }

    /// Height of a scaffold category subtree (a leaf has height 1).
    async fn scaffold_category_subtree_height(
        &self,
        tenant_id: i64,
        category_id: i64,
    ) -> DeployServiceResult<i32> {
        let height: Option<i32> = sqlx::query_scalar(
            "WITH RECURSIVE tree AS (
                SELECT id, 1 AS depth
                  FROM deploy_application_scaffold_category
                 WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL
                UNION ALL
                SELECT c.id, tree.depth + 1
                  FROM deploy_application_scaffold_category c
                  JOIN tree ON c.parent_id = tree.id
                 WHERE c.tenant_id = $1 AND c.deleted_at IS NULL AND tree.depth < 10
            )
            SELECT MAX(depth) FROM tree",
        )
        .bind(tenant_id)
        .bind(category_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("read scaffold category subtree height", error))?;
        height.ok_or_else(|| DeployServiceError::validation("scaffold category not found"))
    }

    /// `true` when `ancestor_id` appears on `category_id`'s parent chain.
    async fn scaffold_category_is_ancestor(
        &self,
        tenant_id: i64,
        category_id: i64,
        ancestor_id: i64,
    ) -> DeployServiceResult<bool> {
        let is_ancestor: bool = sqlx::query_scalar(
            "WITH RECURSIVE chain AS (
                SELECT id, parent_id, 1 AS depth
                  FROM deploy_application_scaffold_category
                 WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL
                UNION ALL
                SELECT c.id, c.parent_id, chain.depth + 1
                  FROM deploy_application_scaffold_category c
                  JOIN chain ON c.id = chain.parent_id
                 WHERE c.tenant_id = $1 AND c.deleted_at IS NULL AND chain.depth < 10
            )
            SELECT EXISTS(SELECT 1 FROM chain WHERE id = $3)",
        )
        .bind(tenant_id)
        .bind(category_id)
        .bind(ancestor_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("read scaffold category ancestor chain", error))?;
        Ok(is_ancestor)
    }

    // -- marketplace browse ------------------------------------------------------

    pub(super) async fn list_marketplace_scaffolds_repo(
        &self,
        tenant_id: i64,
        query: &ListMarketplaceScaffoldsQuery,
    ) -> DeployServiceResult<ApplicationScaffoldSummaryPage> {
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
            "t.tenant_id = $1 AND {MARKET_VISIBLE_SCAFFOLD}
            AND ($2 = '' OR c.uuid = $2)
            AND ($3 = '' OR LOWER(t.display_name) LIKE $3 OR LOWER(t.summary) LIKE $3)
            AND ($4 = '' OR t.pricing_model = $4)
            AND ($5 = '' OR t.template_type = $5)"
        );
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_application_scaffold t
             JOIN deploy_application_scaffold_category c ON c.id = t.category_id
             WHERE {predicate}"
        )))
        .bind(tenant_id)
        .bind(query.category_uuid.as_deref().unwrap_or(""))
        .bind(&keyword)
        .bind(pricing)
        .bind(template_type)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count marketplace scaffolds", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {SCAFFOLD_SUMMARY_SELECT} FROM deploy_application_scaffold t
             JOIN deploy_application_scaffold_category c ON c.id = t.category_id
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
        .map_err(|error| store_error("list marketplace scaffolds", error))?;
        let items = rows
            .iter()
            .map(map_scaffold_summary_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                DeployServiceError::Internal(format!("map marketplace scaffold: {error}"))
            })?;
        Ok(ApplicationScaffoldSummaryPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_marketplace_scaffold_repo(
        &self,
        tenant_id: i64,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<ApplicationScaffoldResponse> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {SCAFFOLD_SELECT} FROM deploy_application_scaffold t
             JOIN deploy_application_scaffold_category c ON c.id = t.category_id
             WHERE t.tenant_id = $1 AND t.uuid = $2 AND {MARKET_VISIBLE_SCAFFOLD}"
        )))
        .bind(tenant_id)
        .bind(scaffold_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve marketplace scaffold", error))?;
        row.as_ref()
            .map(map_scaffold_row)
            .transpose()
            .map_err(|error| DeployServiceError::Internal(format!("map scaffold: {error}")))?
            .ok_or_else(|| DeployServiceError::not_found("scaffold not found"))
    }

    /// Ownership/marketplace state of one listing: the install flow decides
    /// between the author read and the marketplace read from this, never from
    /// the listing body. The lookup is tenant-scoped and soft-delete aware, so
    /// a foreign or deleted uuid answers `None` exactly like a missing one.
    pub(super) async fn scaffold_read_scope_repo(
        &self,
        tenant_id: i64,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<Option<ScaffoldReadScope>> {
        let row = sqlx::query(
            "SELECT author_user_id, visibility, status FROM deploy_application_scaffold
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(scaffold_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read deploy_application_scaffold read scope", error))?;
        row.map(|row| {
            Ok::<_, sqlx::Error>(ScaffoldReadScope {
                author_user_id: row.try_get("author_user_id")?,
                visibility: row.try_get("visibility")?,
                status: row.try_get("status")?,
            })
        })
        .transpose()
        .map_err(|error| DeployServiceError::Internal(format!("map scaffold read scope: {error}")))
    }

    // -- author scaffolds --------------------------------------------------------

    pub(super) async fn list_application_scaffolds_repo(
        &self,
        tenant_id: i64,
        author_user_id: Option<i64>,
        query: &ListApplicationScaffoldsQuery,
    ) -> DeployServiceResult<ApplicationScaffoldPage> {
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
            "SELECT COUNT(*) FROM deploy_application_scaffold t WHERE {predicate}"
        )))
        .bind(tenant_id)
        .bind(author_user_id.unwrap_or(0))
        .bind(status)
        .bind(&keyword)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_application_scaffold", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {SCAFFOLD_SELECT} FROM deploy_application_scaffold t
             JOIN deploy_application_scaffold_category c ON c.id = t.category_id
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
        .map_err(|error| store_error("list deploy_application_scaffold", error))?;
        let items = rows
            .iter()
            .map(map_scaffold_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DeployServiceError::Internal(format!("map scaffold: {error}")))?;
        Ok(ApplicationScaffoldPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn list_application_scaffolds_admin_repo(
        &self,
        tenant_id: Option<i64>,
        query: &ListApplicationScaffoldsAdminQuery,
    ) -> DeployServiceResult<ApplicationScaffoldPage> {
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
            "SELECT COUNT(*) FROM deploy_application_scaffold t
             JOIN deploy_application_scaffold_category c ON c.id = t.category_id
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
        .map_err(|error| store_error("count admin scaffolds", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {SCAFFOLD_SELECT} FROM deploy_application_scaffold t
             JOIN deploy_application_scaffold_category c ON c.id = t.category_id
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
        .map_err(|error| store_error("list admin scaffolds", error))?;
        let items = rows
            .iter()
            .map(map_scaffold_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DeployServiceError::Internal(format!("map scaffold: {error}")))?;
        Ok(ApplicationScaffoldPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_application_scaffold_repo(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<ApplicationScaffoldResponse> {
        // `$3 <= 0` reads "no ownership gate": the marketplace and admin reads
        // pass NULL, the author read passes its subject.
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {SCAFFOLD_SELECT} FROM deploy_application_scaffold t
             JOIN deploy_application_scaffold_category c ON c.id = t.category_id
             WHERE t.uuid = $1 AND t.deleted_at IS NULL
             AND (t.tenant_id = $2 OR $2 = 0)
             AND (t.author_user_id = $3 OR $3 <= 0)"
        )))
        .bind(scaffold_uuid)
        .bind(tenant_id.unwrap_or(0))
        .bind(author_user_id.unwrap_or(0))
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_application_scaffold", error))?;
        row.as_ref()
            .map(map_scaffold_row)
            .transpose()
            .map_err(|error| DeployServiceError::Internal(format!("map scaffold: {error}")))?
            .ok_or_else(|| DeployServiceError::not_found("scaffold not found"))
    }

    pub(super) async fn create_application_scaffold_repo(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        request: &CreateApplicationScaffoldRequest,
    ) -> DeployServiceResult<ApplicationScaffoldResponse> {
        let template_key = require_text(&request.template_key, "templateKey", 64)?;
        let display_name = require_text(&request.display_name, "displayName", 200)?;
        let summary = require_text(&request.summary, "summary", 512)?;
        let template_type = request
            .template_type
            .as_deref()
            .unwrap_or(SCAFFOLD_TYPE_APP);
        if template_type != SCAFFOLD_TYPE_APP
            && template_type != SCAFFOLD_TYPE_PPT
            && template_type != SCAFFOLD_TYPE_VIDEO
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
            .unwrap_or(SCAFFOLD_PRICING_FREE);
        let price_minor = match pricing_model {
            SCAFFOLD_PRICING_PAID => {
                let price = parse_price_minor(request.price_minor.as_deref())?;
                if price <= 0 {
                    return Err(DeployServiceError::validation(
                        "paid scaffolds require a positive priceMinor",
                    ));
                }
                price
            }
            SCAFFOLD_PRICING_FREE => 0,
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
            DeployServiceError::forbidden("author identity is required to publish a scaffold")
        })?;
        let screenshots = normalize_screenshots(&request.screenshots)?;
        let metadata = metadata_with_screenshots(serde_json::json!({}), &screenshots)?;

        let template_id = next_id(self.id_generator())?;
        let scaffold_uuid = new_uuid();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin create deploy_application_scaffold", error))?;
        sqlx::query(
            "INSERT INTO deploy_application_scaffold (
                id, uuid, tenant_id, organization_id, category_id, author_user_id, app_uuid,
                template_type, template_key, display_name, summary, description, icon_media_ref,
                cover_media_ref, visibility, pricing_model, price_minor, currency, status,
                metadata, created_by, updated_by
            ) VALUES ($1, $2, $3, COALESCE($4, 0), $5, $6, $7, $8, $9, $10, $11, $12, $13,
                $14, $15, $16, $17, $18, 'DRAFT', $20, $19, $19)",
        )
        .bind(template_id)
        .bind(&scaffold_uuid)
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
        .bind(metadata)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy_application_scaffold", error))?;
        let initial_version_snapshot =
            load_git_source_snapshot(&mut transaction, tenant_id, &request.app_uuid).await?;
        if let Some(initial) = request.initial_version.as_ref() {
            self.insert_template_version(
                &mut transaction,
                tenant_id,
                organization_id,
                template_id,
                actor_id,
                initial,
                SCAFFOLD_VERSION_STATUS_DRAFT,
                initial_version_snapshot,
            )
            .await?;
        }
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit create deploy_application_scaffold", error))?;
        self.retrieve_application_scaffold_repo(Some(tenant_id), None, &scaffold_uuid)
            .await
    }

    pub(super) async fn update_application_scaffold_repo(
        &self,
        tenant_id: i64,
        author_user_id: i64,
        scaffold_uuid: &str,
        request: &UpdateApplicationScaffoldRequest,
    ) -> DeployServiceResult<ApplicationScaffoldResponse> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin update deploy_application_scaffold", error))?;
        let row = sqlx::query(
            "SELECT status, pricing_model, price_minor, metadata FROM deploy_application_scaffold
             WHERE tenant_id = $1 AND uuid = $2 AND author_user_id = $3 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(scaffold_uuid)
        .bind(author_user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_application_scaffold", error))?;
        let row = row.ok_or_else(|| DeployServiceError::not_found("scaffold not found"))?;
        let status: String = row
            .try_get("status")
            .map_err(|error| DeployServiceError::Internal(format!("read status: {error}")))?;
        if status == SCAFFOLD_STATUS_DISABLED {
            return Err(DeployServiceError::conflict(
                "a disabled scaffold cannot be edited",
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
            SCAFFOLD_PRICING_PAID => {
                let price = match request.price_minor.as_deref() {
                    None => current_price,
                    Some(raw) => parse_price_minor(Some(raw))?,
                };
                if price <= 0 {
                    return Err(DeployServiceError::validation(
                        "paid scaffolds require a positive priceMinor",
                    ));
                }
                price
            }
            SCAFFOLD_PRICING_FREE => 0,
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
        let metadata = match request.screenshots.as_ref() {
            None => None,
            Some(screenshots) => {
                let current: serde_json::Value = row.try_get("metadata").map_err(|error| {
                    DeployServiceError::Internal(format!("read scaffold metadata: {error}"))
                })?;
                Some(metadata_with_screenshots(
                    current,
                    &normalize_screenshots(screenshots)?,
                )?)
            }
        };
        sqlx::query(
            "UPDATE deploy_application_scaffold SET
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
                metadata = COALESCE($15, metadata),
                updated_by = $14,
                updated_at = NOW(),
                version = version + 1
            WHERE tenant_id = $1 AND uuid = $2 AND author_user_id = $3 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(scaffold_uuid)
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
        .bind(metadata)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("update deploy_application_scaffold", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit update deploy_application_scaffold", error))?;
        self.retrieve_application_scaffold_repo(Some(tenant_id), None, scaffold_uuid)
            .await
    }

    pub(super) async fn delete_application_scaffold_repo(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<()> {
        let result = sqlx::query(
            "UPDATE deploy_application_scaffold SET deleted_at = NOW()
             WHERE uuid = $1 AND deleted_at IS NULL
             AND (tenant_id = $2 OR $2 = 0)
             AND (author_user_id = $3 OR $3 <= 0)",
        )
        .bind(scaffold_uuid)
        .bind(tenant_id.unwrap_or(0))
        .bind(author_user_id.unwrap_or(0))
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("delete deploy_application_scaffold", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found("scaffold not found"));
        }
        Ok(())
    }

    pub(super) async fn submit_application_scaffold_repo(
        &self,
        tenant_id: i64,
        author_user_id: i64,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<ApplicationScaffoldResponse> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin submit deploy_application_scaffold", error))?;
        let row = sqlx::query(
            "SELECT id, status FROM deploy_application_scaffold
             WHERE tenant_id = $1 AND uuid = $2 AND author_user_id = $3 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(scaffold_uuid)
        .bind(author_user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_application_scaffold", error))?;
        let row = row.ok_or_else(|| DeployServiceError::not_found("scaffold not found"))?;
        let template_id: i64 = row
            .try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("read id: {error}")))?;
        let status: String = row
            .try_get("status")
            .map_err(|error| DeployServiceError::Internal(format!("read status: {error}")))?;
        if status != "DRAFT" && status != "REJECTED" {
            return Err(DeployServiceError::conflict(&format!(
                "a scaffold in status {status} cannot be submitted for review"
            )));
        }
        let versions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deploy_application_scaffold_version
             WHERE template_id = $1 AND deleted_at IS NULL",
        )
        .bind(template_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error("count scaffold versions", error))?;
        if versions == 0 {
            return Err(DeployServiceError::validation(
                "a scaffold needs at least one version before review",
            ));
        }
        sqlx::query(
            "UPDATE deploy_application_scaffold SET status = 'PENDING_REVIEW', updated_at = NOW(),
                updated_by = $2, version = version + 1
             WHERE id = $1",
        )
        .bind(template_id)
        .bind(author_user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("submit deploy_application_scaffold", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit submit deploy_application_scaffold", error))?;
        self.retrieve_application_scaffold_repo(Some(tenant_id), None, scaffold_uuid)
            .await
    }

    // -- versions ----------------------------------------------------------------

    pub(super) async fn list_application_scaffold_versions_repo(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        scaffold_uuid: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ApplicationScaffoldVersionPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let predicate = format!(
            "t.uuid = $1 AND t.deleted_at IS NULL AND v.deleted_at IS NULL
            AND (t.tenant_id = $2 OR $2 = 0)
            AND (t.author_user_id = $3 OR $3 <= 0)"
        );
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_application_scaffold_version v
             JOIN deploy_application_scaffold t ON t.id = v.template_id
             WHERE {predicate}"
        )))
        .bind(scaffold_uuid)
        .bind(tenant_id.unwrap_or(0))
        .bind(author_user_id.unwrap_or(0))
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_application_scaffold_version", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {VERSION_SELECT} FROM deploy_application_scaffold_version v
             JOIN deploy_application_scaffold t ON t.id = v.template_id
             WHERE {predicate}
             ORDER BY v.created_at DESC, v.id DESC LIMIT $4 OFFSET $5"
        )))
        .bind(scaffold_uuid)
        .bind(tenant_id.unwrap_or(0))
        .bind(author_user_id.unwrap_or(0))
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_application_scaffold_version", error))?;
        let items = rows
            .iter()
            .map(map_version_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DeployServiceError::Internal(format!("map version: {error}")))?;
        Ok(ApplicationScaffoldVersionPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_application_scaffold_version_repo(
        &self,
        tenant_id: Option<i64>,
        author_user_id: Option<i64>,
        scaffold_uuid: &str,
        version_uuid: &str,
    ) -> DeployServiceResult<ApplicationScaffoldVersionResponse> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {VERSION_SELECT} FROM deploy_application_scaffold_version v
             JOIN deploy_application_scaffold t ON t.id = v.template_id
             WHERE v.uuid = $1 AND t.uuid = $2 AND v.deleted_at IS NULL AND t.deleted_at IS NULL
             AND (t.tenant_id = $3 OR $3 = 0)
             AND (t.author_user_id = $4 OR $4 <= 0)"
        )))
        .bind(version_uuid)
        .bind(scaffold_uuid)
        .bind(tenant_id.unwrap_or(0))
        .bind(author_user_id.unwrap_or(0))
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_application_scaffold_version", error))?;
        row.as_ref()
            .map(map_version_row)
            .transpose()
            .map_err(|error| DeployServiceError::Internal(format!("map version: {error}")))?
            .ok_or_else(|| DeployServiceError::not_found("scaffold version not found"))
    }

    /// Published versions of a marketplace-visible listing, for a consumer
    /// installing someone else's scaffold.
    ///
    /// The listing is resolved first under the marketplace gate: an invisible
    /// listing answers "not found" rather than an empty page, so a draft's
    /// existence never becomes observable through the shape of the answer. Only
    /// versions the marketplace published are returned; a draft (an unpublished
    /// edit) and a withdrawn version stay author-only.
    pub(super) async fn list_marketplace_scaffold_versions_repo(
        &self,
        tenant_id: i64,
        scaffold_uuid: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<ApplicationScaffoldVersionPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let template_id: Option<i64> = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT t.id FROM deploy_application_scaffold t
             WHERE t.tenant_id = $1 AND t.uuid = $2 AND {MARKET_VISIBLE_SCAFFOLD}"
        )))
        .bind(tenant_id)
        .bind(scaffold_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve marketplace scaffold", error))?;
        let template_id =
            template_id.ok_or_else(|| DeployServiceError::not_found("scaffold not found"))?;
        let predicate = format!(
            "v.template_id = $1 AND v.deleted_at IS NULL AND v.status = '{}'",
            SCAFFOLD_VERSION_STATUS_PUBLISHED
        );
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_application_scaffold_version v WHERE {predicate}"
        )))
        .bind(template_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count marketplace scaffold versions", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {VERSION_SELECT} FROM deploy_application_scaffold_version v
             JOIN deploy_application_scaffold t ON t.id = v.template_id
             WHERE {predicate}
             ORDER BY v.created_at DESC, v.id DESC LIMIT $2 OFFSET $3"
        )))
        .bind(template_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list marketplace scaffold versions", error))?;
        let items = rows
            .iter()
            .map(map_version_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DeployServiceError::Internal(format!("map version: {error}")))?;
        Ok(ApplicationScaffoldVersionPage {
            items,
            total,
            page,
            page_size,
        })
    }

    /// Increments the marketplace install counter after a successful install.
    ///
    /// The `NOT EXISTS` guard keys the increment to the installed application's
    /// audit entry, so an idempotent replay (which returns the same app) does
    /// not inflate the counter.
    pub(super) async fn record_app_scaffold_install_repo(
        &self,
        tenant_id: i64,
        scaffold_uuid: &str,
        app_id: &str,
    ) -> DeployServiceResult<()> {
        sqlx::query(
            "UPDATE deploy_application_scaffold
             SET install_count = install_count + 1, updated_at = NOW()
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
               AND NOT EXISTS (
                 SELECT 1 FROM deploy_audit_log a
                 WHERE a.tenant_id = $1
                   AND a.action = 'app.scaffold.install'
                   AND a.target_uuid = $3
               )",
        )
        .bind(tenant_id)
        .bind(scaffold_uuid)
        .bind(app_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("record deploy_application_scaffold install", error))?;
        Ok(())
    }

    /// Read-only verification that `order_id` is a settled app-scaffold order
    /// for this buyer and listing. The order center owns the row; Deploy never
    /// writes it.
    pub(super) async fn verify_paid_application_scaffold_order_repo(
        &self,
        tenant_id: i64,
        user_id: i64,
        order_id: &str,
        scaffold_uuid: &str,
    ) -> DeployServiceResult<bool> {
        let exists: Option<String> =
            sqlx::query_scalar("SELECT to_regclass('commerce_order')::text")
                .fetch_one(&self.pool)
                .await
                .map_err(|error| store_error("probe commerce order table", error))?;
        if exists.is_none() {
            return Ok(false);
        }
        let found: Option<i32> = sqlx::query_scalar(
            "SELECT 1
             FROM commerce_order o
             JOIN commerce_order_item i
               ON i.order_id = o.id AND i.tenant_id = o.tenant_id
             WHERE o.tenant_id = CAST($1 AS TEXT)
               AND o.id = CAST($2 AS TEXT)
               AND COALESCE(o.owner_user_id, '') = CAST($3 AS TEXT)
               AND LOWER(COALESCE(o.payment_status, '')) IN ('paid', 'success', 'succeeded')
               AND COALESCE(o.subject, '') = 'app_template'
               AND COALESCE(
                     NULLIF(
                       COALESCE(
                         NULLIF(to_jsonb(i) ->> 'sku_snapshot_json', ''),
                         '{}'
                       )::jsonb ->> 'scaffoldUuid',
                       ''
                     ),
                     NULLIF(to_jsonb(i) ->> 'sku_id', '')
                   ) = $4
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(order_id)
        .bind(user_id)
        .bind(scaffold_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("verify paid app scaffold order", error))?;
        Ok(found.is_some())
    }

    ///
    /// A scaffold's artifact belongs to the author's application. Installing it
    /// must not point the new app at a foreign artifact row (`apps.source.update`
    /// rejects that), and the Drive node is immutable, so this copies the
    /// artifact/session metadata under the target app and keeps the same Drive
    /// Clones a published scaffold artifact for an installed application.
    ///
    /// node and checksum. The session idempotency key makes a replay reuse the
    /// cloned artifact instead of creating a second one.
    pub(super) async fn clone_marketplace_artifact_for_app_repo(
        &self,
        tenant_id: i64,
        source_artifact_uuid: &str,
        target_app_uuid: &str,
    ) -> DeployServiceResult<String> {
        let target_app_id = resolve_app_internal_id(&self.pool, tenant_id, target_app_uuid).await?;
        let row = sqlx::query(
            "SELECT a.package_type, a.file_name, a.content_type, a.content_length,
                    a.checksum_sha256, a.drive_node_id, a.drive_space_id, a.drive_path,
                    a.status AS artifact_status,
                    s.drive_upload_session_id, s.drive_upload_item_id, s.checksum,
                    s.status AS session_status
             FROM deploy_artifact a
             JOIN deploy_upload_session_ref s ON s.id = a.upload_session_ref_id
             WHERE a.tenant_id = $1 AND a.uuid = $2 AND a.status = 1
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(source_artifact_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load scaffold artifact for clone", error))?
        .ok_or_else(|| DeployServiceError::not_found("scaffold artifact not found"))?;

        let idempotency_key = format!("scaffold-install:{target_app_uuid}:{source_artifact_uuid}");
        let session_id: Option<i64> = sqlx::query_scalar(
            "INSERT INTO deploy_upload_session_ref (
                id, uuid, tenant_id, app_id, drive_upload_session_id, drive_upload_item_id,
                drive_space_id, drive_node_id, package_type, file_name, content_type,
                content_length, checksum, status, idempotency_key, created_at, updated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, NOW(), NOW())
             ON CONFLICT DO NOTHING
             RETURNING id",
        )
        .bind(next_id(self.id_generator())?)
        .bind(new_uuid())
        .bind(tenant_id)
        .bind(target_app_id)
        .bind(row.try_get::<String, _>("drive_upload_session_id").unwrap_or_default())
        .bind(row.try_get::<Option<String>, _>("drive_upload_item_id").ok().flatten())
        .bind(row.try_get::<Option<String>, _>("drive_space_id").ok().flatten())
        .bind(row.try_get::<String, _>("drive_node_id").unwrap_or_default())
        .bind(row.try_get::<i32, _>("package_type").unwrap_or(1))
        .bind(row.try_get::<String, _>("file_name").unwrap_or_default())
        .bind(row.try_get::<String, _>("content_type").unwrap_or_default())
        .bind(row.try_get::<i64, _>("content_length").unwrap_or(0))
        .bind(row.try_get::<Option<String>, _>("checksum").ok().flatten())
        .bind(row.try_get::<i32, _>("session_status").unwrap_or(1))
        .bind(&idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("clone scaffold upload session", error))?;

        let Some(session_id) = session_id else {
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT a.uuid FROM deploy_artifact a
                 JOIN deploy_upload_session_ref s ON s.id = a.upload_session_ref_id
                 WHERE a.tenant_id = $1 AND s.idempotency_key = $2 AND a.status = 1
                 LIMIT 1",
            )
            .bind(tenant_id)
            .bind(&idempotency_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve cloned scaffold artifact", error))?;
            return existing.ok_or_else(|| {
                DeployServiceError::conflict("scaffold artifact clone is already in progress")
            });
        };

        let artifact_uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_artifact (
                id, uuid, tenant_id, app_id, upload_session_ref_id, package_type,
                file_name, content_type, content_length, checksum_sha256, drive_node_id,
                drive_space_id, drive_path, status, metadata, created_at, updated_at, version
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, '{}', NOW(), NOW(), 0)",
        )
        .bind(next_id(self.id_generator())?)
        .bind(&artifact_uuid)
        .bind(tenant_id)
        .bind(target_app_id)
        .bind(session_id)
        .bind(row.try_get::<i32, _>("package_type").unwrap_or(1))
        .bind(row.try_get::<String, _>("file_name").unwrap_or_default())
        .bind(row.try_get::<String, _>("content_type").unwrap_or_default())
        .bind(row.try_get::<i64, _>("content_length").unwrap_or(0))
        .bind(row.try_get::<Option<String>, _>("checksum_sha256").ok().flatten())
        .bind(row.try_get::<String, _>("drive_node_id").unwrap_or_default())
        .bind(row.try_get::<Option<String>, _>("drive_space_id").ok().flatten())
        .bind(row.try_get::<String, _>("drive_path").unwrap_or_default())
        .bind(row.try_get::<i32, _>("artifact_status").unwrap_or(1))
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("clone scaffold artifact", error))?;
        Ok(artifact_uuid)
    }

    pub(super) async fn assert_marketplace_artifact_readable_repo(
        &self,
        tenant_id: i64,
        artifact_uuid: &str,
    ) -> DeployServiceResult<()> {
        let version_id: Option<i64> = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT v.id FROM deploy_application_scaffold_version v
             JOIN deploy_application_scaffold t ON t.id = v.template_id
             WHERE v.tenant_id = $1 AND v.artifact_uuid = $2
             AND v.deleted_at IS NULL AND v.status = '{}'
             AND {MARKET_VISIBLE_SCAFFOLD}
             LIMIT 1",
            SCAFFOLD_VERSION_STATUS_PUBLISHED
        )))
        .bind(tenant_id)
        .bind(artifact_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve marketplace artifact", error))?;
        version_id
            .map(|_| ())
            .ok_or_else(|| DeployServiceError::not_found("artifact not found"))
    }

    pub(super) async fn create_application_scaffold_version_repo(
        &self,
        tenant_id: i64,
        author_user_id: i64,
        scaffold_uuid: &str,
        request: &CreateApplicationScaffoldVersionRequest,
    ) -> DeployServiceResult<ApplicationScaffoldVersionResponse> {
        // Validate up front so a malformed version fails before the row lock;
        // the insert helper re-validates when it binds.
        require_text(&request.version, "version", 64)?;
        let mut transaction = self.pool.begin().await.map_err(|error| {
            store_error("begin create deploy_application_scaffold_version", error)
        })?;
        let row = sqlx::query(
            "SELECT id, status, organization_id, app_uuid FROM deploy_application_scaffold
             WHERE tenant_id = $1 AND uuid = $2 AND author_user_id = $3 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(scaffold_uuid)
        .bind(author_user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_application_scaffold", error))?;
        let row = row.ok_or_else(|| DeployServiceError::not_found("scaffold not found"))?;
        let template_id: i64 = row
            .try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("read id: {error}")))?;
        let organization_id: i64 = row
            .try_get("organization_id")
            .map_err(|error| DeployServiceError::Internal(format!("read organization: {error}")))?;
        let app_uuid: String = row
            .try_get("app_uuid")
            .map_err(|error| DeployServiceError::Internal(format!("read app_uuid: {error}")))?;
        let template_status: String = row
            .try_get("status")
            .map_err(|error| DeployServiceError::Internal(format!("read status: {error}")))?;
        if template_status == SCAFFOLD_STATUS_DISABLED {
            return Err(DeployServiceError::conflict(
                "a disabled scaffold cannot take new versions",
            ));
        }
        // A live listing publishes new versions as they land; a listing that is
        // still Draft/In Review/Rejected keeps them Draft until approval.
        let version_status = if template_status == SCAFFOLD_STATUS_PUBLISHED {
            SCAFFOLD_VERSION_STATUS_PUBLISHED
        } else {
            SCAFFOLD_VERSION_STATUS_DRAFT
        };
        let version_snapshot =
            load_git_source_snapshot(&mut transaction, tenant_id, &app_uuid).await?;
        let version_uuid = self
            .insert_template_version(
                &mut transaction,
                tenant_id,
                Some(organization_id),
                template_id,
                Some(author_user_id),
                request,
                version_status,
                version_snapshot,
            )
            .await?;
        if version_status == SCAFFOLD_VERSION_STATUS_PUBLISHED {
            sqlx::query(
                "UPDATE deploy_application_scaffold SET latest_version_uuid = $2, updated_at = NOW(),
                    updated_by = $3, version = version + 1
                 WHERE id = $1",
            )
            .bind(template_id)
            .bind(&version_uuid)
            .bind(author_user_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("advance deploy_application_scaffold latest version", error))?;
        }
        transaction.commit().await.map_err(|error| {
            store_error("commit create deploy_application_scaffold_version", error)
        })?;
        self.retrieve_application_scaffold_version_repo(
            Some(tenant_id),
            None,
            scaffold_uuid,
            &version_uuid,
        )
        .await
    }

    pub(super) async fn retrieve_application_scaffold_version_source_snapshot_repo(
        &self,
        tenant_id: i64,
        version_uuid: &str,
    ) -> DeployServiceResult<Option<serde_json::Value>> {
        let row = sqlx::query(
            "SELECT metadata FROM deploy_application_scaffold_version
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(version_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read scaffold version source snapshot", error))?;
        Ok(row.and_then(|row| row.try_get::<serde_json::Value, _>("metadata").ok()))
    }

    /// Shared version insert used by scaffold creation (initial version) and
    /// `create_application_scaffold_version`. Returns the new version's uuid.
    #[allow(clippy::too_many_arguments)]
    async fn insert_template_version(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: i64,
        organization_id: Option<i64>,
        template_id: i64,
        actor_id: Option<i64>,
        request: &CreateApplicationScaffoldVersionRequest,
        status: &str,
        metadata: Option<serde_json::Value>,
    ) -> DeployServiceResult<String> {
        let template_version = require_text(&request.version, "version", 64)?;
        let package_size = parse_package_size(request.package_size_bytes.as_deref())?;
        let version_id = next_id(self.id_generator())?;
        let version_uuid = new_uuid();
        let published_at = if status == SCAFFOLD_VERSION_STATUS_PUBLISHED {
            Some(chrono::Utc::now())
        } else {
            None
        };
        sqlx::query(
            "INSERT INTO deploy_application_scaffold_version (
                id, uuid, tenant_id, organization_id, template_id, template_version, changelog,
                artifact_uuid, source_app_version, platform_targets_json, package_size_bytes,
                checksum_sha256, status, published_at, metadata, created_by, updated_by
            ) VALUES ($1, $2, $3, COALESCE($4, 0), $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16)",
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
        .bind(metadata.unwrap_or_else(|| serde_json::json!({})))
        .bind(actor_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| store_error("insert deploy_application_scaffold_version", error))?;
        Ok(version_uuid)
    }

    // -- admin moderation ----------------------------------------------------------

    pub(super) async fn review_application_scaffold_repo(
        &self,
        tenant_id: Option<i64>,
        operator_id: Option<i64>,
        scaffold_uuid: &str,
        request: &UpdateApplicationScaffoldAdminRequest,
    ) -> DeployServiceResult<ApplicationScaffoldResponse> {
        if let Some(status) = request.status.as_deref() {
            if status != SCAFFOLD_STATUS_PUBLISHED
                && status != "REJECTED"
                && status != SCAFFOLD_STATUS_DISABLED
                && status != "PENDING_REVIEW"
            {
                return Err(DeployServiceError::validation(
                    "admin scaffold status must be PUBLISHED, REJECTED, DISABLED or PENDING_REVIEW",
                ));
            }
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin review deploy_application_scaffold", error))?;
        let row = sqlx::query(
            "SELECT id, status FROM deploy_application_scaffold
             WHERE uuid = $1 AND deleted_at IS NULL AND (tenant_id = $2 OR $2 = 0)
             FOR UPDATE",
        )
        .bind(scaffold_uuid)
        .bind(tenant_id.unwrap_or(0))
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_application_scaffold", error))?;
        let row = row.ok_or_else(|| DeployServiceError::not_found("scaffold not found"))?;
        let template_id: i64 = row
            .try_get("id")
            .map_err(|error| DeployServiceError::Internal(format!("read id: {error}")))?;
        let next_status = request.status.as_deref();
        sqlx::query(
            "UPDATE deploy_application_scaffold SET
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
        .map_err(|error| store_error("review deploy_application_scaffold", error))?;
        if next_status == Some(SCAFFOLD_STATUS_PUBLISHED) {
            // Approval publishes every draft version: the author submitted the
            // listing as a whole, and partial approvals would silently strand
            // the newest artifact.
            sqlx::query(
                "UPDATE deploy_application_scaffold_version SET status = $2,
                    published_at = COALESCE(published_at, NOW()),
                    updated_by = COALESCE($3, updated_by),
                    updated_at = NOW(), version = version + 1
                 WHERE template_id = $1 AND status = $4 AND deleted_at IS NULL",
            )
            .bind(template_id)
            .bind(SCAFFOLD_VERSION_STATUS_PUBLISHED)
            .bind(operator_id)
            .bind(SCAFFOLD_VERSION_STATUS_DRAFT)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("publish scaffold versions", error))?;
            let latest: Option<String> = sqlx::query_scalar(
                "SELECT uuid FROM deploy_application_scaffold_version
                 WHERE template_id = $1 AND status = $2 AND deleted_at IS NULL
                 ORDER BY created_at DESC, id DESC LIMIT 1",
            )
            .bind(template_id)
            .bind(SCAFFOLD_VERSION_STATUS_PUBLISHED)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("resolve latest scaffold version", error))?;
            if latest.is_none() {
                return Err(DeployServiceError::conflict(
                    "a scaffold cannot be published without at least one version",
                ));
            }
            sqlx::query(
                "UPDATE deploy_application_scaffold SET latest_version_uuid = $2 WHERE id = $1",
            )
            .bind(template_id)
            .bind(latest)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("pin latest scaffold version", error))?;
        }
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit review deploy_application_scaffold", error))?;
        self.retrieve_application_scaffold_repo(tenant_id, None, scaffold_uuid)
            .await
    }
}
