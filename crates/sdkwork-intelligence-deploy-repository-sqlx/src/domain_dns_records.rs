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
    DomainDnsRecordFilter, DomainDnsSnapshotWrite, DomainHostnameAsset, ZoneDnsSyncTarget,
};
use sqlx::{AssertSqlSafe, Row, postgres::PgRow};

use crate::DeployRepository;
use crate::support::{datetime_from_row, new_uuid, next_id, pagination, store_error};

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
        let predicate = format!(
            "FROM deploy_domain_dns_record rec
             LEFT JOIN deploy_domain d ON d.id = rec.domain_id
             WHERE rec.tenant_id = $1
               AND rec.zone_id = (SELECT z.id FROM deploy_dns_zone z
                                  WHERE z.tenant_id = $1 AND z.uuid = $2
                                    AND z.deleted_at IS NULL)
               AND rec.deleted_at IS NULL
               AND (rec.domain_id IS NULL OR d.deleted_at IS NULL)
               AND ($3::text IS NULL OR d.uuid = $3)"
        );

        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!("SELECT COUNT(*) {predicate}")))
            .bind(tenant_id)
            .bind(zone_id)
            .bind(hostname_uuid)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count deploy_domain_dns_record", error))?;

        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT rec.uuid, rec.record_name, rec.record_type, rec.record_value,
                    rec.ttl_seconds, rec.priority, rec.record_line, rec.dns_provider,
                    rec.provider_account_id, rec.provider_record_ref, d.uuid AS hostname_uuid,
                    rec.synced_at
             {predicate}
             ORDER BY rec.synced_at DESC, rec.id DESC LIMIT $4 OFFSET $5"
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_uuid)
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
}

fn map_dns_record_row(row: &PgRow) -> Result<DomainDnsRecordResponse, sqlx::Error> {
    Ok(DomainDnsRecordResponse {
        id: row.try_get("uuid")?,
        record_name: row.try_get("record_name")?,
        record_type: row.try_get("record_type")?,
        record_value: row.try_get("record_value")?,
        ttl_seconds: row.try_get("ttl_seconds")?,
        priority: row.try_get("priority")?,
        record_line: row.try_get("record_line")?,
        hostname_id: row.try_get("hostname_uuid")?,
        dns_provider: row.try_get("dns_provider")?,
        provider_account_id: row.try_get("provider_account_id")?,
        provider_record_ref: row.try_get("provider_record_ref")?,
        synced_at: datetime_from_row(row, "synced_at")?,
    })
}
