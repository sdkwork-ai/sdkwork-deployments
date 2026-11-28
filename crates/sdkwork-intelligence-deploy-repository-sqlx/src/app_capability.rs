//! `deploy_app_capability*` persistence: the six-dimension configuration
//! document, its immutable revisions and its review ledger.
//!
//! # Write model
//!
//! The document is replaced as a whole (`PUT .../capability`). The repository
//! therefore:
//!
//! 1. locks the owning `deploy_app` row, which serializes every capability writer
//!    for one application — the same granularity `deploy_application_resource_link`
//!    uses, and the reason two concurrent saves cannot both insert the default work
//!    mode;
//! 2. enforces the optimistic-concurrency token when the caller sent one;
//! 3. deletes the child rows of the document and re-inserts them from the
//!    normalized document. These rows are configuration, not history: their history
//!    lives in `deploy_app_capability_revision`, so a hard delete here does not lose
//!    an audit trail and keeps the partial unique indexes from filling with
//!    soft-deleted duplicates;
//! 4. stores the canonical snapshot as a revision, reusing an existing revision when
//!    the bytes are identical (the digest unique index makes duplicate bytes a
//!    conflict, and a re-save of an unchanged document is not a new version).
//!
//! # Review ledger
//!
//! Approval publishes the revision the reviewer read, not the document as it is
//! now: `deploy_app_capability_review.revision_id` is the frozen pointer, so an
//! author editing the draft during review cannot change what gets published.

use std::collections::BTreeMap;

use sdkwork_deploy_contract::{
    AppCapabilityResponse, CapabilityAssetKind, CapabilityDecisionCode, CapabilityMarketSection,
    CapabilityReviewPage, CapabilityReviewResponse, CapabilityReviewStatus, CapabilityRevisionPage,
    CapabilityRevisionResponse, CapabilityRowStatus, CapabilityStatus, CapabilityToolKind,
    DistributionScope, EmbedDefinition, LocalizedText, MarketAssetDefinition,
    ModelBindingDefinition, SceneAsset, SceneDefinition, WorkModeDefinition, WorkModeTool,
};
use sdkwork_intelligence_deploy_service::{
    DecideAppCapabilityReviewCommand, ListAppCapabilityReviewsCommand,
    ListAppCapabilityRevisionsCommand, LoadAppCapabilityCommand, ReplaceAppCapabilityCommand,
    RetrieveAppCapabilityRevisionCommand, SubmitAppCapabilityReviewCommand,
};
use sqlx::postgres::PgRow;
use sqlx::{AssertSqlSafe, Row};

use crate::support::{
    json_from_row, new_uuid, next_id, pagination, required_datetime, resolve_app_internal_id,
    sha256_hex, store_error,
};
use crate::DeployRepository;

/// Column list of the capability root, in the order [`CapabilityRow`] reads it.
const CAPABILITY_COLUMNS: &str = "id, uuid, capability_status, distribution_scope, avatar_url, \
     oauth_client_id, default_work_mode_key, current_revision_id, published_revision_id, published_at, \
     config_schema_version, created_at, updated_at, version";

/// The capability root row, decoded once and reused by every assembly path.
#[derive(Clone)]
struct CapabilityRow {
    internal_id: i64,
    status: CapabilityStatus,
    distribution_scope: DistributionScope,
    avatar_url: Option<String>,
    oauth_client_id: Option<String>,
    default_work_mode_key: Option<String>,
    current_revision_id: Option<i64>,
    published_revision_id: Option<i64>,
    published_at: Option<String>,
    config_schema_version: i32,
    created_at: String,
    updated_at: String,
    version: i64,
}

fn map_capability_row(
    row: &PgRow,
) -> Result<CapabilityRow, sdkwork_deploy_contract::DeployServiceError> {
    let read = |column: &str, error: &str| {
        row.try_get::<Option<String>, _>(column)
            .map_err(|cause| store_error(error, cause))
    };
    let status_raw: String = row
        .try_get("capability_status")
        .map_err(|cause| store_error("map capability status", cause))?;
    let status = CapabilityStatus::parse(&status_raw).ok_or_else(|| {
        sdkwork_deploy_contract::DeployServiceError::Internal(format!(
            "unknown capability status `{status_raw}` in deploy_app_capability"
        ))
    })?;
    let scope_raw: String = row
        .try_get("distribution_scope")
        .map_err(|cause| store_error("map capability scope", cause))?;
    let distribution_scope = DistributionScope::parse(&scope_raw).ok_or_else(|| {
        sdkwork_deploy_contract::DeployServiceError::Internal(format!(
            "unknown distribution scope `{scope_raw}` in deploy_app_capability"
        ))
    })?;
    Ok(CapabilityRow {
        internal_id: row
            .try_get("id")
            .map_err(|cause| store_error("map capability id", cause))?,
        status,
        distribution_scope,
        avatar_url: read("avatar_url", "map capability avatar")?,
        oauth_client_id: read("oauth_client_id", "map capability oauth client")?,
        default_work_mode_key: read("default_work_mode_key", "map capability default mode")?,
        current_revision_id: row
            .try_get("current_revision_id")
            .map_err(|cause| store_error("map capability current revision", cause))?,
        published_revision_id: row
            .try_get("published_revision_id")
            .map_err(|cause| store_error("map capability published revision", cause))?,
        published_at: match row
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("published_at")
            .map_err(|cause| store_error("map capability published_at", cause))?
        {
            Some(value) => Some(value.to_rfc3339()),
            None => None,
        },
        config_schema_version: row
            .try_get("config_schema_version")
            .map_err(|cause| store_error("map capability schema version", cause))?,
        created_at: required_datetime(row, "created_at")?,
        updated_at: required_datetime(row, "updated_at")?,
        version: row
            .try_get("version")
            .map_err(|cause| store_error("map capability version", cause))?,
    })
}

/// Text rows of one document, indexed by node.
#[derive(Default)]
struct TextIndex {
    root: sdkwork_deploy_contract::CapabilityRootTexts,
    modes: BTreeMap<String, (LocalizedText, LocalizedText)>,
    scenes: BTreeMap<String, (LocalizedText, LocalizedText)>,
}

impl TextIndex {
    fn mode(&self, key: &str) -> (LocalizedText, LocalizedText) {
        self.modes.get(key).cloned().unwrap_or_default()
    }

    fn scene(&self, key: &str) -> (LocalizedText, LocalizedText) {
        self.scenes.get(key).cloned().unwrap_or_default()
    }
}

