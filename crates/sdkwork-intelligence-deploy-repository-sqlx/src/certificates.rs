use std::collections::{BTreeSet, HashMap};

use sdkwork_deploy_contract::{
    plan_certificate_identifiers, resolve_validation_method, CertificatePage, CertificatePlanError,
    CertificateResponse, CertificateScope, CreateCertificateRequest, DeployServiceError,
    DeployServiceResult, ValidationMethod, CERTIFICATE_RENEWAL_STATUS_PLANNED,
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

const CERTIFICATE_SELECT: &str = "c.uuid, c.cert_name, c.certificate_source, c.ca_profile,
     c.certificate_scope, c.validation_method, c.provider_account_id,
     c.preferred_key_algorithm, c.auto_renew, c.renewal_status, c.status,
     c.renew_before_days, c.renewal_failure_count, c.last_renewal_at,
     c.created_at, c.updated_at, c.version,
     v.uuid AS current_version_uuid, v.issuer, v.not_before, v.not_after,
     COALESCE((
         SELECT jsonb_agg(ci.hostname_ascii ORDER BY ci.position)
         FROM deploy_certificate_identifier ci
         WHERE ci.certificate_id = c.id
     ), '[]'::jsonb) AS identifiers";

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

        let domain_ids = request
            .domain_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        // The gate is the tenant *and* the zone's owner, not the tenant alone.
        //
        // A tenant-scoped check is not enough: the same tenant also holds the
        // platform's own `app.<suffix>` zones, whose hostnames the deployment
        // verifies for app publishing. Those rows are `VERIFIED` + `ACTIVE`, so a
        // tenant-only predicate accepts them, and any member of the tenant could
        // then order a certificate covering platform infrastructure — the edge's
        // own names. The zone is the unit of ownership (`deploy_dns_zone.user_id`),
        // so it is the unit the gate has to speak in.
        //
        // "Reachable" is the same rule the domain inventory states, spelled the
        // same way (`zone_owner_gate`): a zone is reachable when it is the
        // caller's own, or when it is tenant-level (`user_id IS NULL`) and
        // therefore shared with every member. It is deliberately *not* "the
        // caller is the platform": no such caller exists here, and denying
        // tenant-level zones outright would hide the platform zones the
        // inventory documents as visible to everyone.
        //
        // `$3 IS NULL` (an unauthenticated actor) keeps the pre-existing
        // tenant-wide behaviour rather than silently denying every request, since
        // this path is also reached by internal callers that carry no user.
        let requested_domains = sqlx::query(
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
        if requested_domains.len() != request.domain_ids.len() {
            return Err(DeployServiceError::validation(
                "every domainId must reference an active verified hostname in the current tenant",
            ));
        }
        let requested_hostnames = requested_domains
            .iter()
            .map(|row| row.try_get::<String, _>("hostname_ascii"))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                DeployServiceError::Internal(format!("map certificate hostname: {error}"))
            })?;

        // Scope rules (apex completion, wildcard depth, challenge legality) are
        // pure and live in the contract crate; this layer only supplies the
        // claims the caller selected.
        let planned_identifiers =
            plan_certificate_identifiers(request.certificate_scope, &requested_hostnames)
                .map_err(map_certificate_plan_error)?;
        let validation_method =
            resolve_validation_method(request.certificate_scope, request.validation_method)
                .map_err(map_certificate_plan_error)?;

        // A wildcard scope adds the apex, which the caller did not select, so the
        // planned set is resolved against `deploy_domain` a second time rather
        // than reusing `requested_domains`. The added apex must already be an
        // active verified claim of this tenant — and, for the same reason the
        // caller's own selection is zone-gated, of a zone this caller can reach.
        // Resolving it tenant-wide would have let a wildcard pull in an apex from
        // someone else's private zone, which is the caller's own selection rule
        // bypassed by a side door.
        let planned_hostnames = planned_identifiers
            .iter()
            .map(|identifier| identifier.hostname.clone())
            .collect::<Vec<_>>();
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
                DeployServiceError::Internal(format!("map planned certificate hostname: {error}"))
            })?;
            let domain_id: i64 = claim.try_get("id").map_err(|error| {
                DeployServiceError::Internal(format!("map planned certificate domain id: {error}"))
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

        let certificate_id = next_id(self.id_generator())?;
        let certificate_uuid = new_uuid();
        sqlx::query(
            "INSERT INTO deploy_certificate (
                id, uuid, tenant_id, organization_id, cert_name, certificate_source,
                ca_profile, certificate_scope, validation_method, preferred_key_algorithm,
                auto_renew, renew_before_days, renewal_status, status, provider_account_id,
                idempotency_key, request_sha256, created_by, updated_by
             ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'NONE','PENDING',$13,$14,$15,$16,$16
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

    pub(super) async fn renew_certificate_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateResponse> {
        let result = sqlx::query(
            "UPDATE deploy_certificate
             SET renewal_status = $3, updated_at = CURRENT_TIMESTAMP, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND certificate_source = $4
               AND auto_renew = TRUE AND status IN ('ACTIVE', 'FAILED')
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
            return Err(DeployServiceError::validation(
                "only active managed certificates with auto-renew enabled can be renewed",
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
    if request.domain_ids.is_empty() || request.domain_ids.len() > MAX_CERTIFICATE_IDENTIFIERS {
        return Err(DeployServiceError::validation(
            "domainIds must contain between 1 and 100 hostnames",
        ));
    }
    let unique_domain_ids = request.domain_ids.iter().collect::<BTreeSet<_>>();
    if unique_domain_ids.len() != request.domain_ids.len()
        || request.domain_ids.iter().any(|id| id.trim().is_empty())
    {
        return Err(DeployServiceError::validation(
            "domainIds must contain unique non-empty hostname identifiers",
        ));
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

    Ok(CertificateResponse {
        id: row.try_get("uuid")?,
        cert_name: row.try_get("cert_name")?,
        certificate_source: row.try_get("certificate_source")?,
        ca_profile: row.try_get("ca_profile")?,
        certificate_scope,
        validation_method,
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
        renewal_failure_count: row.try_get("renewal_failure_count")?,
        status: row.try_get("status")?,
        created_at: datetime_from_row(row, "created_at")?,
        updated_at: datetime_from_row(row, "updated_at")?,
        version: row.try_get::<i64, _>("version")?.to_string(),
    })
}
