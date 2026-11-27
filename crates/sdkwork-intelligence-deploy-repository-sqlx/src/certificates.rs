use std::collections::{BTreeSet, HashMap};

use sdkwork_deploy_contract::{
    plan_certificate_identifiers, resolve_certificate_identifier_source, resolve_presentation,
    resolve_validation_method, CertificateArchiveStatus, CertificateIdentifierSource,
    CertificatePage, CertificatePlanError, CertificateRenewalInProgress, CertificateResponse,
    CertificateScope, ChallengeAvailability, ChallengePresentation, CreateCertificateRequest,
    DeployServiceError, DeployServiceResult, ValidationMethod, CERTIFICATE_RENEWAL_STATUS_PLANNED,
    CERTIFICATE_SOURCE_MANAGED, CERTIFICATE_STATUS_REVOKED, MAX_CERTIFICATE_IDENTIFIERS,
};
use sqlx::{postgres::PgRow, AssertSqlSafe, Row};

use crate::support::{
    datetime_from_row, json_from_row, new_uuid, next_id, optional_datetime_from_row, pagination,
    sha256_hex, store_error,
};
use crate::DeployRepository;
use chrono::Utc;
use sdkwork_intelligence_deploy_service::certificate_renewal::ValidityWindow;

/// Why a manual renewal request is allowed, refused, or already running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenewalEligibility {
    Allowed,
    NotRenewable,
    InProgress,
}

const CERTIFICATE_SELECT: &str = "c.uuid, c.cert_name, c.certificate_source, c.ca_profile,
     c.certificate_scope, c.validation_method, c.provider_account_id,
     c.preferred_key_algorithm, c.presentation_mode, c.auto_renew, c.renewal_status, c.status,
     c.renew_before_days, c.renewal_failure_count, c.last_renewal_at,
     c.created_at, c.updated_at, c.version,
     v.uuid AS current_version_uuid, v.issuer, v.not_before, v.not_after,
     COALESCE((
         SELECT jsonb_agg(ci.hostname_ascii ORDER BY ci.position)
         FROM deploy_certificate_identifier ci
         WHERE ci.certificate_id = c.id
     ), '[]'::jsonb) AS identifiers,
     -- The running renewal attempt, if any. Kept as a correlated subquery rather
     -- than a join so the list query cannot multiply rows when a certificate
     -- accumulated several attempts.
     (SELECT r.uuid FROM deploy_certificate_renewal r
       WHERE r.certificate_id = c.id
         AND r.status IN ('PLANNED', 'PROCESSING', 'ORDERED')
       ORDER BY r.id DESC LIMIT 1) AS active_renewal_uuid,
     (SELECT r.status FROM deploy_certificate_renewal r
       WHERE r.certificate_id = c.id
         AND r.status IN ('PLANNED', 'PROCESSING', 'ORDERED')
       ORDER BY r.id DESC LIMIT 1) AS active_renewal_status,
     -- Where the served version's material lives in Drive, and in what state.
     --
     -- Scoped to the current version (v.id) rather than to the certificate: the
     -- question the console asks is whether the certificate that is being served
     -- can be downloaded right now, and a previous version's ARCHIVED row would
     -- answer yes while pointing at different bytes. The bundle kind is filtered
     -- to PUBLIC explicitly because the unique key is (version, kind) and a
     -- future FULL row for the same version must not decide what the public read
     -- model reports.
     --
     -- A certificate with no current version (never issued, or issued before the
     -- archive feature existed) joins to nothing and reads as no archive, which
     -- is the honest answer: there is nothing to have archived.
     a.status AS archive_status, a.drive_space_id AS archive_space_id,
     a.drive_folder_node_id AS archive_folder_node_id";

