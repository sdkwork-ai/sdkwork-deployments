//! App aggregate, platform target, source repository, build template, and
//! signing identity repository operations (REQ-2026-0002).

use sdkwork_deploy_contract::{
    AppPage, AppResponse, AppStatus, BuildTemplatePage, BuildTemplateResponse, CreateAppRequest,
    CreateBuildTemplateRequest, CreatePlatformTargetRequest, CreateSigningIdentityRequest,
    CreateSourceRepositoryRequest, DeployServiceError, DeployServiceResult, PlatformTargetPage,
    PlatformTargetResponse, SigningIdentityPage, SigningIdentityResponse, SourceRepositoryPage,
    SourceRepositoryResponse, UpdateAppRequest,
};
use sdkwork_deploy_core::{
    effective_app_domain_label, effective_app_domain_suffixes, normalize_app_domain_label,
    normalize_app_domain_suffixes,
};
use sqlx::{postgres::PgRow, AssertSqlSafe, Row};

use crate::support::{
    new_uuid, next_id, optional_datetime, pagination, required_datetime, resolve_app_internal_id,
    sha256_hex, store_error, string_list_from_row, string_list_to_json,
};
use crate::DeployRepository;

const APP_SELECT: &str = "a.uuid, a.name, a.slug, a.app_kind, a.app_status, a.type,
    a.description, a.runtime_config, a.metadata, a.current_revision_id, a.desired_revision_id,
    a.default_environment,
    (SELECT COUNT(*) FROM deploy_app_platform_target t
      WHERE t.app_id = a.id AND t.deleted_at IS NULL) AS platform_target_count,
    (SELECT r.semantic_version FROM deploy_release r
      WHERE r.app_id = a.id AND r.release_status = 'ACTIVE'
      ORDER BY r.created_at DESC LIMIT 1) AS latest_release_tag,
    a.app_domain_label, a.app_domain_suffixes,
    a.created_at, a.updated_at, a.version";

/// Mirrors the DDL default for `deploy_app.type` (`DEFAULT 1`,
/// `CHECK (type BETWEEN 1 AND 6)`). Kept in lockstep with
/// `database/ddl/baseline/postgres/0001_deploy_baseline.sql`; the repository
/// must never write a NULL here because the column is NOT NULL and PostgreSQL
/// treats an explicitly bound NULL as an override of the column DEFAULT.
const DEFAULT_APP_TYPE: i32 = 1;

/// Longest slug the DDL accepts (`slug VARCHAR(120)`), kept in lockstep with the
/// contract's `slug: maxLength: 120`.
const MAX_SLUG_LEN: usize = 120;

/// Prefix of the generated slug when the name carries no ASCII to derive one
/// from (see [`resolve_app_slug`]).
const GENERATED_SLUG_PREFIX: &str = "app";

/// Resolve the slug actually written to `deploy_app.slug`.
///
/// `deploy_app.slug` is `VARCHAR(120) NOT NULL` with a partial unique index
/// (`uk_deploy_app_tenant_slug`), and it is *also* the fallback label of the
/// app's default publishing domain (`<app_domain_label|slug>.app[-<env>].<suffix>`).
/// So the column can never be empty and never repeated within a tenant.
///
/// Both RFC-1123-style derivation paths used by clients collapse a non-ASCII
/// name to the empty string — `sdkwork_utils_rust::slugify` filters to
/// `[a-z0-9-]`, and the console's `deriveAppSlug` does the same. A Chinese-only
/// name such as `放大` therefore used to fall through to `slug = ''`, which the
/// **first** app happily claimed and every later app then collided with on
/// `uk_deploy_app_tenant_slug`: the tenant could only ever create one such app,
/// and the caller saw a permanent, unexplained `409 conflict: app slug  already
/// exists in this tenant`.
///
/// Deriving a stable, unique, ASCII slug here (rather than rejecting the
/// request) keeps the name free-form while the slug stays a valid DNS label.
/// A caller-supplied slug is always authoritative and never rewritten.
fn resolve_app_slug(requested: Option<&str>, name: &str, app_uuid: &str) -> String {
    let explicit = requested.map(str::trim).filter(|slug| !slug.is_empty());
    let candidate = explicit
        .map(str::to_owned)
        .unwrap_or_else(|| sdkwork_utils_rust::slugify(name));
    if !candidate.is_empty() {
        return candidate;
    }
    // Deterministic per app, so a retried `apps.create` derives the same slug
    // instead of losing its own row to a fresh random suffix.
    let uuid_tail: String = app_uuid
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    format!("{GENERATED_SLUG_PREFIX}-{uuid_tail}")
        .chars()
        .take(MAX_SLUG_LEN)
        .collect()
}

