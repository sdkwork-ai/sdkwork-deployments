//! Download catalog persistence.
//!
//! Four tables behind the download ports: `deploy_download_catalog`,
//! `deploy_download_card`, `deploy_download_item` and `deploy_download_update`.
//! The catalog is the aggregate root; cards, items and update configurations are
//! always read together with it, so the store exposes one aggregate read instead
//! of per-table reads callers would have to stitch.
//!
//! Upgrade semantics are "create or patch": every upsert reads the current row
//! first and then either inserts or applies only the supplied fields, so a
//! console form that renders half the fields cannot clear the other half.
//!
//! Tenancy: operator reads bind `Option<i64>` where `None` is the cross-tenant
//! scope (`($n = 0 OR tenant_id = $n)`); writes always bind a concrete tenant.
//! Public reads are fenced to `status = 'PUBLISHED'` and never return tenant
//! identifiers.

use sdkwork_deploy_contract::{
    DeployServiceError, DeployServiceResult, DownloadCardDto, DownloadCatalogDto,
    DownloadCatalogSummaryDto, DownloadItemDto, DownloadUpdateDto, UpsertDownloadCardRequest,
    UpsertDownloadCatalogRequest, UpsertDownloadItemRequest, UpsertDownloadUpdateRequest,
    DOWNLOAD_CATALOG_STATUS_ARCHIVED, DOWNLOAD_CATALOG_STATUS_DRAFT,
    DOWNLOAD_CATALOG_STATUS_PUBLISHED, DOWNLOAD_CHANNEL_NIGHTLY,
};
use sdkwork_intelligence_deploy_service::repository::{
    PublishedDownloadPackage, PublishedDownloadRecord,
};
use sqlx::postgres::PgRow;
use sqlx::{AssertSqlSafe, Row};

use crate::support::{
    datetime_from_row, new_uuid, next_id, optional_datetime_from_row, store_error,
    string_list_from_row, string_list_to_json,
};
use crate::DeployRepository;

const CATALOG_SELECT: &str = "c.uuid, c.catalog_key, a.uuid AS app_uuid, c.product_key,
    c.product_name, c.product_version, c.default_channel_key, c.status, c.revision,
    c.published_at, c.created_at, c.updated_at, c.version";

const CARD_SELECT: &str = "cd.uuid, cd.card_key, cd.title, cd.description, cd.icon, cd.tone,
    cd.kind, cd.badge, cd.primary_strategy, cd.hidden, cd.sort_order";

const ITEM_SELECT: &str = "i.uuid, i.item_key, i.platform, i.architecture, i.label,
    i.file_name, i.installer_version, i.release_tag, p.uuid AS package_uuid, ar.uuid AS artifact_uuid,
    i.download_source, i.size_bytes, i.checksum_sha256, i.primary_action, i.disabled,
    i.unavailable_label, i.sort_order";

const UPDATE_SELECT: &str = "u.enabled, u.provider, u.feed_url, u.github_owner, u.github_repo,
    ch.uuid AS channel_uuid, u.channel_key, u.channel_file, u.allowed_channels,
    u.allow_prerelease, u.allow_downgrade, u.minimum_supported_version,
    u.check_interval_minutes, u.rollout_percentage, u.release_notes_url";

/// `($n = 0 OR column = $n)` scoped to one bound parameter.
fn optional_tenant_predicate(column: &str, parameter: usize) -> String {
    format!("({column} = ${parameter} OR ${parameter} = 0)")
}

fn map_catalog_summary(row: &PgRow) -> Result<DownloadCatalogSummaryDto, sqlx::Error> {
    Ok(DownloadCatalogSummaryDto {
        id: row.try_get("uuid")?,
        catalog_key: row.try_get("catalog_key")?,
        product_key: row.try_get("product_key")?,
        product_name: row.try_get("product_name")?,
        product_version: row.try_get("product_version")?,
        status: row.try_get("status")?,
        revision: row.try_get("revision")?,
        published_at: optional_datetime_from_row(row, "published_at")?,
        updated_at: datetime_from_row(row, "updated_at")?,
        version: row.try_get::<i64, _>("version")?.to_string(),
    })
}

fn map_catalog(row: &PgRow) -> Result<DownloadCatalogDto, sqlx::Error> {
    Ok(DownloadCatalogDto {
        id: row.try_get("uuid")?,
        catalog_key: row.try_get("catalog_key")?,
        app_id: row.try_get::<Option<String>, _>("app_uuid")?,
        product_key: row.try_get("product_key")?,
        product_name: row.try_get("product_name")?,
        product_version: row.try_get("product_version")?,
        default_channel_key: row.try_get("default_channel_key")?,
        status: row.try_get("status")?,
        revision: row.try_get("revision")?,
        published_at: optional_datetime_from_row(row, "published_at")?,
        created_at: datetime_from_row(row, "created_at")?,
        updated_at: datetime_from_row(row, "updated_at")?,
        version: row.try_get::<i64, _>("version")?.to_string(),
        cards: Vec::new(),
    })
}

fn map_card(row: &PgRow) -> Result<DownloadCardDto, sqlx::Error> {
    Ok(DownloadCardDto {
        id: row.try_get("uuid")?,
        card_key: row.try_get("card_key")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        icon: row.try_get("icon")?,
        tone: row.try_get("tone")?,
        kind: row.try_get("kind")?,
        badge: row.try_get("badge")?,
        primary_strategy: row.try_get("primary_strategy")?,
        hidden: row.try_get("hidden")?,
        sort_order: row.try_get("sort_order")?,
        items: Vec::new(),
    })
}