impl DeployRepository {
    pub(super) async fn load_app_capability_repo(
        &self,
        command: LoadAppCapabilityCommand,
    ) -> sdkwork_deploy_contract::DeployServiceResult<AppCapabilityResponse> {
        let app_internal_id =
            resolve_app_internal_id(&self.pool, command.tenant_id, &command.app_uuid).await?;
        let capability = self
            .load_capability_row(command.tenant_id, app_internal_id)
            .await?;
        let Some(capability) = capability else {
            // Never configured: an empty draft, not a 404. The console shows the
            // six dimensions empty and the first save creates the row.
            return Ok(AppCapabilityResponse {
                app_id: command.app_uuid,
                status: CapabilityStatus::Draft,
                distribution_scope: DistributionScope::Private,
                avatar_url: None,
                oauth_client_id: None,
                default_work_mode_key: None,
                config_schema_version: sdkwork_deploy_contract::APP_CAPABILITY_SCHEMA_VERSION,
                current_revision_id: None,
                published_revision_id: None,
                published_at: None,
                texts: None,
                work_modes: Vec::new(),
                scenes: Vec::new(),
                models: Vec::new(),
                market: Vec::new(),
                embeds: Vec::new(),
                created_at: None,
                updated_at: None,
                version: "0".to_owned(),
            });
        };
        self.assemble_capability(
            command.tenant_id,
            app_internal_id,
            &command.app_uuid,
            capability,
        )
        .await
    }