impl DeployRepository {
    pub(super) async fn list_certificates_repo(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificatePage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deploy_certificate
             WHERE tenant_id = $1 AND status <> $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(CERTIFICATE_STATUS_REVOKED)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_certificate", error))?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {CERTIFICATE_SELECT}
             FROM deploy_certificate c
             LEFT JOIN deploy_certificate_version v ON v.id = c.current_version_id
             LEFT JOIN deploy_certificate_archive a
                    ON a.certificate_version_id = v.id AND a.bundle_kind = 'PUBLIC'
             WHERE c.tenant_id = $1 AND c.status <> $2 AND c.deleted_at IS NULL
             ORDER BY c.updated_at DESC, c.id DESC LIMIT $3 OFFSET $4"
        )))
        .bind(tenant_id)
        .bind(CERTIFICATE_STATUS_REVOKED)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_certificate", error))?;
        let items = rows
            .iter()
            .map(map_certificate_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                DeployServiceError::Internal(format!("map deploy_certificate row: {error}"))
            })?;
        Ok(CertificatePage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_certificate_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateResponse> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {CERTIFICATE_SELECT}
             FROM deploy_certificate c
             LEFT JOIN deploy_certificate_version v ON v.id = c.current_version_id
             LEFT JOIN deploy_certificate_archive a
                    ON a.certificate_version_id = v.id AND a.bundle_kind = 'PUBLIC'
             WHERE c.tenant_id = $1 AND c.uuid = $2
               AND c.status <> $3 AND c.deleted_at IS NULL"
        )))
        .bind(tenant_id)
        .bind(certificate_id)
        .bind(CERTIFICATE_STATUS_REVOKED)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_certificate", error))?;
        row.as_ref()
            .map(map_certificate_row)
            .transpose()
            .map_err(|error| {
                DeployServiceError::Internal(format!("map deploy_certificate row: {error}"))
            })?
            .ok_or_else(|| DeployServiceError::not_found("certificate not found"))
    }

    pub(super) async fn create_certificate_repo(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        idempotency_key: &str,
        request: &CreateCertificateRequest,
    ) -> DeployServiceResult<CertificateResponse> {
        validate_create_certificate(request, idempotency_key)?;
        let request_json = serde_json::to_string(request)
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        let request_sha256 = sha256_hex(&request_json);
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin create deploy_certificate", error))?;
        if let Some(existing) = sqlx::query(
            "SELECT uuid, request_sha256 FROM deploy_certificate
             WHERE tenant_id = $1 AND idempotency_key = $2",
        )
        .bind(tenant_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("load idempotent deploy_certificate", error))?
        {
            let stored_hash: String = existing.try_get("request_sha256").map_err(|error| {
                DeployServiceError::Internal(format!("map certificate request hash: {error}"))
            })?;
            if stored_hash != request_sha256 {
                return Err(DeployServiceError::conflict(
                    "Idempotency-Key was already used with another certificate request",
                ));
            }
            let certificate_id: String = existing.try_get("uuid").map_err(|error| {
                DeployServiceError::Internal(format!("map certificate UUID: {error}"))
            })?;
            transaction
                .commit()
                .await
                .map_err(|error| store_error("commit idempotent deploy_certificate", error))?;
            return self
                .retrieve_certificate_repo(tenant_id, &certificate_id)
                .await;
        }

        let validation_method = resolve_creation_validation_method(request)?;
        let allow_unverified_manual_claims = validation_method == ValidationMethod::Dns01
            && request.challenge_presentation == ChallengePresentation::Manual;

        // `domainIds` keeps the established ACTIVE + VERIFIED + zone-owner gate.
        // Free-form `requestedIdentifiers` may use the same gate, except for a
        // MANUAL DNS-01 order: there the ACME TXT proof itself establishes
        // control, so requiring a separate pre-verification would make the manual
        // flow impossible for a domain that is not in the inventory yet.
        let (identifier_source, requested_identifiers) = resolve_certificate_identifier_source(
            &request.domain_ids,
            &request.requested_identifiers,
        )
        .map_err(map_certificate_plan_error)?;
        let requested_hostnames = match identifier_source {
            CertificateIdentifierSource::DomainIds => {
                let domain_ids = request
                    .domain_ids
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                let rows = sqlx::query(
                    "SELECT d.id, d.hostname_ascii
                     FROM deploy_domain d
                     JOIN deploy_dns_zone z ON z.id = d.zone_id
                     WHERE d.tenant_id = $1 AND d.uuid = ANY($2)
                       AND d.verification_status = 'VERIFIED' AND d.status = 'ACTIVE'
                       AND d.deleted_at IS NULL
                       AND ($3::BIGINT IS NULL OR z.user_id IS NULL OR z.user_id = $3)
                     FOR SHARE OF d",
                )
                .bind(tenant_id)
                .bind(&domain_ids)
                .bind(actor_id)
                .fetch_all(&mut *transaction)
                .await
                .map_err(|error| store_error("resolve certificate hostname identifiers", error))?;
                if rows.len() != request.domain_ids.len() {
                    return Err(DeployServiceError::validation(
                        "every domainId must reference an active verified hostname in the \
                         current tenant",
                    ));
                }
                rows.iter()
                    .map(|row| row.try_get::<String, _>("hostname_ascii"))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| {
                        DeployServiceError::Internal(format!("map certificate hostname: {error}"))
                    })?
            }
            CertificateIdentifierSource::RequestedIdentifiers => {
                let requested_hostnames = requested_identifiers.unwrap_or_default();
                if allow_unverified_manual_claims {
                    requested_hostnames
                } else {
                    let hostnames = requested_hostnames
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>();
                    let rows = sqlx::query(
                        "SELECT d.id, d.hostname_ascii
                         FROM deploy_domain d
                         JOIN deploy_dns_zone z ON z.id = d.zone_id
                         WHERE d.tenant_id = $1 AND d.hostname_ascii = ANY($2)
                           AND d.verification_status = 'VERIFIED' AND d.status = 'ACTIVE'
                           AND d.deleted_at IS NULL
                           AND ($3::BIGINT IS NULL OR z.user_id IS NULL OR z.user_id = $3)
                         FOR SHARE OF d",
                    )
                    .bind(tenant_id)
                    .bind(&hostnames)
                    .bind(actor_id)
                    .fetch_all(&mut *transaction)
                    .await
                    .map_err(|error| {
                        store_error("resolve requested certificate hostname identifiers", error)
                    })?;
                    let resolved = rows
                        .iter()
                        .filter_map(|row| {
                            row.try_get::<String, _>("hostname_ascii")
                                .ok()
                                .map(|hostname| (hostname, row))
                        })
                        .collect::<HashMap<String, &PgRow>>();
                    if let Some(missing) = requested_hostnames
                        .iter()
                        .find(|hostname| !resolved.contains_key(*hostname))
                    {
                        return Err(DeployServiceError::validation(format!(
                            "requestedIdentifiers contains a hostname that is not an active verified \
                             hostname of this tenant: {missing}"
                        )));
                    }
                    requested_hostnames
                }
            }
        };

        let planned_identifiers =
            plan_certificate_identifiers(request.certificate_scope, &requested_hostnames)
                .map_err(map_certificate_plan_error)?;
        let planned_hostnames = planned_identifiers
            .iter()
            .map(|identifier| identifier.hostname.clone())
            .collect::<Vec<_>>();
        let claim_by_hostname = if allow_unverified_manual_claims
            && identifier_source == CertificateIdentifierSource::RequestedIdentifiers
        {
            self.resolve_or_provision_manual_certificate_claims(
                &mut *transaction,
                tenant_id,
                organization_id,
                actor_id,
                &planned_hostnames,
            )
            .await?
        } else {
            let planned_claims = sqlx::query(
                "SELECT d.id, d.hostname_ascii
                 FROM deploy_domain d
                 JOIN deploy_dns_zone z ON z.id = d.zone_id
                 WHERE d.tenant_id = $1 AND d.hostname_ascii = ANY($2)
                   AND d.verification_status = 'VERIFIED' AND d.status = 'ACTIVE'
                   AND d.deleted_at IS NULL
                   AND ($3::BIGINT IS NULL OR z.user_id IS NULL OR z.user_id = $3)
                 FOR SHARE OF d",
            )
            .bind(tenant_id)
            .bind(&planned_hostnames)
            .bind(actor_id)
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| store_error("resolve planned certificate identifiers", error))?;
            let mut claim_by_hostname = HashMap::with_capacity(planned_claims.len());
            for claim in &planned_claims {
                let hostname: String = claim.try_get("hostname_ascii").map_err(|error| {
                    DeployServiceError::Internal(format!(
                        "map planned certificate hostname: {error}"
                    ))
                })?;
                let domain_id: i64 = claim.try_get("id").map_err(|error| {
                    DeployServiceError::Internal(format!(
                        "map planned certificate domain id: {error}"
                    ))
                })?;
                claim_by_hostname.insert(hostname, domain_id);
            }
            for identifier in &planned_identifiers {
                if !claim_by_hostname.contains_key(&identifier.hostname) {
                    return Err(map_certificate_plan_error(
                        CertificatePlanError::WildcardApexMissing,
                    ));
                }
            }
            claim_by_hostname
        };

        let certificate_id = next_id(self.id_generator())?;
        let certificate_uuid = new_uuid();
        // The presentation the operator chose, resolved through the same planner
        // rules the orders follow. It is stored on the certificate rather than left
        // to the order, because the first order is opened right after this insert
        // and the claim the worker reads ("is this a manual order?") has to be
        // answerable from the certificate itself. `MANAGED` + HTTP-01 is the
        // platform-served default and needs no provider probe; a manual request is
        // explicitly chosen by an operator who is going to publish the records.
        sqlx::query(
            "INSERT INTO deploy_certificate (
                id, uuid, tenant_id, organization_id, cert_name, certificate_source,
                ca_profile, certificate_scope, validation_method, preferred_key_algorithm,
                presentation_mode, auto_renew, renew_before_days, renewal_status, status,
                provider_account_id, idempotency_key, request_sha256, created_by, updated_by
             ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,'NONE','PENDING',$14,$15,$16,$17,$17
             )",
        )
        .bind(certificate_id)
        .bind(&certificate_uuid)
        .bind(tenant_id)
        .bind(organization_id.unwrap_or(0))
        .bind(request.cert_name.trim())
        .bind(CERTIFICATE_SOURCE_MANAGED)
        .bind(&request.ca_profile)
        .bind(request.certificate_scope.as_str())
        .bind(validation_method.as_str())
        .bind(&request.preferred_key_algorithm)
        .bind(request.challenge_presentation.as_str())
        .bind(request.auto_renew)
        .bind(request.renew_before_days)
        .bind(request.provider_account_id.as_deref())
        .bind(idempotency_key)
        .bind(&request_sha256)
        .bind(actor_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy_certificate", error))?;

        for (position, identifier) in planned_identifiers.iter().enumerate() {
            let domain_id = claim_by_hostname
                .get(&identifier.hostname)
                .copied()
                .ok_or_else(|| {
                    map_certificate_plan_error(CertificatePlanError::WildcardApexMissing)
                })?;
            sqlx::query(
                "INSERT INTO deploy_certificate_identifier (
                    id, uuid, tenant_id, certificate_id, domain_id, identifier_type,
                    hostname_ascii, position
                 ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(next_id(self.id_generator())?)
            .bind(new_uuid())
            .bind(tenant_id)
            .bind(certificate_id)
            .bind(domain_id)
            .bind(identifier.identifier_type.as_str())
            .bind(&identifier.hostname)
            .bind(position as i32)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert deploy_certificate_identifier", error))?;
        }
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit deploy_certificate", error))?;
        self.retrieve_certificate_repo(tenant_id, &certificate_uuid)
            .await
    }

    async fn resolve_or_provision_manual_certificate_claims(
        &self,
        connection: &mut sqlx::PgConnection,
        tenant_id: i64,
        organization_id: Option<i64>,
        actor_id: Option<i64>,
        hostnames: &[String],
    ) -> DeployServiceResult<HashMap<String, i64>> {
        let existing = sqlx::query(
            "SELECT d.id, d.hostname_ascii, d.tenant_id, z.user_id
             FROM deploy_domain d
             JOIN deploy_dns_zone z ON z.id = d.zone_id
             WHERE d.hostname_ascii = ANY($1) AND d.deleted_at IS NULL
             FOR UPDATE OF d",
        )
        .bind(hostnames)
        .fetch_all(&mut *connection)
        .await
        .map_err(|error| store_error("resolve manual certificate hostname claims", error))?;
        let mut claims = HashMap::with_capacity(existing.len());
        for row in existing {
            let hostname: String = row.try_get("hostname_ascii").map_err(|error| {
                DeployServiceError::Internal(format!("map manual certificate hostname: {error}"))
            })?;
            let owner_tenant_id: i64 = row.try_get("tenant_id").map_err(|error| {
                DeployServiceError::Internal(format!("map manual certificate tenant: {error}"))
            })?;
            let owner_user_id: Option<i64> = row.try_get("user_id").map_err(|error| {
                DeployServiceError::Internal(format!("map manual certificate zone owner: {error}"))
            })?;
            if owner_tenant_id != tenant_id
                || !(actor_id.is_none() || owner_user_id.is_none() || owner_user_id == actor_id)
            {
                return Err(DeployServiceError::validation(format!(
                    "requestedIdentifiers contains a hostname that is not available for manual \
                     DNS-01: {hostname}"
                )));
            }
            let domain_id: i64 = row.try_get("id").map_err(|error| {
                DeployServiceError::Internal(format!("map manual certificate domain id: {error}"))
            })?;
            claims.insert(hostname, domain_id);
        }

        for hostname in hostnames {
            if claims.contains_key(hostname) {
                continue;
            }
            let lookup_name = hostname.strip_prefix("*.").unwrap_or(hostname);
            let zone = sqlx::query(
                "SELECT id, tenant_id, user_id
                 FROM deploy_dns_zone
                 WHERE deleted_at IS NULL
                   AND ($1 = apex_hostname OR $1 LIKE '%.' || apex_hostname)
                 ORDER BY LENGTH(apex_hostname) DESC
                 LIMIT 1
                 FOR UPDATE",
            )
            .bind(lookup_name)
            .fetch_optional(&mut *connection)
            .await
            .map_err(|error| store_error("resolve manual certificate DNS zone", error))?;
            let zone_id: i64 = if let Some(zone) = zone {
                let owner_tenant_id: i64 = zone.try_get("tenant_id").map_err(|error| {
                    DeployServiceError::Internal(format!("map manual DNS zone tenant: {error}"))
                })?;
                let owner_user_id: Option<i64> = zone.try_get("user_id").map_err(|error| {
                    DeployServiceError::Internal(format!("map manual DNS zone owner: {error}"))
                })?;
                if owner_tenant_id != tenant_id
                    || !(actor_id.is_none() || owner_user_id.is_none() || owner_user_id == actor_id)
                {
                    return Err(DeployServiceError::validation(format!(
                        "requestedIdentifiers contains a hostname in a DNS zone that is not \
                         available to this caller: {hostname}"
                    )));
                }
                zone.try_get("id").map_err(|error| {
                    DeployServiceError::Internal(format!("map manual DNS zone id: {error}"))
                })?
            } else {
                let apex = certificate_zone_apex(lookup_name);
                let apex_taken = sqlx::query(
                    "SELECT EXISTS (
                        SELECT 1 FROM deploy_domain
                        WHERE hostname_ascii = $1 AND deleted_at IS NULL
                     )",
                )
                .bind(&apex)
                .fetch_one(&mut *connection)
                .await
                .map_err(|error| store_error("check manual certificate zone apex", error))?
                .try_get::<bool, _>(0)
                .map_err(|error| {
                    DeployServiceError::Internal(format!(
                        "map manual certificate zone apex: {error}"
                    ))
                })?;
                if apex_taken {
                    return Err(DeployServiceError::conflict(format!(
                        "domain zone apex {apex} is already registered"
                    )));
                }
                let zone_id = next_id(self.id_generator())?;
                let organization_id = organization_id.unwrap_or_default();
                sqlx::query(
                    "INSERT INTO deploy_dns_zone (
                        id, uuid, tenant_id, organization_id, apex_hostname, display_name,
                        status, user_id, created_by, updated_by
                     ) VALUES ($1, $2, $3, $4, $5, $5, 'ACTIVE', $6, $6, $6)",
                )
                .bind(zone_id)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id)
                .bind(&apex)
                .bind(actor_id)
                .execute(&mut *connection)
                .await
                .map_err(|error| store_error("insert manual certificate DNS zone", error))?;
                let apex_domain_id = next_id(self.id_generator())?;
                sqlx::query(
                    "INSERT INTO deploy_domain (
                        id, uuid, tenant_id, organization_id, zone_id, hostname_ascii,
                        hostname_type, verification_status, status, created_by, updated_by
                     ) VALUES ($1, $2, $3, $4, $5, $6, 'EXACT', 'PENDING', 'ACTIVE', $7, $7)",
                )
                .bind(apex_domain_id)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id)
                .bind(zone_id)
                .bind(&apex)
                .bind(actor_id)
                .execute(&mut *connection)
                .await
                .map_err(|error| store_error("insert manual certificate zone apex", error))?;
                claims.insert(apex, apex_domain_id);
                zone_id
            };

            if !claims.contains_key(hostname) {
                let domain_id = next_id(self.id_generator())?;
                let hostname_type = if hostname.starts_with("*.") {
                    "WILDCARD"
                } else {
                    "EXACT"
                };
                sqlx::query(
                    "INSERT INTO deploy_domain (
                        id, uuid, tenant_id, organization_id, zone_id, hostname_ascii,
                        hostname_type, verification_status, status, created_by, updated_by
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'PENDING', 'ACTIVE', $8, $8)",
                )
                .bind(domain_id)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(organization_id.unwrap_or_default())
                .bind(zone_id)
                .bind(hostname)
                .bind(hostname_type)
                .bind(actor_id)
                .execute(&mut *connection)
                .await
                .map_err(|error| store_error("insert manual certificate hostname", error))?;
                claims.insert(hostname.clone(), domain_id);
            }
        }
        Ok(claims)
    }

    pub(super) async fn delete_certificate_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<()> {
        let result = sqlx::query(
            "UPDATE deploy_certificate
             SET status = $3, auto_renew = FALSE, renewal_status = 'NONE',
                 updated_at = CURRENT_TIMESTAMP, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND status <> $3 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(certificate_id)
        .bind(CERTIFICATE_STATUS_REVOKED)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("revoke deploy_certificate", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found("certificate not found"));
        }
        Ok(())
    }

    /// Why a manual renewal request may be refused before any row is touched.
    ///
    /// Kept as a pure decision so the rule is testable without a database, and so
    /// the reason reaches the caller as a stable code instead of "0 rows
    /// affected".
    fn renewal_eligibility(
        certificate_source: &str,
        status: &str,
        renewal_status: &str,
    ) -> RenewalEligibility {
        // `auto_renew` is deliberately absent: it governs the scheduler, not the
        // operator. A certificate excluded from automatic renewal is the one an
        // operator is most likely to refresh by hand.
        if certificate_source != CERTIFICATE_SOURCE_MANAGED
            || !matches!(status, "ACTIVE" | "FAILED" | "EXPIRED")
        {
            return RenewalEligibility::NotRenewable;
        }
        if matches!(renewal_status, "PLANNED" | "PROCESSING") {
            return RenewalEligibility::InProgress;
        }
        RenewalEligibility::Allowed
    }

    /// Records the CA's suggested renewal window (RFC 9773 ARI) for a certificate.
    ///
    /// `version` is deliberately not bumped: the suggestion is CA metadata, and
    /// bumping the row version would invalidate optimistic updates the renewal
    /// scheduler already holds for a certificate whose content did not change.
    pub(super) async fn record_certificate_ari_window_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        window_start: &str,
        window_end: &str,
        checked_at: &str,
    ) -> DeployServiceResult<()> {
        let result = sqlx::query(
            "UPDATE deploy_certificate
                SET ari_window_start = CAST($3 AS TIMESTAMPTZ),
                    ari_window_end = CAST($4 AS TIMESTAMPTZ),
                    ari_checked_at = CAST($5 AS TIMESTAMPTZ)
              WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(certificate_id)
        .bind(window_start)
        .bind(window_end)
        .bind(checked_at)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("record deploy_certificate ARI window", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found("certificate not found"));
        }
        Ok(())
    }

    pub(super) async fn renew_certificate_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateResponse> {
        let current = self
            .retrieve_certificate_repo(tenant_id, certificate_id)
            .await?;
        match Self::renewal_eligibility(
            &current.certificate_source,
            &current.status,
            &current.renewal_status,
        ) {
            RenewalEligibility::Allowed => {}
            RenewalEligibility::InProgress => {
                return Err(DeployServiceError::conflict(format!(
                    "CERTIFICATE_RENEWAL_IN_PROGRESS: renewal for {} is already {}",
                    current.cert_name, current.renewal_status
                )));
            }
            RenewalEligibility::NotRenewable => {
                return Err(DeployServiceError::validation(format!(
                    "CERTIFICATE_NOT_RENEWABLE: only managed certificates in ACTIVE, FAILED or \
                     EXPIRED can be renewed (source={}, status={})",
                    current.certificate_source, current.status
                )));
            }
        }
        let result = sqlx::query(
            "UPDATE deploy_certificate
             SET renewal_status = $3, updated_at = CURRENT_TIMESTAMP, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND certificate_source = $4
               AND status IN ('ACTIVE', 'FAILED', 'EXPIRED')
               AND renewal_status NOT IN ('PLANNED', 'PROCESSING')
               AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(certificate_id)
        .bind(CERTIFICATE_RENEWAL_STATUS_PLANNED)
        .bind(CERTIFICATE_SOURCE_MANAGED)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("schedule deploy_certificate renewal", error))?;
        if result.rows_affected() == 0 {
            // The row changed between the decision and the update: another caller
            // has just scheduled or started a renewal, which is the in-progress
            // case rather than a validation failure.
            return Err(DeployServiceError::conflict(
                "CERTIFICATE_RENEWAL_IN_PROGRESS: a renewal was scheduled concurrently",
            ));
        }
        self.retrieve_certificate_repo(tenant_id, certificate_id)
            .await
    }
}