fn map_app_row(row: &PgRow) -> Result<AppResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    let slug: String = row.try_get("slug").unwrap_or_default();
    // `app_domain_label` / `app_domain_suffixes` are the app's *overrides*; the
    // response carries the *effective* values so the console can render the
    // real hostname without re-implementing the fallback (and without drifting
    // from what `provision_app_default_domains*` actually wrote).
    let app_domain_label_override: Option<String> = row.try_get("app_domain_label").ok().flatten();
    // Decoded through the JSONB helper, not `try_get::<Option<Vec<String>>>`: sqlx
    // maps `Vec<String>` onto `TEXT[]`, so the direct decode fails against a
    // `JSONB` column. Swallowing that with `.ok()` silently discarded the
    // override and made every app look like it used the platform catalog.
    let app_domain_suffixes_override =
        string_list_from_row(row, "app_domain_suffixes").map_err(|error| {
            DeployServiceError::Internal(format!("read app domain suffixes: {error}"))
        })?;
    let effective_label =
        effective_app_domain_label(app_domain_label_override.as_deref(), &slug).to_owned();
    let effective_suffixes = effective_app_domain_suffixes(app_domain_suffixes_override.as_deref());
    Ok(AppResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        name: row.try_get("name").unwrap_or_default(),
        slug,
        app_kind: row.try_get("app_kind").unwrap_or_default(),
        app_status: row.try_get("app_status").unwrap_or_default(),
        app_type: row.try_get("type").unwrap_or(1),
        description: row.try_get("description").ok(),
        runtime_config: row.try_get("runtime_config").ok(),
        metadata: row.try_get("metadata").ok(),
        current_revision_id: row.try_get("current_revision_id").ok(),
        desired_revision_id: row.try_get("desired_revision_id").ok(),
        default_environment: row.try_get("default_environment").unwrap_or_default(),
        platform_target_count: row.try_get("platform_target_count").unwrap_or(0),
        app_domain_label: effective_label,
        app_domain_suffixes: effective_suffixes,
        latest_release_tag: row.try_get("latest_release_tag").ok(),
        created_at,
        updated_at,
        version: row.try_get::<i64, _>("version").unwrap_or(1).to_string(),
    })
}

impl DeployRepository {
    pub(super) async fn create_app_repo(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        idempotency_key: Option<&str>,
        request: &CreateAppRequest,
    ) -> DeployServiceResult<AppResponse> {
        let app_id = next_id(self.id_generator())?;
        let app_uuid = new_uuid();
        let slug = resolve_app_slug(request.slug.as_deref(), &request.name, &app_uuid);
        let app_kind = request.app_kind.as_str();
        let default_environment = request
            .default_environment
            .as_deref()
            .unwrap_or("production")
            .to_owned();

        // `deploy_app.type` is `INTEGER NOT NULL DEFAULT 1 CHECK (type BETWEEN 1 AND 6)`
        // (DDL 0001_deploy_baseline.sql:1798/1834). Binding a bare `Option<i32>`
        // would turn `None` into an *explicit* NULL, which **overrides the column
        // DEFAULT** and fails the NOT NULL constraint — the caller then sees an
        // opaque `insert deploy_app` 500. `apps.create` in the contract does not
        // even declare `type` (`CreateAppRequest` carries only name/appKind/
        // metadata/…), so `None` is the normal case, not an edge case.
        // COALESCE keeps the column DEFAULT authoritative while still honouring
        // an explicit value when a caller does send one.
        let app_type = request.app_type.unwrap_or(DEFAULT_APP_TYPE);

        // `metadata` is `JSONB NOT NULL DEFAULT '{}'` — same NULL-override trap
        // as `type` above, so an absent payload must fall back to `{}`.
        let metadata = request
            .metadata
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        // Idempotent replay. `apps.create` is `x-sdkwork-idempotent: true` and the
        // contract calls `Idempotency-Key` required, so the same key must yield
        // the row the first attempt created rather than a `409 conflict` from
        // `uk_deploy_app_tenant_slug`. Resolved *before* the INSERT: a replay
        // cannot be detected from a unique violation alone, because the insert
        // that lost also left `deploy_app_platform_target` / default-domain
        // side effects to reconcile.
        let idempotency_key = idempotency_key.map(str::trim).filter(|key| !key.is_empty());
        let request_sha256 = idempotency_key.map(|_| {
            // Hash only the fields the create actually persists, so a retry that
            // differs in a purely-presentational field still counts as the same
            // command.
            sha256_hex(
                &serde_json::json!({
                    "name": &request.name,
                    "slug": request.slug.as_deref(),
                    "appKind": request.app_kind.as_str(),
                    "description": request.description.as_deref(),
                    "defaultEnvironment": request.default_environment.as_deref(),
                    "metadata": metadata,
                })
                .to_string(),
            )
        });
        if let (Some(key), Some(request_sha256)) = (idempotency_key, request_sha256.as_deref()) {
            if let Some(existing) = sqlx::query(
                "SELECT uuid, request_sha256 FROM deploy_app
                 WHERE tenant_id = $1 AND idempotency_key = $2",
            )
            .bind(tenant_id)
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("load idempotent deploy_app", error))?
            {
                let stored_hash: Option<String> = existing.try_get("request_sha256").ok().flatten();
                // Only compare when the earlier row recorded a hash; rows written
                // before this column existed must still replay rather than 409.
                if let Some(stored_hash) = stored_hash {
                    if stored_hash != request_sha256 {
                        return Err(DeployServiceError::conflict(
                            "Idempotency-Key was already used with another app create request",
                        ));
                    }
                }
                let existing_uuid: String = existing.try_get("uuid").map_err(|error| {
                    DeployServiceError::Internal(format!("read deploy_app uuid: {error}"))
                })?;
                return self.retrieve_app_repo(tenant_id, &existing_uuid).await;
            }
        }