fn map_item(row: &PgRow) -> Result<DownloadItemDto, sqlx::Error> {
    Ok(DownloadItemDto {
        id: row.try_get("uuid")?,
        item_key: row.try_get("item_key")?,
        platform: row.try_get("platform")?,
        architecture: row.try_get("architecture")?,
        label: row.try_get("label")?,
        file_name: row.try_get("file_name")?,
        version: row.try_get("installer_version")?,
        release_tag: row.try_get("release_tag")?,
        package_id: row.try_get::<Option<String>, _>("package_uuid")?,
        artifact_id: row.try_get::<Option<String>, _>("artifact_uuid")?,
        download_source: row.try_get("download_source")?,
        size_bytes: row.try_get("size_bytes")?,
        checksum_sha256: row.try_get("checksum_sha256")?,
        primary_action: row.try_get("primary_action")?,
        disabled: row.try_get("disabled")?,
        unavailable_label: row.try_get("unavailable_label")?,
        sort_order: row.try_get("sort_order")?,
        update: None,
    })
}

fn map_update(row: &PgRow) -> Result<DownloadUpdateDto, sqlx::Error> {
    Ok(DownloadUpdateDto {
        enabled: row.try_get("enabled")?,
        provider: row.try_get("provider")?,
        feed_url: row.try_get("feed_url")?,
        github_owner: row.try_get("github_owner")?,
        github_repo: row.try_get("github_repo")?,
        channel_id: row.try_get::<Option<String>, _>("channel_uuid")?,
        channel_key: row.try_get("channel_key")?,
        channel_file: row.try_get("channel_file")?,
        allowed_channels: string_list_from_row(row, "allowed_channels")?.unwrap_or_default(),
        allow_prerelease: row.try_get("allow_prerelease")?,
        allow_downgrade: row.try_get("allow_downgrade")?,
        minimum_supported_version: row.try_get("minimum_supported_version")?,
        check_interval_minutes: row.try_get("check_interval_minutes")?,
        rollout_percentage: row.try_get("rollout_percentage")?,
        release_notes_url: row.try_get("release_notes_url")?,
    })
}