fn validate_create_certificate(
    request: &CreateCertificateRequest,
    idempotency_key: &str,
) -> DeployServiceResult<()> {
    if request.cert_name.trim().is_empty() || request.cert_name.trim().len() > 200 {
        return Err(DeployServiceError::validation(
            "certName must contain between 1 and 200 characters",
        ));
    }
    if idempotency_key.trim().is_empty() || idempotency_key.len() > 128 {
        return Err(DeployServiceError::validation(
            "Idempotency-Key must contain between 1 and 128 characters",
        ));
    }
    // Which channel names the identifiers, and whether exactly one of them does.
    // Resolved through the contract so the rule is the same one the service and
    // the console describe, instead of a second copy that can drift.
    resolve_certificate_identifier_source(&request.domain_ids, &request.requested_identifiers)
        .map_err(map_certificate_plan_error)?;
    if !request.domain_ids.is_empty() {
        if request.domain_ids.len() > MAX_CERTIFICATE_IDENTIFIERS {
            return Err(DeployServiceError::validation(format!(
                "domainIds must contain between 1 and {MAX_CERTIFICATE_IDENTIFIERS} hostnames"
            )));
        }
        let unique_domain_ids = request.domain_ids.iter().collect::<BTreeSet<_>>();
        if unique_domain_ids.len() != request.domain_ids.len()
            || request.domain_ids.iter().any(|id| id.trim().is_empty())
        {
            return Err(DeployServiceError::validation(
                "domainIds must contain unique non-empty hostname identifiers",
            ));
        }
    }
    if !matches!(
        request.ca_profile.as_str(),
        "LETS_ENCRYPT_STAGING" | "LETS_ENCRYPT_PRODUCTION"
    ) {
        return Err(DeployServiceError::validation(
            "caProfile must be LETS_ENCRYPT_STAGING or LETS_ENCRYPT_PRODUCTION",
        ));
    }
    if !matches!(request.preferred_key_algorithm.as_str(), "RSA" | "ECDSA") {
        return Err(DeployServiceError::validation(
            "preferredKeyAlgorithm must be RSA or ECDSA",
        ));
    }
    Ok(())
}

