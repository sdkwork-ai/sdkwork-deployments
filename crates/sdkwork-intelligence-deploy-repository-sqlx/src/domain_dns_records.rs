// The synced DNS resolution-record snapshot of a Zone.
//
// One sync run replaces the Zone's whole snapshot in one transaction, so the
// table always answers with a single provider answer. Every read here is
// store-only: the resolution page renders this snapshot, and only the sync
// operation talks to the provider.

use sdkwork_deploy_contract::{
    DeployServiceError, DeployServiceResult, DomainDnsRecordPage, DomainDnsRecordResponse,
};
use sdkwork_intelligence_deploy_service::{
    DomainDnsRecordFilter, DomainDnsRecordUpsert, DomainDnsSnapshotWrite, DomainHostnameAsset,
    ZoneDnsSyncTarget,
};
use sqlx::{postgres::PgRow, AssertSqlSafe, Row};

use crate::support::{datetime_from_row, new_uuid, next_id, pagination, store_error};
use crate::DeployRepository;

/// The owner gate every caller-facing zone query carries; see
/// `domain_zones.rs` for the ownership model it expresses.
fn zone_owner_gate(parameter: usize) -> String {
    format!("(z.user_id IS NULL OR z.user_id = ${parameter})")
}

impl DeployRepository {
    pub(super) async fn domain_zone_dns_sync_target_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<Option<ZoneDnsSyncTarget>> {
        // $1 tenant, $2 zone uuid, $3 owner (the gate is `user_id IS NULL OR
        // user_id = $3`; see `zone_owner_gate`).
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT z.id, z.apex_hostname, z.dns_provider, z.provider_account_id
             FROM deploy_dns_zone z
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND z.deleted_at IS NULL
               AND {}",
            zone_owner_gate(3)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_dns_zone sync target", error))?;
        row.map(|row| {
            Ok(ZoneDnsSyncTarget {
                zone_id: row
                    .try_get("id")
                    .map_err(|error| store_error("map deploy_dns_zone id", error))?,
                apex_hostname: row
                    .try_get("apex_hostname")
                    .map_err(|error| store_error("map deploy_dns_zone apex", error))?,
                dns_provider: row
                    .try_get("dns_provider")
                    .map_err(|error| store_error("map deploy_dns_zone provider", error))?,
                provider_account_id: row
                    .try_get("provider_account_id")
                    .map_err(|error| store_error("map deploy_dns_zone account", error))?,
            })
        })
        .transpose()
    }