        // Default publishing-domain overrides. Validated here rather than at the
        // HTTP edge so a value that reaches the DB is always a valid DNS label /
        // suffix list — `provision_app_default_domains*` composes hostnames from
        // them without re-validating.
        let app_domain_label = match request.app_domain_label.as_deref() {
            Some(raw) if !raw.trim().is_empty() => Some(
                normalize_app_domain_label(raw)
                    .map_err(|reason| DeployServiceError::validation(reason))?,
            ),
            _ => None,
        };
        let app_domain_suffixes = match request.app_domain_suffixes.as_deref() {
            Some(suffixes) if !suffixes.is_empty() => Some(
                normalize_app_domain_suffixes(suffixes)
                    .map_err(|reason| DeployServiceError::validation(reason))?,
            ),
            _ => None,
        };
        // `deploy_app.app_domain_suffixes` is `JSONB`, not `TEXT[]`. Binding the
        // `Vec<String>` directly makes sqlx infer a text array and PostgreSQL
        // rejects the insert with `column "app_domain_suffixes" is of type jsonb
        // but expression is of type text[]` — a masked 500 on `apps.create`.
        // `None` keeps meaning "no override; use the platform catalog".
        let app_domain_suffixes_json = string_list_to_json(app_domain_suffixes.as_ref());

        let result = sqlx::query(
            "INSERT INTO deploy_app
                (id, uuid, tenant_id, organization_id, name, slug, app_kind, description,
                 app_status, type, metadata, default_environment,
                 app_domain_label, app_domain_suffixes,
                 created_by, updated_by,
                 idempotency_key, request_sha256, created_at, updated_at, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                     $15, $16, $17, $18, NOW(), NOW(), 1)
             ON CONFLICT (tenant_id, slug) WHERE deleted_at IS NULL DO NOTHING
             RETURNING uuid",
        )
        .bind(app_id)
        .bind(&app_uuid)
        .bind(tenant_id)
        .bind(organization_id.unwrap_or(0))
        .bind(&request.name)
        .bind(&slug)
        .bind(app_kind)
        .bind(request.description.as_deref())
        .bind(AppStatus::Draft.as_str())
        .bind(app_type)
        .bind(&metadata)
        .bind(&default_environment)
        .bind(app_domain_label.as_deref())
        .bind(&app_domain_suffixes_json)
        .bind(actor_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_sha256.as_deref())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_app", error))?;

