use sdkwork_deploy_contract::{
    CreateDomainHostnameRequest, CreateDomainZoneRequest, DeployServiceError, DeployServiceResult,
    DomainHostnamePage, DomainHostnameResponse, DomainZonePage, DomainZoneResponse,
    ListDomainZonesQuery, UpdateDomainHostnameRequest, UpdateDomainZoneRequest, ZoneScope,
};
use sdkwork_intelligence_deploy_service::{dns_txt_record_name, DomainVerificationChallenge};
use sdkwork_utils_rust::crypto::sha256_hash;
use sqlx::{postgres::PgRow, AssertSqlSafe, Row};

use crate::support::{
    datetime_from_row, new_uuid, next_id, optional_datetime_from_row, pagination, store_error,
};
use crate::DeployRepository;

const ZONE_SELECT: &str =
    "z.uuid, z.apex_hostname, z.display_name, z.dns_provider, z.provider_account_id, z.status,
     z.updated_at, z.version,
     (SELECT COUNT(*) FROM deploy_domain d WHERE d.zone_id = z.id AND d.deleted_at IS NULL) AS hostname_count,
     (SELECT COUNT(*) FROM deploy_domain d WHERE d.zone_id = z.id AND d.verification_status = 'VERIFIED' AND d.deleted_at IS NULL) AS verified_hostname_count,
     (SELECT COUNT(DISTINCT ci.certificate_id) FROM deploy_certificate_identifier ci JOIN deploy_domain d ON d.id = ci.domain_id WHERE d.zone_id = z.id AND d.deleted_at IS NULL) AS certificate_count,
     (SELECT COUNT(*) FROM deploy_app_binding b JOIN deploy_domain d ON d.id = b.domain_id WHERE d.zone_id = z.id AND d.deleted_at IS NULL AND b.deleted_at IS NULL) AS binding_count";

const HOSTNAME_SELECT: &str =
    "d.uuid, z.uuid AS zone_uuid, z.apex_hostname, d.hostname_ascii, d.hostname_type,
     d.verification_status, d.verified_at, d.status, d.created_at, d.updated_at, d.version,
     (SELECT COUNT(DISTINCT ci.certificate_id) FROM deploy_certificate_identifier ci WHERE ci.domain_id = d.id) AS certificate_count,
     (SELECT COUNT(*) FROM deploy_app_binding b WHERE b.domain_id = d.id AND b.deleted_at IS NULL) AS binding_count";

/// The owner gate every caller-facing zone query carries.
///
/// A zone created through the console belongs to the user who created it, so a
/// caller only ever reaches the zones it owns. `user_id IS NULL` marks a zone
/// that is not user-private but tenant-level - the platform-owned
/// `app.<suffix>` inventory the deployment provisions for the whole tenant -
/// and it stays visible to every member of the tenant.
///
/// A caller with no user subject therefore reads exactly those tenant-level
/// zones and nothing else, because `user_id = NULL` is never true. That is the
/// honest answer for a service principal, and it needs no separate branch.
///
/// This gate answers "may the caller reach this row". It deliberately does not
/// answer "does this row belong on the page being asked for" - that is the
/// separate `scope` facet in `list_domain_zones_repo`, and the two stay apart
/// because the console's root-domain list wants only the caller's own root
/// domains while the certificate coverage picker still needs the tenant-level
/// platform zones.
fn zone_owner_gate(parameter: usize) -> String {
    format!("(z.user_id IS NULL OR z.user_id = ${parameter})")
}