    pub(super) async fn list_domain_hostname_assets_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<Vec<DomainHostnameAsset>> {
        // Resolve the zone under the same owner gate the sync target read
        // uses, so the assets can never come from a zone the caller cannot
        // reach.
        let zone = sqlx::query(AssertSqlSafe(format!(
            "SELECT z.id FROM deploy_dns_zone z
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND z.deleted_at IS NULL
               AND {}",
            zone_owner_gate(3)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_dns_zone for assets", error))?
        .ok_or_else(|| DeployServiceError::NotFound("domain zone not found".to_string()))?;
        let zone_internal_id: i64 = zone
            .try_get("id")
            .map_err(|error| store_error("map deploy_dns_zone id", error))?;
        let rows = sqlx::query(
            "SELECT d.id, d.hostname_ascii, d.hostname_type FROM deploy_domain d
             WHERE d.tenant_id = $1 AND d.zone_id = $2 AND d.deleted_at IS NULL
             ORDER BY d.id",
        )
        .bind(tenant_id)
        .bind(zone_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_domain assets", error))?;
        rows.iter()
            .map(|row| {
                Ok(DomainHostnameAsset {
                    domain_id: row.try_get("id")?,
                    hostname_ascii: row.try_get("hostname_ascii")?,
                    hostname_type: row.try_get("hostname_type")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(|error| {
                DeployServiceError::Internal(format!("map deploy_domain asset row: {error}"))
            })
    }

    /// Replaces the Zone's snapshot under the same zone-row lock the delete
    /// path takes, so a snapshot can never outlive its zone.
    pub(super) async fn replace_domain_zone_dns_records_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        snapshot: &DomainDnsSnapshotWrite,
    ) -> DeployServiceResult<i64> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin dns snapshot replace", error))?;
        let zone = sqlx::query(
            "SELECT id FROM deploy_dns_zone
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(zone_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("lock deploy_dns_zone for dns snapshot", error))?
        .ok_or_else(|| DeployServiceError::NotFound("domain zone not found".to_string()))?;
        let zone_internal_id: i64 = zone
            .try_get("id")
            .map_err(|error| store_error("map deploy_dns_zone id", error))?;

        // Whole-snapshot replace, not a merge: the truth is what the provider
        // answered at `synced_at`, and stale rows that merely *look* plausible
        // are worse than absent ones.
        sqlx::query("DELETE FROM deploy_domain_dns_record WHERE tenant_id = $1 AND zone_id = $2")
            .bind(tenant_id)
            .bind(zone_internal_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("clear deploy_domain_dns_record", error))?;

        for row in &snapshot.records {
            let id = next_id(&self.id_generator)?;
            let uuid = new_uuid();
            sqlx::query(AssertSqlSafe(
                "INSERT INTO deploy_domain_dns_record (
                    id, uuid, tenant_id, zone_id, domain_id,
                    record_name, record_type, record_value, ttl_seconds, priority,
                    record_line, dns_provider, provider_account_id, provider_record_ref,
                    synced_at
                 ) VALUES (
                    $1, $2, $3, $4, $5,
                    $6, $7, $8, $9, $10,
                    $11, $12, $13, $14,
                    $15::timestamptz
                 )"
                .to_string(),
            ))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(zone_internal_id)
            .bind(row.domain_id)
            .bind(&row.record_name)
            .bind(&row.record_type)
            .bind(&row.record_value)
            .bind(row.ttl_seconds)
            .bind(row.priority)
            .bind(row.record_line.as_deref())
            .bind(&snapshot.dns_provider)
            .bind(&snapshot.provider_account_id)
            .bind(row.provider_record_ref.as_deref())
            .bind(&snapshot.synced_at)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("insert deploy_domain_dns_record", error))?;
        }
        let stored = i64::try_from(snapshot.records.len()).unwrap_or(i64::MAX);
        tx.commit()
            .await
            .map_err(|error| store_error("commit dns snapshot replace", error))?;
        Ok(stored)
    }

    pub(super) async fn list_domain_zone_dns_records_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        filter: &DomainDnsRecordFilter,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<DomainDnsRecordPage> {
        let (_page, page_size, offset) = pagination(page, page_size);

        // The hostname restriction is the uuid the sync stamped on the rows,
        // so the filter is one equality against the joined hostname row — no
        // owner-string parsing, and the wildcard semantics were already
        // applied when the match was made. A record matched to a hostname
        // deleted after the sync leaves the Zone view with the hostname.
        let hostname_uuid = filter
            .hostname_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let host_keyword = filter
            .host
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                format!(
                    "%{}%",
                    value
                        .to_ascii_lowercase()
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                )
            });
        let record_type = filter
            .record_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_uppercase);
        let predicate = format!(
            "FROM deploy_domain_dns_record rec
             INNER JOIN deploy_dns_zone z ON z.id = rec.zone_id
             LEFT JOIN deploy_domain d ON d.id = rec.domain_id
             WHERE rec.tenant_id = $1
               AND z.uuid = $2 AND z.deleted_at IS NULL
               AND {owner_gate}
               AND rec.deleted_at IS NULL
               AND (rec.domain_id IS NULL OR d.deleted_at IS NULL)
               AND ($3::text IS NULL OR d.uuid = $3)
               AND ($4::text IS NULL OR LOWER(rec.record_name) LIKE $4 ESCAPE '\\\\')
               AND ($5::text IS NULL OR UPPER(rec.record_type) = $5)",
            owner_gate = zone_owner_gate(6)
        );

        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!("SELECT COUNT(*) {predicate}")))
            .bind(tenant_id)
            .bind(zone_id)
            .bind(hostname_uuid)
            .bind(host_keyword.as_deref())
            .bind(record_type.as_deref())
            .bind(owner_user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count deploy_domain_dns_record", error))?;

        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT rec.uuid, z.apex_hostname AS zone_apex, rec.record_name, rec.record_type,
                    rec.record_value, rec.ttl_seconds, rec.priority, rec.record_line,
                    rec.record_status, rec.dns_provider, rec.provider_account_id,
                    rec.provider_record_ref, d.uuid AS hostname_uuid,
                    rec.synced_at
             {predicate}
             ORDER BY rec.synced_at DESC, rec.id DESC LIMIT $7 OFFSET $8"
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_uuid)
        .bind(host_keyword.as_deref())
        .bind(record_type.as_deref())
        .bind(owner_user_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_domain_dns_record", error))?;

        let items = rows
            .iter()
            .map(map_dns_record_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(|error| {
                DeployServiceError::Internal(format!("map deploy_domain_dns_record row: {error}"))
            })?;
        let page_size = page_size;
        Ok(DomainDnsRecordPage {
            items,
            total,
            page,
            page_size,
        })
    }

    /// Appends one write-through row and reads it back through the same
    /// projection the page renders.
    pub(super) async fn insert_domain_zone_dns_record_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        record: &DomainDnsRecordUpsert,
    ) -> DeployServiceResult<DomainDnsRecordResponse> {
        let zone = sqlx::query(
            "SELECT id FROM deploy_dns_zone
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(zone_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_dns_zone for insert", error))?
        .ok_or_else(|| DeployServiceError::NotFound("domain zone not found".to_string()))?;
        let zone_internal_id: i64 = zone
            .try_get("id")
            .map_err(|error| store_error("map deploy_dns_zone id", error))?;
        let id = next_id(&self.id_generator)?;
        let uuid = new_uuid();
        sqlx::query(AssertSqlSafe(
            "INSERT INTO deploy_domain_dns_record (
                id, uuid, tenant_id, zone_id, domain_id,
                record_name, record_type, record_value, ttl_seconds, priority,
                record_line, dns_provider, provider_account_id, provider_record_ref,
                record_status, synced_at
             ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10,
                $11, $12, $13, $14,
                'ENABLED', $15::timestamptz
             )"
            .to_string(),
        ))
        .bind(id)
        .bind(&uuid)
        .bind(tenant_id)
        .bind(zone_internal_id)
        .bind(record.domain_id)
        .bind(&record.record_name)
        .bind(&record.record_type)
        .bind(&record.record_value)
        .bind(record.ttl_seconds)
        .bind(record.priority)
        .bind(record.record_line.as_deref())
        .bind(&record.dns_provider)
        .bind(&record.provider_account_id)
        .bind(record.provider_record_ref.as_deref())
        .bind(sdkwork_utils_rust::datetime::format_datetime(
            sdkwork_utils_rust::datetime::now(),
            None,
        ))
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_domain_dns_record", error))?;
        self.retrieve_domain_zone_dns_record_repo(tenant_id, zone_internal_id, &uuid)
            .await
    }

    pub(super) async fn update_domain_zone_dns_record_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        record_id: &str,
        record: &DomainDnsRecordUpsert,
    ) -> DeployServiceResult<DomainDnsRecordResponse> {
        let zone_internal_id = self.resolve_gated_zone_internal_id(tenant_id, owner_user_id, zone_id).await?;
        // `record_status` is deliberately absent: an edit does not touch the
        // provider-side pause state, which only the status operation flips.
        let result = sqlx::query(AssertSqlSafe(
            "UPDATE deploy_domain_dns_record
             SET record_name = $4, record_type = $5, record_value = $6,
                 ttl_seconds = $7, priority = $8, record_line = $9,
                 domain_id = $10, dns_provider = $11, provider_account_id = $12,
                 provider_record_ref = $13, updated_at = NOW(), version = version + 1
             WHERE tenant_id = $1 AND zone_id = $2 AND uuid = $3
               AND deleted_at IS NULL"
                .to_string(),
        ))
        .bind(tenant_id)
        .bind(zone_internal_id)
        .bind(record_id)
        .bind(&record.record_name)
        .bind(&record.record_type)
        .bind(&record.record_value)
        .bind(record.ttl_seconds)
        .bind(record.priority)
        .bind(record.record_line.as_deref())
        .bind(record.domain_id)
        .bind(&record.dns_provider)
        .bind(&record.provider_account_id)
        .bind(record.provider_record_ref.as_deref())
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("update deploy_domain_dns_record", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::NotFound(
                "dns record not found".to_string(),
            ));
        }
        self.retrieve_domain_zone_dns_record_repo(tenant_id, zone_internal_id, record_id)
            .await
    }