    async fn load_capability_row(
        &self,
        tenant_id: i64,
        app_internal_id: i64,
    ) -> sdkwork_deploy_contract::DeployServiceResult<Option<CapabilityRow>> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {CAPABILITY_COLUMNS} FROM deploy_app_capability
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL"
        )))
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load deploy_app_capability", error))?;
        match row {
            None => Ok(None),
            Some(row) => Ok(Some(map_capability_row(&row)?)),
        }
    }

    async fn assemble_capability(
        &self,
        tenant_id: i64,
        app_internal_id: i64,
        app_uuid: &str,
        capability: CapabilityRow,
    ) -> sdkwork_deploy_contract::DeployServiceResult<AppCapabilityResponse> {
        let texts = self
            .load_capability_texts(tenant_id, app_internal_id)
            .await?;

        let mode_rows = sqlx::query(
            "SELECT id, mode_key, system_prompt, icon_ref, sort_order, is_default, mode_status
             FROM deploy_app_work_mode
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             ORDER BY sort_order ASC, id ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_work_mode", error))?;

        let tool_rows = sqlx::query(
            "SELECT work_mode_id, tool_kind, tool_ref, policy, sort_order
             FROM deploy_app_work_mode_tool
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             ORDER BY sort_order ASC, id ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_work_mode_tool", error))?;
        let mut tools_by_mode: BTreeMap<i64, Vec<WorkModeTool>> = BTreeMap::new();
        for row in &tool_rows {
            let mode_id: i64 = row
                .try_get("work_mode_id")
                .map_err(|error| store_error("map work mode tool parent", error))?;
            let kind_raw: String = row
                .try_get("tool_kind")
                .map_err(|error| store_error("map work mode tool kind", error))?;
            let kind = CapabilityToolKind::parse(&kind_raw).ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                    "unknown capability tool kind `{kind_raw}`"
                ))
            })?;
            let policy = json_from_row(row, "policy")
                .map_err(|error| store_error("map work mode tool policy", error))?
                .unwrap_or_else(|| serde_json::json!({}));
            tools_by_mode
                .entry(mode_id)
                .or_default()
                .push(WorkModeTool {
                    kind,
                    tool_ref: row
                        .try_get("tool_ref")
                        .map_err(|error| store_error("map work mode tool ref", error))?,
                    policy,
                    sort_order: row
                        .try_get("sort_order")
                        .map_err(|error| store_error("map work mode tool sort", error))?,
                });
        }

        let mut work_modes = Vec::with_capacity(mode_rows.len());
        for row in &mode_rows {
            let id: i64 = row
                .try_get("id")
                .map_err(|error| store_error("map work mode id", error))?;
            let mode_key: String = row
                .try_get("mode_key")
                .map_err(|error| store_error("map work mode key", error))?;
            let status_raw: String = row
                .try_get("mode_status")
                .map_err(|error| store_error("map work mode status", error))?;
            let (name, description) = texts.mode(&mode_key);
            work_modes.push(WorkModeDefinition {
                key: mode_key,
                name,
                description,
                system_prompt: row
                    .try_get("system_prompt")
                    .map_err(|error| store_error("map work mode prompt", error))?,
                icon_ref: row
                    .try_get("icon_ref")
                    .map_err(|error| store_error("map work mode icon", error))?,
                sort_order: row
                    .try_get("sort_order")
                    .map_err(|error| store_error("map work mode sort", error))?,
                is_default: row
                    .try_get("is_default")
                    .map_err(|error| store_error("map work mode default", error))?,
                status: CapabilityRowStatus::parse(&status_raw).ok_or_else(|| {
                    sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                        "unknown work mode status `{status_raw}`"
                    ))
                })?,
                tools: tools_by_mode.remove(&id).unwrap_or_default(),
            });
        }

        let scene_rows = sqlx::query(
            "SELECT id, scene_key, prompt_template, icon_ref, sort_order, is_featured, scene_status
             FROM deploy_app_scene
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             ORDER BY sort_order ASC, id ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_scene", error))?;

        let asset_rows = sqlx::query(
            "SELECT scene_id, asset_kind, asset_ref, asset_snapshot, sort_order
             FROM deploy_app_scene_asset
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             ORDER BY sort_order ASC, id ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_scene_asset", error))?;
        let mut assets_by_scene: BTreeMap<i64, Vec<SceneAsset>> = BTreeMap::new();
        for row in &asset_rows {
            let scene_id: i64 = row
                .try_get("scene_id")
                .map_err(|error| store_error("map scene asset parent", error))?;
            let kind_raw: String = row
                .try_get("asset_kind")
                .map_err(|error| store_error("map scene asset kind", error))?;
            let kind = CapabilityAssetKind::parse(&kind_raw).ok_or_else(|| {
                sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                    "unknown scene asset kind `{kind_raw}`"
                ))
            })?;
            let snapshot = json_from_row(row, "asset_snapshot")
                .map_err(|error| store_error("map scene asset snapshot", error))?
                .unwrap_or_else(|| serde_json::json!({}));
            assets_by_scene
                .entry(scene_id)
                .or_default()
                .push(SceneAsset {
                    kind,
                    asset_ref: row
                        .try_get("asset_ref")
                        .map_err(|error| store_error("map scene asset ref", error))?,
                    snapshot,
                    sort_order: row
                        .try_get("sort_order")
                        .map_err(|error| store_error("map scene asset sort", error))?,
                });
        }

        let mut scenes = Vec::with_capacity(scene_rows.len());
        for row in &scene_rows {
            let id: i64 = row
                .try_get("id")
                .map_err(|error| store_error("map scene id", error))?;
            let scene_key: String = row
                .try_get("scene_key")
                .map_err(|error| store_error("map scene key", error))?;
            let status_raw: String = row
                .try_get("scene_status")
                .map_err(|error| store_error("map scene status", error))?;
            let (title, subtitle) = texts.scene(&scene_key);
            scenes.push(SceneDefinition {
                key: scene_key,
                title,
                subtitle,
                prompt_template: row
                    .try_get("prompt_template")
                    .map_err(|error| store_error("map scene prompt", error))?,
                icon_ref: row
                    .try_get("icon_ref")
                    .map_err(|error| store_error("map scene icon", error))?,
                sort_order: row
                    .try_get("sort_order")
                    .map_err(|error| store_error("map scene sort", error))?,
                is_featured: row
                    .try_get("is_featured")
                    .map_err(|error| store_error("map scene featured", error))?,
                status: CapabilityRowStatus::parse(&status_raw).ok_or_else(|| {
                    sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                        "unknown scene status `{status_raw}`"
                    ))
                })?,
                assets: assets_by_scene.remove(&id).unwrap_or_default(),
            });
        }

        let model_rows = sqlx::query(
            "SELECT provider_code, model_key, sort_order, is_default, binding_status
             FROM deploy_app_model_binding
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             ORDER BY sort_order ASC, id ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_model_binding", error))?;
        let mut models = Vec::with_capacity(model_rows.len());
        for row in &model_rows {
            models.push(ModelBindingDefinition {
                provider_code: row
                    .try_get("provider_code")
                    .map_err(|error| store_error("map model provider", error))?,
                model_key: row
                    .try_get("model_key")
                    .map_err(|error| store_error("map model key", error))?,
                sort_order: row
                    .try_get("sort_order")
                    .map_err(|error| store_error("map model sort", error))?,
                is_default: row
                    .try_get("is_default")
                    .map_err(|error| store_error("map model default", error))?,
                status: map_row_status(row, "binding_status")?,
            });
        }

        let market_rows = sqlx::query(
            "SELECT section, asset_kind, asset_ref, category_code, sort_order, is_featured,
                    asset_status, asset_snapshot
             FROM deploy_app_market_asset
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             ORDER BY section ASC, sort_order ASC, id ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_market_asset", error))?;
        let mut market = Vec::with_capacity(market_rows.len());
        for row in &market_rows {
            let section_raw: String = row
                .try_get("section")
                .map_err(|error| store_error("map market section", error))?;
            let kind_raw: String = row
                .try_get("asset_kind")
                .map_err(|error| store_error("map market kind", error))?;
            let snapshot = json_from_row(row, "asset_snapshot")
                .map_err(|error| store_error("map market snapshot", error))?
                .unwrap_or_else(|| serde_json::json!({}));
            market.push(MarketAssetDefinition {
                section: CapabilityMarketSection::parse(&section_raw).ok_or_else(|| {
                    sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                        "unknown market section `{section_raw}`"
                    ))
                })?,
                kind: CapabilityAssetKind::parse(&kind_raw).ok_or_else(|| {
                    sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                        "unknown market asset kind `{kind_raw}`"
                    ))
                })?,
                asset_ref: row
                    .try_get("asset_ref")
                    .map_err(|error| store_error("map market ref", error))?,
                category_code: row
                    .try_get("category_code")
                    .map_err(|error| store_error("map market category", error))?,
                sort_order: row
                    .try_get("sort_order")
                    .map_err(|error| store_error("map market sort", error))?,
                is_featured: row
                    .try_get("is_featured")
                    .map_err(|error| store_error("map market featured", error))?,
                status: map_row_status(row, "asset_status")?,
                snapshot,
            });
        }

        let embed_rows = sqlx::query(
            "SELECT embed_key, embed_kind, server_ref, endpoint_url, transport, auth_mode,
                    scopes_json, sort_order, embed_status
             FROM deploy_app_embed
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             ORDER BY sort_order ASC, id ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_embed", error))?;
        let mut embeds = Vec::with_capacity(embed_rows.len());
        for row in &embed_rows {
            let kind_raw: String = row
                .try_get("embed_kind")
                .map_err(|error| store_error("map embed kind", error))?;
            let transport_raw: Option<String> = row
                .try_get("transport")
                .map_err(|error| store_error("map embed transport", error))?;
            let auth_raw: String = row
                .try_get("auth_mode")
                .map_err(|error| store_error("map embed auth mode", error))?;
            let scopes_value = json_from_row(row, "scopes_json")
                .map_err(|error| store_error("map embed scopes", error))?
                .unwrap_or_else(|| serde_json::json!([]));
            let scopes: Vec<String> = serde_json::from_value(scopes_value).map_err(|error| {
                sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                    "embed scopes_json is not a string array: {error}"
                ))
            })?;
            embeds.push(EmbedDefinition {
                key: row
                    .try_get("embed_key")
                    .map_err(|error| store_error("map embed key", error))?,
                kind: sdkwork_deploy_contract::CapabilityEmbedKind::parse(&kind_raw).ok_or_else(
                    || {
                        sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                            "unknown embed kind `{kind_raw}`"
                        ))
                    },
                )?,
                server_ref: row
                    .try_get("server_ref")
                    .map_err(|error| store_error("map embed server", error))?,
                endpoint_url: row
                    .try_get("endpoint_url")
                    .map_err(|error| store_error("map embed endpoint", error))?,
                transport: transport_raw
                    .as_deref()
                    .and_then(sdkwork_deploy_contract::CapabilityEmbedTransport::parse),
                auth_mode: sdkwork_deploy_contract::CapabilityEmbedAuthMode::parse(&auth_raw)
                    .ok_or_else(|| {
                        sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                            "unknown embed auth mode `{auth_raw}`"
                        ))
                    })?,
                scopes,
                sort_order: row
                    .try_get("sort_order")
                    .map_err(|error| store_error("map embed sort", error))?,
                status: map_row_status(row, "embed_status")?,
            });
        }

        Ok(AppCapabilityResponse {
            app_id: app_uuid.to_owned(),
            status: capability.status,
            distribution_scope: capability.distribution_scope,
            avatar_url: capability.avatar_url,
            oauth_client_id: capability.oauth_client_id,
            default_work_mode_key: capability.default_work_mode_key,
            config_schema_version: capability.config_schema_version,
            current_revision_id: capability
                .current_revision_id
                .map(|value| value.to_string()),
            published_revision_id: capability
                .published_revision_id
                .map(|value| value.to_string()),
            published_at: capability.published_at,
            texts: Some(texts.root),
            work_modes,
            scenes,
            models,
            market,
            embeds,
            created_at: Some(capability.created_at),
            updated_at: Some(capability.updated_at),
            version: capability.version.to_string(),
        })
    }

    async fn load_capability_texts(
        &self,
        tenant_id: i64,
        app_internal_id: i64,
    ) -> sdkwork_deploy_contract::DeployServiceResult<TextIndex> {
        let rows = sqlx::query(
            "SELECT scope_kind, scope_key, field_name, locale, text_value
             FROM deploy_app_capability_text
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             ORDER BY scope_kind ASC, scope_key ASC, field_name ASC, locale ASC",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_app_capability_text", error))?;
        let mut index = TextIndex::default();
        for row in &rows {
            let scope_kind: String = row
                .try_get("scope_kind")
                .map_err(|error| store_error("map text scope kind", error))?;
            let scope_key: String = row
                .try_get("scope_key")
                .map_err(|error| store_error("map text scope key", error))?;
            let field_name: String = row
                .try_get("field_name")
                .map_err(|error| store_error("map text field", error))?;
            let locale: String = row
                .try_get("locale")
                .map_err(|error| store_error("map text locale", error))?;
            let value: String = row
                .try_get("text_value")
                .map_err(|error| store_error("map text value", error))?;
            match scope_kind.as_str() {
                "CAPABILITY" => {
                    let target = match field_name.as_str() {
                        "slogan" => &mut index.root.slogan,
                        "input_placeholder" => &mut index.root.input_placeholder,
                        "summary" => &mut index.root.summary,
                        _ => continue,
                    };
                    target.insert(locale, value);
                }
                "WORK_MODE" => {
                    let entry = index.modes.entry(scope_key).or_default();
                    let target = match field_name.as_str() {
                        "name" => &mut entry.0,
                        "description" => &mut entry.1,
                        _ => continue,
                    };
                    target.insert(locale, value);
                }
                "SCENE" => {
                    let entry = index.scenes.entry(scope_key).or_default();
                    let target = match field_name.as_str() {
                        "title" => &mut entry.0,
                        "subtitle" => &mut entry.1,
                        _ => continue,
                    };
                    target.insert(locale, value);
                }
                _ => continue,
            }
        }
        Ok(index)
    }

    pub(super) async fn replace_app_capability_repo(
        &self,
        command: ReplaceAppCapabilityCommand,
    ) -> sdkwork_deploy_contract::DeployServiceResult<AppCapabilityResponse> {
        let ReplaceAppCapabilityCommand {
            tenant_id,
            organization_id,
            actor_id,
            app_uuid,
            document,
            snapshot,
        } = command;
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, &app_uuid).await?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin capability transaction", error))?;

        // Serialize capability writers for this application (see module docs).
        let locked = sqlx::query(
            "UPDATE deploy_app SET version = version
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_app for capability write", error))?;
        if locked.rows_affected() != 1 {
            return Err(sdkwork_deploy_contract::DeployServiceError::not_found(
                "app not found",
            ));
        }

        let existing = sqlx::query(AssertSqlSafe(format!(
            "SELECT {CAPABILITY_COLUMNS} FROM deploy_app_capability
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             FOR UPDATE"
        )))
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_app_capability", error))?;

        let (capability_id, previous_version) = match &existing {
            None => {
                let id = next_id(&self.id_generator)?;
                sqlx::query(
                    "INSERT INTO deploy_app_capability
                        (id, uuid, tenant_id, organization_id, app_id, capability_status,
                         distribution_scope, avatar_url, oauth_client_id, default_work_mode_key,
                         config_schema_version, created_by, updated_by, created_at, updated_at,
                         version)
                     VALUES ($1, $2, $3, $4, $5, 'draft', $6, $7, $8, $9, $10, $11, $11, NOW(), NOW(), 1)",
                )
                .bind(id)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id)
                .bind(app_internal_id)
                .bind(document.distribution_scope)
                .bind(&document.avatar_url)
                .bind(&document.oauth_client_id)
                .bind(&document.default_work_mode_key)
                .bind(sdkwork_deploy_contract::APP_CAPABILITY_SCHEMA_VERSION)
                .bind(actor_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("insert deploy_app_capability", error))?;
                (id, 0i64)
            }
            Some(row) => {
                let capability = map_capability_row(row)?;
                if !capability.status.allows_edit() {
                    return Err(sdkwork_deploy_contract::DeployServiceError::conflict(
                        "a retired capability document can no longer be edited",
                    ));
                }
                (capability.internal_id, capability.version)
            }
        };

        if let Some(expected) = document.expected_version {
            if expected != previous_version {
                return Err(sdkwork_deploy_contract::DeployServiceError::conflict(
                    format!(
                        "capability was modified since version {expected}; reload before saving"
                    ),
                ));
            }
        }

        // Replace the document's children. Configuration rows are not history;
        // the revision table is.
        for statement in [
            "DELETE FROM deploy_app_scene_asset WHERE tenant_id = $1 AND app_id = $2",
            "DELETE FROM deploy_app_scene WHERE tenant_id = $1 AND app_id = $2",
            "DELETE FROM deploy_app_work_mode_tool WHERE tenant_id = $1 AND app_id = $2",
            "DELETE FROM deploy_app_work_mode WHERE tenant_id = $1 AND app_id = $2",
            "DELETE FROM deploy_app_capability_text WHERE tenant_id = $1 AND app_id = $2",
            "DELETE FROM deploy_app_model_binding WHERE tenant_id = $1 AND app_id = $2",
            "DELETE FROM deploy_app_market_asset WHERE tenant_id = $1 AND app_id = $2",
            "DELETE FROM deploy_app_embed WHERE tenant_id = $1 AND app_id = $2",
        ] {
            sqlx::query(statement)
                .bind(tenant_id)
                .bind(app_internal_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("clear capability children", error))?;
        }

        for text in &document.texts {
            sqlx::query(
                "INSERT INTO deploy_app_capability_text
                    (id, uuid, tenant_id, organization_id, capability_id, app_id, scope_kind,
                     scope_key, field_name, locale, text_value, created_by, updated_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)",
            )
            .bind(next_id(&self.id_generator)?)
            .bind(new_uuid())
            .bind(tenant_id)
            .bind(organization_id)
            .bind(capability_id)
            .bind(app_internal_id)
            .bind(text.scope_kind)
            .bind(&text.scope_key)
            .bind(text.field_name)
            .bind(&text.locale)
            .bind(&text.text_value)
            .bind(actor_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert capability text", error))?;
        }

        for mode in &document.work_modes {
            let mode_id = next_id(&self.id_generator)?;
            sqlx::query(
                "INSERT INTO deploy_app_work_mode
                    (id, uuid, tenant_id, organization_id, capability_id, app_id, mode_key,
                     system_prompt, icon_ref, sort_order, is_default, mode_status, created_by,
                     updated_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)",
            )
            .bind(mode_id)
            .bind(new_uuid())
            .bind(tenant_id)
            .bind(organization_id)
            .bind(capability_id)
            .bind(app_internal_id)
            .bind(&mode.mode_key)
            .bind(&mode.system_prompt)
            .bind(&mode.icon_ref)
            .bind(mode.sort_order)
            .bind(mode.is_default)
            .bind(mode.status)
            .bind(actor_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert deploy_app_work_mode", error))?;

            for text in &mode.texts {
                sqlx::query(
                    "INSERT INTO deploy_app_capability_text
                        (id, uuid, tenant_id, organization_id, capability_id, app_id, scope_kind,
                         scope_key, field_name, locale, text_value, created_by, updated_by)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)",
                )
                .bind(next_id(&self.id_generator)?)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id)
                .bind(capability_id)
                .bind(app_internal_id)
                .bind(text.scope_kind)
                .bind(&text.scope_key)
                .bind(text.field_name)
                .bind(&text.locale)
                .bind(&text.text_value)
                .bind(actor_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("insert work mode text", error))?;
            }

            for tool in &mode.tools {
                sqlx::query(
                    "INSERT INTO deploy_app_work_mode_tool
                        (id, uuid, tenant_id, organization_id, capability_id, app_id, work_mode_id,
                         tool_kind, tool_ref, source_module, policy, sort_order, created_by, updated_by)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)",
                )
                .bind(next_id(&self.id_generator)?)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id)
                .bind(capability_id)
                .bind(app_internal_id)
                .bind(mode_id)
                .bind(tool.tool_kind)
                .bind(&tool.tool_ref)
                .bind(tool.source_module)
                .bind(&tool.policy)
                .bind(tool.sort_order)
                .bind(actor_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("insert work mode tool", error))?;
            }
        }

        for scene in &document.scenes {
            let scene_id = next_id(&self.id_generator)?;
            sqlx::query(
                "INSERT INTO deploy_app_scene
                    (id, uuid, tenant_id, organization_id, capability_id, app_id, scene_key,
                     prompt_template, icon_ref, sort_order, is_featured, scene_status, created_by,
                     updated_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)",
            )
            .bind(scene_id)
            .bind(new_uuid())
            .bind(tenant_id)
            .bind(organization_id)
            .bind(capability_id)
            .bind(app_internal_id)
            .bind(&scene.scene_key)
            .bind(&scene.prompt_template)
            .bind(&scene.icon_ref)
            .bind(scene.sort_order)
            .bind(scene.is_featured)
            .bind(scene.status)
            .bind(actor_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert deploy_app_scene", error))?;

            for text in &scene.texts {
                sqlx::query(
                    "INSERT INTO deploy_app_capability_text
                        (id, uuid, tenant_id, organization_id, capability_id, app_id, scope_kind,
                         scope_key, field_name, locale, text_value, created_by, updated_by)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)",
                )
                .bind(next_id(&self.id_generator)?)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id)
                .bind(capability_id)
                .bind(app_internal_id)
                .bind(text.scope_kind)
                .bind(&text.scope_key)
                .bind(text.field_name)
                .bind(&text.locale)
                .bind(&text.text_value)
                .bind(actor_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("insert scene text", error))?;
            }

            for asset in &scene.assets {
                sqlx::query(
                    "INSERT INTO deploy_app_scene_asset
                        (id, uuid, tenant_id, organization_id, capability_id, app_id, scene_id,
                         asset_kind, source_module, asset_ref, asset_snapshot, sort_order,
                         created_by, updated_by)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)",
                )
                .bind(next_id(&self.id_generator)?)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id)
                .bind(capability_id)
                .bind(app_internal_id)
                .bind(scene_id)
                .bind(asset.asset_kind)
                .bind(asset.source_module)
                .bind(&asset.asset_ref)
                .bind(&asset.snapshot)
                .bind(asset.sort_order)
                .bind(actor_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("insert scene asset", error))?;
            }
        }

        for model in &document.models {
            sqlx::query(
                "INSERT INTO deploy_app_model_binding
                    (id, uuid, tenant_id, organization_id, capability_id, app_id, provider_code,
                     model_key, sort_order, is_default, binding_status, created_by, updated_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)",
            )
            .bind(next_id(&self.id_generator)?)
            .bind(new_uuid())
            .bind(tenant_id)
            .bind(organization_id)
            .bind(capability_id)
            .bind(app_internal_id)
            .bind(&model.provider_code)
            .bind(&model.model_key)
            .bind(model.sort_order)
            .bind(model.is_default)
            .bind(model.status)
            .bind(actor_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert model binding", error))?;
        }

        for asset in &document.market {
            sqlx::query(
                "INSERT INTO deploy_app_market_asset
                    (id, uuid, tenant_id, organization_id, capability_id, app_id, section,
                     asset_kind, source_module, asset_ref, category_code, sort_order, is_featured,
                     asset_status, asset_snapshot, created_by, updated_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16)",
            )
            .bind(next_id(&self.id_generator)?)
            .bind(new_uuid())
            .bind(tenant_id)
            .bind(organization_id)
            .bind(capability_id)
            .bind(app_internal_id)
            .bind(asset.section)
            .bind(asset.asset_kind)
            .bind(asset.source_module)
            .bind(&asset.asset_ref)
            .bind(&asset.category_code)
            .bind(asset.sort_order)
            .bind(asset.is_featured)
            .bind(asset.status)
            .bind(&asset.snapshot)
            .bind(actor_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert market asset", error))?;
        }

        for embed in &document.embeds {
            sqlx::query(
                "INSERT INTO deploy_app_embed
                    (id, uuid, tenant_id, organization_id, capability_id, app_id, embed_key,
                     embed_kind, server_ref, endpoint_url, transport, auth_mode, scopes_json,
                     sort_order, embed_status, created_by, updated_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16)",
            )
            .bind(next_id(&self.id_generator)?)
            .bind(new_uuid())
            .bind(tenant_id)
            .bind(organization_id)
            .bind(capability_id)
            .bind(app_internal_id)
            .bind(&embed.embed_key)
            .bind(embed.embed_kind)
            .bind(&embed.server_ref)
            .bind(&embed.endpoint_url)
            .bind(embed.transport)
            .bind(embed.auth_mode)
            .bind(serde_json::json!(embed.scopes))
            .bind(embed.sort_order)
            .bind(embed.status)
            .bind(actor_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert embed", error))?;
        }

        // Immutable revision. Identical bytes reuse the existing revision: the
        // digest unique index makes a second row a conflict, and re-saving an
        // unchanged document is not a new version.
        let snapshot_text = serde_json::to_string(&snapshot).map_err(|error| {
            sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                "serialize capability snapshot: {error}"
            ))
        })?;
        let digest = sha256_hex(&snapshot_text);
        let revision_source = if document.submit_for_review {
            "REVIEW"
        } else {
            "DRAFT"
        };
        let existing_revision: Option<(i64, i32)> = sqlx::query_as(
            "SELECT id, revision_no FROM deploy_app_capability_revision
             WHERE capability_id = $1 AND snapshot_sha256 = $2",
        )
        .bind(capability_id)
        .bind(&digest)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("find capability revision by digest", error))?;

        let revision_id = match existing_revision {
            Some((id, _)) => id,
            None => {
                let next_no: i32 = sqlx::query_scalar(
                    "SELECT COALESCE(MAX(revision_no), 0) + 1 FROM deploy_app_capability_revision
                     WHERE capability_id = $1",
                )
                .bind(capability_id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| store_error("next capability revision number", error))?;
                let id = next_id(&self.id_generator)?;
                sqlx::query(
                    "INSERT INTO deploy_app_capability_revision
                        (id, uuid, tenant_id, organization_id, capability_id, app_id, revision_no,
                         revision_source, snapshot, snapshot_sha256, snapshot_schema_version,
                         created_by)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
                )
                .bind(id)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id)
                .bind(capability_id)
                .bind(app_internal_id)
                .bind(next_no)
                .bind(revision_source)
                .bind(&snapshot)
                .bind(&digest)
                .bind(sdkwork_deploy_contract::APP_CAPABILITY_SCHEMA_VERSION)
                .bind(actor_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("insert capability revision", error))?;
                id
            }
        };

        let next_status = if document.submit_for_review {
            // A pending review must not be duplicated; the partial unique index
            // enforces it, and this check turns the SQLSTATE into a sentence.
            let pending: Option<String> = sqlx::query_scalar(
                "SELECT uuid FROM deploy_app_capability_review
                 WHERE capability_id = $1 AND review_status = 'pending' AND deleted_at IS NULL",
            )
            .bind(capability_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("find pending capability review", error))?;
            if pending.is_some() {
                return Err(sdkwork_deploy_contract::DeployServiceError::conflict(
                    "a capability review is already pending for this application",
                ));
            }
            insert_capability_review(
                &mut transaction,
                &self.id_generator,
                capability_id,
                app_internal_id,
                tenant_id,
                organization_id,
                revision_id,
                actor_id,
                document.note.as_deref(),
            )
            .await?;
            "pending_review"
        } else {
            // Editing a published document returns it to draft while keeping the
            // published revision, so the live configuration stays live. A pending
            // review keeps the document in `pending_review`: the draft may still be
            // edited while under review, but the status must not claim the review
            // is over.
            let pending: Option<String> = sqlx::query_scalar(
                "SELECT uuid FROM deploy_app_capability_review
                 WHERE capability_id = $1 AND review_status = 'pending' AND deleted_at IS NULL",
            )
            .bind(capability_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("find pending capability review", error))?;
            if pending.is_some() {
                "pending_review"
            } else {
                "draft"
            }
        };

        sqlx::query(
            "UPDATE deploy_app_capability
             SET capability_status = $2,
                 distribution_scope = $3,
                 avatar_url = $4,
                 oauth_client_id = $8,
                 default_work_mode_key = $5,
                 current_revision_id = $6,
                 updated_by = $7,
                 updated_at = NOW(),
                 version = version + 1
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(capability_id)
        .bind(next_status)
        .bind(document.distribution_scope)
        .bind(&document.avatar_url)
        .bind(&document.default_work_mode_key)
        .bind(revision_id)
        .bind(actor_id)
        .bind(&document.oauth_client_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("update deploy_app_capability", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit capability transaction", error))?;

        // Re-read through the same assembly path so the response is exactly what
        // a subsequent GET returns (including generated text maps).
        self.load_app_capability_repo(LoadAppCapabilityCommand {
            tenant_id,
            app_uuid,
        })
        .await
    }

    pub(super) async fn submit_app_capability_review_repo(
        &self,
        command: SubmitAppCapabilityReviewCommand,
    ) -> sdkwork_deploy_contract::DeployServiceResult<CapabilityReviewResponse> {
        let SubmitAppCapabilityReviewCommand {
            tenant_id,
            organization_id,
            actor_id,
            app_uuid,
            note,
        } = command;
        let app_internal_id = resolve_app_internal_id(&self.pool, tenant_id, &app_uuid).await?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin capability review transaction", error))?;

        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {CAPABILITY_COLUMNS} FROM deploy_app_capability
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             FOR UPDATE"
        )))
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock capability for submission", error))?;
        let Some(row) = row else {
            return Err(sdkwork_deploy_contract::DeployServiceError::not_found(
                "the application has no capability document to submit",
            ));
        };
        let capability = map_capability_row(&row)?;
        if !matches!(
            capability.status,
            CapabilityStatus::Draft | CapabilityStatus::Approved | CapabilityStatus::Published
        ) {
            return Err(sdkwork_deploy_contract::DeployServiceError::conflict(
                format!(
                    "a capability in status `{}` cannot be submitted for review",
                    capability.status.as_str()
                ),
            ));
        }
        let pending: Option<String> = sqlx::query_scalar(
            "SELECT uuid FROM deploy_app_capability_review
             WHERE capability_id = $1 AND review_status = 'pending' AND deleted_at IS NULL",
        )
        .bind(capability.internal_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("find pending capability review", error))?;
        if pending.is_some() {
            return Err(sdkwork_deploy_contract::DeployServiceError::conflict(
                "a capability review is already pending for this application",
            ));
        }

        // Freeze the current document into a REVIEW revision and review row.
        let document = self
            .assemble_capability(tenant_id, app_internal_id, &app_uuid, capability.clone())
            .await?;
        let snapshot =
            sdkwork_intelligence_deploy_service::snapshot_from_capability_response(&document);
        let snapshot_text = serde_json::to_string(&snapshot).map_err(|error| {
            sdkwork_deploy_contract::DeployServiceError::Internal(format!(
                "serialize capability snapshot: {error}"
            ))
        })?;
        let digest = sha256_hex(&snapshot_text);
        let existing_revision: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM deploy_app_capability_revision
             WHERE capability_id = $1 AND snapshot_sha256 = $2",
        )
        .bind(capability.internal_id)
        .bind(&digest)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("find revision by digest for submission", error))?;
        let revision_id = match existing_revision {
            Some(id) => id,
            None => {
                let next_no: i32 = sqlx::query_scalar(
                    "SELECT COALESCE(MAX(revision_no), 0) + 1 FROM deploy_app_capability_revision
                     WHERE capability_id = $1",
                )
                .bind(capability.internal_id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| store_error("next capability revision number", error))?;
                let id = next_id(&self.id_generator)?;
                sqlx::query(
                    "INSERT INTO deploy_app_capability_revision
                        (id, uuid, tenant_id, organization_id, capability_id, app_id, revision_no,
                         revision_source, snapshot, snapshot_sha256, snapshot_schema_version,
                         created_by)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, 'REVIEW', $8, $9, $10, $11)",
                )
                .bind(id)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id)
                .bind(capability.internal_id)
                .bind(app_internal_id)
                .bind(next_no)
                .bind(&snapshot)
                .bind(&digest)
                .bind(sdkwork_deploy_contract::APP_CAPABILITY_SCHEMA_VERSION)
                .bind(actor_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("insert review revision", error))?;
                id
            }
        };

        let review_uuid = insert_capability_review(
            &mut transaction,
            &self.id_generator,
            capability.internal_id,
            app_internal_id,
            tenant_id,
            organization_id,
            revision_id,
            actor_id,
            note.as_deref(),
        )
        .await?;

        sqlx::query(
            "UPDATE deploy_app_capability
             SET capability_status = 'pending_review', current_revision_id = $2,
                 updated_by = $3, updated_at = NOW(), version = version + 1
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(capability.internal_id)
        .bind(revision_id)
        .bind(actor_id)
        .bind(&document.oauth_client_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("mark capability pending review", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit capability review submission", error))?;

        self.load_capability_review(tenant_id, &review_uuid).await
    }

    pub(super) async fn list_app_capability_reviews_repo(
        &self,
        command: ListAppCapabilityReviewsCommand,
    ) -> sdkwork_deploy_contract::DeployServiceResult<CapabilityReviewPage> {
        let app_internal_id =
            resolve_app_internal_id(&self.pool, command.tenant_id, &command.app_uuid).await?;
        let (page, page_size, offset) = pagination(command.page, command.page_size);
        let status = command.status.unwrap_or_default();
        let predicate = "tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
                         AND ($3 = '' OR review_status = $3)";
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_app_capability_review WHERE {predicate}"
        )))
        .bind(command.tenant_id)
        .bind(app_internal_id)
        .bind(&status)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count capability reviews", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT uuid, revision_id, attempt_no, review_status, submitted_by, submitted_at,
                    reviewer_id, decided_at, decision_code, decision_note, metadata, version
             FROM deploy_app_capability_review
             WHERE {predicate}
             ORDER BY submitted_at DESC, id DESC
             LIMIT $4 OFFSET $5"
        )))
        .bind(command.tenant_id)
        .bind(app_internal_id)
        .bind(&status)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list capability reviews", error))?;
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_review_row(row, &command.app_uuid)?);
        }
        Ok(CapabilityReviewPage {
            items,
            total,
            page,
            page_size,
        })
    }

    async fn load_capability_review(
        &self,
        tenant_id: i64,
        review_uuid: &str,
    ) -> sdkwork_deploy_contract::DeployServiceResult<CapabilityReviewResponse> {
        let row = sqlx::query(
            "SELECT app_id, uuid, revision_id, attempt_no, review_status, submitted_by,
                    submitted_at, reviewer_id, decided_at, decision_code, decision_note,
                    metadata, version
             FROM deploy_app_capability_review
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(review_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load capability review", error))?;
        let Some(row) = row else {
            return Err(sdkwork_deploy_contract::DeployServiceError::not_found(
                "capability review not found",
            ));
        };
        let app_internal_id: i64 = row
            .try_get("app_id")
            .map_err(|error| store_error("map review app id", error))?;
        let app_uuid = self
            .app_uuid_by_internal_id(tenant_id, app_internal_id)
            .await?;
        map_review_row(&row, &app_uuid)
    }

    pub(super) async fn decide_app_capability_review_repo(
        &self,
        command: DecideAppCapabilityReviewCommand,
    ) -> sdkwork_deploy_contract::DeployServiceResult<CapabilityReviewResponse> {
        let app_internal_id =
            resolve_app_internal_id(&self.pool, command.tenant_id, &command.app_uuid).await?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin capability decision transaction", error))?;
        let row = sqlx::query(
            "SELECT id, capability_id, revision_id, review_status
             FROM deploy_app_capability_review
             WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(command.tenant_id)
        .bind(app_internal_id)
        .bind(&command.review_uuid)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock capability review", error))?;
        let Some(row) = row else {
            return Err(sdkwork_deploy_contract::DeployServiceError::not_found(
                "capability review not found",
            ));
        };
        let review_internal_id: i64 = row
            .try_get("id")
            .map_err(|error| store_error("map review id", error))?;
        let capability_id: i64 = row
            .try_get("capability_id")
            .map_err(|error| store_error("map review capability", error))?;
        let revision_id: i64 = row
            .try_get("revision_id")
            .map_err(|error| store_error("map review revision", error))?;
        let status_raw: String = row
            .try_get("review_status")
            .map_err(|error| store_error("map review status", error))?;
        if CapabilityReviewStatus::parse(&status_raw) != Some(CapabilityReviewStatus::Pending) {
            return Err(sdkwork_deploy_contract::DeployServiceError::conflict(
                format!(
                    "review `{}` is `{status_raw}` and can no longer be decided",
                    command.review_uuid
                ),
            ));
        }

        let approved =
            command.decision == sdkwork_deploy_contract::CapabilityReviewDecision::Approve;
        let review_status = if approved { "approved" } else { "rejected" };
        sqlx::query(
            "UPDATE deploy_app_capability_review
             SET review_status = $2, reviewer_id = $3, decided_at = NOW(), decision_code = $4,
                 decision_note = $5, updated_by = $3, updated_at = NOW(), version = version + 1
             WHERE id = $1",
        )
        .bind(review_internal_id)
        .bind(review_status)
        .bind(command.actor_id)
        .bind(command.decision_code)
        .bind(&command.decision_note)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("decide capability review", error))?;

        if approved {
            // Approval publishes the revision the reviewer read, not whatever the
            // draft looks like now.
            sqlx::query(
                "UPDATE deploy_app_capability
                 SET capability_status = 'published', published_revision_id = $2,
                     published_at = NOW(), current_revision_id = $2,
                     updated_by = $3, updated_at = NOW(), version = version + 1
                 WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(capability_id)
            .bind(revision_id)
            .bind(command.actor_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("publish capability revision", error))?;
        } else {
            sqlx::query(
                "UPDATE deploy_app_capability
                 SET capability_status = 'draft', updated_by = $2, updated_at = NOW(),
                     version = version + 1
                 WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(capability_id)
            .bind(command.actor_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("return capability to draft", error))?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit capability decision", error))?;
        self.load_capability_review(command.tenant_id, &command.review_uuid)
            .await
    }

    pub(super) async fn list_app_capability_revisions_repo(
        &self,
        command: ListAppCapabilityRevisionsCommand,
    ) -> sdkwork_deploy_contract::DeployServiceResult<CapabilityRevisionPage> {
        let app_internal_id =
            resolve_app_internal_id(&self.pool, command.tenant_id, &command.app_uuid).await?;
        let (page, page_size, offset) = pagination(command.page, command.page_size);
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deploy_app_capability_revision
             WHERE tenant_id = $1 AND app_id = $2",
        )
        .bind(command.tenant_id)
        .bind(app_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count capability revisions", error))?;
        let rows = sqlx::query(
            "SELECT uuid, revision_no, revision_source, snapshot_sha256, snapshot_schema_version,
                    created_at, created_by
             FROM deploy_app_capability_revision
             WHERE tenant_id = $1 AND app_id = $2
             ORDER BY revision_no DESC, id DESC
             LIMIT $3 OFFSET $4",
        )
        .bind(command.tenant_id)
        .bind(app_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list capability revisions", error))?;
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_revision_row(row, &command.app_uuid, None)?);
        }
        Ok(CapabilityRevisionPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_app_capability_revision_repo(
        &self,
        command: RetrieveAppCapabilityRevisionCommand,
    ) -> sdkwork_deploy_contract::DeployServiceResult<CapabilityRevisionResponse> {
        let app_internal_id =
            resolve_app_internal_id(&self.pool, command.tenant_id, &command.app_uuid).await?;
        let row = sqlx::query(
            "SELECT uuid, revision_no, revision_source, snapshot_sha256, snapshot_schema_version,
                    created_at, created_by, snapshot
             FROM deploy_app_capability_revision
             WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3",
        )
        .bind(command.tenant_id)
        .bind(app_internal_id)
        .bind(&command.revision_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load capability revision", error))?;
        let Some(row) = row else {
            return Err(sdkwork_deploy_contract::DeployServiceError::not_found(
                "capability revision not found",
            ));
        };
        let snapshot = json_from_row(&row, "snapshot")
            .map_err(|error| store_error("map capability revision snapshot", error))?;
        map_revision_row(&row, &command.app_uuid, snapshot)
    }

    async fn app_uuid_by_internal_id(
        &self,
        tenant_id: i64,
        app_internal_id: i64,
    ) -> sdkwork_deploy_contract::DeployServiceResult<String> {
        let uuid: Option<String> = sqlx::query_scalar(
            "SELECT uuid FROM deploy_app WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_internal_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve app uuid", error))?;
        uuid.ok_or_else(|| sdkwork_deploy_contract::DeployServiceError::not_found("app not found"))
    }
}

fn map_row_status(
    row: &PgRow,
    column: &str,
) -> Result<CapabilityRowStatus, sdkwork_deploy_contract::DeployServiceError> {
    let raw: String = row
        .try_get(column)
        .map_err(|error| store_error("map capability row status", error))?;
    CapabilityRowStatus::parse(&raw).ok_or_else(|| {
        sdkwork_deploy_contract::DeployServiceError::Internal(format!(
            "unknown row status `{raw}` in column `{column}`"
        ))
    })
}

fn map_review_row(
    row: &PgRow,
    app_uuid: &str,
) -> sdkwork_deploy_contract::DeployServiceResult<CapabilityReviewResponse> {
    let status_raw: String = row
        .try_get("review_status")
        .map_err(|error| store_error("map review status", error))?;
    let status = CapabilityReviewStatus::parse(&status_raw).ok_or_else(|| {
        sdkwork_deploy_contract::DeployServiceError::Internal(format!(
            "unknown review status `{status_raw}`"
        ))
    })?;
    let decision_code_raw: Option<String> = row
        .try_get("decision_code")
        .map_err(|error| store_error("map review decision code", error))?;
    let decision_code = decision_code_raw
        .as_deref()
        .and_then(CapabilityDecisionCode::parse);
    let metadata = json_from_row(row, "metadata")
        .map_err(|error| store_error("map review metadata", error))?
        .unwrap_or_else(|| serde_json::json!({}));
    let note = metadata
        .get("note")
        .and_then(|value| value.as_str())
        .map(str::to_owned);
    let decided_at = row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("decided_at")
        .map_err(|error| store_error("map review decided_at", error))?;
    let submitted_at = required_datetime(row, "submitted_at")?;
    Ok(CapabilityReviewResponse {
        id: row
            .try_get("uuid")
            .map_err(|error| store_error("map review uuid", error))?,
        app_id: app_uuid.to_owned(),
        revision_id: row
            .try_get::<i64, _>("revision_id")
            .map_err(|error| store_error("map review revision id", error))?
            .to_string(),
        attempt_no: row
            .try_get("attempt_no")
            .map_err(|error| store_error("map review attempt", error))?,
        status,
        submitted_by: row
            .try_get::<i64, _>("submitted_by")
            .map_err(|error| store_error("map review submitter", error))?
            .to_string(),
        submitted_at,
        reviewer_id: row
            .try_get::<Option<i64>, _>("reviewer_id")
            .map_err(|error| store_error("map review reviewer", error))?
            .map(|value| value.to_string()),
        decided_at: decided_at.map(|value| value.to_rfc3339()),
        decision_code,
        decision_note: row
            .try_get("decision_note")
            .map_err(|error| store_error("map review note", error))?,
        note,
        version: row
            .try_get::<i64, _>("version")
            .map_err(|error| store_error("map review version", error))?
            .to_string(),
    })
}

fn map_revision_row(
    row: &PgRow,
    app_uuid: &str,
    snapshot: Option<serde_json::Value>,
) -> sdkwork_deploy_contract::DeployServiceResult<CapabilityRevisionResponse> {
    Ok(CapabilityRevisionResponse {
        id: row
            .try_get("uuid")
            .map_err(|error| store_error("map revision uuid", error))?,
        app_id: app_uuid.to_owned(),
        revision_no: row
            .try_get("revision_no")
            .map_err(|error| store_error("map revision number", error))?,
        source: row
            .try_get("revision_source")
            .map_err(|error| store_error("map revision source", error))?,
        sha256: row
            .try_get("snapshot_sha256")
            .map_err(|error| store_error("map revision digest", error))?,
        schema_version: row
            .try_get("snapshot_schema_version")
            .map_err(|error| store_error("map revision schema", error))?,
        created_at: required_datetime(row, "created_at")?,
        created_by: row
            .try_get::<Option<i64>, _>("created_by")
            .map_err(|error| store_error("map revision author", error))?
            .map(|value| value.to_string()),
        snapshot,
    })
}

async fn insert_capability_review(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id_generator: &sdkwork_database_id::SnowflakeIdGenerator,
    capability_id: i64,
    app_internal_id: i64,
    tenant_id: i64,
    organization_id: i64,
    revision_id: i64,
    actor_id: i64,
    note: Option<&str>,
) -> sdkwork_deploy_contract::DeployServiceResult<String> {
    let attempt_no: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(attempt_no), 0) + 1 FROM deploy_app_capability_review
         WHERE capability_id = $1",
    )
    .bind(capability_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| store_error("next capability review attempt", error))?;
    let review_uuid = new_uuid();
    let metadata = match note {
        Some(note) => serde_json::json!({ "note": note }),
        None => serde_json::json!({}),
    };
    sqlx::query(
        "INSERT INTO deploy_app_capability_review
            (id, uuid, tenant_id, organization_id, capability_id, app_id, revision_id, attempt_no,
             review_status, submitted_by, submitted_at, metadata, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', $9, NOW(), $10, $9, $9)",
    )
    .bind(next_id(id_generator)?)
    .bind(&review_uuid)
    .bind(tenant_id)
    .bind(organization_id)
    .bind(capability_id)
    .bind(app_internal_id)
    .bind(revision_id)
    .bind(attempt_no)
    .bind(actor_id)
    .bind(&metadata)
    .execute(&mut **transaction)
    .await
    .map_err(|error| store_error("insert capability review", error))?;
    Ok(review_uuid)
}