/// Maps a pure planning rejection onto the API problem surface.
///
/// The planner reports a stable machine code plus a message that never echoes
/// the offending hostname; only the message is surfaced so a rejection cannot
/// be used to probe another tenant's hostname inventory.
fn map_certificate_plan_error(error: CertificatePlanError) -> DeployServiceError {
    DeployServiceError::validation(format!("{}: {}", error.code(), error.message()))
}

fn certificate_zone_apex(hostname: &str) -> String {
    let mut labels = hostname.split('.');
    let _first = labels.next();
    let remainder = labels.collect::<Vec<_>>();
    if remainder.len() >= 2 {
        remainder.join(".")
    } else {
        hostname.to_owned()
    }
}

fn resolve_creation_validation_method(
    request: &CreateCertificateRequest,
) -> DeployServiceResult<ValidationMethod> {
    if request.challenge_presentation == ChallengePresentation::Manual {
        resolve_presentation(
            request.certificate_scope,
            request.validation_method,
            request.challenge_presentation,
            ChallengeAvailability::default(),
        )
        .map(|plan| plan.method)
        .map_err(map_certificate_plan_error)
    } else {
        resolve_validation_method(request.certificate_scope, request.validation_method)
            .map_err(map_certificate_plan_error)
    }
}

/// Decodes a certificate scope column that the baseline constrains to
/// `SINGLE_DOMAIN` / `WILDCARD`.
fn decode_certificate_scope(raw: &str) -> Result<CertificateScope, sqlx::Error> {
    CertificateScope::parse(raw)
        .ok_or_else(|| sqlx::Error::Decode(format!("unknown certificate_scope: {raw}").into()))
}