    pub(super) async fn set_domain_zone_dns_record_status_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        record_id: &str,
        enabled: bool,
    ) -> DeployServiceResult<DomainDnsRecordResponse> {
        let zone_internal_id = self.resolve_gated_zone_internal_id(tenant_id, owner_user_id, zone_id).await?;
        let status = if enabled { "ENABLED" } else { "DISABLED" };
        let result = sqlx::query(AssertSqlSafe(
            "UPDATE deploy_domain_dns_record
             SET record_status = $4, updated_at = NOW(), version = version + 1
             WHERE tenant_id = $1 AND zone_id = $2 AND uuid = $3
               AND deleted_at IS NULL"
                .to_string(),
        ))
        .bind(tenant_id)
        .bind(zone_internal_id)
        .bind(record_id)
        .bind(status)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("flip deploy_domain_dns_record status", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::NotFound(
                "dns record not found".to_string(),
            ));
        }
        self.retrieve_domain_zone_dns_record_repo(tenant_id, zone_internal_id, record_id)
            .await
    }

    pub(super) async fn delete_domain_zone_dns_record_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        record_id: &str,
    ) -> DeployServiceResult<()> {
        let zone_internal_id = self.resolve_gated_zone_internal_id(tenant_id, owner_user_id, zone_id).await?;
        // Hard delete: the provider no longer holds the record, so a row left
        // behind would answer a resolution the zone does not have.
        let result = sqlx::query(
            "DELETE FROM deploy_domain_dns_record
             WHERE tenant_id = $1 AND zone_id = $2 AND uuid = $3",
        )
        .bind(tenant_id)
        .bind(zone_internal_id)
        .bind(record_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("delete deploy_domain_dns_record", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::NotFound(
                "dns record not found".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) async fn domain_zone_dns_record_ref_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        record_id: &str,
    ) -> DeployServiceResult<Option<String>> {
        let zone_internal_id = self.resolve_gated_zone_internal_id(tenant_id, owner_user_id, zone_id).await?;
        let row = sqlx::query(
            "SELECT provider_record_ref FROM deploy_domain_dns_record
             WHERE tenant_id = $1 AND zone_id = $2 AND uuid = $3
               AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(zone_internal_id)
        .bind(record_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load deploy_domain_dns_record ref", error))?;
        row.map(|row| {
            row.try_get("provider_record_ref")
                .map_err(|error| store_error("map deploy_domain_dns_record ref", error))
        })
        .transpose()
    }

    /// Resolves the Zone's internal id under the same owner gate every
    /// caller-facing zone query carries, so a snapshot read can never reach a
    /// zone the caller does not own.
    async fn resolve_gated_zone_internal_id(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<i64> {
        // $1 tenant, $2 zone uuid, $3 owner (the gate is `user_id IS NULL OR
        // user_id = $3`; see `zone_owner_gate`).
        let zone = sqlx::query(AssertSqlSafe(format!(
            "SELECT z.id FROM deploy_dns_zone z
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND z.deleted_at IS NULL
               AND {}",
            zone_owner_gate(3)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_dns_zone id", error))?
        .ok_or_else(|| DeployServiceError::NotFound("domain zone not found".to_string()))?;
        zone.try_get("id")
            .map_err(|error| store_error("map deploy_dns_zone id", error))
    }

    async fn retrieve_domain_zone_dns_record_repo(
        &self,
        tenant_id: i64,
        zone_internal_id: i64,
        record_id: &str,
    ) -> DeployServiceResult<DomainDnsRecordResponse> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT rec.uuid, z.apex_hostname AS zone_apex, rec.record_name, rec.record_type,
                    rec.record_value, rec.ttl_seconds, rec.priority, rec.record_line,
                    rec.record_status, rec.dns_provider, rec.provider_account_id,
                    rec.provider_record_ref, d.uuid AS hostname_uuid,
                    rec.synced_at
             FROM deploy_domain_dns_record rec
             INNER JOIN deploy_dns_zone z ON z.id = rec.zone_id
             LEFT JOIN deploy_domain d ON d.id = rec.domain_id
             WHERE rec.tenant_id = $1 AND rec.zone_id = $2 AND rec.uuid = $3
               AND rec.deleted_at IS NULL"
        )))
        .bind(tenant_id)
        .bind(zone_internal_id)
        .bind(record_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_domain_dns_record", error))?
        .ok_or_else(|| DeployServiceError::NotFound("dns record not found".to_string()))?;
        map_dns_record_row(&row)
            .map_err(|error| DeployServiceError::Internal(format!("map dns record: {error}")))
    }
}

fn map_dns_record_row(row: &PgRow) -> Result<DomainDnsRecordResponse, sqlx::Error> {
    let record_name: String = row.try_get("record_name")?;
    let zone_apex: String = row.try_get("zone_apex")?;
    // 主机记录 derived against the zone the row belongs to: `@` for the apex,
    // the owner minus the zone suffix for everything else — the same fold the
    // hostname rows apply, so a page never shows two spellings of one name.
    let host = if record_name == zone_apex {
        "@".to_string()
    } else {
        record_name
            .strip_suffix(&format!(".{zone_apex}"))
            .unwrap_or(&record_name)
            .to_string()
    };
    Ok(DomainDnsRecordResponse {
        id: row.try_get("uuid")?,
        record_name,
        host,
        record_type: row.try_get("record_type")?,
        record_value: row.try_get("record_value")?,
        ttl_seconds: row.try_get("ttl_seconds")?,
        priority: row.try_get("priority")?,
        record_line: row.try_get("record_line")?,
        record_status: row.try_get("record_status")?,
        hostname_id: row.try_get("hostname_uuid")?,
        dns_provider: row.try_get("dns_provider")?,
        provider_account_id: row.try_get("provider_account_id")?,
        provider_record_ref: row.try_get("provider_record_ref")?,
        synced_at: datetime_from_row(row, "synced_at")?,
    })
}