impl DeployRepository {
    async fn download_catalog_internal_id(
        &self,
        tenant_id: i64,
        catalog_key: &str,
    ) -> DeployServiceResult<i64> {
        let row = sqlx::query(
            "SELECT id FROM deploy_download_catalog
             WHERE tenant_id = $1 AND catalog_key = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(catalog_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve download catalog", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("download catalog not found"));
        };
        row.try_get("id")
            .map_err(|error| store_error("read download catalog id", error))
    }

    async fn download_card_internal_id(
        &self,
        catalog_id: i64,
        card_key: &str,
    ) -> DeployServiceResult<i64> {
        let row = sqlx::query(
            "SELECT id FROM deploy_download_card
             WHERE catalog_id = $1 AND card_key = $2 AND deleted_at IS NULL",
        )
        .bind(catalog_id)
        .bind(card_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve download card", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("download card not found"));
        };
        row.try_get("id")
            .map_err(|error| store_error("read download card id", error))
    }

    async fn download_item_internal_id(
        &self,
        card_id: i64,
        item_key: &str,
    ) -> DeployServiceResult<i64> {
        let row = sqlx::query(
            "SELECT id FROM deploy_download_item
             WHERE card_id = $1 AND item_key = $2 AND deleted_at IS NULL",
        )
        .bind(card_id)
        .bind(item_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve download item", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("download item not found"));
        };
        row.try_get("id")
            .map_err(|error| store_error("read download item id", error))
    }

    async fn download_app_internal_id(
        &self,
        tenant_id: i64,
        app_uuid: Option<&str>,
    ) -> DeployServiceResult<Option<i64>> {
        let Some(app_uuid) = app_uuid.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT id FROM deploy_app WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(app_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve download catalog app", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("app not found"));
        };
        Ok(Some(row.try_get("id").map_err(|error| {
            store_error("read download catalog app id", error)
        })?))
    }

    async fn download_package_internal_id(
        &self,
        tenant_id: i64,
        package_uuid: Option<&str>,
    ) -> DeployServiceResult<Option<i64>> {
        let Some(package_uuid) = package_uuid
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT id FROM deploy_package WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(package_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve download item package", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("package not found"));
        };
        Ok(Some(row.try_get("id").map_err(|error| {
            store_error("read download item package id", error)
        })?))
    }

    async fn download_artifact_internal_id(
        &self,
        tenant_id: i64,
        artifact_uuid: Option<&str>,
    ) -> DeployServiceResult<Option<i64>> {
        let Some(artifact_uuid) = artifact_uuid
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        let row = sqlx::query("SELECT id FROM deploy_artifact WHERE tenant_id = $1 AND uuid = $2")
            .bind(tenant_id)
            .bind(artifact_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve download item artifact", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("artifact not found"));
        };
        Ok(Some(row.try_get("id").map_err(|error| {
            store_error("read download item artifact id", error)
        })?))
    }

    async fn download_channel_internal_id(
        &self,
        tenant_id: i64,
        channel_uuid: Option<&str>,
    ) -> DeployServiceResult<Option<i64>> {
        let Some(channel_uuid) = channel_uuid
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT id FROM deploy_release_channel
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(channel_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve download update channel", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("release channel not found"));
        };
        Ok(Some(row.try_get("id").map_err(|error| {
            store_error("read download update channel id", error)
        })?))
    }

    async fn download_catalog_row(
        &self,
        tenant_id: Option<i64>,
        catalog_key: &str,
    ) -> DeployServiceResult<DownloadCatalogDto> {
        let sql = format!(
            "SELECT {CATALOG_SELECT}
             FROM deploy_download_catalog c
             LEFT JOIN deploy_app a ON a.id = c.app_id AND a.deleted_at IS NULL
             WHERE c.catalog_key = $1 AND c.deleted_at IS NULL
               AND {}",
            optional_tenant_predicate("c.tenant_id", 2)
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(catalog_key)
            .bind(tenant_id.unwrap_or(0))
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("read download catalog", error))?;
        match row {
            Some(row) => {
                map_catalog(&row).map_err(|error| store_error("map download catalog", error))
            }
            None => Err(DeployServiceError::not_found("download catalog not found")),
        }
    }

    async fn download_catalog_aggregate(
        &self,
        mut catalog: DownloadCatalogDto,
        public_only: bool,
    ) -> DeployServiceResult<DownloadCatalogDto> {
        let card_filter = if public_only {
            "AND cd.hidden = FALSE"
        } else {
            ""
        };
        let sql = format!(
            "SELECT {CARD_SELECT}
             FROM deploy_download_card cd
             JOIN deploy_download_catalog c ON c.id = cd.catalog_id
             WHERE c.uuid = $1 AND cd.deleted_at IS NULL {card_filter}
             ORDER BY cd.sort_order, cd.id"
        );
        let card_rows = sqlx::query(AssertSqlSafe(sql))
            .bind(&catalog.id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("read download cards", error))?;
        let mut cards = Vec::with_capacity(card_rows.len());
        for row in &card_rows {
            cards.push(map_card(row).map_err(|error| store_error("map download card", error))?);
        }

        let item_filter = if public_only {
            "AND i.disabled = FALSE"
        } else {
            ""
        };
        let sql = format!(
            "SELECT {ITEM_SELECT}, cd.uuid AS card_uuid
             FROM deploy_download_item i
             JOIN deploy_download_card cd ON cd.id = i.card_id
             JOIN deploy_download_catalog c ON c.id = cd.catalog_id
             LEFT JOIN deploy_package p ON p.id = i.package_id AND p.deleted_at IS NULL
             LEFT JOIN deploy_artifact ar ON ar.id = i.artifact_id
             WHERE c.uuid = $1 AND i.deleted_at IS NULL {item_filter}
             ORDER BY i.sort_order, i.id"
        );
        let item_rows = sqlx::query(AssertSqlSafe(sql))
            .bind(&catalog.id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("read download items", error))?;
        let mut items: Vec<(String, DownloadItemDto)> = Vec::with_capacity(item_rows.len());
        for row in &item_rows {
            let card_uuid: String = row
                .try_get("card_uuid")
                .map_err(|error| store_error("read download item card", error))?;
            let item = map_item(row).map_err(|error| store_error("map download item", error))?;
            items.push((card_uuid, item));
        }

        let sql = format!(
            "SELECT {UPDATE_SELECT}, i.uuid AS item_uuid
             FROM deploy_download_update u
             JOIN deploy_download_item i ON i.id = u.item_id
             JOIN deploy_download_card cd ON cd.id = i.card_id
             JOIN deploy_download_catalog c ON c.id = cd.catalog_id
             LEFT JOIN deploy_release_channel ch ON ch.id = u.channel_id AND ch.deleted_at IS NULL
             WHERE c.uuid = $1 AND i.deleted_at IS NULL"
        );
        let update_rows = sqlx::query(AssertSqlSafe(sql))
            .bind(&catalog.id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("read download updates", error))?;
        let mut updates: Vec<(String, DownloadUpdateDto)> = Vec::with_capacity(update_rows.len());
        for row in &update_rows {
            let item_uuid: String = row
                .try_get("item_uuid")
                .map_err(|error| store_error("read download update item", error))?;
            updates.push((
                item_uuid,
                map_update(row).map_err(|error| store_error("map download update", error))?,
            ));
        }

        for (card_uuid, mut item) in items {
            if let Some((_, update)) = updates.iter().find(|(uuid, _)| uuid == &item.id) {
                item.update = Some(update.clone());
            }
            if let Some(card) = cards.iter_mut().find(|card| card.id == card_uuid) {
                card.items.push(item);
            }
        }
        catalog.cards = cards;
        Ok(catalog)
    }

    pub(crate) async fn list_download_catalogs_repo(
        &self,
        tenant_id: Option<i64>,
    ) -> DeployServiceResult<Vec<DownloadCatalogSummaryDto>> {
        let sql = format!(
            "SELECT c.uuid, c.catalog_key, c.product_key, c.product_name, c.product_version,
                    c.status, c.revision, c.published_at, c.updated_at, c.version
             FROM deploy_download_catalog c
             WHERE c.deleted_at IS NULL AND {}
             ORDER BY c.updated_at DESC, c.id DESC",
            optional_tenant_predicate("c.tenant_id", 1)
        );
        let rows = sqlx::query(AssertSqlSafe(sql))
            .bind(tenant_id.unwrap_or(0))
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list download catalogs", error))?;
        rows.iter()
            .map(|row| {
                map_catalog_summary(row)
                    .map_err(|error| store_error("map download catalog summary", error))
            })
            .collect()
    }

    pub(crate) async fn retrieve_download_catalog_repo(
        &self,
        tenant_id: Option<i64>,
        catalog_key: &str,
    ) -> DeployServiceResult<DownloadCatalogDto> {
        let catalog = self.download_catalog_row(tenant_id, catalog_key).await?;
        self.download_catalog_aggregate(catalog, false).await
    }

    pub(crate) async fn upsert_download_catalog_repo(
        &self,
        tenant_id: i64,
        operator_id: Option<i64>,
        catalog_key: &str,
        request: &UpsertDownloadCatalogRequest,
    ) -> DeployServiceResult<DownloadCatalogDto> {
        let app_id = self
            .download_app_internal_id(tenant_id, request.app_id.as_deref())
            .await?;
        let existing = sqlx::query(
            "SELECT id FROM deploy_download_catalog
             WHERE tenant_id = $1 AND organization_id = 0 AND catalog_key = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(catalog_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read download catalog for upsert", error))?;

        if let Some(row) = existing {
            let id: i64 = row
                .try_get("id")
                .map_err(|error| store_error("read download catalog id", error))?;
            sqlx::query(
                "UPDATE deploy_download_catalog
                 SET product_key = COALESCE($2, product_key),
                     product_name = COALESCE($3, product_name),
                     product_version = COALESCE($4, product_version),
                     default_channel_key = COALESCE($5, default_channel_key),
                     app_id = COALESCE($6, app_id),
                     updated_by = $7,
                     updated_at = NOW(),
                     version = version + 1
                 WHERE id = $1",
            )
            .bind(id)
            .bind(request.product_key.as_deref())
            .bind(request.product_name.as_deref())
            .bind(request.product_version.as_deref())
            .bind(request.default_channel_key.as_deref())
            .bind(app_id)
            .bind(operator_id)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update download catalog", error))?;
        } else {
            let id = next_id(&self.id_generator)?;
            let uuid = new_uuid();
            sqlx::query(
                "INSERT INTO deploy_download_catalog
                    (id, uuid, tenant_id, organization_id, catalog_key, app_id, product_key,
                     product_name, product_version, default_channel_key, status, revision,
                     created_by, updated_by)
                 VALUES ($1, $2, $3, 0, $4, $5, $6, $7, $8, $9, $10, 1, $11, $11)",
            )
            .bind(id)
            .bind(uuid)
            .bind(tenant_id)
            .bind(catalog_key)
            .bind(app_id)
            .bind(request.product_key.clone().unwrap_or_default())
            .bind(request.product_name.clone().unwrap_or_default())
            .bind(request.product_version.clone().unwrap_or_default())
            .bind(
                request
                    .default_channel_key
                    .clone()
                    .unwrap_or_else(|| DOWNLOAD_CHANNEL_NIGHTLY.to_owned()),
            )
            .bind(DOWNLOAD_CATALOG_STATUS_DRAFT)
            .bind(operator_id)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert download catalog", error))?;
        }
        self.retrieve_download_catalog_repo(Some(tenant_id), catalog_key)
            .await
    }

    pub(crate) async fn publish_download_catalog_repo(
        &self,
        tenant_id: i64,
        operator_id: Option<i64>,
        catalog_key: &str,
    ) -> DeployServiceResult<DownloadCatalogDto> {
        self.download_catalog_internal_id(tenant_id, catalog_key)
            .await?;
        sqlx::query(
            "UPDATE deploy_download_catalog
             SET status = $3, revision = revision + 1, published_at = NOW(),
                 updated_by = $4, updated_at = NOW(), version = version + 1
             WHERE tenant_id = $1 AND catalog_key = $2 AND deleted_at IS NULL
               AND status <> $5",
        )
        .bind(tenant_id)
        .bind(catalog_key)
        .bind(DOWNLOAD_CATALOG_STATUS_PUBLISHED)
        .bind(operator_id)
        .bind(DOWNLOAD_CATALOG_STATUS_ARCHIVED)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("publish download catalog", error))?;
        self.retrieve_download_catalog_repo(Some(tenant_id), catalog_key)
            .await
    }

    pub(crate) async fn upsert_download_card_repo(
        &self,
        tenant_id: i64,
        operator_id: Option<i64>,
        catalog_key: &str,
        card_key: &str,
        request: &UpsertDownloadCardRequest,
    ) -> DeployServiceResult<DownloadCardDto> {
        let catalog_id = self
            .download_catalog_internal_id(tenant_id, catalog_key)
            .await?;
        let existing = sqlx::query(
            "SELECT id FROM deploy_download_card
             WHERE catalog_id = $1 AND card_key = $2 AND deleted_at IS NULL",
        )
        .bind(catalog_id)
        .bind(card_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read download card for upsert", error))?;
        if let Some(row) = existing {
            let id: i64 = row
                .try_get("id")
                .map_err(|error| store_error("read download card id", error))?;
            sqlx::query(
                "UPDATE deploy_download_card
                 SET title = COALESCE($2, title),
                     description = COALESCE($3, description),
                     icon = COALESCE($4, icon),
                     tone = COALESCE($5, tone),
                     kind = COALESCE($6, kind),
                     badge = COALESCE($7, badge),
                     primary_strategy = COALESCE($8, primary_strategy),
                     hidden = COALESCE($9, hidden),
                     sort_order = COALESCE($10, sort_order),
                     updated_by = $11,
                     updated_at = NOW(),
                     version = version + 1
                 WHERE id = $1",
            )
            .bind(id)
            .bind(request.title.as_deref())
            .bind(request.description.as_deref())
            .bind(request.icon.as_deref())
            .bind(request.tone.as_deref())
            .bind(request.kind.as_deref())
            .bind(request.badge.as_deref())
            .bind(request.primary_strategy.as_deref())
            .bind(request.hidden)
            .bind(request.sort_order)
            .bind(operator_id)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update download card", error))?;
        } else {
            let id = next_id(&self.id_generator)?;
            let uuid = new_uuid();
            sqlx::query(
                "INSERT INTO deploy_download_card
                    (id, uuid, tenant_id, organization_id, catalog_id, card_key, title, description,
                     icon, tone, kind, badge, primary_strategy, hidden, sort_order, created_by, updated_by)
                 VALUES ($1, $2, $3, 0, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15)",
            )
            .bind(id)
            .bind(uuid)
            .bind(tenant_id)
            .bind(catalog_id)
            .bind(card_key)
            .bind(request.title.clone().unwrap_or_else(|| card_key.to_owned()))
            .bind(request.description.clone().unwrap_or_default())
            .bind(request.icon.clone().unwrap_or_default())
            .bind(request.tone.clone().unwrap_or_default())
            .bind(
                request
                    .kind
                    .clone()
                    .unwrap_or_else(|| "desktop".to_owned()),
            )
            .bind(request.badge.clone().unwrap_or_default())
            .bind(
                request
                    .primary_strategy
                    .clone()
                    .unwrap_or_else(|| "detected-platform".to_owned()),
            )
            .bind(request.hidden.unwrap_or(false))
            .bind(request.sort_order.unwrap_or(0))
            .bind(operator_id)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert download card", error))?;
        }
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {CARD_SELECT} FROM deploy_download_card cd
             WHERE cd.catalog_id = $1 AND cd.card_key = $2 AND cd.deleted_at IS NULL"
        )))
        .bind(catalog_id)
        .bind(card_key)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("read download card after upsert", error))?;
        map_card(&row).map_err(|error| store_error("map download card", error))
    }

    pub(crate) async fn delete_download_card_repo(
        &self,
        tenant_id: i64,
        catalog_key: &str,
        card_key: &str,
    ) -> DeployServiceResult<()> {
        let catalog_id = self
            .download_catalog_internal_id(tenant_id, catalog_key)
            .await?;
        let card_id = self.download_card_internal_id(catalog_id, card_key).await?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin download card delete", error))?;
        sqlx::query(
            "UPDATE deploy_download_item
             SET deleted_at = NOW(), updated_at = NOW(), version = version + 1
             WHERE card_id = $1 AND deleted_at IS NULL",
        )
        .bind(card_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("delete download card items", error))?;
        sqlx::query(
            "UPDATE deploy_download_card
             SET deleted_at = NOW(), updated_at = NOW(), version = version + 1
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(card_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("delete download card", error))?;
        tx.commit()
            .await
            .map_err(|error| store_error("commit download card delete", error))?;
        Ok(())
    }

    pub(crate) async fn upsert_download_item_repo(
        &self,
        tenant_id: i64,
        operator_id: Option<i64>,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
        request: &UpsertDownloadItemRequest,
    ) -> DeployServiceResult<DownloadItemDto> {
        let catalog_id = self
            .download_catalog_internal_id(tenant_id, catalog_key)
            .await?;
        let card_id = self.download_card_internal_id(catalog_id, card_key).await?;
        let package_id = self
            .download_package_internal_id(tenant_id, request.package_id.as_deref())
            .await?;
        let artifact_id = self
            .download_artifact_internal_id(tenant_id, request.artifact_id.as_deref())
            .await?;
        let download_source = request
            .download_source
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        let existing = sqlx::query(
            "SELECT id FROM deploy_download_item
             WHERE card_id = $1 AND item_key = $2 AND deleted_at IS NULL",
        )
        .bind(card_id)
        .bind(item_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read download item for upsert", error))?;
        if let Some(row) = existing {
            let id: i64 = row
                .try_get("id")
                .map_err(|error| store_error("read download item id", error))?;
            sqlx::query(
                "UPDATE deploy_download_item
                 SET platform = COALESCE($2, platform),
                     architecture = COALESCE($3, architecture),
                     label = COALESCE($4, label),
                     file_name = COALESCE($5, file_name),
                     installer_version = COALESCE($6, installer_version),
                     release_tag = COALESCE($7, release_tag),
                     package_id = COALESCE($8, package_id),
                     artifact_id = COALESCE($9, artifact_id),
                     download_source = COALESCE($10, download_source),
                     size_bytes = COALESCE($11, size_bytes),
                     checksum_sha256 = COALESCE($12, checksum_sha256),
                     primary_action = COALESCE($13, primary_action),
                     disabled = COALESCE($14, disabled),
                     unavailable_label = COALESCE($15, unavailable_label),
                     sort_order = COALESCE($16, sort_order),
                     updated_by = $17,
                     updated_at = NOW(),
                     version = version + 1
                 WHERE id = $1",
            )
            .bind(id)
            .bind(request.platform.as_deref())
            .bind(request.architecture.as_deref())
            .bind(request.label.as_deref())
            .bind(request.file_name.as_deref())
            .bind(request.version.as_deref())
            .bind(request.release_tag.as_deref())
            .bind(package_id)
            .bind(artifact_id)
            .bind(download_source.clone())
            .bind(request.size_bytes)
            .bind(request.checksum_sha256.as_deref())
            .bind(request.primary_action)
            .bind(request.disabled)
            .bind(request.unavailable_label.as_deref())
            .bind(request.sort_order)
            .bind(operator_id)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update download item", error))?;
        } else {
            let id = next_id(&self.id_generator)?;
            let uuid = new_uuid();
            sqlx::query(
                "INSERT INTO deploy_download_item
                    (id, uuid, tenant_id, organization_id, card_id, item_key, platform, architecture,
                     label, file_name, installer_version, release_tag, package_id, artifact_id, download_source,
                     size_bytes, checksum_sha256, primary_action, disabled, unavailable_label,
                     sort_order, created_by, updated_by)
                 VALUES ($1, $2, $3, 0, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                         $16, $17, $18, $19, $20, $21, $21)",
            )
            .bind(id)
            .bind(uuid)
            .bind(tenant_id)
            .bind(card_id)
            .bind(item_key)
            .bind(request.platform.clone().unwrap_or_else(|| "generic".to_owned()))
            .bind(
                request
                    .architecture
                    .clone()
                    .unwrap_or_else(|| "universal".to_owned()),
            )
            .bind(request.label.clone().unwrap_or_else(|| item_key.to_owned()))
            .bind(request.file_name.clone().unwrap_or_default())
            .bind(request.version.clone().unwrap_or_default())
            .bind(request.release_tag.clone().unwrap_or_default())
            .bind(package_id)
            .bind(artifact_id)
            .bind(download_source.clone())
            .bind(request.size_bytes)
            .bind(request.checksum_sha256.as_deref())
            .bind(request.primary_action.unwrap_or(false))
            .bind(request.disabled.unwrap_or(false))
            .bind(request.unavailable_label.clone().unwrap_or_default())
            .bind(request.sort_order.unwrap_or(0))
            .bind(operator_id)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert download item", error))?;
        }
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {ITEM_SELECT}
             FROM deploy_download_item i
             LEFT JOIN deploy_package p ON p.id = i.package_id AND p.deleted_at IS NULL
             LEFT JOIN deploy_artifact ar ON ar.id = i.artifact_id
             WHERE i.card_id = $1 AND i.item_key = $2 AND i.deleted_at IS NULL"
        )))
        .bind(card_id)
        .bind(item_key)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("read download item after upsert", error))?;
        map_item(&row).map_err(|error| store_error("map download item", error))
    }

    pub(crate) async fn delete_download_item_repo(
        &self,
        tenant_id: i64,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
    ) -> DeployServiceResult<()> {
        let catalog_id = self
            .download_catalog_internal_id(tenant_id, catalog_key)
            .await?;
        let card_id = self.download_card_internal_id(catalog_id, card_key).await?;
        let item_id = self.download_item_internal_id(card_id, item_key).await?;
        sqlx::query(
            "UPDATE deploy_download_item
             SET deleted_at = NOW(), updated_at = NOW(), version = version + 1
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(item_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("delete download item", error))?;
        Ok(())
    }

    pub(crate) async fn upsert_download_update_repo(
        &self,
        tenant_id: i64,
        operator_id: Option<i64>,
        catalog_key: &str,
        card_key: &str,
        item_key: &str,
        request: &UpsertDownloadUpdateRequest,
    ) -> DeployServiceResult<DownloadUpdateDto> {
        let catalog_id = self
            .download_catalog_internal_id(tenant_id, catalog_key)
            .await?;
        let card_id = self.download_card_internal_id(catalog_id, card_key).await?;
        let item_id = self.download_item_internal_id(card_id, item_key).await?;
        let channel_id = self
            .download_channel_internal_id(tenant_id, request.channel_id.as_deref())
            .await?;
        let allowed_channels = string_list_to_json(request.allowed_channels.as_ref());

        let existing = sqlx::query("SELECT id FROM deploy_download_update WHERE item_id = $1")
            .bind(item_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("read download update for upsert", error))?;
        if let Some(row) = existing {
            let id: i64 = row
                .try_get("id")
                .map_err(|error| store_error("read download update id", error))?;
            sqlx::query(
                "UPDATE deploy_download_update
                 SET enabled = COALESCE($2, enabled),
                     provider = COALESCE($3, provider),
                     feed_url = COALESCE($4, feed_url),
                     github_owner = COALESCE($5, github_owner),
                     github_repo = COALESCE($6, github_repo),
                     channel_id = COALESCE($7, channel_id),
                     channel_key = COALESCE($8, channel_key),
                     channel_file = COALESCE($9, channel_file),
                     allowed_channels = COALESCE($10, allowed_channels),
                     allow_prerelease = COALESCE($11, allow_prerelease),
                     allow_downgrade = COALESCE($12, allow_downgrade),
                     minimum_supported_version = COALESCE($13, minimum_supported_version),
                     check_interval_minutes = COALESCE($14, check_interval_minutes),
                     rollout_percentage = COALESCE($15, rollout_percentage),
                     release_notes_url = COALESCE($16, release_notes_url),
                     updated_by = $17,
                     updated_at = NOW(),
                     version = version + 1
                 WHERE id = $1",
            )
            .bind(id)
            .bind(request.enabled)
            .bind(request.provider.as_deref())
            .bind(request.feed_url.as_deref())
            .bind(request.github_owner.as_deref())
            .bind(request.github_repo.as_deref())
            .bind(channel_id)
            .bind(request.channel_key.as_deref())
            .bind(request.channel_file.as_deref())
            .bind(allowed_channels.clone())
            .bind(request.allow_prerelease)
            .bind(request.allow_downgrade)
            .bind(request.minimum_supported_version.as_deref())
            .bind(request.check_interval_minutes)
            .bind(request.rollout_percentage)
            .bind(request.release_notes_url.as_deref())
            .bind(operator_id)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update download update", error))?;
        } else {
            let id = next_id(&self.id_generator)?;
            let uuid = new_uuid();
            sqlx::query(
                "INSERT INTO deploy_download_update
                    (id, uuid, tenant_id, organization_id, item_id, enabled, provider, feed_url,
                     github_owner, github_repo, channel_id, channel_key, channel_file, allowed_channels,
                     allow_prerelease, allow_downgrade, minimum_supported_version,
                     check_interval_minutes, rollout_percentage, release_notes_url,
                     created_by, updated_by)
                 VALUES ($1, $2, $3, 0, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                         $16, $17, $18, $19, $20, $20)",
            )
            .bind(id)
            .bind(uuid)
            .bind(tenant_id)
            .bind(item_id)
            .bind(request.enabled.unwrap_or(true))
            .bind(
                request
                    .provider
                    .clone()
                    .unwrap_or_else(|| "deployments".to_owned()),
            )
            .bind(request.feed_url.clone().unwrap_or_default())
            .bind(request.github_owner.clone().unwrap_or_default())
            .bind(request.github_repo.clone().unwrap_or_default())
            .bind(channel_id)
            .bind(
                request
                    .channel_key
                    .clone()
                    .unwrap_or_else(|| DOWNLOAD_CHANNEL_NIGHTLY.to_owned()),
            )
            .bind(request.channel_file.clone().unwrap_or_default())
            .bind(
                allowed_channels
                    .clone()
                    .unwrap_or_else(|| serde_json::json!([])),
            )
            .bind(request.allow_prerelease.unwrap_or(true))
            .bind(request.allow_downgrade.unwrap_or(false))
            .bind(
                request
                    .minimum_supported_version
                    .clone()
                    .unwrap_or_default(),
            )
            .bind(request.check_interval_minutes.unwrap_or(10))
            .bind(request.rollout_percentage.unwrap_or(100))
            .bind(request.release_notes_url.clone().unwrap_or_default())
            .bind(operator_id)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert download update", error))?;
        }
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {UPDATE_SELECT}
             FROM deploy_download_update u
             JOIN deploy_download_item i ON i.id = u.item_id
             LEFT JOIN deploy_release_channel ch ON ch.id = u.channel_id AND ch.deleted_at IS NULL
             WHERE i.id = $1"
        )))
        .bind(item_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("read download update after upsert", error))?;
        map_update(&row).map_err(|error| store_error("map download update", error))
    }

    pub(crate) async fn published_download_catalogs_repo(
        &self,
        catalog_key: &str,
    ) -> DeployServiceResult<Vec<DownloadCatalogDto>> {
        let sql = format!(
            "SELECT {CATALOG_SELECT}
             FROM deploy_download_catalog c
             LEFT JOIN deploy_app a ON a.id = c.app_id AND a.deleted_at IS NULL
             WHERE c.catalog_key = $1 AND c.deleted_at IS NULL AND c.status = $2
             ORDER BY c.revision DESC"
        );
        let rows = sqlx::query(AssertSqlSafe(sql))
            .bind(catalog_key)
            .bind(DOWNLOAD_CATALOG_STATUS_PUBLISHED)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("read published download catalogs", error))?;
        let mut catalogs = Vec::with_capacity(rows.len());
        for row in &rows {
            let catalog =
                map_catalog(row).map_err(|error| store_error("map published catalog", error))?;
            catalogs.push(self.download_catalog_aggregate(catalog, true).await?);
        }
        Ok(catalogs)
    }

    pub(crate) async fn published_download_record_repo(
        &self,
        product_key: &str,
        platform: &str,
        architecture: &str,
    ) -> DeployServiceResult<Option<PublishedDownloadRecord>> {
        let row = sqlx::query(
            "SELECT c.uuid AS catalog_uuid, i.uuid AS item_uuid
             FROM deploy_download_item i
             JOIN deploy_download_card cd ON cd.id = i.card_id
             JOIN deploy_download_catalog c ON c.id = cd.catalog_id
             WHERE c.product_key = $1 AND i.platform = $2 AND i.architecture = $3
               AND c.status = $4 AND c.deleted_at IS NULL AND cd.deleted_at IS NULL
               AND i.deleted_at IS NULL AND cd.hidden = FALSE AND i.disabled = FALSE
             ORDER BY c.revision DESC
             LIMIT 1",
        )
        .bind(product_key)
        .bind(platform)
        .bind(architecture)
        .bind(DOWNLOAD_CATALOG_STATUS_PUBLISHED)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve published download item", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let catalog_uuid: String = row
            .try_get("catalog_uuid")
            .map_err(|error| store_error("read published catalog uuid", error))?;
        let item_uuid: String = row
            .try_get("item_uuid")
            .map_err(|error| store_error("read published item uuid", error))?;
        self.published_download_record_inner(&catalog_uuid, &item_uuid)
            .await
    }

    pub(crate) async fn published_download_record_by_item_repo(
        &self,
        item_uuid: &str,
    ) -> DeployServiceResult<Option<PublishedDownloadRecord>> {
        let row = sqlx::query(
            "SELECT c.uuid AS catalog_uuid, i.uuid AS item_uuid
             FROM deploy_download_item i
             JOIN deploy_download_card cd ON cd.id = i.card_id
             JOIN deploy_download_catalog c ON c.id = cd.catalog_id
             WHERE i.uuid = $1 AND c.status = $2
               AND c.deleted_at IS NULL AND cd.deleted_at IS NULL AND i.deleted_at IS NULL",
        )
        .bind(item_uuid)
        .bind(DOWNLOAD_CATALOG_STATUS_PUBLISHED)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve published download item by uuid", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let catalog_uuid: String = row
            .try_get("catalog_uuid")
            .map_err(|error| store_error("read published catalog uuid", error))?;
        let item_uuid: String = row
            .try_get("item_uuid")
            .map_err(|error| store_error("read published item uuid", error))?;
        self.published_download_record_inner(&catalog_uuid, &item_uuid)
            .await
    }

    async fn published_download_record_inner(
        &self,
        catalog_uuid: &str,
        item_uuid: &str,
    ) -> DeployServiceResult<Option<PublishedDownloadRecord>> {
        let sql = format!(
            "SELECT {CATALOG_SELECT}
             FROM deploy_download_catalog c
             LEFT JOIN deploy_app a ON a.id = c.app_id AND a.deleted_at IS NULL
             WHERE c.uuid = $1 AND c.status = $2 AND c.deleted_at IS NULL"
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(catalog_uuid)
            .bind(DOWNLOAD_CATALOG_STATUS_PUBLISHED)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("read published download catalog", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let catalog =
            map_catalog(&row).map_err(|error| store_error("map published catalog", error))?;
        let catalog = self.download_catalog_aggregate(catalog, true).await?;
        let item = catalog
            .cards
            .iter()
            .flat_map(|card| card.items.iter())
            .find(|item| item.id == item_uuid)
            .cloned();
        let Some(item) = item else {
            return Ok(None);
        };
        let package = self
            .published_download_package(item.package_id.as_deref(), item.artifact_id.as_deref())
            .await?;
        let update = item.update.clone();
        Ok(Some(PublishedDownloadRecord {
            catalog,
            item,
            update,
            package,
        }))
    }

    /// Joins the release-domain package and artifact facts onto one installer.
    async fn published_download_package(
        &self,
        package_uuid: Option<&str>,
        artifact_uuid: Option<&str>,
    ) -> DeployServiceResult<Option<PublishedDownloadPackage>> {
        let mut facts = PublishedDownloadPackage::default();
        let mut found = false;
        if let Some(package_uuid) = package_uuid {
            let row = sqlx::query(
                "SELECT semantic_version, checksum_sha512, package_size_bytes
                 FROM deploy_package
                 WHERE uuid = $1 AND deleted_at IS NULL",
            )
            .bind(package_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("read published download package", error))?;
            if let Some(row) = row {
                found = true;
                facts.version = row
                    .try_get("semantic_version")
                    .map_err(|error| store_error("read package version", error))?;
                facts.size_bytes = row
                    .try_get::<Option<i64>, _>("package_size_bytes")
                    .map_err(|error| store_error("read package size", error))?
                    .unwrap_or(0);
                facts.sha512 = row
                    .try_get("checksum_sha512")
                    .map_err(|error| store_error("read package sha512", error))?;
            }
        }
        if let Some(artifact_uuid) = artifact_uuid {
            let row = sqlx::query(
                "SELECT file_name, drive_node_id, drive_space_id
                 FROM deploy_artifact
                 WHERE uuid = $1",
            )
            .bind(artifact_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("read published download artifact", error))?;
            if let Some(row) = row {
                found = true;
                facts.file_name = row
                    .try_get::<Option<String>, _>("file_name")
                    .map_err(|error| store_error("read artifact file name", error))?
                    .unwrap_or_default();
                facts.drive_node_id = row
                    .try_get("drive_node_id")
                    .map_err(|error| store_error("read artifact drive node", error))?;
                facts.drive_space_id = row
                    .try_get("drive_space_id")
                    .map_err(|error| store_error("read artifact drive space", error))?;
            }
        }
        Ok(if found { Some(facts) } else { None })
    }
}