impl DeployRepository {
    pub(super) async fn list_domain_zones_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        query: &ListDomainZonesQuery,
    ) -> DeployServiceResult<DomainZonePage> {
        let (page, page_size, offset) = pagination(query.page, query.page_size);
        let status = query.status.as_deref().unwrap_or("");
        // The ownership level as the literal the predicate compares against.
        // Absent maps to the empty string, which the predicate reads as "both
        // levels" — that keeps every caller that never sends `scope` on the
        // answer it had before the parameter existed.
        let scope = match query.scope {
            Some(ZoneScope::User) => "USER",
            Some(ZoneScope::Platform) => "PLATFORM",
            None => "",
        };
        let keyword = query
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{}%", value.to_ascii_lowercase()))
            .unwrap_or_default();
        // Conjunctive with the owner gate, not a replacement for it: the gate
        // decides *which rows the caller may reach*, the scope narrows that set
        // to one ownership level. A caller asking for PLATFORM sees only the
        // tenant-level zones it could already reach; asking for USER sees only
        // its own, which is what the root-domain list wants. The platform
        // inventory is `app.<suffix>`, so it is never a root domain and never
        // belongs on that list.
        let predicate = format!(
            "z.tenant_id = $1 AND {} AND z.deleted_at IS NULL
            AND ($3 = '' OR z.status = $3)
            AND ($4 = '' OR LOWER(z.apex_hostname) LIKE $4 OR LOWER(COALESCE(z.display_name, '')) LIKE $4)
            AND ($5 = '' OR ($5 = 'USER' AND z.user_id IS NOT NULL) OR ($5 = 'PLATFORM' AND z.user_id IS NULL))",
            zone_owner_gate(2)
        );
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_dns_zone z WHERE {predicate}"
        )))
        .bind(tenant_id)
        .bind(owner_user_id)
        .bind(status)
        .bind(&keyword)
        .bind(scope)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_dns_zone", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {ZONE_SELECT} FROM deploy_dns_zone z WHERE {predicate}
             ORDER BY z.updated_at DESC, z.id DESC LIMIT $6 OFFSET $7"
        )))
        .bind(tenant_id)
        .bind(owner_user_id)
        .bind(status)
        .bind(keyword)
        .bind(scope)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_dns_zone", error))?;
        let items = rows
            .iter()
            .map(map_zone_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                DeployServiceError::Internal(format!("map deploy_dns_zone: {error}"))
            })?;
        Ok(DomainZonePage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn create_domain_zone_repo(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        request: &CreateDomainZoneRequest,
    ) -> DeployServiceResult<DomainZoneResponse> {
        let apex = request.apex_hostname.as_str();
        // The apex must not already be registered as a hostname (zone apexes
        // live in deploy_domain as well), and it must not nest under or
        // contain an existing zone apex so zones never overlap. Both checks
        // are global, matching the global unique indexes.
        let conflicts: (bool, bool) = sqlx::query_as(
            "SELECT
                EXISTS (SELECT 1 FROM deploy_domain
                        WHERE deleted_at IS NULL AND hostname_ascii = $1),
                EXISTS (SELECT 1 FROM deploy_dns_zone
                        WHERE deleted_at IS NULL AND (apex_hostname = $1
                            OR apex_hostname LIKE '%.' || $1
                            OR $1 LIKE '%.' || apex_hostname))",
        )
        .bind(apex)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("check deploy_dns_zone conflicts", error))?;
        if conflicts.0 {
            return Err(DeployServiceError::conflict(
                "apex hostname is already registered as a hostname",
            ));
        }
        if conflicts.1 {
            return Err(DeployServiceError::conflict(
                "zone apex overlaps an existing domain zone",
            ));
        }
        let zone_id = next_id(self.id_generator())?;
        let zone_uuid = new_uuid();
        let hostname_id = next_id(self.id_generator())?;
        let hostname_uuid = new_uuid();
        let organization_id = organization_id.unwrap_or_default();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin create deploy_dns_zone", error))?;
        sqlx::query(
            "INSERT INTO deploy_dns_zone (
                id, uuid, tenant_id, organization_id, apex_hostname, display_name, dns_provider,
                provider_zone_ref, provider_account_id, status, user_id, created_by, updated_by
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'ACTIVE', $10, $11, $11)",
        )
        .bind(zone_id)
        .bind(&zone_uuid)
        .bind(tenant_id)
        .bind(organization_id)
        .bind(&request.apex_hostname)
        .bind(request.display_name.as_deref())
        .bind(request.dns_provider.as_deref())
        .bind(request.provider_zone_ref.as_deref())
        .bind(request.provider_account_id.as_deref())
        // The creator is the owner: this row is the caller's own domain from
        // here on, and `created_by` stays the audit half of the same fact.
        .bind(actor_id)
        .bind(actor_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy_dns_zone", error))?;
        sqlx::query(
            "INSERT INTO deploy_domain (
                id, uuid, tenant_id, organization_id, zone_id, hostname_ascii, hostname_type,
                verification_status, status, created_by, updated_by
             ) VALUES ($1, $2, $3, $4, $5, $6, 'EXACT', 'PENDING', 'ACTIVE', $7, $7)",
        )
        .bind(hostname_id)
        .bind(hostname_uuid)
        .bind(tenant_id)
        .bind(organization_id)
        .bind(zone_id)
        .bind(&request.apex_hostname)
        .bind(actor_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert apex deploy_domain", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit create deploy_dns_zone", error))?;
        // Returned as the owner sees it, so a zone the caller cannot reach is
        // never echoed back from the create path either.
        self.retrieve_domain_zone_repo(tenant_id, actor_id, &zone_uuid)
            .await
    }

    pub(super) async fn retrieve_domain_zone_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<DomainZoneResponse> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {ZONE_SELECT} FROM deploy_dns_zone z
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND {} AND z.deleted_at IS NULL",
            zone_owner_gate(3)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_dns_zone", error))?;
        row.as_ref()
            .map(map_zone_row)
            .transpose()
            .map_err(|error| DeployServiceError::Internal(format!("map deploy_dns_zone: {error}")))?
            .ok_or_else(|| DeployServiceError::not_found("domain zone not found"))
    }

    /// `actor_id` is both the audit actor and the owner this write must belong
    /// to: the service resolves it from the caller's own session, so a zone the
    /// caller does not own never matches and answers `not found`.
    pub(super) async fn update_domain_zone_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        request: &UpdateDomainZoneRequest,
    ) -> DeployServiceResult<DomainZoneResponse> {
        // "Leave the pin alone" and "unpin it" are different requests, and
        // `COALESCE` cannot express the second: it treats NULL as "keep". A
        // deliberate empty string therefore arrives as its own flag and writes
        // NULL, while an omitted field keeps whatever is pinned.
        let clear_provider_account = matches!(
            request.provider_account_id.as_deref(),
            Some(value) if value.trim().is_empty()
        );
        let provider_account_id = request
            .provider_account_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let result = sqlx::query(
            "UPDATE deploy_dns_zone SET
                display_name = COALESCE($3, display_name), dns_provider = COALESCE($4, dns_provider),
                provider_zone_ref = COALESCE($5, provider_zone_ref), status = COALESCE($6, status),
                provider_account_id = CASE WHEN $8 THEN NULL
                    ELSE COALESCE($9, provider_account_id) END,
                updated_by = $7, updated_at = CURRENT_TIMESTAMP, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2
               AND (user_id IS NULL OR user_id = $10) AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(zone_id)
        .bind(request.display_name.as_deref())
        .bind(request.dns_provider.as_deref())
        .bind(request.provider_zone_ref.as_deref())
        .bind(request.status.as_deref())
        .bind(actor_id)
        .bind(clear_provider_account)
        .bind(provider_account_id)
        .bind(actor_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("update deploy_dns_zone", error))?;
        if result.rows_affected() != 1 {
            return Err(DeployServiceError::not_found("domain zone not found"));
        }
        self.retrieve_domain_zone_repo(tenant_id, actor_id, zone_id)
            .await
    }

    /// Resolves the zone that owns `hostname` and the account pinned to it.
    ///
    /// The lookup walks the hostname's own `deploy_domain` row up to its zone
    /// instead of matching `apex_hostname` by suffix, so a name that is a suffix of
    /// another zone's apex (`notexample.com` against `example.com`) is not
    /// misattributed, and a claim that was never declared here answers `None`.
    pub(super) async fn retrieve_dns_challenge_zone_repo(
        &self,
        tenant_id: i64,
        hostname: &str,
    ) -> DeployServiceResult<Option<sdkwork_intelligence_deploy_service::DnsChallengeZone>> {
        let normalized = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
        let row = sqlx::query(
            "SELECT z.apex_hostname, z.dns_provider, z.provider_zone_ref, z.provider_account_id
             FROM deploy_domain d
             JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE d.tenant_id = $1 AND d.hostname_ascii = $2
               AND d.deleted_at IS NULL AND z.deleted_at IS NULL
             ORDER BY d.id DESC
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(&normalized)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_dns_zone for hostname", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let zone_apex: String = row.try_get("apex_hostname").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_dns_zone apex: {error}"))
        })?;
        let dns_provider: Option<String> = row.try_get("dns_provider").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_dns_zone provider: {error}"))
        })?;
        let provider_zone_ref: Option<String> =
            row.try_get("provider_zone_ref").map_err(|error| {
                DeployServiceError::Internal(format!("map deploy_dns_zone zone ref: {error}"))
            })?;
        let provider_account_id: Option<String> =
            row.try_get("provider_account_id").map_err(|error| {
                DeployServiceError::Internal(format!("map deploy_dns_zone account: {error}"))
            })?;
        Ok(Some(
            sdkwork_intelligence_deploy_service::DnsChallengeZone {
                zone_apex,
                dns_provider,
                provider_zone_ref,
                provider_account_id,
            },
        ))
    }

    pub(super) async fn delete_domain_zone_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
    ) -> DeployServiceResult<()> {
        // The apex hostname row belongs to the zone itself (created together
        // with the zone), so only user-added hostnames block the deletion.
        let hostname_count: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_domain d JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND {} AND z.deleted_at IS NULL
               AND d.deleted_at IS NULL AND d.hostname_ascii <> z.apex_hostname",
            zone_owner_gate(3)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_dns_zone hostnames", error))?;
        if hostname_count > 0 {
            return Err(DeployServiceError::conflict(
                "domain zone still contains hostnames",
            ));
        }
        // Any hostname (including the apex) referenced by an app binding or a
        // certificate identifier must be released before the zone is removed.
        let reference_count: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_domain d JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND {} AND z.deleted_at IS NULL AND d.deleted_at IS NULL
               AND ((SELECT COUNT(*) FROM deploy_app_binding b
                     WHERE b.domain_id = d.id AND b.deleted_at IS NULL) > 0
                 OR (SELECT COUNT(*) FROM deploy_certificate_identifier ci
                     WHERE ci.domain_id = d.id) > 0)",
            zone_owner_gate(3)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_dns_zone references", error))?;
        if reference_count > 0 {
            return Err(DeployServiceError::conflict(
                "domain zone hostnames are still bound to an application or certificate",
            ));
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin delete deploy_dns_zone", error))?;
        let result = sqlx::query(
            "UPDATE deploy_dns_zone
             SET deleted_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2
               AND (user_id IS NULL OR user_id = $3) AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("delete deploy_dns_zone", error))?;
        if result.rows_affected() != 1 {
            return Err(DeployServiceError::not_found("domain zone not found"));
        }
        // The apex hostname row is removed together with its zone so it cannot
        // linger as a dangling domain asset after the zone is gone.
        sqlx::query(AssertSqlSafe(format!(
            "UPDATE deploy_domain d
             SET deleted_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP, version = d.version + 1
             FROM deploy_dns_zone z
             WHERE d.zone_id = z.id AND z.tenant_id = $1 AND z.uuid = $2 AND {}
               AND d.hostname_ascii = z.apex_hostname AND d.deleted_at IS NULL",
            zone_owner_gate(3)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("delete deploy_dns_zone apex hostname", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit delete deploy_dns_zone", error))?;
        Ok(())
    }
}

impl DeployRepository {
    pub(super) async fn domain_hostname_verification_challenge_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<DomainVerificationChallenge> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT d.id, d.hostname_ascii, d.verification_status
             FROM deploy_domain d JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND d.uuid = $3 AND {}
               AND z.deleted_at IS NULL AND d.deleted_at IS NULL",
            zone_owner_gate(4)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_id)
        .bind(owner_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load deploy_domain verification target", error))?
        .ok_or_else(|| DeployServiceError::not_found("domain hostname not found"))?;
        let domain_id: i64 = row.try_get("id").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_domain id: {error}"))
        })?;
        let hostname: String = row.try_get("hostname_ascii").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_domain hostname: {error}"))
        })?;
        let status: String = row.try_get("verification_status").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_domain verification: {error}"))
        })?;
        if status == "VERIFIED" {
            return Ok(DomainVerificationChallenge {
                verification_id: None,
                hostname,
                record_name: None,
                verified: true,
                proof_sha256: None,
                token: None,
                expires_at: None,
            });
        }

        let now = chrono::Utc::now();

        // 并发防重：同一 domain 的 challenge 创建必须串行化。事务内先对
        // domain 行加 `FOR UPDATE` 锁，再检查/创建 challenge，避免并发
        // 请求同时通过 check-then-insert 产生多个活跃 challenge（对齐
        // Web repository 的 domains.rs 行锁语义）。
        let mut transaction = self.pool.begin().await.map_err(|error| {
            store_error(
                "begin deploy_domain_verification challenge transaction",
                error,
            )
        })?;
        sqlx::query(
            "SELECT id FROM deploy_domain
             WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
        )
        .bind(domain_id)
        .bind(tenant_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_domain for verification", error))?;

        let active = sqlx::query(
            "SELECT uuid, record_name, proof_sha256, expires_at
             FROM deploy_domain_verification
             WHERE tenant_id = $1 AND domain_id = $2 AND status IN ('PENDING', 'CHECKING')
             ORDER BY id DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(domain_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("load active deploy_domain_verification", error))?;
        if let Some(active) = active {
            let expires_at: chrono::DateTime<chrono::Utc> =
                active.try_get("expires_at").map_err(|error| {
                    DeployServiceError::Internal(format!("map verification expiry: {error}"))
                })?;
            if expires_at > now {
                let result = DomainVerificationChallenge {
                    verification_id: Some(active.try_get("uuid").map_err(|error| {
                        DeployServiceError::Internal(format!("map verification uuid: {error}"))
                    })?),
                    hostname,
                    record_name: Some(active.try_get("record_name").map_err(|error| {
                        DeployServiceError::Internal(format!("map verification record: {error}"))
                    })?),
                    verified: false,
                    proof_sha256: Some(active.try_get("proof_sha256").map_err(|error| {
                        DeployServiceError::Internal(format!("map verification proof: {error}"))
                    })?),
                    token: None,
                    expires_at: Some(expires_at.to_rfc3339()),
                };
                transaction.commit().await.map_err(|error| {
                    store_error("commit deploy_domain_verification challenge", error)
                })?;
                return Ok(result);
            }
            sqlx::query(
                "UPDATE deploy_domain_verification
                 SET status = 'EXPIRED', updated_at = $3, version = version + 1
                 WHERE tenant_id = $1 AND domain_id = $2 AND status IN ('PENDING', 'CHECKING')",
            )
            .bind(tenant_id)
            .bind(domain_id)
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("expire deploy_domain_verification", error))?;
        }

        let verification_id = new_uuid();
        let token = format!("sdkwork-domain-verification={}", new_uuid());
        let proof_sha256 = sha256_hash(token.as_bytes());
        let record_name = dns_txt_record_name(&hostname)?;
        let expires_at = now + chrono::Duration::minutes(30);
        sqlx::query(
            "INSERT INTO deploy_domain_verification (
                id, uuid, tenant_id, domain_id, method, record_name, proof_sha256, status,
                attempt_count, next_attempt_at, expires_at, created_at, updated_at, version
             ) VALUES ($1, $2, $3, $4, 'DNS_TXT', $5, $6, 'PENDING', 0, $7, $8, $7, $7, 1)",
        )
        .bind(next_id(self.id_generator())?)
        .bind(&verification_id)
        .bind(tenant_id)
        .bind(domain_id)
        .bind(&record_name)
        .bind(&proof_sha256)
        .bind(now)
        .bind(expires_at)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy_domain_verification", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit deploy_domain_verification challenge", error))?;
        Ok(DomainVerificationChallenge {
            verification_id: Some(verification_id),
            hostname,
            record_name: Some(record_name),
            verified: false,
            proof_sha256: Some(proof_sha256),
            token: Some(token),
            expires_at: Some(expires_at.to_rfc3339()),
        })
    }

    pub(super) async fn confirm_domain_hostname_verification_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
        verification_id: &str,
        observed_sha256: &str,
        verifier_identity: &str,
    ) -> DeployServiceResult<bool> {
        let now = chrono::Utc::now();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin domain hostname verification", error))?;
        let verification = sqlx::query(AssertSqlSafe(format!(
            "UPDATE deploy_domain_verification v
             SET status = 'VERIFIED', observed_sha256 = $5, verifier_identity = $6,
                 checked_at = $7, verified_at = $7, attempt_count = attempt_count + 1,
                 updated_at = $7, version = version + 1
             WHERE v.tenant_id = $1 AND v.uuid = $4 AND v.domain_id = (
                 SELECT d.id FROM deploy_domain d JOIN deploy_dns_zone z ON z.id = d.zone_id
                 WHERE z.tenant_id = $1 AND z.uuid = $2 AND d.uuid = $3 AND {}
                   AND d.verification_status <> 'VERIFIED'
                   AND z.deleted_at IS NULL AND d.deleted_at IS NULL
             ) AND v.proof_sha256 = $5 AND v.status IN ('PENDING', 'CHECKING') AND v.expires_at > $7",
            zone_owner_gate(8)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_id)
        .bind(verification_id)
        .bind(observed_sha256)
        .bind(verifier_identity)
        .bind(now)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("confirm deploy_domain_verification", error))?;
        if verification.rows_affected() != 1 {
            transaction
                .rollback()
                .await
                .map_err(|error| store_error("rollback rejected domain verification", error))?;
            return Ok(false);
        }
        let domain = sqlx::query(AssertSqlSafe(format!(
            "UPDATE deploy_domain d
             SET verification_status = 'VERIFIED', verified_at = $4,
                 updated_at = $4, version = d.version + 1
             FROM deploy_dns_zone z
             WHERE d.zone_id = z.id AND z.tenant_id = $1 AND z.uuid = $2 AND d.uuid = $3 AND {}
               AND d.verification_status <> 'VERIFIED'
               AND z.deleted_at IS NULL AND d.deleted_at IS NULL",
            zone_owner_gate(5)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_id)
        .bind(now)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("activate verified deploy_domain", error))?;
        if domain.rows_affected() != 1 {
            transaction
                .rollback()
                .await
                .map_err(|error| store_error("rollback domain activation", error))?;
            return Ok(false);
        }
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit domain hostname verification", error))?;
        Ok(true)
    }
}

impl DeployRepository {
    pub(super) async fn list_domain_hostnames_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<DomainHostnamePage> {
        let (page, page_size, offset) = pagination(page, page_size);
        // The zone is resolved before the page is built so an unreachable zone
        // answers "not found" rather than an empty page: an empty page would
        // claim the caller owns a zone that simply has no hostnames yet, which
        // is a different fact and the one the console renders as "nothing here".
        let reachable = sqlx::query_scalar::<_, i64>(AssertSqlSafe(format!(
            "SELECT z.id FROM deploy_dns_zone z
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND {} AND z.deleted_at IS NULL",
            zone_owner_gate(3)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_dns_zone for hostnames", error))?;
        if reachable.is_none() {
            return Err(DeployServiceError::not_found("domain zone not found"));
        }
        let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM deploy_domain d JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND {} AND z.deleted_at IS NULL AND d.deleted_at IS NULL",
            zone_owner_gate(3)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_domain by zone", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {HOSTNAME_SELECT} FROM deploy_domain d
             JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND {} AND z.deleted_at IS NULL AND d.deleted_at IS NULL
             ORDER BY CASE WHEN d.hostname_ascii = z.apex_hostname THEN 0 ELSE 1 END,
                      d.hostname_ascii, d.id LIMIT $4 OFFSET $5",
            zone_owner_gate(3)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(owner_user_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_domain by zone", error))?;
        let items = rows
            .iter()
            .map(map_hostname_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DeployServiceError::Internal(format!("map deploy_domain: {error}")))?;
        Ok(DomainHostnamePage {
            items,
            total,
            page,
            page_size,
        })
    }

    /// Declares `relative_name` in the zone, or returns the row that already
    /// holds it.
    ///
    /// [`Self::create_domain_hostname_repo`] is deliberately strict — "this
    /// hostname already exists" is the right answer when an operator adds a name
    /// by hand. A certificate order needs the opposite: the wizard states the
    /// hostnames it is about to cover and must land on the rows it already owns
    /// (the apex row ships with the zone, so it always pre-exists) without
    /// treating that as an error. Only collisions **outside** this zone stay
    /// refusals, because a hostname belongs to exactly one zone.
    pub(super) async fn ensure_domain_hostname_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        relative_name: &str,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        let zone = sqlx::query(
            "SELECT id, organization_id, apex_hostname FROM deploy_dns_zone
             WHERE tenant_id = $1 AND uuid = $2
               AND (user_id IS NULL OR user_id = $3)
               AND status = 'ACTIVE' AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(zone_id)
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_dns_zone", error))?
        .ok_or_else(|| DeployServiceError::not_found("domain zone not found"))?;
        let zone_internal_id: i64 = zone.try_get("id").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_dns_zone id: {error}"))
        })?;
        let organization_id: i64 = zone.try_get("organization_id").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_dns_zone organization: {error}"))
        })?;
        let apex_hostname: String = zone.try_get("apex_hostname").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_dns_zone apex: {error}"))
        })?;
        let hostname = hostname_from_relative_name(relative_name, &apex_hostname)?;

        if let Some(uuid) = self
            .find_domain_hostname_uuid(tenant_id, zone_internal_id, &hostname)
            .await?
        {
            return self
                .retrieve_domain_hostname_repo(tenant_id, actor_id, zone_id, &uuid)
                .await;
        }

        let conflicts: (bool, bool) = sqlx::query_as(
            "SELECT
                EXISTS (SELECT 1 FROM deploy_dns_zone
                        WHERE deleted_at IS NULL AND apex_hostname = $1),
                EXISTS (SELECT 1 FROM deploy_domain
                        WHERE deleted_at IS NULL AND hostname_ascii = $1)",
        )
        .bind(&hostname)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("check deploy_domain hostname conflicts", error))?;
        if conflicts.0 {
            return Err(DeployServiceError::conflict(
                "hostname is already registered as a domain zone apex",
            ));
        }
        if conflicts.1 {
            return Err(DeployServiceError::conflict(
                "hostname already exists in another domain zone",
            ));
        }

        let hostname_type = if hostname.starts_with("*.") {
            "WILDCARD"
        } else {
            "EXACT"
        };
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let insert = sqlx::query(
            "INSERT INTO deploy_domain (
                id, uuid, tenant_id, organization_id, zone_id, hostname_ascii, hostname_type,
                verification_status, status, created_by, updated_by
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'PENDING', 'ACTIVE', $8, $8)",
        )
        .bind(id)
        .bind(&uuid)
        .bind(tenant_id)
        .bind(organization_id)
        .bind(zone_internal_id)
        .bind(&hostname)
        .bind(hostname_type)
        .bind(actor_id)
        .execute(&self.pool)
        .await;
        if let Err(error) = insert {
            // A concurrent claim of the same hostname can slip between the reuse
            // lookup above and this insert; the global unique index turns that
            // race into a duplicate-key error rather than a duplicate row. Since
            // the caller asked for exactly "make sure I own this", reporting the
            // row the other writer just created is the honest answer — and if the
            // failure was anything else, nothing is found here and the original
            // error is returned untouched.
            if let Some(existing) = self
                .find_domain_hostname_uuid(tenant_id, zone_internal_id, &hostname)
                .await?
            {
                return self
                    .retrieve_domain_hostname_repo(tenant_id, actor_id, zone_id, &existing)
                    .await;
            }
            return Err(store_error("insert deploy_domain hostname", error));
        }
        self.retrieve_domain_hostname_repo(tenant_id, actor_id, zone_id, &uuid)
            .await
    }

    async fn find_domain_hostname_uuid(
        &self,
        tenant_id: i64,
        zone_internal_id: i64,
        hostname_ascii: &str,
    ) -> DeployServiceResult<Option<String>> {
        let uuid: Option<String> = sqlx::query_scalar(
            "SELECT uuid FROM deploy_domain
             WHERE tenant_id = $1 AND zone_id = $2 AND hostname_ascii = $3 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(zone_internal_id)
        .bind(hostname_ascii)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("find deploy_domain hostname", error))?;
        Ok(uuid)
    }

    pub(super) async fn create_domain_hostname_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        request: &CreateDomainHostnameRequest,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        let zone = sqlx::query(
            "SELECT id, organization_id, apex_hostname FROM deploy_dns_zone
             WHERE tenant_id = $1 AND uuid = $2
               AND (user_id IS NULL OR user_id = $3)
               AND status = 'ACTIVE' AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(zone_id)
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_dns_zone", error))?
        .ok_or_else(|| DeployServiceError::not_found("domain zone not found"))?;
        let zone_internal_id: i64 = zone.try_get("id").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_dns_zone id: {error}"))
        })?;
        let organization_id: i64 = zone.try_get("organization_id").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_dns_zone organization: {error}"))
        })?;
        let apex_hostname: String = zone.try_get("apex_hostname").map_err(|error| {
            DeployServiceError::Internal(format!("map deploy_dns_zone apex: {error}"))
        })?;
        let hostname = hostname_from_relative_name(&request.relative_name, &apex_hostname)?;
        if request.relative_name == "@" {
            // The apex hostname row is created together with the zone, so a
            // relative name of "@" can never be added a second time.
            return Err(DeployServiceError::conflict(
                "the apex hostname already belongs to this zone",
            ));
        }
        // The full hostname must not collide with an existing zone apex or an
        // existing hostname; the global unique index backs this up for
        // concurrent writers.
        let conflicts: (bool, bool) = sqlx::query_as(
            "SELECT
                EXISTS (SELECT 1 FROM deploy_dns_zone
                        WHERE deleted_at IS NULL AND apex_hostname = $1),
                EXISTS (SELECT 1 FROM deploy_domain
                        WHERE deleted_at IS NULL AND hostname_ascii = $1)",
        )
        .bind(&hostname)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("check deploy_domain hostname conflicts", error))?;
        if conflicts.0 {
            return Err(DeployServiceError::conflict(
                "hostname is already registered as a domain zone apex",
            ));
        }
        if conflicts.1 {
            return Err(DeployServiceError::conflict(
                "hostname already exists in a domain zone",
            ));
        }
        let hostname_type = if hostname.starts_with("*.") {
            "WILDCARD"
        } else {
            "EXACT"
        };
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_domain (
                id, uuid, tenant_id, organization_id, zone_id, hostname_ascii, hostname_type,
                verification_status, status, created_by, updated_by
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'PENDING', 'ACTIVE', $8, $8)",
        )
        .bind(id)
        .bind(&uuid)
        .bind(tenant_id)
        .bind(organization_id)
        .bind(zone_internal_id)
        .bind(hostname)
        .bind(hostname_type)
        .bind(actor_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_domain hostname", error))?;
        self.retrieve_domain_hostname_repo(tenant_id, actor_id, zone_id, &uuid)
            .await
    }

    pub(super) async fn retrieve_domain_hostname_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {HOSTNAME_SELECT} FROM deploy_domain d JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND d.uuid = $3 AND {}
               AND z.deleted_at IS NULL AND d.deleted_at IS NULL",
            zone_owner_gate(4)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_id)
        .bind(owner_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_domain hostname", error))?;
        row.as_ref()
            .map(map_hostname_row)
            .transpose()
            .map_err(|error| DeployServiceError::Internal(format!("map deploy_domain: {error}")))?
            .ok_or_else(|| DeployServiceError::not_found("domain hostname not found"))
    }

    pub(super) async fn delete_domain_hostname_repo(
        &self,
        tenant_id: i64,
        owner_user_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
    ) -> DeployServiceResult<()> {
        let (references, is_apex): (i64, bool) = sqlx::query_as(AssertSqlSafe(format!(
            "SELECT
                (SELECT COUNT(*) FROM deploy_app_binding b WHERE b.domain_id = d.id AND b.deleted_at IS NULL)
                + (SELECT COUNT(*) FROM deploy_certificate_identifier ci WHERE ci.domain_id = d.id),
                d.hostname_ascii = z.apex_hostname
             FROM deploy_domain d JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND d.uuid = $3 AND {}
               AND z.deleted_at IS NULL AND d.deleted_at IS NULL",
            zone_owner_gate(4)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_id)
        .bind(owner_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_domain references", error))?
        .ok_or_else(|| DeployServiceError::not_found("domain hostname not found"))?;
        if is_apex {
            return Err(DeployServiceError::conflict(
                "the apex hostname belongs to the zone and cannot be deleted independently",
            ));
        }
        if references > 0 {
            return Err(DeployServiceError::conflict(
                "domain hostname is still bound to an application or certificate",
            ));
        }
        let result = sqlx::query(AssertSqlSafe(format!(
            "UPDATE deploy_domain d
             SET deleted_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP, version = d.version + 1
             FROM deploy_dns_zone z
             WHERE d.zone_id = z.id AND z.tenant_id = $1 AND z.uuid = $2 AND d.uuid = $3 AND {}
               AND z.deleted_at IS NULL AND d.deleted_at IS NULL",
            zone_owner_gate(4)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_id)
        .bind(owner_user_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("delete deploy_domain hostname", error))?;
        if result.rows_affected() != 1 {
            return Err(DeployServiceError::not_found("domain hostname not found"));
        }
        Ok(())
    }

    pub(super) async fn update_domain_hostname_repo(
        &self,
        tenant_id: i64,
        actor_id: Option<i64>,
        zone_id: &str,
        hostname_id: &str,
        request: &UpdateDomainHostnameRequest,
    ) -> DeployServiceResult<DomainHostnameResponse> {
        // The apex hostname is owned by the zone itself and cannot be
        // renamed; referenced hostnames keep their name so certificate
        // coverage and application bindings stay valid.
        let (apex_hostname, is_apex, references): (String, bool, i64) = sqlx::query_as(AssertSqlSafe(format!(
            "SELECT
                z.apex_hostname,
                d.hostname_ascii = z.apex_hostname,
                (SELECT COUNT(*) FROM deploy_app_binding b WHERE b.domain_id = d.id AND b.deleted_at IS NULL)
                + (SELECT COUNT(*) FROM deploy_certificate_identifier ci WHERE ci.domain_id = d.id)
             FROM deploy_domain d JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE z.tenant_id = $1 AND z.uuid = $2 AND d.uuid = $3 AND {}
               AND z.deleted_at IS NULL AND d.deleted_at IS NULL",
            zone_owner_gate(4)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_id)
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_domain for rename", error))?
        .ok_or_else(|| DeployServiceError::not_found("domain hostname not found"))?;
        if is_apex {
            return Err(DeployServiceError::conflict(
                "the apex hostname belongs to the zone and cannot be renamed",
            ));
        }
        if references > 0 {
            return Err(DeployServiceError::conflict(
                "domain hostname is still bound to an application or certificate",
            ));
        }
        let hostname = hostname_from_relative_name(&request.relative_name, &apex_hostname)?;
        if hostname == apex_hostname {
            return Err(DeployServiceError::conflict(
                "the apex hostname belongs to the zone and cannot be recreated",
            ));
        }
        // The renamed hostname must not collide with another zone apex or an
        // existing hostname; the global unique index backs this up for
        // concurrent writers.
        let conflicts: (bool, bool) = sqlx::query_as(
            "SELECT
                EXISTS (SELECT 1 FROM deploy_dns_zone
                        WHERE deleted_at IS NULL AND apex_hostname = $1),
                EXISTS (SELECT 1 FROM deploy_domain
                        WHERE deleted_at IS NULL AND hostname_ascii = $1)",
        )
        .bind(&hostname)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("check deploy_domain hostname conflicts", error))?;
        if conflicts.0 {
            return Err(DeployServiceError::conflict(
                "hostname is already registered as a domain zone apex",
            ));
        }
        if conflicts.1 {
            return Err(DeployServiceError::conflict(
                "hostname already exists in a domain zone",
            ));
        }
        let hostname_type = if hostname.starts_with("*.") {
            "WILDCARD"
        } else {
            "EXACT"
        };
        // Renaming changes the DNS name: the ownership proof for the old
        // name is invalidated (verification resets to PENDING and any active
        // challenge expires), so the new name must be verified again.
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin rename deploy_domain transaction", error))?;
        let result = sqlx::query(AssertSqlSafe(format!(
            "UPDATE deploy_domain d SET
                hostname_ascii = $4, hostname_type = $5,
                verification_status = 'PENDING', verified_at = NULL,
                updated_by = $6, updated_at = CURRENT_TIMESTAMP, version = d.version + 1
             FROM deploy_dns_zone z
             WHERE d.zone_id = z.id AND z.tenant_id = $1 AND z.uuid = $2 AND d.uuid = $3 AND {}
               AND z.deleted_at IS NULL AND d.deleted_at IS NULL",
            zone_owner_gate(7)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_id)
        .bind(&hostname)
        .bind(hostname_type)
        .bind(actor_id)
        .bind(actor_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("rename deploy_domain hostname", error))?;
        if result.rows_affected() != 1 {
            transaction
                .rollback()
                .await
                .map_err(|error| store_error("rollback rename deploy_domain", error))?;
            return Err(DeployServiceError::not_found("domain hostname not found"));
        }
        sqlx::query(AssertSqlSafe(format!(
            "UPDATE deploy_domain_verification v SET
                status = 'EXPIRED', updated_at = CURRENT_TIMESTAMP, version = v.version + 1
             FROM deploy_domain d JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE v.domain_id = d.id AND v.status IN ('PENDING', 'CHECKING')
               AND z.tenant_id = $1 AND z.uuid = $2 AND d.uuid = $3 AND {}
               AND z.deleted_at IS NULL AND d.deleted_at IS NULL",
            zone_owner_gate(4)
        )))
        .bind(tenant_id)
        .bind(zone_id)
        .bind(hostname_id)
        .bind(actor_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("expire deploy_domain_verification on rename", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit rename deploy_domain", error))?;
        self.retrieve_domain_hostname_repo(tenant_id, actor_id, zone_id, hostname_id)
            .await
    }
}

fn hostname_from_relative_name(
    relative_name: &str,
    apex_hostname: &str,
) -> DeployServiceResult<String> {
    let relative_name = relative_name.trim().to_ascii_lowercase();
    if relative_name == "@" {
        return Ok(apex_hostname.to_owned());
    }
    if relative_name.is_empty()
        || relative_name.ends_with('.')
        || relative_name.contains("..")
        || relative_name.split('.').enumerate().any(|(index, label)| {
            if label == "*" {
                // A wildcard label is permitted only as the leftmost label
                // (e.g. *.a.example.com); it cannot appear deeper in the path.
                return index != 0;
            }
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(DeployServiceError::validation(
            "relative domain name is invalid",
        ));
    }
    let hostname = format!("{relative_name}.{apex_hostname}");
    if hostname.len() > 253 {
        return Err(DeployServiceError::validation("hostname is too long"));
    }
    Ok(hostname)
}

fn map_zone_row(row: &PgRow) -> Result<DomainZoneResponse, sqlx::Error> {
    Ok(DomainZoneResponse {
        id: row.try_get("uuid")?,
        apex_hostname: row.try_get("apex_hostname")?,
        display_name: row.try_get("display_name").ok(),
        dns_provider: row.try_get("dns_provider").ok(),
        provider_account_id: row.try_get("provider_account_id").ok(),
        status: row.try_get("status")?,
        hostname_count: row.try_get("hostname_count")?,
        verified_hostname_count: row.try_get("verified_hostname_count")?,
        certificate_count: row.try_get("certificate_count")?,
        binding_count: row.try_get("binding_count")?,
        updated_at: datetime_from_row(row, "updated_at")?,
        version: row.try_get::<i64, _>("version")?.to_string(),
    })
}

fn map_hostname_row(row: &PgRow) -> Result<DomainHostnameResponse, sqlx::Error> {
    let hostname: String = row.try_get("hostname_ascii")?;
    let apex: String = row.try_get("apex_hostname")?;
    let relative_name = if hostname == apex {
        "@".to_owned()
    } else {
        hostname
            .strip_suffix(&format!(".{apex}"))
            .unwrap_or(&hostname)
            .to_owned()
    };
    Ok(DomainHostnameResponse {
        id: row.try_get("uuid")?,
        zone_id: row.try_get("zone_uuid")?,
        hostname,
        relative_name,
        hostname_type: row.try_get("hostname_type")?,
        verification_status: row.try_get("verification_status")?,
        verified_at: optional_datetime_from_row(row, "verified_at")?,
        status: row.try_get("status")?,
        certificate_count: row.try_get("certificate_count")?,
        binding_count: row.try_get("binding_count")?,
        created_at: datetime_from_row(row, "created_at")?,
        updated_at: datetime_from_row(row, "updated_at")?,
        version: row.try_get::<i64, _>("version")?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::hostname_from_relative_name;

    #[test]
    fn relative_names_are_scoped_to_the_zone() {
        assert_eq!(
            hostname_from_relative_name("@", "example.com").unwrap(),
            "example.com"
        );
        assert_eq!(
            hostname_from_relative_name("docs", "example.com").unwrap(),
            "docs.example.com"
        );
        assert_eq!(
            hostname_from_relative_name("api.eu", "example.com").unwrap(),
            "api.eu.example.com"
        );
        assert_eq!(
            hostname_from_relative_name("*", "example.com").unwrap(),
            "*.example.com"
        );
        assert_eq!(
            hostname_from_relative_name("*.a", "example.com").unwrap(),
            "*.a.example.com"
        );
        for name in ["", "foo..bar", "foo.*", "a.*.b", "*.", "-bad", "bad-", "*x"] {
            assert!(
                hostname_from_relative_name(name, "example.com").is_err(),
                "{name}"
            );
        }
    }
}

impl DeployRepository {
    /// Resolves the WeChat domain-verification file published under `hostname`.
    ///
    /// Walks `deploy_domain` rather than suffix-matching `apex_hostname`: a zone's
    /// apex holds its own row there (which is what `create_domain_zone_repo`'s
    /// overlap check is built on), so this path covers the apex *and* the names
    /// under it while still refusing to attribute `notexample.com` to
    /// `example.com`.
    ///
    /// No `tenant_id` filter: active `hostname_ascii` values are globally unique
    /// (`uk_deploy_domain_active_hostname`), and the file is content the operator
    /// deliberately publishes to the WeChat crawler — it is not anyone's private
    /// data, so there is no owner predicate to apply and none is invented.
    pub async fn wechat_verification_by_hostname_lookup(
        &self,
        hostname: &str,
    ) -> DeployServiceResult<Option<(String, String)>> {
        let normalized = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
        let row = sqlx::query(
            "SELECT v.file_name, v.content
             FROM deploy_domain d
             JOIN deploy_dns_zone z ON z.id = d.zone_id
             JOIN deploy_dns_zone_wechat_verification v ON v.zone_id = z.id
             WHERE d.hostname_ascii = $1
               AND d.deleted_at IS NULL AND z.deleted_at IS NULL
             ORDER BY d.id DESC
             LIMIT 1",
        )
        .bind(&normalized)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_dns_zone_wechat_verification", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let file_name: String = row.try_get("file_name").map_err(|error| {
            DeployServiceError::Internal(format!("map wechat verification file name: {error}"))
        })?;
        let content: String = row.try_get("content").map_err(|error| {
            DeployServiceError::Internal(format!("map wechat verification content: {error}"))
        })?;
        Ok(Some((file_name, content)))
    }
}