        let Some(row) = result else {
            return Err(DeployServiceError::conflict(format!(
                "app slug {slug} already exists in this tenant"
            )));
        };
        let inserted_uuid: String = row.try_get("uuid").map_err(|error| {
            DeployServiceError::Internal(format!("read deploy_app uuid: {error}"))
        })?;
        self.retrieve_app_repo(tenant_id, &inserted_uuid).await
    }

    pub(super) async fn list_apps_repo(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AppPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_app
             WHERE tenant_id = $1 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_app", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let query = format!(
            "SELECT {APP_SELECT}
             FROM deploy_app a
             WHERE a.tenant_id = $1 AND a.deleted_at IS NULL
             ORDER BY a.created_at DESC, a.id DESC LIMIT $2 OFFSET $3"
        );
        let rows = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list deploy_app", error))?;

        let items = rows
            .iter()
            .map(map_app_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AppPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_app_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<AppResponse> {
        let query = format!(
            "SELECT {APP_SELECT}
             FROM deploy_app a
             WHERE a.tenant_id = $1 AND a.uuid = $2 AND a.deleted_at IS NULL"
        );
        let row = sqlx::query(AssertSqlSafe(&*query))
            .bind(tenant_id)
            .bind(app_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve deploy_app", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("app not found"));
        };
        map_app_row(&row)
    }

    pub(super) async fn update_app_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        app_id: &str,
        request: &UpdateAppRequest,
    ) -> DeployServiceResult<AppResponse> {
        resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let app_status = request
            .app_status
            .map(|status| status.as_str())
            .unwrap_or_default();
        // Default-domain overrides. `Some(inner)` means the field was on the
        // wire: `Some(None)` is the declared `null` and clears the override,
        // while `None` (absent) leaves the column untouched — hence the two
        // separate booleans rather than a single `COALESCE`.
        let has_domain_label = request.app_domain_label.is_some();
        let domain_label = match request.app_domain_label.as_ref() {
            Some(Some(raw)) if !raw.trim().is_empty() => Some(
                normalize_app_domain_label(raw)
                    .map_err(|reason| DeployServiceError::validation(reason))?,
            ),
            _ => None,
        };
        let has_domain_suffixes = request.app_domain_suffixes.is_some();
        let domain_suffixes = match request.app_domain_suffixes.as_ref() {
            Some(Some(suffixes)) if !suffixes.is_empty() => Some(
                normalize_app_domain_suffixes(suffixes)
                    .map_err(|reason| DeployServiceError::validation(reason))?,
            ),
            _ => None,
        };
        // Same JSONB-vs-text-array contract as `apps.create`: the override list
        // has to reach the column as a JSON array, and `has_domain_suffixes`
        // still distinguishes "field absent" (leave the column) from "cleared".
        let domain_suffixes_json = string_list_to_json(domain_suffixes.as_ref());
        // `metadata` arrives as the *complete* object the console wants stored
        // (`{...existing, media}`), so it replaces the column rather than being
        // shallow-merged server-side — merging here would resurrect keys the
        // caller intentionally dropped. A NULL payload leaves the column alone.
        let updated = sqlx::query(
            "UPDATE deploy_app SET
                name = COALESCE($3, name),
                description = COALESCE($4, description),
                app_status = CASE WHEN $5 = '' THEN app_status ELSE $5 END,
                default_environment = COALESCE($6, default_environment),
                metadata = COALESCE($8, metadata),
                app_domain_label = CASE WHEN $9 THEN $10 ELSE app_domain_label END,
                app_domain_suffixes = CASE WHEN $11 THEN $12 ELSE app_domain_suffixes END,
                updated_by = $7, updated_at = NOW(),
                version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
             RETURNING uuid",
        )
        .bind(tenant_id)
        .bind(app_id)
        .bind(request.name.as_deref())
        .bind(request.description.as_deref())
        .bind(app_status)
        .bind(request.default_environment.as_deref())
        .bind(actor_id)
        .bind(request.metadata.as_ref())
        .bind(has_domain_label)
        .bind(domain_label.as_deref())
        .bind(has_domain_suffixes)
        .bind(&domain_suffixes_json)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("update deploy_app", error))?;

        if updated.is_none() {
            return Err(DeployServiceError::not_found("app not found"));
        }
        self.retrieve_app_repo(tenant_id, app_id).await
    }

    pub(super) async fn set_app_status_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        status: i32,
    ) -> DeployServiceResult<AppResponse> {
        let app_status = match status {
            1 => "ACTIVE",
            2 => "PAUSED",
            3 => "ARCHIVED",
            _ => {
                return Err(DeployServiceError::validation(
                    "unsupported app status; use 1=ACTIVE, 2=PAUSED, 3=ARCHIVED",
                ))
            }
        };
        let now_expr = if status == 1 {
            ", activated_at = NOW()"
        } else if status == 2 {
            ", paused_at = NOW()"
        } else {
            ", archived_at = NOW()"
        };
        let sql = format!(
            "UPDATE deploy_app SET app_status = $3, updated_at = NOW(), version = version + 1{now_expr}
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
             RETURNING uuid"
        );
        let updated = sqlx::query(AssertSqlSafe(&*sql))
            .bind(tenant_id)
            .bind(app_id)
            .bind(app_status)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("set deploy_app status", error))?;
        if updated.is_none() {
            return Err(DeployServiceError::not_found("app not found"));
        }
        self.retrieve_app_repo(tenant_id, app_id).await
    }

    // -- platform targets --------------------------------------------------

    pub(super) async fn create_platform_target_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        actor_id: Option<i64>,
        request: &CreatePlatformTargetRequest,
    ) -> DeployServiceResult<PlatformTargetResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let target_id = next_id(self.id_generator())?;
        let target_uuid = new_uuid();
        let tech_stack = request
            .tech_stack
            .map(|stack| stack.as_str())
            .unwrap_or("OTHER");
        let allowed_channels = request
            .allowed_channels
            .clone()
            .unwrap_or_else(|| vec!["stable".to_owned()]);
        let allowed_channels_json = serde_json::to_value(&allowed_channels).map_err(|error| {
            DeployServiceError::Internal(format!("serialize allowed channels: {error}"))
        })?;

        let result = sqlx::query(
            "INSERT INTO deploy_app_platform_target
                (id, uuid, tenant_id, organization_id, app_id, target_key, platform,
                 tech_stack, bundle_id, package_name, app_id_value, bundle_name,
                 allowed_channels_json, target_status, created_by, updated_by,
                 created_at, updated_at, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15, NOW(), NOW(), 1)
             ON CONFLICT (app_id, target_key) WHERE deleted_at IS NULL DO NOTHING
             RETURNING uuid",
        )
        .bind(target_id)
        .bind(&target_uuid)
        .bind(tenant_id)
        .bind(0)
        .bind(app_internal_id)
        .bind(&request.target_key)
        .bind(request.platform.as_str())
        .bind(tech_stack)
        .bind(request.bundle_id.as_deref())
        .bind(request.package_name.as_deref())
        .bind(request.app_id.as_deref())
        .bind(request.bundle_name.as_deref())
        .bind(allowed_channels_json)
        .bind("ACTIVE")
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_app_platform_target", error))?;

        let Some(row) = result else {
            return Err(DeployServiceError::conflict(format!(
                "platform target key {} already exists in app {app_id}",
                request.target_key
            )));
        };
        let inserted_uuid: String = row.try_get("uuid").map_err(|error| {
            DeployServiceError::Internal(format!("read platform target uuid: {error}"))
        })?;
        self.retrieve_platform_target_repo(tenant_id, app_id, &inserted_uuid)
            .await
    }

    pub(super) async fn list_platform_targets_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<PlatformTargetPage> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let rows = sqlx::query(
            "SELECT t.uuid, a.uuid AS app_uuid, t.target_key, t.platform, t.tech_stack,
                    t.bundle_id, t.package_name, t.app_id_value, t.bundle_name,
                    bt.uuid AS build_template_uuid, t.allowed_channels_json, t.target_status,
                    t.created_at, t.updated_at, t.version
             FROM deploy_app_platform_target t
             JOIN deploy_app a ON a.id = t.app_id
             LEFT JOIN deploy_build_template bt ON bt.id = t.build_template_id
             WHERE t.tenant_id = $1 AND t.app_id = $2 AND t.deleted_at IS NULL
             ORDER BY t.created_at ASC, t.id ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_platform_target", error))?;

        let items = rows
            .iter()
            .map(map_platform_target_row)
            .collect::<Result<Vec<_>, _>>()?;
        let total = items.len() as i64;
        Ok(PlatformTargetPage {
            items,
            total,
            page: 1,
            page_size: total.max(1) as i32,
        })
    }

    pub(super) async fn retrieve_platform_target_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        target_id: &str,
    ) -> DeployServiceResult<PlatformTargetResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let row = sqlx::query(
            "SELECT t.uuid, a.uuid AS app_uuid, t.target_key, t.platform, t.tech_stack,
                    t.bundle_id, t.package_name, t.app_id_value, t.bundle_name,
                    bt.uuid AS build_template_uuid, t.allowed_channels_json, t.target_status,
                    t.created_at, t.updated_at, t.version
             FROM deploy_app_platform_target t
             JOIN deploy_app a ON a.id = t.app_id
             LEFT JOIN deploy_build_template bt ON bt.id = t.build_template_id
             WHERE t.tenant_id = $1 AND t.app_id = $2 AND t.uuid = $3 AND t.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(target_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_app_platform_target", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("platform target not found"));
        };
        map_platform_target_row(&row)
    }

    // -- source repositories ------------------------------------------------

    pub(super) async fn create_source_repository_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        actor_id: Option<i64>,
        request: &CreateSourceRepositoryRequest,
    ) -> DeployServiceResult<SourceRepositoryResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let repo_id = next_id(self.id_generator())?;
        let repo_uuid = new_uuid();
        let default_branch = request
            .default_branch
            .clone()
            .unwrap_or_else(|| "main".to_owned());
        let clone_mode = request
            .clone_mode
            .clone()
            .unwrap_or_else(|| "SHALLOW".to_owned());

        let result = sqlx::query(
            "INSERT INTO deploy_source_repository
                (id, uuid, tenant_id, organization_id, app_id, repo_key, repo_provider,
                 repo_url, default_branch, clone_mode, credential_secret_ref, repo_status,
                 created_by, updated_by, created_at, updated_at, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13, NOW(), NOW(), 1)
             ON CONFLICT (app_id, repo_key) WHERE deleted_at IS NULL DO NOTHING
             RETURNING uuid",
        )
        .bind(repo_id)
        .bind(&repo_uuid)
        .bind(tenant_id)
        .bind(0)
        .bind(app_internal_id)
        .bind(&request.repo_key)
        .bind(&request.repo_provider)
        .bind(&request.repo_url)
        .bind(&default_branch)
        .bind(&clone_mode)
        .bind(request.credential_secret_ref.as_deref())
        .bind("PENDING")
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_source_repository", error))?;

        let Some(row) = result else {
            return Err(DeployServiceError::conflict(format!(
                "source repository key {} already exists in app {app_id}",
                request.repo_key
            )));
        };
        let inserted_uuid: String = row.try_get("uuid").map_err(|error| {
            DeployServiceError::Internal(format!("read source repository uuid: {error}"))
        })?;
        self.retrieve_source_repository_repo(tenant_id, app_id, &inserted_uuid)
            .await
    }

    pub(super) async fn list_source_repositories_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
    ) -> DeployServiceResult<SourceRepositoryPage> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let rows = sqlx::query(
            "SELECT r.uuid, a.uuid AS app_uuid, r.repo_key, r.repo_provider, r.repo_url,
                    r.default_branch, r.clone_mode, r.credential_secret_ref, r.repo_status,
                    r.last_error_code, r.created_at, r.updated_at, r.version
             FROM deploy_source_repository r
             JOIN deploy_app a ON a.id = r.app_id
             WHERE r.tenant_id = $1 AND r.app_id = $2 AND r.deleted_at IS NULL
             ORDER BY r.created_at ASC, r.id ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_source_repository", error))?;

        let items = rows
            .iter()
            .map(map_source_repository_row)
            .collect::<Result<Vec<_>, _>>()?;
        let total = items.len() as i64;
        Ok(SourceRepositoryPage {
            items,
            total,
            page: 1,
            page_size: total.max(1) as i32,
        })
    }

    pub(super) async fn retrieve_source_repository_repo(
        &self,
        tenant_id: i64,
        app_id: &str,
        repo_id: &str,
    ) -> DeployServiceResult<SourceRepositoryResponse> {
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, app_id).await?;
        let row = sqlx::query(
            "SELECT r.uuid, a.uuid AS app_uuid, r.repo_key, r.repo_provider, r.repo_url,
                    r.default_branch, r.clone_mode, r.credential_secret_ref, r.repo_status,
                    r.last_error_code, r.created_at, r.updated_at, r.version
             FROM deploy_source_repository r
             JOIN deploy_app a ON a.id = r.app_id
             WHERE r.tenant_id = $1 AND r.app_id = $2 AND r.uuid = $3 AND r.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .bind(repo_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_source_repository", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("source repository not found"));
        };
        map_source_repository_row(&row)
    }

    // -- build templates -----------------------------------------------------

    pub(super) async fn create_build_template_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateBuildTemplateRequest,
    ) -> DeployServiceResult<BuildTemplateResponse> {
        let template_id = next_id(self.id_generator())?;
        let template_uuid = new_uuid();
        let toolchain = request
            .toolchain
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let commands = request.commands.clone().unwrap_or_default();
        let commands_json = serde_json::to_value(&commands).map_err(|error| {
            DeployServiceError::Internal(format!("serialize template commands: {error}"))
        })?;
        let artifact_outputs = request.artifact_outputs.clone().unwrap_or_default();
        let artifact_json = serde_json::to_value(&artifact_outputs).map_err(|error| {
            DeployServiceError::Internal(format!("serialize template outputs: {error}"))
        })?;
        let quality_gates = request
            .quality_gates
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        let result = sqlx::query(
            "INSERT INTO deploy_build_template
                (id, uuid, tenant_id, organization_id, template_name, template_version,
                 platform, tech_stack, toolchain_json, commands_json, artifact_outputs_json,
                 quality_gates_json, template_status, created_by, updated_by,
                 created_at, updated_at, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14, NOW(), NOW(), 1)
             ON CONFLICT (tenant_id, template_name, template_version) WHERE deleted_at IS NULL DO NOTHING
             RETURNING uuid",
        )
        .bind(template_id)
        .bind(&template_uuid)
        .bind(tenant_id)
        .bind(0)
        .bind(&request.template_name)
        .bind(&request.template_version)
        .bind(request.platform.as_str())
        .bind(
            request
                .tech_stack
                .map(|stack| stack.as_str())
                .unwrap_or("OTHER"),
        )
        .bind(toolchain)
        .bind(commands_json)
        .bind(artifact_json)
        .bind(quality_gates)
        .bind("ACTIVE")
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_build_template", error))?;

        let Some(row) = result else {
            return Err(DeployServiceError::conflict(format!(
                "build template {}.{} already exists",
                request.template_name, request.template_version
            )));
        };
        let inserted_uuid: String = row.try_get("uuid").map_err(|error| {
            DeployServiceError::Internal(format!("read build template uuid: {error}"))
        })?;
        self.retrieve_build_template_repo(tenant_id, &inserted_uuid)
            .await
    }

    pub(super) async fn list_build_templates_repo(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<BuildTemplatePage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_build_template
             WHERE tenant_id = $1 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_build_template", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT t.uuid, t.template_name, t.template_version, t.platform, t.tech_stack,
                    t.toolchain_json, t.commands_json, t.artifact_outputs_json,
                    t.quality_gates_json, t.template_status, t.created_at, t.updated_at, t.version
             FROM deploy_build_template t
             WHERE t.tenant_id = $1 AND t.deleted_at IS NULL
             ORDER BY t.created_at DESC, t.id DESC LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_build_template", error))?;

        let items = rows
            .iter()
            .map(map_build_template_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(BuildTemplatePage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_build_template_repo(
        &self,
        tenant_id: i64,
        template_id: &str,
    ) -> DeployServiceResult<BuildTemplateResponse> {
        let row = sqlx::query(
            "SELECT t.uuid, t.template_name, t.template_version, t.platform, t.tech_stack,
                    t.toolchain_json, t.commands_json, t.artifact_outputs_json,
                    t.quality_gates_json, t.template_status, t.created_at, t.updated_at, t.version
             FROM deploy_build_template t
             WHERE t.tenant_id = $1 AND t.uuid = $2 AND t.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(template_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_build_template", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("build template not found"));
        };
        map_build_template_row(&row)
    }

    // -- signing identities ---------------------------------------------------

    pub(super) async fn create_signing_identity_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        request: &CreateSigningIdentityRequest,
    ) -> DeployServiceResult<SigningIdentityResponse> {
        let identity_id = next_id(self.id_generator())?;
        let identity_uuid = new_uuid();
        // Tenant-scoped platform target resolution (no App context on the
        // request); the FK stays nullable when the target is absent.
        let target_internal_id = match request.platform_target_id.as_deref() {
            Some(target_id) => Some(
                sqlx::query(
                    "SELECT id FROM deploy_app_platform_target
                     WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
                )
                .bind(tenant_id)
                .bind(target_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error("resolve signing platform target id", error))?
                .and_then(|row| row.try_get::<i64, _>("id").ok())
                .ok_or_else(|| DeployServiceError::not_found("platform target not found"))?,
            ),
            None => None,
        };

        let result = sqlx::query(
            "INSERT INTO deploy_signing_identity
                (id, uuid, tenant_id, organization_id, identity_name, signing_kind,
                 platform_target_id, fingerprint_sha256, expires_at, secret_ref,
                 identity_status, created_by, updated_by, created_at, updated_at, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CAST($9 AS TIMESTAMPTZ), $10, $11, $12, $12,
                     NOW(), NOW(), 1)
             RETURNING uuid",
        )
        .bind(identity_id)
        .bind(&identity_uuid)
        .bind(tenant_id)
        .bind(0)
        .bind(&request.identity_name)
        .bind(request.signing_kind.as_str())
        .bind(target_internal_id)
        .bind(request.fingerprint_sha256.as_deref())
        .bind(request.expires_at.as_deref())
        .bind(request.secret_ref.as_deref())
        .bind("PENDING")
        .bind(actor_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_signing_identity", error))?;

        let inserted_uuid: String = result.try_get("uuid").map_err(|error| {
            DeployServiceError::Internal(format!("read signing identity uuid: {error}"))
        })?;
        self.retrieve_signing_identity_repo(tenant_id, &inserted_uuid)
            .await
    }

    pub(super) async fn list_signing_identities_repo(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<SigningIdentityPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_signing_identity
             WHERE tenant_id = $1 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_signing_identity", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT i.uuid, i.identity_name, i.signing_kind, i.platform_target_id,
                    t.uuid AS platform_target_uuid, i.fingerprint_sha256, i.expires_at,
                    i.secret_ref, i.identity_status, i.created_at, i.updated_at, i.version
             FROM deploy_signing_identity i
             LEFT JOIN deploy_app_platform_target t ON t.id = i.platform_target_id
             WHERE i.tenant_id = $1 AND i.deleted_at IS NULL
             ORDER BY i.created_at DESC, i.id DESC LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_signing_identity", error))?;

        let items = rows
            .iter()
            .map(map_signing_identity_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SigningIdentityPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_signing_identity_repo(
        &self,
        tenant_id: i64,
        identity_id: &str,
    ) -> DeployServiceResult<SigningIdentityResponse> {
        let row = sqlx::query(
            "SELECT i.uuid, i.identity_name, i.signing_kind, i.platform_target_id,
                    t.uuid AS platform_target_uuid, i.fingerprint_sha256, i.expires_at,
                    i.secret_ref, i.identity_status, i.created_at, i.updated_at, i.version
             FROM deploy_signing_identity i
             LEFT JOIN deploy_app_platform_target t ON t.id = i.platform_target_id
             WHERE i.tenant_id = $1 AND i.uuid = $2 AND i.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(identity_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_signing_identity", error))?;

        let Some(row) = row else {
            return Err(DeployServiceError::not_found("signing identity not found"));
        };
        map_signing_identity_row(&row)
    }
}

fn map_platform_target_row(row: &PgRow) -> Result<PlatformTargetResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    let allowed_channels: Vec<String> = row
        .try_get::<Option<serde_json::Value>, _>("allowed_channels_json")
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    Ok(PlatformTargetResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        target_key: row.try_get("target_key").unwrap_or_default(),
        platform: row.try_get("platform").unwrap_or_default(),
        tech_stack: row.try_get("tech_stack").unwrap_or_default(),
        bundle_id: row.try_get("bundle_id").ok(),
        package_name: row.try_get("package_name").ok(),
        app_id_value: row.try_get("app_id_value").ok(),
        bundle_name: row.try_get("bundle_name").ok(),
        build_template_id: row.try_get("build_template_uuid").ok(),
        allowed_channels,
        target_status: row.try_get("target_status").unwrap_or_default(),
        created_at,
        updated_at,
        version: row.try_get::<i64, _>("version").unwrap_or(1).to_string(),
    })
}

fn map_source_repository_row(row: &PgRow) -> Result<SourceRepositoryResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    Ok(SourceRepositoryResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        app_id: row.try_get("app_uuid").unwrap_or_default(),
        repo_key: row.try_get("repo_key").unwrap_or_default(),
        repo_provider: row.try_get("repo_provider").unwrap_or_default(),
        repo_url: row.try_get("repo_url").unwrap_or_default(),
        default_branch: row.try_get("default_branch").unwrap_or_default(),
        clone_mode: row.try_get("clone_mode").unwrap_or_default(),
        credential_secret_ref: row.try_get("credential_secret_ref").ok(),
        repo_status: row.try_get("repo_status").unwrap_or_default(),
        last_error_code: row.try_get("last_error_code").ok(),
        created_at,
        updated_at,
        version: row.try_get::<i64, _>("version").unwrap_or(1).to_string(),
    })
}

fn map_build_template_row(row: &PgRow) -> Result<BuildTemplateResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    let json_value = |column: &str| -> Option<serde_json::Value> {
        row.try_get::<Option<serde_json::Value>, _>(column)
            .ok()
            .flatten()
    };
    Ok(BuildTemplateResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        template_name: row.try_get("template_name").unwrap_or_default(),
        template_version: row.try_get("template_version").unwrap_or_default(),
        platform: row.try_get("platform").unwrap_or_default(),
        tech_stack: row.try_get("tech_stack").unwrap_or_default(),
        toolchain: json_value("toolchain_json"),
        commands: json_value("commands_json").and_then(|value| serde_json::from_value(value).ok()),
        artifact_outputs: json_value("artifact_outputs_json")
            .and_then(|value| serde_json::from_value(value).ok()),
        quality_gates: json_value("quality_gates_json"),
        template_status: row.try_get("template_status").unwrap_or_default(),
        created_at,
        updated_at,
        version: row.try_get::<i64, _>("version").unwrap_or(1).to_string(),
    })
}

fn map_signing_identity_row(row: &PgRow) -> Result<SigningIdentityResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    Ok(SigningIdentityResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        identity_name: row.try_get("identity_name").unwrap_or_default(),
        signing_kind: row.try_get("signing_kind").unwrap_or_default(),
        platform_target_id: row.try_get("platform_target_uuid").ok(),
        fingerprint_sha256: row.try_get("fingerprint_sha256").ok(),
        // `expires_at` is TIMESTAMPTZ. Decoding it straight into `String` is
        // rejected on the wire, and `.ok()` swallowed that rejection — so the
        // expiry, which is the whole point of a signing identity record, was
        // silently absent from every response.
        expires_at: optional_datetime(row, "expires_at")?,
        secret_ref: row.try_get("secret_ref").ok(),
        identity_status: row.try_get("identity_status").unwrap_or_default(),
        created_at,
        updated_at,
        version: row.try_get::<i64, _>("version").unwrap_or(1).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{resolve_app_slug, GENERATED_SLUG_PREFIX, MAX_SLUG_LEN};

    const UUID: &str = "1b5eb653-9c2e-4f1a-8b77-2c4d5e6f7a8b";

    #[test]
    fn explicit_slug_is_authoritative() {
        assert_eq!(resolve_app_slug(Some("my-app"), "放大", UUID), "my-app");
    }

    #[test]
    fn blank_explicit_slug_falls_back_to_derivation() {
        // The console sends `slug: ""` when the operator clears the field; an
        // empty string must not be treated as a real slug.
        assert_eq!(resolve_app_slug(Some("   "), "My App", UUID), "my-app");
    }

    #[test]
    fn ascii_names_keep_the_historical_derivation() {
        assert_eq!(resolve_app_slug(None, "My App", UUID), "my-app");
        // Mirrors the shared conformance fixture
        // (`sdkwork-utils/specs/conformance/fixtures.json` → `string.slugify[0]`),
        // so the app slug keeps matching every other SDKWork slug field.
        assert_eq!(
            resolve_app_slug(None, "Hello, SDKWork!", UUID),
            "hello-sdk-work"
        );
    }

    #[test]
    fn non_ascii_name_never_produces_an_empty_slug() {
        // The regression this guard exists for: `slugify("放大")` is `""`, and
        // persisting that made every later Chinese-named app collide on
        // `uk_deploy_app_tenant_slug`.
        for name in ["放大", "我的应用", "放大 测试", "   "] {
            let slug = resolve_app_slug(None, name, UUID);
            assert!(!slug.is_empty(), "name {name:?} produced an empty slug");
            assert!(
                slug.bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
                "name {name:?} produced a non-DNS-safe slug {slug:?}"
            );
            assert!(slug.len() <= MAX_SLUG_LEN);
        }
    }

    #[test]
    fn generated_slug_is_stable_for_the_same_app() {
        // Determinism matters: a retried `apps.create` must derive the same slug,
        // otherwise each retry would reserve a fresh one.
        assert_eq!(
            resolve_app_slug(None, "放大", UUID),
            resolve_app_slug(None, "放大", UUID)
        );
        assert!(resolve_app_slug(None, "放大", UUID).starts_with(GENERATED_SLUG_PREFIX));
    }

    #[test]
    fn distinct_apps_get_distinct_generated_slugs() {
        let other = "9f8e7d6c-5b4a-3928-1706-0f1e2d3c4b5a";
        assert_ne!(
            resolve_app_slug(None, "放大", UUID),
            resolve_app_slug(None, "放大", other)
        );
    }
}