/// Decodes a validation method column that the baseline constrains to
/// `AUTO` / `HTTP_01` / `DNS_01`.
fn decode_validation_method(raw: &str) -> Result<ValidationMethod, sqlx::Error> {
    ValidationMethod::parse(raw)
        .ok_or_else(|| sqlx::Error::Decode(format!("unknown validation_method: {raw}").into()))
}

/// Decodes a presentation mode column that the baseline constrains to
/// `MANAGED` / `MANUAL`.
///
/// An unreadable value reads as `MANAGED` rather than failing the row: `MANAGED`
/// is what every issuance before the switch was, so a row written by an older
/// build (or carrying debris) is described as what it actually does instead of
/// disappearing from the console.
fn decode_presentation_mode(raw: Option<&str>) -> ChallengePresentation {
    raw.and_then(ChallengePresentation::parse)
        .unwrap_or(ChallengePresentation::Managed)
}

/// Decodes an archive status column that the baseline constrains to the six
/// `deploy_certificate_archive.status` values.
///
/// An absent row (no archive intent for the served version) reads as `None`, and
/// so does an unrecognised token: `None` is the statement "this control plane has
/// nothing to report about archiving here", which is true for a value written by a
/// newer build this one does not understand. Mapping it to `Failed` would send an
/// operator to debug a Drive write that may have succeeded.
fn decode_archive_status(raw: Option<&str>) -> CertificateArchiveStatus {
    raw.and_then(CertificateArchiveStatus::parse)
        .unwrap_or(CertificateArchiveStatus::None)
}

/// Reads an optional text column, treating a NULL and a decode failure alike.
///
/// Used for the Drive ids, which are nullable in both the table and the join. A
/// value this driver cannot decode is reported as absent rather than failing the
/// whole page: one unreadable Drive id must not hide every certificate a tenant
/// owns.
fn optional_text_column(row: &PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column)
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty())
}

fn map_certificate_row(row: &PgRow) -> Result<CertificateResponse, sqlx::Error> {
    let identifiers =
        json_from_row(row, "identifiers")?.unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let identifiers = serde_json::from_value::<Vec<String>>(identifiers)
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    let certificate_scope =
        decode_certificate_scope(&row.try_get::<String, _>("certificate_scope")?)?;
    let validation_method =
        decode_validation_method(&row.try_get::<String, _>("validation_method")?)?;
    let not_before = optional_datetime_from_row(row, "not_before")?;
    let not_after = optional_datetime_from_row(row, "not_after")?;
    let renew_before_days: i32 = row.try_get("renew_before_days")?;

    // The phase, the due instant and the remaining days are derived from the
    // served version's own X.509 window rather than from `status`, so a row whose
    // status lags the clock still reports the truth. They are computed through
    // the same functions the renewal sweep uses, which is what stops the console
    // and the scheduler from ever disagreeing about when a certificate is due.
    //
    // A certificate with no active version has no window and therefore no phase:
    // reporting `VALID` for a `PENDING` certificate would be a lie an operator
    // would act on.
    let (renewal_due_at, days_until_expiry, validity_phase) =
        match (not_before.as_deref(), not_after.as_deref()) {
            (Some(not_before), Some(not_after)) => {
                match ValidityWindow::from_rfc3339(not_before, not_after) {
                    Ok(window) => {
                        let now = Utc::now();
                        (
                            Some(window.renewal_due_at(renew_before_days).to_rfc3339()),
                            Some(window.whole_days_remaining(now)),
                            Some(window.classify(renew_before_days, now).as_str().to_owned()),
                        )
                    }
                    // A stored window the rule cannot reason about is surfaced as an
                    // absent phase rather than as a 500: the certificate is still
                    // listable, and the broken row is visible instead of swallowing
                    // the whole page.
                    Err(_) => (None, None, None),
                }
            }
            _ => (None, None, None),
        };

    let renewal_failure_count: i32 = row.try_get("renewal_failure_count").unwrap_or(0);
    Ok(CertificateResponse {
        id: row.try_get("uuid")?,
        cert_name: row.try_get("cert_name")?,
        certificate_source: row.try_get("certificate_source")?,
        ca_profile: row.try_get("ca_profile")?,
        certificate_scope,
        validation_method,
        // Rows written before migration 0018 carry the column default; reading it
        // back through `decode_presentation_mode` keeps a legacy `NULL` (or an
        // unexpected token) described as `MANAGED` instead of failing the page.
        presentation_mode: decode_presentation_mode(
            row.try_get::<Option<String>, _>("presentation_mode")
                .ok()
                .flatten()
                .as_deref(),
        ),
        provider_account_id: row.try_get("provider_account_id")?,
        preferred_key_algorithm: row.try_get("preferred_key_algorithm")?,
        identifiers,
        current_version_id: row.try_get("current_version_uuid")?,
        issuer: row.try_get("issuer")?,
        not_before,
        not_after,
        auto_renew: row.try_get("auto_renew")?,
        renewal_status: row.try_get("renewal_status")?,
        renew_before_days,
        renewal_due_at,
        days_until_expiry,
        validity_phase,
        last_renewal_at: optional_datetime_from_row(row, "last_renewal_at")?,
        renewal_failure_count,
        renewal_in_progress: {
            let renewal_id: Option<String> = row.try_get("active_renewal_uuid").unwrap_or(None);
            let renewal_status: Option<String> =
                row.try_get("active_renewal_status").unwrap_or(None);
            match (renewal_id, renewal_status) {
                (Some(renewal_id), Some(status)) if !renewal_id.is_empty() => {
                    Some(CertificateRenewalInProgress { renewal_id, status })
                }
                _ => None,
            }
        },
        alert_phase: days_until_expiry.map(|days| {
            sdkwork_intelligence_deploy_service::certificate_renewal::expiry_alert_phase(
                days,
                renewal_failure_count,
            )
            .as_str()
            .to_owned()
        }),
        // The archive columns come from a LEFT JOIN on the current version, so
        // their absence means "this version was never given an archive intent"
        // rather than "the archive failed". The two ids are reported only when the
        // row actually names them: an `ARCHIVED` row is guaranteed by
        // `chk_deploy_certificate_archive_located` to carry both, and a link into
        // a Drive location that does not exist is worse than no link.
        archive_status: decode_archive_status(
            row.try_get::<Option<String>, _>("archive_status")
                .ok()
                .flatten()
                .as_deref(),
        ),
        archive_space_id: optional_text_column(row, "archive_space_id"),
        archive_folder_node_id: optional_text_column(row, "archive_folder_node_id"),
        status: row.try_get("status")?,
        created_at: datetime_from_row(row, "created_at")?,
        updated_at: datetime_from_row(row, "updated_at")?,
        version: row.try_get::<i64, _>("version")?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        certificate_zone_apex, resolve_creation_validation_method, DeployRepository,
        RenewalEligibility,
    };
    use sdkwork_deploy_contract::{
        ChallengePresentation, CreateCertificateRequest, ValidationMethod,
    };

    fn eligibility(source: &str, status: &str, renewal_status: &str) -> RenewalEligibility {
        DeployRepository::renewal_eligibility(source, status, renewal_status)
    }

    #[test]
    fn a_managed_certificate_is_renewable_while_healthy_expired_or_failed() {
        for status in ["ACTIVE", "FAILED", "EXPIRED"] {
            assert_eq!(
                eligibility("MANAGED", status, "NONE"),
                RenewalEligibility::Allowed,
                "{status} must be manually renewable"
            );
        }
    }

    #[test]
    fn an_in_flight_renewal_is_reported_as_such() {
        // The distinction matters: "already running" is a retry-safe answer with a
        // renewal to poll, while "not renewable" is a rejected request.
        for renewal_status in ["PLANNED", "PROCESSING"] {
            assert_eq!(
                eligibility("MANAGED", "ACTIVE", renewal_status),
                RenewalEligibility::InProgress
            );
        }
    }

    #[test]
    fn imported_revoked_and_pending_certificates_are_not_renewable() {
        assert_eq!(
            eligibility("CUSTOM", "ACTIVE", "NONE"),
            RenewalEligibility::NotRenewable
        );
        assert_eq!(
            eligibility("MANAGED", "REVOKED", "NONE"),
            RenewalEligibility::NotRenewable
        );
        assert_eq!(
            eligibility("MANAGED", "PENDING", "NONE"),
            RenewalEligibility::NotRenewable
        );
    }

    #[test]
    fn auto_renew_is_not_part_of_the_manual_decision() {
        // A certificate excluded from automatic renewal must still be refreshable
        // by hand; it is the case manual refresh exists for. The decision function
        // therefore takes no `auto_renew` input at all, and this test is the note
        // explaining why it never should.
        assert_eq!(
            eligibility("MANAGED", "ACTIVE", "NONE"),
            RenewalEligibility::Allowed
        );
    }

    #[test]
    fn manual_auto_validation_is_persisted_as_dns01() {
        let request = CreateCertificateRequest {
            challenge_presentation: ChallengePresentation::Manual,
            validation_method: ValidationMethod::Auto,
            ..CreateCertificateRequest::default()
        };
        assert_eq!(
            resolve_creation_validation_method(&request).expect("manual AUTO is DNS-01"),
            ValidationMethod::Dns01
        );
    }

    #[test]
    fn managed_scope_validation_keeps_its_existing_resolution() {
        let request = CreateCertificateRequest::default();
        assert_eq!(
            resolve_creation_validation_method(&request).expect("managed AUTO is valid"),
            ValidationMethod::Http01
        );
    }

    #[test]
    fn manual_certificate_zone_apex_uses_the_parent_namespace() {
        assert_eq!(certificate_zone_apex("example.com"), "example.com");
        assert_eq!(certificate_zone_apex("api.example.com"), "example.com");
        assert_eq!(certificate_zone_apex("a.b.example.com"), "b.example.com");
    }
}
