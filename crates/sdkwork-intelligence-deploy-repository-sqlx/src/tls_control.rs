//! TLS control plane repository operations (TECH-cloud-app-publishing §4.5):
//! ACME accounts, certificate order/challenge state machines, and certificate
//! version storage. The table schema (migration 0004) is the state machine
//! authority; every transition is an optimistic UPDATE guarded by the current
//! status column.

use sdkwork_deploy_certificate_material::{
    MaterialKind, Protection, SealedCertificateFile, SealedMaterial,
};
use sdkwork_deploy_contract::{
    next_order_transition, AcmeAccountPage, AcmeAccountResponse,
    CertificateArchiveClaim, CertificateArchiveOutcome, CertificateArchiveStatus,
    CertificateChallengePresentationList, CertificateChallengePresentationResponse,
    CertificateChallengePage, CertificateChallengeResponse, CertificateOrderPage,
    CertificateOrderResponse, CertificateScope, CreateAcmeAccountRequest, DeployServiceError,
    DeployServiceResult, ValidationMethod, CAA_DECISION_LOOKUP_FAILED, CAA_DECISION_PERMITTED,
    CAA_DECISION_UNAUTHORIZED_CA, ORDER_ERROR_DEADLINE_EXCEEDED, ORDER_STATUS_CANCELLED,
    ORDER_STATUS_FAILED, ORDER_STATUS_FINALIZING, ORDER_STATUS_VERSION_STORED,
    RENEWAL_STATUS_SUCCEEDED,
};
use sdkwork_intelligence_deploy_service::{
    resolve_challenge_type, CertificateOrderCaaSubject, CertificateOrderClaim,
};
use sqlx::Row;

use crate::support::{
    new_uuid, next_id, now_rfc3339, optional_datetime, pagination, required_datetime, sha256_hex,
    store_error,
};
use crate::DeployRepository;

impl DeployRepository {
    pub(super) async fn create_acme_account_repo(
        &self,
        tenant_id: i64,
        request: &CreateAcmeAccountRequest,
    ) -> DeployServiceResult<AcmeAccountResponse> {
        let account_id = next_id(self.id_generator())?;
        let account_uuid = new_uuid();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO deploy_acme_account
                (id, uuid, tenant_id, ca_profile, directory_url, contact_email,
                 external_account_digest, account_key_secret_ref, status,
                 created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'secret://acme-account-key/' || $2, 'ACTIVE',
                     CAST($8 AS TIMESTAMPTZ), CAST($8 AS TIMESTAMPTZ))",
        )
        .bind(account_id)
        .bind(&account_uuid)
        .bind(tenant_id)
        .bind(&request.ca_profile)
        .bind(&request.directory_url)
        .bind(&request.contact_email)
        .bind(request.external_account_digest.as_deref())
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert deploy_acme_account", error))?;
        self.retrieve_acme_account_internal_repo(tenant_id, &account_uuid)
            .await
    }

    pub(super) async fn list_acme_accounts_repo(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<AcmeAccountPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_acme_account
             WHERE tenant_id = $1 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_acme_account", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT uuid, tenant_id, ca_profile, directory_url, contact_email,
                    external_account_digest, status, created_at, updated_at, version
             FROM deploy_acme_account
             WHERE tenant_id = $1 AND deleted_at IS NULL
             ORDER BY updated_at DESC, id DESC LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_acme_account", error))?;

        let items = rows
            .iter()
            .map(map_acme_account_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AcmeAccountPage {
            items,
            total,
            page,
            page_size,
        })
    }

    async fn retrieve_acme_account_internal_repo(
        &self,
        tenant_id: i64,
        account_id: &str,
    ) -> DeployServiceResult<AcmeAccountResponse> {
        let row = sqlx::query(
            "SELECT uuid, tenant_id, ca_profile, directory_url, contact_email,
                    external_account_digest, status, created_at, updated_at, version
             FROM deploy_acme_account
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_acme_account", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("acme account not found"));
        };
        map_acme_account_row(&row)
    }

    /// Creates a certificate order with one challenge per identifier.
    /// Idempotent on `(tenant_id, idempotency_key)`: a replay returns the
    /// existing order. `requested_version_no` is the next version of the
    /// certificate being requested; the caller computes it.
    pub(super) async fn request_certificate_order_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        idempotency_key: &str,
        requested_challenge_type: Option<&str>,
    ) -> DeployServiceResult<CertificateOrderResponse> {
        let certificate_row = sqlx::query(
            "SELECT c.id, c.uuid, c.renewal_status,
                    COALESCE(c.current_version_id, 0) AS current_version_id,
                    c.certificate_scope, c.validation_method, c.presentation_mode
             FROM deploy_certificate c
             WHERE c.tenant_id = $1 AND c.uuid = $2 AND c.status <> 'REVOKED' AND c.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(certificate_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_certificate for order", error))?;
        let Some(certificate_row) = certificate_row else {
            return Err(DeployServiceError::not_found("certificate not found"));
        };
        let certificate_internal_id: i64 = certificate_row.try_get("id").unwrap_or(0);

        // The challenge row has to name the method that will actually be used. It is what the
        // console reports and what the manual DNS-01 flow reads to decide whether to hand the
        // operator a TXT record, so a row that names the wrong one misdescribes the issuance
        // it belongs to. A caller may name a method — that is how the operator endpoint asks
        // for DNS-01 on an exact name — but when it does not, the certificate's own scope and
        // validation method decide. Defaulting to HTTP-01 instead wrote an HTTP-01 challenge
        // for a wildcard, which no CA will ever validate and which only looked harmless
        // because the executor resolves the method again for itself.
        let scope_raw: String = certificate_row
            .try_get("certificate_scope")
            .map_err(|error| {
                DeployServiceError::Internal(format!("map certificate scope: {error}"))
            })?;
        let scope = CertificateScope::parse(&scope_raw).ok_or_else(|| {
            DeployServiceError::Internal(format!("unknown certificate scope {scope_raw}"))
        })?;
        // Who presents the challenges is a property of the certificate, copied onto the
        // order here because the worker's claim query filters on it: a `MANUAL` order
        // parks after its records are prepared and is only resumed once an operator
        // releases it. A row written before the presentation switch reads back as the
        // column default (`MANAGED`), which is what those issuances were.
        let presentation_mode: String = certificate_row
            .try_get("presentation_mode")
            .unwrap_or_else(|_| "MANAGED".to_owned());
        let method_raw: String = certificate_row
            .try_get("validation_method")
            .map_err(|error| {
                DeployServiceError::Internal(format!("map certificate validation method: {error}"))
            })?;
        let method = ValidationMethod::parse(&method_raw).ok_or_else(|| {
            DeployServiceError::Internal(format!(
                "unknown certificate validation method {method_raw}"
            ))
        })?;
        let challenge_type = match requested_challenge_type {
            Some(requested) if !requested.trim().is_empty() => {
                let requested = ValidationMethod::parse(requested).ok_or_else(|| {
                    DeployServiceError::validation(
                        "challengeType must be HTTP_01 or DNS_01".to_owned(),
                    )
                })?;
                // Scope beats the request rather than rejecting it: a wildcard cannot be
                // validated any other way, so DNS-01 is the honest row.
                resolve_challenge_type(scope, requested)
            }
            _ => resolve_challenge_type(scope, method),
        };

        // Idempotency replay: return the existing order when present.
        let existing = sqlx::query(
            "SELECT o.uuid, o.tenant_id, c.uuid AS certificate_uuid, a.uuid AS account_uuid,
                    o.requested_version_no, o.request_sha256, o.idempotency_key,
                    o.external_order_digest, o.status, o.attempt_count, o.last_error_code,
                    o.caa_decision, o.caa_checked_at,
                    o.deadline_at, o.created_at, o.updated_at, o.version
             FROM deploy_certificate_order o
             JOIN deploy_certificate c ON c.id = o.certificate_id
             JOIN deploy_acme_account a ON a.id = o.acme_account_id
             WHERE o.tenant_id = $1 AND o.idempotency_key = $2
             ORDER BY o.created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("find certificate order by idempotency", error))?;
        if let Some(row) = existing {
            return map_certificate_order_row(&row);
        }

        // Resolve the tenant's active ACME account (prefer production profile,
        // fall back to any ACTIVE account).
        let account_row = sqlx::query(
            "SELECT uuid FROM deploy_acme_account
             WHERE tenant_id = $1 AND status = 'ACTIVE' AND deleted_at IS NULL
             ORDER BY (ca_profile = 'LETS_ENCRYPT_PRODUCTION') DESC, updated_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve acme account for order", error))?;
        let Some(account_row) = account_row else {
            return Err(DeployServiceError::validation(
                "no active ACME account for this tenant; create one first",
            ));
        };
        let account_uuid: String = account_row.try_get("uuid").unwrap_or_default();

        // Next version of the certificate being requested.
        let next_version_row = sqlx::query(
            "SELECT COALESCE(MAX(version_no), 0) + 1 AS next_version_no
             FROM deploy_certificate_version WHERE certificate_id = $1",
        )
        .bind(certificate_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("compute next certificate version", error))?;
        let requested_version_no: i64 = next_version_row.try_get("next_version_no").unwrap_or(1);

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin certificate order", error))?;
        let order_id = next_id(self.id_generator())?;
        let order_uuid = new_uuid();
        let now = now_rfc3339();
        let request_sha256 = sha256_hex(&format!("{certificate_id}:{idempotency_key}"));
        let deadline = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::hours(24))
            .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
            .unwrap_or(now.clone());
        let account_internal_id =
            resolve_acme_account_internal_id(&mut transaction, &account_uuid).await?;
        let inserted = sqlx::query(
            "INSERT INTO deploy_certificate_order
                (id, uuid, tenant_id, certificate_id, acme_account_id, requested_version_no,
                 request_sha256, idempotency_key, presentation_mode, status, attempt_count,
                 deadline_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $11, 'REQUESTED', 0,
                     CAST($9 AS TIMESTAMPTZ), CAST($10 AS TIMESTAMPTZ), CAST($10 AS TIMESTAMPTZ))
             ON CONFLICT (tenant_id, idempotency_key) DO NOTHING",
        )
        .bind(order_id)
        .bind(&order_uuid)
        .bind(tenant_id)
        .bind(certificate_internal_id)
        .bind(account_internal_id)
        .bind(requested_version_no)
        .bind(&request_sha256)
        .bind(idempotency_key)
        .bind(&deadline)
        .bind(&now)
        .bind(&presentation_mode)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy_certificate_order", error))?;
        if inserted.rows_affected() == 0 {
            // Concurrent replay of the same idempotency key: commit and return
            // the winner's order.
            transaction
                .commit()
                .await
                .map_err(|error| store_error("commit certificate order replay", error))?;
            return self
                .find_certificate_order_by_idempotency_repo(tenant_id, idempotency_key)
                .await?
                .ok_or_else(|| {
                    DeployServiceError::Internal(
                        "certificate order disappeared after concurrent insert".into(),
                    )
                });
        }

        // One challenge per certificate identifier.
        let identifiers = sqlx::query(
            "SELECT id, hostname_ascii FROM deploy_certificate_identifier
             WHERE certificate_id = $1 ORDER BY position ASC",
        )
        .bind(certificate_internal_id)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| store_error("list certificate identifiers", error))?;
        for identifier in &identifiers {
            let identifier_id: i64 = identifier.try_get("id").unwrap_or(0);
            let hostname: String = identifier.try_get("hostname_ascii").unwrap_or_default();
            let challenge_id = next_id(self.id_generator())?;
            let challenge_uuid = new_uuid();
            // Deterministic proof placeholder: the key authorization hash is
            // produced by the ACME client boundary once credentials exist; the
            // placeholder keeps the state machine exercisable end to end.
            let proof = sha256_hex(&format!("{order_uuid}:{hostname}:{challenge_type}"));
            sqlx::query(
                "INSERT INTO deploy_certificate_challenge
                    (id, uuid, tenant_id, order_id, identifier_id, challenge_type,
                     proof_sha256, status, attempt_count, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 'PENDING', 0,
                         CAST($8 AS TIMESTAMPTZ), CAST($8 AS TIMESTAMPTZ))",
            )
            .bind(challenge_id)
            .bind(&challenge_uuid)
            .bind(tenant_id)
            .bind(order_id)
            .bind(identifier_id)
            .bind(challenge_type)
            .bind(&proof)
            .bind(&now)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert deploy_certificate_challenge", error))?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit certificate order", error))?;
        self.retrieve_certificate_order_internal_repo(tenant_id, &order_uuid)
            .await
    }

    pub(super) async fn find_certificate_order_by_idempotency_repo(
        &self,
        tenant_id: i64,
        idempotency_key: &str,
    ) -> DeployServiceResult<Option<CertificateOrderResponse>> {
        let row = sqlx::query(
            "SELECT o.uuid, o.tenant_id, c.uuid AS certificate_uuid, a.uuid AS account_uuid,
                    o.requested_version_no, o.request_sha256, o.idempotency_key,
                    o.external_order_digest, o.status, o.attempt_count, o.last_error_code,
                    o.caa_decision, o.caa_checked_at,
                    o.deadline_at, o.created_at, o.updated_at, o.version
             FROM deploy_certificate_order o
             JOIN deploy_certificate c ON c.id = o.certificate_id
             JOIN deploy_acme_account a ON a.id = o.acme_account_id
             WHERE o.tenant_id = $1 AND o.idempotency_key = $2",
        )
        .bind(tenant_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("find certificate order by idempotency", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(map_certificate_order_row(&row)?))
    }

    /// Subjects the CAA pre-flight needs for a certificate that is about to be
    /// ordered: its planned identifiers, its resolved validation method, and the
    /// directory URL of the ACME account the order would use.
    ///
    /// The account resolution is deliberately the same query the order itself
    /// performs, so the CA identity a CAA check matches and the CA that is later
    /// asked cannot disagree.
    pub(super) async fn certificate_order_caa_subject_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateOrderCaaSubject> {
        let certificate_row = sqlx::query(
            "SELECT id, validation_method FROM deploy_certificate
             WHERE tenant_id = $1 AND uuid = $2 AND status <> 'REVOKED' AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(certificate_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_certificate for CAA", error))?;
        let Some(certificate_row) = certificate_row else {
            return Err(DeployServiceError::not_found("certificate not found"));
        };
        let certificate_internal_id: i64 = certificate_row.try_get("id").unwrap_or(0);
        let validation_method: String = certificate_row
            .try_get("validation_method")
            .unwrap_or_default();

        // Wildcard identifiers keep their `*.` marker here; the CAA layer reduces
        // them to the RRset start name itself so the storage form stays the plan
        // form.
        let identifiers = sqlx::query(
            "SELECT hostname_ascii FROM deploy_certificate_identifier
             WHERE certificate_id = $1 ORDER BY position ASC",
        )
        .bind(certificate_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list certificate identifiers for CAA", error))?
        .iter()
        .filter_map(|row| row.try_get::<String, _>("hostname_ascii").ok())
        .collect::<Vec<_>>();

        let directory_url = sqlx::query(
            "SELECT directory_url FROM deploy_acme_account
             WHERE tenant_id = $1 AND status = 'ACTIVE' AND deleted_at IS NULL
             ORDER BY (ca_profile = 'LETS_ENCRYPT_PRODUCTION') DESC, updated_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve acme account for CAA", error))?
        .and_then(|row| row.try_get::<String, _>("directory_url").ok());

        Ok(CertificateOrderCaaSubject {
            identifiers,
            validation_method,
            directory_url,
        })
    }

    /// Records the CAA pre-issuance decision and the instant it was observed.
    ///
    /// The update is guarded on the stored decision differing, so re-recording
    /// an identical observation does not burn a row version, and the `CHECK` on
    /// the table keeps the decision bounded to the three contract values.
    pub(super) async fn record_certificate_order_caa_decision_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
        decision: &str,
        checked_at: &str,
    ) -> DeployServiceResult<()> {
        if !matches!(
            decision,
            CAA_DECISION_PERMITTED | CAA_DECISION_UNAUTHORIZED_CA | CAA_DECISION_LOOKUP_FAILED
        ) {
            return Err(DeployServiceError::validation(
                "caaDecision must be PERMITTED, UNAUTHORIZED_CA, or LOOKUP_FAILED",
            ));
        }
        if chrono::DateTime::parse_from_rfc3339(checked_at).is_err() {
            return Err(DeployServiceError::validation(
                "caaCheckedAt must be an RFC 3339 timestamp",
            ));
        }
        let order_internal_id =
            resolve_certificate_order_internal_id(&self.pool, tenant_id, order_id).await?;
        sqlx::query(
            "UPDATE deploy_certificate_order
                SET caa_decision = $1, caa_checked_at = CAST($2 AS TIMESTAMPTZ),
                    updated_at = NOW(), version = version + 1
             WHERE id = $3 AND caa_decision IS DISTINCT FROM $1",
        )
        .bind(decision)
        .bind(checked_at)
        .bind(order_internal_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("record deploy_certificate_order CAA decision", error))?;
        Ok(())
    }

    /// Optimistically advances the order state machine one step. Returns the
    /// previous status when the transition is not applicable (no-op) and the
    /// new status on success.
    ///
    /// The transition must be the canonical successor of `from_status`. The
    /// optimistic `WHERE status = from_status` guard only proves the row has not
    /// moved, not that the step is legal, so without this check a caller could
    /// skip states — `REQUESTED -> CHALLENGE_PRESENTING` would match the guard
    /// and jump straight past the ACME order and presentation stages.
    pub(super) async fn advance_certificate_order_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
        from_status: &str,
        to_status: &str,
    ) -> DeployServiceResult<String> {
        let order_internal_id =
            resolve_certificate_order_internal_id(&self.pool, tenant_id, order_id).await?;
        // Checked after resolving the order so an unknown order is still a
        // not-found rather than a silent no-op.
        if next_order_transition(from_status) != Some(to_status) {
            return Ok(from_status.to_owned());
        }
        let result = sqlx::query(
            "UPDATE deploy_certificate_order
                SET status = $1, attempt_count = attempt_count + 1, updated_at = NOW(),
                    version = version + 1
             WHERE id = $2 AND status = $3",
        )
        .bind(to_status)
        .bind(order_internal_id)
        .bind(from_status)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("advance deploy_certificate_order", error))?;
        if result.rows_affected() == 0 {
            return Ok(from_status.to_owned());
        }
        Ok(to_status.to_owned())
    }

    pub(super) async fn fail_certificate_order_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
        error_code: &str,
    ) -> DeployServiceResult<()> {
        let order_internal_id =
            resolve_certificate_order_internal_id(&self.pool, tenant_id, order_id).await?;
        sqlx::query(
            "UPDATE deploy_certificate_order
                SET status = $1, last_error_code = $2, updated_at = NOW(), version = version + 1
             WHERE id = $3 AND status NOT IN ($1, 'VERSION_STORED', 'CANCELLED')",
        )
        .bind(ORDER_STATUS_FAILED)
        .bind(error_code)
        .bind(order_internal_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("fail deploy_certificate_order", error))?;
        Ok(())
    }

    /// Claims non-terminal certificate orders for the issuance worker.
    ///
    /// One statement, for the same reason the renewal claim is one statement:
    /// choosing the candidates and taking their lease must not be separable, or
    /// two workers both conclude they own the same order and the CA is asked to
    /// issue twice for one intent. `FOR UPDATE SKIP LOCKED` makes concurrent
    /// workers partition the queued orders instead of serialising behind each
    /// other.
    ///
    /// The lease is always taken with an expiry, and `attempt_count` is charged
    /// here rather than by the caller: a bounded-retry rule can then read one row
    /// and see the truth, and a worker that dies mid-issuance releases the order
    /// when its lease expires — which is what turns a crash into a retry instead
    /// of a certificate that never issues.
    ///
    /// `deadline_at` is respected rather than cleared: an order whose deadline has
    /// passed is not claimable, so a CA outage cannot leave the sweep grinding on
    /// work that can no longer be delivered.
    pub(super) async fn claim_certificate_orders_repo(
        &self,
        worker_id: &str,
        batch_size: i64,
        lease_seconds: i64,
        now: &str,
    ) -> DeployServiceResult<Vec<CertificateOrderClaim>> {
        let rows = sqlx::query(
            "UPDATE deploy_certificate_order o
                SET lease_owner = $1,
                    lease_expires_at = CAST($4 AS TIMESTAMPTZ) + make_interval(secs => $2),
                    attempt_count = o.attempt_count + 1,
                    updated_at = NOW(),
                    version = o.version + 1
              WHERE o.id IN (
                    SELECT id FROM deploy_certificate_order
                     WHERE status NOT IN ($5, $6, $7)
                       AND (deadline_at IS NULL OR deadline_at > CAST($4 AS TIMESTAMPTZ))
                       AND (next_attempt_at IS NULL OR next_attempt_at <= CAST($4 AS TIMESTAMPTZ))
                       AND (lease_expires_at IS NULL OR lease_expires_at <= CAST($4 AS TIMESTAMPTZ))
                       -- A manual order needs exactly two worker passes. The first
                       -- has no CA order yet (`external_order_url IS NULL`) and
                       -- prepares the challenge records; after that it is parked
                       -- until an operator releases it. Without the first arm the
                       -- order would never be prepared at all, and without the
                       -- third it would be validated before anyone published the
                       -- records.
                       AND (
                           presentation_mode <> 'MANUAL'
                           OR external_order_url IS NULL
                           OR presentation_ready_at IS NOT NULL
                       )
                     ORDER BY created_at, id
                     LIMIT $3
                     FOR UPDATE SKIP LOCKED
              )
             RETURNING o.uuid, o.tenant_id, o.status, o.attempt_count, o.requested_version_no,
                       o.presentation_mode, o.external_order_url,
                       (SELECT c.uuid FROM deploy_certificate c WHERE c.id = o.certificate_id)
                           AS certificate_uuid",
        )
        .bind(worker_id)
        .bind(lease_seconds as f64)
        .bind(batch_size)
        .bind(now)
        .bind(ORDER_STATUS_VERSION_STORED)
        .bind(ORDER_STATUS_FAILED)
        .bind(ORDER_STATUS_CANCELLED)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("claim deploy_certificate_order", error))?;

        let mut claims = Vec::with_capacity(rows.len());
        for row in rows {
            claims.push(CertificateOrderClaim {
                tenant_id: row.try_get("tenant_id").unwrap_or(0),
                order_uuid: row
                    .try_get("uuid")
                    .map_err(|error| store_error("read claimed order uuid", error))?,
                certificate_uuid: row
                    .try_get("certificate_uuid")
                    .map_err(|error| store_error("read claimed order certificate", error))?,
                // The status is deliberately read back rather than assumed: a
                // reclaimed order resumes from where the previous worker actually
                // stopped, not from where it was expected to stop.
                status: row.try_get("status").unwrap_or_default(),
                attempt_count: row.try_get("attempt_count").unwrap_or(0),
                requested_version_no: row.try_get("requested_version_no").unwrap_or(1),
                presentation_mode: row
                    .try_get("presentation_mode")
                    .unwrap_or_else(|_| "MANAGED".to_owned()),
                external_order_url: row.try_get("external_order_url").unwrap_or(None),
            });
        }
        Ok(claims)
    }

    /// Advances an order only while `lease_owner` still holds a live lease.
    ///
    /// The lease predicate is inside the same `UPDATE` as the status predicate, so
    /// the fence cannot be lost between checking it and acting on it: if the lease
    /// expired in the microseconds before this statement, the row does not match and
    /// the transition does not happen. Reading the current status afterwards is
    /// deliberately a second statement — the point of the returned value is to tell
    /// the caller what happened, not to make the decision.
    pub(super) async fn advance_leased_certificate_order_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
        lease_owner: &str,
        from_status: &str,
        to_status: &str,
    ) -> DeployServiceResult<String> {
        let order_internal_id =
            resolve_certificate_order_internal_id(&self.pool, tenant_id, order_id).await?;
        // An illegal edge is not this method's to judge — mirroring
        // `advance_certificate_order`, it reports the order as unchanged so the
        // single source of truth for the lifecycle stays `next_order_transition`.
        if next_order_transition(from_status) != Some(to_status) {
            return Ok(from_status.to_owned());
        }
        let advanced = sqlx::query(
            "UPDATE deploy_certificate_order
                SET status = $1, updated_at = NOW(), version = version + 1
              WHERE id = $2 AND status = $3
                AND lease_owner = $4
                AND lease_expires_at IS NOT NULL
                AND lease_expires_at > NOW()",
        )
        .bind(to_status)
        .bind(order_internal_id)
        .bind(from_status)
        .bind(lease_owner)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("advance leased deploy_certificate_order", error))?;
        if advanced.rows_affected() > 0 {
            return Ok(to_status.to_owned());
        }
        let current = sqlx::query("SELECT status FROM deploy_certificate_order WHERE id = $1")
            .bind(order_internal_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("read deploy_certificate_order status", error))?;
        Ok(current
            .and_then(|row| row.try_get::<String, _>("status").ok())
            .unwrap_or_else(|| from_status.to_owned()))
    }

    /// Fails non-terminal orders whose deadline has passed, returning how many.
    pub(super) async fn fail_expired_certificate_orders_repo(
        &self,
        now: &str,
        batch_size: i64,
    ) -> DeployServiceResult<i64> {
        let result = sqlx::query(
            "UPDATE deploy_certificate_order o
                SET status = $1, last_error_code = $2,
                    lease_owner = NULL, lease_expires_at = NULL,
                    updated_at = NOW(), version = o.version + 1
              WHERE o.id IN (
                    SELECT id FROM deploy_certificate_order
                     WHERE status NOT IN ($3, $4, $5)
                       AND deadline_at <= CAST($6 AS TIMESTAMPTZ)
                     ORDER BY deadline_at, id
                     LIMIT $7
                     FOR UPDATE SKIP LOCKED
              )",
        )
        .bind(ORDER_STATUS_FAILED)
        .bind(ORDER_ERROR_DEADLINE_EXCEEDED)
        .bind(ORDER_STATUS_VERSION_STORED)
        .bind(ORDER_STATUS_FAILED)
        .bind(ORDER_STATUS_CANCELLED)
        .bind(now)
        .bind(batch_size)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("fail expired deploy_certificate_order", error))?;
        Ok(result.rows_affected() as i64)
    }

    /// Records a challenge validation result. A VALID challenge advances the
    /// order to FINALIZING; a FAILED challenge fails the order. `challenge_id`
    /// may be `None` when the order has no challenges (all-order validation).
    pub(super) async fn record_challenge_result_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
        challenge_id: Option<&str>,
        valid: bool,
        error_code: Option<&str>,
    ) -> DeployServiceResult<()> {
        let order_internal_id =
            resolve_certificate_order_internal_id(&self.pool, tenant_id, order_id).await?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin challenge result", error))?;

        if let Some(challenge_id) = challenge_id {
            let challenge_internal_id = sqlx::query(
                "SELECT id FROM deploy_certificate_challenge
                 WHERE tenant_id = $1 AND uuid = $2 AND order_id = $3",
            )
            .bind(tenant_id)
            .bind(challenge_id)
            .bind(order_internal_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("resolve challenge", error))?
            .and_then(|row| row.try_get::<i64, _>("id").ok())
            .ok_or_else(|| DeployServiceError::not_found("challenge not found"))?;
            let (challenge_status, validated_at) = if valid {
                ("VALID", Some(now_rfc3339()))
            } else {
                ("FAILED", None)
            };
            sqlx::query(
                "UPDATE deploy_certificate_challenge
                    SET status = $1,
                        validated_at = COALESCE(CAST($2 AS TIMESTAMPTZ), validated_at),
                        last_error_code = $3, checked_at = NOW(),
                        updated_at = NOW(), version = version + 1
                 WHERE id = $4 AND status NOT IN ('VALID', 'FAILED', 'CLEANED')",
            )
            .bind(challenge_status)
            .bind(validated_at)
            .bind(error_code)
            .bind(challenge_internal_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("update challenge result", error))?;
        }

        if valid {
            sqlx::query(
                "UPDATE deploy_certificate_order
                    SET status = $1, updated_at = NOW(), version = version + 1
                 WHERE id = $2 AND status = 'CHALLENGE_VALIDATING'",
            )
            .bind(ORDER_STATUS_FINALIZING)
            .bind(order_internal_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("advance order to finalizing", error))?;
        } else {
            sqlx::query(
                "UPDATE deploy_certificate_order
                    SET status = $1, last_error_code = COALESCE($2, last_error_code),
                        updated_at = NOW(), version = version + 1
                 WHERE id = $3 AND status NOT IN ('VERSION_STORED', 'CANCELLED')",
            )
            .bind(ORDER_STATUS_FAILED)
            .bind(error_code)
            .bind(order_internal_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("fail order on challenge result", error))?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit challenge result", error))?;
        Ok(())
    }

    /// Records the manual DNS-01 presentation for one challenge.
    ///
    /// Keyed by `(order, hostname)` rather than challenge uuid because the caller
    /// that computed the presentation (the ACME engine) knows the identifier, not
    /// the control plane's challenge row. A challenge already terminal is left
    /// alone: presenting a record for a validated authorization would only
    /// resurrect an instruction nobody needs.
    pub(super) async fn record_challenge_presentation_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
        hostname: &str,
        record_name: &str,
        record_value: &str,
        expires_at: &str,
        presentation_ref: Option<&str>,
    ) -> DeployServiceResult<()> {
        let order_internal_id =
            resolve_certificate_order_internal_id(&self.pool, tenant_id, order_id).await?;
        let result = sqlx::query(
            "UPDATE deploy_certificate_challenge ch
                SET presentation_record_name = $3,
                    presentation_record_value = $4,
                    presentation_expires_at = CAST($5 AS TIMESTAMPTZ),
                    presentation_ref = COALESCE($6, ch.presentation_ref),
                    status = 'PRESENTING',
                    updated_at = NOW(), version = ch.version + 1
               FROM deploy_certificate_identifier i
              WHERE ch.identifier_id = i.id
                AND ch.order_id = $1
                AND i.hostname_ascii = $2
                AND ch.tenant_id = $7
                AND ch.status NOT IN ('VALID', 'FAILED', 'CLEANED')",
        )
        .bind(order_internal_id)
        .bind(hostname)
        .bind(record_name)
        .bind(record_value)
        .bind(expires_at)
        .bind(presentation_ref)
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("record deploy_certificate_challenge presentation", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::not_found(
                "certificate challenge not found for presentation",
            ));
        }
        Ok(())
    }

    /// Releases a parked manual order so the issuance worker may validate it.
    ///
    /// Only a `MANUAL` order still sitting in `CHALLENGE_PRESENTING` can be
    /// released, and only once: a second release is a no-op rather than an error,
    /// because two operators clicking "verify" at the same moment is a retry, not
    /// a mistake. Returns whether this call was the one that released it.
    pub(super) async fn mark_order_presentation_ready_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
        ready_at: &str,
    ) -> DeployServiceResult<bool> {
        let result = sqlx::query(
            "UPDATE deploy_certificate_order
                SET presentation_ready_at = CAST($3 AS TIMESTAMPTZ),
                    next_attempt_at = NULL,
                    updated_at = NOW(), version = version + 1
              WHERE tenant_id = $1 AND uuid = $2
                AND presentation_mode = 'MANUAL'
                AND status = 'CHALLENGE_PRESENTING'
                AND presentation_ready_at IS NULL",
        )
        .bind(tenant_id)
        .bind(order_id)
        .bind(ready_at)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("release deploy_certificate_order presentation", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Records the CA order URL produced by a two-phase preparation pass.
    ///
    /// Written once: the resume pass needs it, and overwriting it with a second
    /// order's URL would orphan the challenges already published for the first.
    pub(super) async fn record_certificate_order_external_url_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
        external_order_url: &str,
    ) -> DeployServiceResult<()> {
        let result = sqlx::query(
            "UPDATE deploy_certificate_order
                SET external_order_url = $3,
                    updated_at = NOW(), version = version + 1
              WHERE tenant_id = $1 AND uuid = $2 AND external_order_url IS NULL",
        )
        .bind(tenant_id)
        .bind(order_id)
        .bind(external_order_url)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("record deploy_certificate_order external url", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::conflict(
                "certificate order already has an external order url",
            ));
        }
        Ok(())
    }

    /// Reads the manual presentation instructions of a certificate's latest
    /// manual order.
    ///
    /// Only the newest manual order is reported: an earlier parked order was
    /// superseded, and showing both would hand the operator two TXT values for the
    /// same name with no way to tell which one the CA is waiting for.
    pub(super) async fn list_certificate_presentations_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateChallengePresentationList> {
        let rows = sqlx::query(
            "SELECT o.presentation_mode,
                    (o.presentation_ready_at IS NOT NULL) AS released,
                    i.hostname_ascii AS identifier,
                    ch.presentation_record_name,
                    ch.presentation_record_value,
                    ch.presentation_expires_at,
                    ch.status,
                    c.presentation_mode AS certificate_presentation_mode
               FROM deploy_certificate_order o
               JOIN deploy_certificate c ON c.id = o.certificate_id
               LEFT JOIN deploy_certificate_challenge ch ON ch.order_id = o.id
               LEFT JOIN deploy_certificate_identifier i ON i.id = ch.identifier_id
              WHERE c.tenant_id = $1 AND c.uuid = $2
                AND o.id = (
                    SELECT MAX(o2.id) FROM deploy_certificate_order o2
                     WHERE o2.certificate_id = c.id
                       AND o2.presentation_mode = 'MANUAL'
                )
              ORDER BY i.position",
        )
        .bind(tenant_id)
        .bind(certificate_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_certificate presentations", error))?;

        let mut list = CertificateChallengePresentationList::default();
        if rows.is_empty() {
            // No manual order yet — either the certificate is managed, or its
            // manual order has not been prepared. Answering the certificate's own
            // choice is what lets the console tell "nothing to publish, this is
            // managed" from "manual, but the records are not ready yet", instead
            // of reporting a managed default for both and asking the operator to
            // wait for records that will never appear.
            list.presentation_mode = self
                .certificate_presentation_mode_repo(tenant_id, certificate_id)
                .await?
                .unwrap_or_else(|| "MANAGED".to_owned());
            return Ok(list);
        }
        for row in rows {
            list.presentation_mode = row
                .try_get("presentation_mode")
                .unwrap_or_else(|_| "MANAGED".to_owned());
            list.presentation_released = row.try_get("released").unwrap_or(false);
            let identifier: Option<String> = row.try_get("identifier").ok();
            let Some(identifier) = identifier else {
                // A challenge row without its identifier cannot be presented; the
                // join is LEFT so the order itself still reports its mode.
                continue;
            };
            list.items
                .push(CertificateChallengePresentationResponse {
                    record_type: row
                        .try_get::<Option<String>, _>("presentation_record_name")
                        .ok()
                        .flatten()
                        .map(|_| "TXT".to_owned()),
                    record_name: row.try_get("presentation_record_name").unwrap_or(None),
                    record_value: row.try_get("presentation_record_value").unwrap_or(None),
                    expires_at: optional_datetime(&row, "presentation_expires_at")?,
                    status: row.try_get("status").unwrap_or_default(),
                    identifier,
                });
        }
        Ok(list)
    }

    /// The presentation mode recorded on a certificate.
    ///
    /// `None` when the certificate does not exist for this tenant, so the caller
    /// can tell "this tenant has no such certificate" apart from "the certificate
    /// exists and is managed".
    async fn certificate_presentation_mode_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<Option<String>> {
        let row = sqlx::query(
            "SELECT presentation_mode FROM deploy_certificate
              WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(certificate_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read deploy_certificate presentation mode", error))?;
        Ok(row.and_then(|row| row.try_get::<String, _>("presentation_mode").ok()))
    }

    /// Releases the parked manual order of one certificate.
    ///
    /// Keyed by certificate rather than order id because the console acts on a
    /// certificate; the parked order is a detail. Returns whether this call was
    /// the one that released it, so a double click is a retry and not an error.
    pub(super) async fn release_manual_order_for_certificate_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        ready_at: &str,
    ) -> DeployServiceResult<bool> {
        let result = sqlx::query(
            "UPDATE deploy_certificate_order o
                SET presentation_ready_at = CAST($3 AS TIMESTAMPTZ),
                    next_attempt_at = NULL,
                    updated_at = NOW(), version = o.version + 1
               FROM deploy_certificate c
              WHERE c.id = o.certificate_id
                AND c.uuid = $2
                AND o.tenant_id = $1
                AND o.presentation_mode = 'MANUAL'
                AND o.status = 'CHALLENGE_PRESENTING'
                AND o.presentation_ready_at IS NULL",
        )
        .bind(tenant_id)
        .bind(certificate_id)
        .bind(ready_at)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("release manual deploy_certificate_order", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Stores the issued certificate version (CANDIDATE), activates it, writes
    /// the sealed material files, and advances the order to VERSION_STORED in one
    /// transaction.
    ///
    /// `certificate_version_uuid` arrives from the caller because the material was
    /// already sealed under it — the AAD binds each file to that uuid — so this
    /// function must not mint its own.
    ///
    /// The version row and its material rows are written in the same transaction,
    /// which is what makes the conflict path below safe: a version that exists
    /// therefore already has its material, because the only way to create one is
    /// to commit both.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn store_certificate_version_repo(
        &self,
        tenant_id: i64,
        certificate_version_uuid: &str,
        order_id: &str,
        version_no: i64,
        serial_sha256: &str,
        fingerprint_sha256: &str,
        spki_sha256: &str,
        chain_sha256: &str,
        issuer: &str,
        subject: &str,
        key_algorithm: &str,
        not_before: &str,
        not_after: &str,
        secret_bundle_ref: &str,
        material: &[SealedCertificateFile],
    ) -> DeployServiceResult<CertificateOrderResponse> {
        let order_internal_id =
            resolve_certificate_order_internal_id(&self.pool, tenant_id, order_id).await?;
        let order_row = sqlx::query(
            "SELECT o.certificate_id, o.uuid, o.status FROM deploy_certificate_order o
             WHERE o.id = $1",
        )
        .bind(order_internal_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve order for version storage", error))?;
        let Some(order_row) = order_row else {
            return Err(DeployServiceError::not_found("certificate order not found"));
        };
        let certificate_internal_id: i64 = order_row.try_get("certificate_id").unwrap_or(0);
        let order_uuid: String = order_row.try_get("uuid").unwrap_or_default();
        let order_status: String = order_row.try_get("status").unwrap_or_default();
        if order_status != ORDER_STATUS_FINALIZING {
            return Err(DeployServiceError::conflict(format!(
                "certificate order must be FINALIZING to store a version, got {order_status}"
            )));
        }

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin certificate version storage", error))?;
        let version_id = next_id(self.id_generator())?;
        let version_uuid = certificate_version_uuid.to_owned();
        let now = now_rfc3339();
        let inserted = sqlx::query(
            "INSERT INTO deploy_certificate_version
                (id, uuid, tenant_id, certificate_id, version_no, serial_sha256,
                 fingerprint_sha256, spki_sha256, chain_sha256, issuer, subject,
                 key_algorithm, not_before, not_after, secret_bundle_ref, source_order_id,
                 status, created_by, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                     CAST($13 AS TIMESTAMPTZ), CAST($14 AS TIMESTAMPTZ), $15, $16,
                     'CANDIDATE', 0, CAST($17 AS TIMESTAMPTZ))
             ON CONFLICT (certificate_id, version_no) DO NOTHING",
        )
        .bind(version_id)
        .bind(&version_uuid)
        .bind(tenant_id)
        .bind(certificate_internal_id)
        .bind(version_no)
        .bind(serial_sha256)
        .bind(fingerprint_sha256)
        .bind(spki_sha256)
        .bind(chain_sha256)
        .bind(issuer)
        .bind(subject)
        .bind(key_algorithm)
        .bind(not_before)
        .bind(not_after)
        .bind(secret_bundle_ref)
        .bind(order_internal_id)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy_certificate_version", error))?;
        // On conflict the version already exists, so it already carries its
        // material: both were committed by the same transaction on the first
        // attempt. Re-inserting would fail on the kind unique index, and
        // re-sealing is impossible here — the material arrived bound to the
        // uuid the caller chose.
        if inserted.rows_affected() > 0 {
            for file in material {
                let material_id = next_id(self.id_generator())?;
                let sealed = &file.sealed;
                let nonce = (!sealed.nonce.is_empty()).then(|| sealed.nonce.as_slice());
                let wrapped_dek =
                    (!sealed.wrapped_dek.is_empty()).then(|| sealed.wrapped_dek.as_slice());
                sqlx::query(
                    "INSERT INTO deploy_certificate_material
                        (id, uuid, tenant_id, certificate_version_id, material_kind, file_name,
                         media_type, protection, content, content_sha256, content_size_bytes,
                         nonce, aad, wrapped_dek, kek_ref, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                             CAST($16 AS TIMESTAMPTZ))",
                )
                .bind(material_id)
                .bind(new_uuid())
                .bind(tenant_id)
                .bind(version_id)
                .bind(file.kind.as_str())
                .bind(file.kind.file_name())
                .bind(file.kind.media_type())
                .bind(sealed.protection.as_str())
                .bind(sealed.content.as_slice())
                .bind(&sealed.content_sha256)
                .bind(sealed.content_size_bytes)
                .bind(nonce)
                .bind(sealed.aad.as_slice())
                .bind(wrapped_dek)
                .bind(sealed.kek_ref.as_deref())
                .bind(&now)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("insert deploy_certificate_material", error))?;
            }
            // Activate the new version and supersede the previous one.
            sqlx::query(
                "UPDATE deploy_certificate_version SET status = 'SUPERSEDED'
                 WHERE certificate_id = $1 AND version_no <> $2 AND status = 'ACTIVE'",
            )
            .bind(certificate_internal_id)
            .bind(version_no)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("supersede certificate version", error))?;
            sqlx::query(
                "UPDATE deploy_certificate_version SET status = 'ACTIVE'
                 WHERE id = $1",
            )
            .bind(version_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("activate certificate version", error))?;
            // The mirrored window is written here and only here, by the same
            // statement that moves `current_version_id`, so the two cannot drift:
            // the scheduler's index reads the mirror, and a mirror left behind by
            // a superseded version would renew against a window the certificate no
            // longer has.
            //
            // The lease and the retry schedule are cleared because this issuance —
            // whoever asked for it — resolves whatever renewal was in flight.
            sqlx::query(
                "UPDATE deploy_certificate
                    SET current_version_id = $1, renewal_status = 'NONE', status = 'ACTIVE',
                        active_not_before = CAST($2 AS TIMESTAMPTZ),
                        active_not_after = CAST($3 AS TIMESTAMPTZ),
                        renewal_lease_owner = NULL, renewal_lease_expires_at = NULL,
                        renewal_next_attempt_at = NULL, renewal_failure_count = 0,
                        updated_at = NOW(), version = version + 1
                 WHERE id = $4 AND deleted_at IS NULL",
            )
            .bind(version_id)
            .bind(not_before)
            .bind(not_after)
            .bind(certificate_internal_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("update certificate current version", error))?;

            // Close the ledger row this issuance satisfies, if there is one. A
            // `PLANNED` row means the worker claimed the certificate but died
            // before the order existed, and that is worth recording as a success
            // rather than leaving it open to be retired as lost.
            let closed = sqlx::query(
                "UPDATE deploy_certificate_renewal
                    SET status = $1, resulting_version_id = $2,
                        new_not_before = CAST($3 AS TIMESTAMPTZ),
                        new_not_after = CAST($4 AS TIMESTAMPTZ),
                        finished_at = NOW(), lease_owner = NULL, lease_expires_at = NULL,
                        updated_at = NOW(), version = version + 1
                  WHERE certificate_id = $5 AND status IN ('PLANNED', 'ORDERED')",
            )
            .bind(RENEWAL_STATUS_SUCCEEDED)
            .bind(version_id)
            .bind(not_before)
            .bind(not_after)
            .bind(certificate_internal_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("close certificate renewal attempt", error))?;

            // Only a fulfilled renewal advances `last_renewal_at`. The first
            // issuance writes the same certificate fields but is not a renewal,
            // and a console that labelled it one would misreport the certificate's
            // history on the day it was created.
            if closed.rows_affected() > 0 {
                sqlx::query(
                    "UPDATE deploy_certificate SET last_renewal_at = CAST($1 AS TIMESTAMPTZ)
                     WHERE id = $2",
                )
                .bind(&now)
                .bind(certificate_internal_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("record certificate last renewal", error))?;
            }
        }
        sqlx::query(
            "UPDATE deploy_certificate_order
                SET status = $1, updated_at = NOW(), version = version + 1
             WHERE id = $2",
        )
        .bind(ORDER_STATUS_VERSION_STORED)
        .bind(order_internal_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("complete certificate order", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit certificate version storage", error))?;
        self.retrieve_certificate_order_internal_repo(tenant_id, &order_uuid)
            .await
    }

    /// Claims one due archive row and marks it `STORING`.
    ///
    /// `FOR UPDATE SKIP LOCKED` is what lets several workers drain the same table
    /// without blocking each other: each takes a role it can act on, and the
    /// others move on instead of queueing behind it. The row is selected with the
    /// widest due predicate the index supports (`PENDING`, `STORING` — a row left
    /// claimed by a worker that died — and `FAILED`, which the console's retry
    /// moves back), and the update is fenced on the exact status that was read, so
    /// a row that changed between select and update is not stolen.
    ///
    /// `exclude_version_id` is how the inline attempt avoids racing its own drain:
    /// the worker that just wrote the row is about to handle it, and its sibling
    /// tick should leave that one row alone.
    pub(super) async fn claim_due_certificate_archive_repo(
        &self,
        now: &str,
        exclude_version_id: Option<&str>,
    ) -> DeployServiceResult<Option<CertificateArchiveClaim>> {
        let row = sqlx::query(
            "WITH candidate AS (
                 SELECT a.id, a.status
                   FROM deploy_certificate_archive a
                  WHERE a.status IN ('PENDING', 'STORING', 'FAILED')
                    AND (a.next_attempt_at IS NULL
                         OR a.next_attempt_at <= CAST($1 AS TIMESTAMPTZ))
                    AND ($2::VARCHAR(36) IS NULL
                         OR a.uuid <> $2)
                  ORDER BY a.next_attempt_at NULLS FIRST, a.id
                  LIMIT 1
                    FOR UPDATE OF a SKIP LOCKED
             )
             UPDATE deploy_certificate_archive a
                SET status = 'STORING', updated_at = NOW(), version = a.version + 1
               FROM candidate
              WHERE a.id = candidate.id
                AND a.status = candidate.status
             RETURNING a.id, a.tenant_id, a.certificate_id, a.certificate_version_id,
                       a.bundle_kind, a.attempt_count",
        )
        .bind(now)
        .bind(exclude_version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("claim deploy_certificate_archive", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        // The claim names only internal ids; the upload needs the certificate's
        // and version's public identity plus the version's facts, so they are read
        // here rather than by the caller in a second round trip that could observe
        // a superseded version.
        let details = sqlx::query(
            "SELECT c.uuid AS certificate_uuid, c.cert_name, c.ca_profile,
                    v.uuid AS certificate_version_uuid, v.version_no,
                    v.issuer, v.subject, v.key_algorithm, v.not_before, v.not_after,
                    v.serial_sha256, v.fingerprint_sha256, v.spki_sha256, v.chain_sha256,
                    o.uuid AS order_uuid,
                    COALESCE((
                        SELECT jsonb_agg(ci.hostname_ascii ORDER BY ci.position)
                          FROM deploy_certificate_identifier ci
                         WHERE ci.certificate_id = c.id
                    ), '[]'::jsonb) AS identifiers
               FROM deploy_certificate c
               JOIN deploy_certificate_version v ON v.id = $2
               LEFT JOIN deploy_certificate_order o ON o.id = v.source_order_id
              WHERE c.id = $1",
        )
        .bind(row.try_get::<i64, _>("certificate_id").unwrap_or_default())
        .bind(
            row.try_get::<i64, _>("certificate_version_id")
                .unwrap_or_default(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("read certificate archive claim details", error))?;
        let Some(details) = details else {
            // The claim refers to a version that no longer exists. Nothing can be
            // archived, so the row is settled as failed rather than left claimed.
            self.record_certificate_archive_failure_repo(
                row.try_get::<i64, _>("tenant_id").unwrap_or_default(),
                row.try_get::<i64, _>("id").unwrap_or_default(),
                "CERTIFICATE_ARCHIVE_VERSION_MISSING",
                1,
                None,
                CertificateArchiveStatus::Failed,
            )
            .await?;
            return Ok(None);
        };
        let identifiers = serde_json::from_value::<Vec<String>>(
            details
                .try_get("identifiers")
                .unwrap_or(serde_json::Value::Array(Vec::new())),
        )
        .unwrap_or_default();
        Ok(Some(CertificateArchiveClaim {
            archive_row_id: row.try_get("id").unwrap_or_default(),
            tenant_id: row.try_get("tenant_id").unwrap_or_default(),
            certificate_id: row.try_get::<i64, _>("certificate_id").unwrap_or_default().to_string(),
            certificate_uuid: details.try_get("certificate_uuid").unwrap_or_default(),
            certificate_version_id: row
                .try_get::<i64, _>("certificate_version_id")
                .unwrap_or_default()
                .to_string(),
            certificate_version_uuid: details
                .try_get("certificate_version_uuid")
                .unwrap_or_default(),
            bundle_kind: row.try_get("bundle_kind").unwrap_or_default(),
            cert_name: details.try_get("cert_name").unwrap_or_default(),
            ca_profile: details.try_get("ca_profile").unwrap_or_default(),
            version_no: details.try_get("version_no").unwrap_or_default(),
            issuer: details.try_get("issuer").unwrap_or_default(),
            subject: details.try_get("subject").unwrap_or_default(),
            key_algorithm: details.try_get("key_algorithm").unwrap_or_default(),
            serial_sha256: details.try_get("serial_sha256").unwrap_or_default(),
            fingerprint_sha256: details.try_get("fingerprint_sha256").unwrap_or_default(),
            spki_sha256: details.try_get("spki_sha256").unwrap_or_default(),
            chain_sha256: details.try_get("chain_sha256").unwrap_or_default(),
            not_before: required_datetime(&details, "not_before")?,
            not_after: required_datetime(&details, "not_after")?,
            order_uuid: details.try_get("order_uuid").unwrap_or_default(),
            identifiers,
            attempt_count: row.try_get("attempt_count").unwrap_or(0),
        }))
    }

    /// Marks one archive row `ARCHIVED` and records where the files landed.
    ///
    /// Fenced on `STORING`: a worker whose claim expired must not overwrite the
    /// result of the worker that took the row after it.
    pub(super) async fn mark_certificate_archive_archived_repo(
        &self,
        tenant_id: i64,
        certificate_version_uuid: &str,
        outcome: &CertificateArchiveOutcome,
    ) -> DeployServiceResult<bool> {
        let result = sqlx::query(
            "UPDATE deploy_certificate_archive a
                SET status = 'ARCHIVED',
                    drive_space_id = $3,
                    drive_folder_node_id = $4,
                    drive_manifest_node_id = $5,
                    manifest_sha256 = $6,
                    next_attempt_at = NULL,
                    last_error_code = NULL,
                    updated_at = NOW(),
                    version = a.version + 1
               FROM deploy_certificate_version v
              WHERE v.id = a.certificate_version_id
                AND v.uuid = $2
                AND a.tenant_id = $1
                AND a.status = 'STORING'",
        )
        .bind(tenant_id)
        .bind(certificate_version_uuid)
        .bind(&outcome.space_id)
        .bind(&outcome.certificate_folder_node_id)
        .bind(&outcome.manifest_node_id)
        .bind(&outcome.manifest_sha256)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("record archived deploy_certificate_archive", error))?;
        // The version folder is part of the Drive location an operator opens; the
        // table keeps the certificate folder on `drive_folder_node_id` and the
        // manifest node separately, so the version folder is not a column of its
        // own. It travels in the outcome for the log line and for the response
        // shape that a follow-up will expose.
        Ok(result.rows_affected() > 0)
    }

    /// Records a failed archive attempt on the row the caller holds.
    pub(super) async fn record_certificate_archive_failure_repo(
        &self,
        tenant_id: i64,
        archive_row_id: i64,
        error_code: &str,
        attempt_count: i32,
        next_attempt_at: Option<&str>,
        status: CertificateArchiveStatus,
    ) -> DeployServiceResult<CertificateArchiveStatus> {
        let result = sqlx::query(
            "UPDATE deploy_certificate_archive
                SET status = $3,
                    attempt_count = $4,
                    next_attempt_at = CAST($5 AS TIMESTAMPTZ),
                    last_error_code = $6,
                    updated_at = NOW(),
                    version = version + 1
              WHERE id = $1 AND tenant_id = $2 AND status = 'STORING'",
        )
        .bind(archive_row_id)
        .bind(tenant_id)
        .bind(status.as_str())
        .bind(attempt_count)
        .bind(next_attempt_at)
        .bind(error_code)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("record deploy_certificate_archive failure", error))?;
        if result.rows_affected() == 0 {
            return Err(DeployServiceError::conflict(
                "certificate archive row is no longer held by this worker",
            ));
        }
        Ok(status)
    }

    /// Re-arms the archive of a certificate's served version.
    ///
    /// Clears the backoff and the recorded error without touching
    /// `attempt_count`: the count is how many attempts this control plane made,
    /// and resetting it would misreport that history. `ARCHIVED` and `SKIPPED`
    /// rows are left alone — there is nothing to retry — which is what makes the
    /// command idempotent.
    pub(super) async fn retry_certificate_archive_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        now: &str,
    ) -> DeployServiceResult<bool> {
        let result = sqlx::query(
            "UPDATE deploy_certificate_archive a
                SET status = 'PENDING',
                    next_attempt_at = NULL,
                    last_error_code = NULL,
                    updated_at = NOW(),
                    version = a.version + 1
               FROM deploy_certificate c
              WHERE c.id = a.certificate_id
                AND c.tenant_id = $1
                AND c.uuid = $2
                AND c.deleted_at IS NULL
                AND a.certificate_version_id = c.current_version_id
                AND a.status IN ('PENDING', 'STORING', 'FAILED')
                AND a.updated_at <= CAST($3 AS TIMESTAMPTZ)",
        )
        .bind(tenant_id)
        .bind(certificate_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("retry deploy_certificate_archive", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Marks the archive row of one version `SKIPPED`.
    ///
    /// Fenced on a non-final status so a row another process already archived — or
    /// already skipped — is left as it is.
    pub(super) async fn skip_certificate_archive_repo(
        &self,
        tenant_id: i64,
        certificate_version_uuid: &str,
        reason_code: &str,
    ) -> DeployServiceResult<()> {
        sqlx::query(
            "UPDATE deploy_certificate_archive a
                SET status = 'SKIPPED',
                    last_error_code = $3,
                    next_attempt_at = NULL,
                    updated_at = NOW(),
                    version = a.version + 1
               FROM deploy_certificate_version v
              WHERE v.id = a.certificate_version_id
                AND v.uuid = $2
                AND a.tenant_id = $1
                AND a.status IN ('PENDING', 'STORING')",
        )
        .bind(tenant_id)
        .bind(certificate_version_uuid)
        .bind(reason_code)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("skip deploy_certificate_archive", error))?;
        Ok(())
    }

    /// Reads the sealed files of one certificate version, in bundle order.
    ///
    /// The rows come back still sealed. Opening them belongs to the service,
    /// which holds the custody key; keeping it out of here means the component
    /// that runs SQL never needs to be trusted with the ability to read private
    /// keys.
    pub(super) async fn retrieve_certificate_material_repo(
        &self,
        tenant_id: i64,
        certificate_version_uuid: &str,
    ) -> DeployServiceResult<Vec<SealedCertificateFile>> {
        let rows = sqlx::query(
            "SELECT m.material_kind, m.protection, m.content, m.nonce, m.aad, m.wrapped_dek,
                    m.kek_ref, m.content_sha256, m.content_size_bytes
               FROM deploy_certificate_material m
               JOIN deploy_certificate_version v ON v.id = m.certificate_version_id
              WHERE v.uuid = $1 AND m.tenant_id = $2",
        )
        .bind(certificate_version_uuid)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("retrieve deploy_certificate_material", error))?;

        let mut material = Vec::with_capacity(rows.len());
        for row in rows {
            let kind: String = row.try_get("material_kind").unwrap_or_default();
            let protection: String = row.try_get("protection").unwrap_or_default();
            let kind = MaterialKind::from_db(&kind).map_err(|error| {
                DeployServiceError::Internal(format!(
                    "stored certificate material is invalid: {error}"
                ))
            })?;
            let protection = Protection::from_db(&protection).map_err(|error| {
                DeployServiceError::Internal(format!(
                    "stored certificate material is invalid: {error}"
                ))
            })?;
            material.push(SealedCertificateFile {
                kind,
                sealed: SealedMaterial {
                    protection,
                    content: row.try_get("content").unwrap_or_default(),
                    nonce: row.try_get("nonce").unwrap_or_default(),
                    aad: row.try_get("aad").unwrap_or_default(),
                    wrapped_dek: row.try_get("wrapped_dek").unwrap_or_default(),
                    kek_ref: row.try_get("kek_ref").unwrap_or_default(),
                    content_sha256: row.try_get("content_sha256").unwrap_or_default(),
                    content_size_bytes: row.try_get("content_size_bytes").unwrap_or_default(),
                },
            });
        }
        // The table returns rows in whatever order it likes; the bundle has a
        // canonical one, and a caller that loads files by position would otherwise
        // depend on the query plan.
        material.sort_by_key(|file| file.kind.order());
        Ok(material)
    }

    pub(super) async fn retrieve_certificate_order_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
    ) -> DeployServiceResult<CertificateOrderResponse> {
        self.retrieve_certificate_order_internal_repo(tenant_id, order_id)
            .await
    }

    pub(super) async fn list_certificate_orders_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateOrderPage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let certificate_internal_id =
            resolve_certificate_internal_id_light(&self.pool, tenant_id, certificate_id).await?;
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_certificate_order
             WHERE tenant_id = $1 AND certificate_id = $2",
        )
        .bind(tenant_id)
        .bind(certificate_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count certificate orders", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT o.uuid, o.tenant_id, c.uuid AS certificate_uuid, a.uuid AS account_uuid,
                    o.requested_version_no, o.request_sha256, o.idempotency_key,
                    o.external_order_digest, o.status, o.attempt_count, o.last_error_code,
                    o.caa_decision, o.caa_checked_at,
                    o.deadline_at, o.created_at, o.updated_at, o.version
             FROM deploy_certificate_order o
             JOIN deploy_certificate c ON c.id = o.certificate_id
             JOIN deploy_acme_account a ON a.id = o.acme_account_id
             WHERE o.tenant_id = $1 AND o.certificate_id = $2
             ORDER BY o.created_at DESC, o.id DESC LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(certificate_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list certificate orders", error))?;

        let items = rows
            .iter()
            .map(map_certificate_order_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CertificateOrderPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn list_certificate_challenges_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateChallengePage> {
        let (page, page_size, offset) = pagination(page, page_size);
        let order_internal_id =
            resolve_certificate_order_internal_id(&self.pool, tenant_id, order_id).await?;
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM deploy_certificate_challenge
             WHERE tenant_id = $1 AND order_id = $2",
        )
        .bind(tenant_id)
        .bind(order_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count certificate challenges", error))?;
        let total: i64 = count_row.try_get("total").unwrap_or(0);

        let rows = sqlx::query(
            "SELECT ch.uuid, ch.tenant_id, o.uuid AS order_uuid, i.uuid AS identifier_uuid,
                    i.hostname_ascii, ch.challenge_type, ch.proof_sha256, ch.presentation_ref,
                    ch.presentation_record_name, ch.presentation_record_value,
                    ch.presentation_expires_at,
                    ch.status, ch.attempt_count, ch.checked_at, ch.validated_at,
                    ch.last_error_code, ch.created_at, ch.updated_at, ch.version
             FROM deploy_certificate_challenge ch
             JOIN deploy_certificate_order o ON o.id = ch.order_id
             JOIN deploy_certificate_identifier i ON i.id = ch.identifier_id
             WHERE ch.tenant_id = $1 AND ch.order_id = $2
             ORDER BY ch.id ASC LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(order_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list certificate challenges", error))?;

        let items = rows
            .iter()
            .map(map_certificate_challenge_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CertificateChallengePage {
            items,
            total,
            page,
            page_size,
        })
    }

    async fn retrieve_certificate_order_internal_repo(
        &self,
        tenant_id: i64,
        order_id: &str,
    ) -> DeployServiceResult<CertificateOrderResponse> {
        let row = sqlx::query(
            "SELECT o.uuid, o.tenant_id, c.uuid AS certificate_uuid, a.uuid AS account_uuid,
                    o.requested_version_no, o.request_sha256, o.idempotency_key,
                    o.external_order_digest, o.status, o.attempt_count, o.last_error_code,
                    o.caa_decision, o.caa_checked_at,
                    o.deadline_at, o.created_at, o.updated_at, o.version
             FROM deploy_certificate_order o
             JOIN deploy_certificate c ON c.id = o.certificate_id
             JOIN deploy_acme_account a ON a.id = o.acme_account_id
             WHERE o.tenant_id = $1 AND o.uuid = $2",
        )
        .bind(tenant_id)
        .bind(order_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve certificate order", error))?;
        let Some(row) = row else {
            return Err(DeployServiceError::not_found("certificate order not found"));
        };
        map_certificate_order_row(&row)
    }
}

async fn resolve_acme_account_internal_id(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_uuid: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_acme_account WHERE uuid = $1 AND status = 'ACTIVE' AND deleted_at IS NULL",
    )
    .bind(account_uuid)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| store_error("resolve acme account id", error))?;
    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("acme account not found"))
}

async fn resolve_certificate_internal_id_light(
    pool: &sqlx::PgPool,
    tenant_id: i64,
    certificate_id: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_certificate
         WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(certificate_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve certificate id", error))?;
    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("certificate not found"))
}

async fn resolve_certificate_order_internal_id(
    pool: &sqlx::PgPool,
    tenant_id: i64,
    order_id: &str,
) -> Result<i64, DeployServiceError> {
    let row =
        sqlx::query("SELECT id FROM deploy_certificate_order WHERE tenant_id = $1 AND uuid = $2")
            .bind(tenant_id)
            .bind(order_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| store_error("resolve certificate order id", error))?;
    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("certificate order not found"))
}

fn map_acme_account_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AcmeAccountResponse, DeployServiceError> {
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    let version: i64 = row.try_get("version").unwrap_or(1);
    Ok(AcmeAccountResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        tenant_id: row.try_get("tenant_id").unwrap_or(0),
        ca_profile: row.try_get("ca_profile").unwrap_or_default(),
        directory_url: row.try_get("directory_url").unwrap_or_default(),
        contact_email: row.try_get("contact_email").unwrap_or_default(),
        external_account_digest: row.try_get("external_account_digest").ok(),
        status: row.try_get("status").unwrap_or_default(),
        created_at,
        updated_at,
        version: version.to_string(),
    })
}

fn map_certificate_order_row(
    row: &sqlx::postgres::PgRow,
) -> Result<CertificateOrderResponse, DeployServiceError> {
    let deadline_at = required_datetime(row, "deadline_at")?;
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    let version: i64 = row.try_get("version").unwrap_or(1);
    Ok(CertificateOrderResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        tenant_id: row.try_get("tenant_id").unwrap_or(0),
        certificate_id: row.try_get("certificate_uuid").unwrap_or_default(),
        acme_account_id: row.try_get("account_uuid").unwrap_or_default(),
        requested_version_no: row.try_get("requested_version_no").unwrap_or(0),
        request_sha256: row.try_get("request_sha256").unwrap_or_default(),
        idempotency_key: row.try_get("idempotency_key").unwrap_or_default(),
        external_order_digest: row.try_get("external_order_digest").ok(),
        status: row.try_get("status").unwrap_or_default(),
        attempt_count: row.try_get("attempt_count").unwrap_or(0),
        last_error_code: row.try_get("last_error_code").ok(),
        caa_decision: row.try_get("caa_decision").ok(),
        caa_checked_at: optional_datetime(row, "caa_checked_at")?,
        deadline_at,
        created_at,
        updated_at,
        version: version.to_string(),
    })
}

fn map_certificate_challenge_row(
    row: &sqlx::postgres::PgRow,
) -> Result<CertificateChallengeResponse, DeployServiceError> {
    let checked_at = optional_datetime(row, "checked_at")?;
    let validated_at = optional_datetime(row, "validated_at")?;
    let created_at = required_datetime(row, "created_at")?;
    let updated_at = required_datetime(row, "updated_at")?;
    let version: i64 = row.try_get("version").unwrap_or(1);
    Ok(CertificateChallengeResponse {
        id: row.try_get("uuid").unwrap_or_default(),
        tenant_id: row.try_get("tenant_id").unwrap_or(0),
        order_id: row.try_get("order_uuid").unwrap_or_default(),
        identifier_id: row.try_get("identifier_uuid").unwrap_or_default(),
        hostname: row.try_get("hostname_ascii").unwrap_or_default(),
        challenge_type: row.try_get("challenge_type").unwrap_or_default(),
        proof_sha256: row.try_get("proof_sha256").unwrap_or_default(),
        presentation_ref: row.try_get("presentation_ref").ok(),
        dns_record_name: row.try_get("presentation_record_name").ok(),
        dns_record_type: row
            .try_get::<Option<String>, _>("presentation_record_name")
            .ok()
            .flatten()
            .map(|_| sdkwork_deploy_contract::DNS01_RECORD_TYPE.to_owned()),
        dns_record_value: row.try_get("presentation_record_value").ok(),
        presentation_expires_at: optional_datetime(row, "presentation_expires_at")?,
        status: row.try_get("status").unwrap_or_default(),
        attempt_count: row.try_get("attempt_count").unwrap_or(0),
        checked_at,
        validated_at,
        last_error_code: row.try_get("last_error_code").ok(),
        created_at,
        updated_at,
        version: version.to_string(),
    })
}

#[cfg(test)]
mod tls_state_machine_tests {
    use sdkwork_deploy_contract::next_order_transition;

    #[test]
    fn order_state_machine_is_forward_only_and_terminates() {
        assert_eq!(next_order_transition("REQUESTED"), Some("ACCOUNT_READY"));
        assert_eq!(
            next_order_transition("ACCOUNT_READY"),
            Some("ORDER_PENDING")
        );
        assert_eq!(
            next_order_transition("ORDER_PENDING"),
            Some("CHALLENGE_PRESENTING")
        );
        assert_eq!(
            next_order_transition("CHALLENGE_PRESENTING"),
            Some("CHALLENGE_VALIDATING")
        );
        assert_eq!(
            next_order_transition("CHALLENGE_VALIDATING"),
            Some("FINALIZING")
        );
        // Terminal states have no forward transition.
        assert_eq!(next_order_transition("FINALIZING"), None);
        assert_eq!(next_order_transition("VERSION_STORED"), None);
        assert_eq!(next_order_transition("FAILED"), None);
        assert_eq!(next_order_transition("CANCELLED"), None);
        // Unknown states never advance.
        assert_eq!(next_order_transition("GARBAGE"), None);
    }

    #[test]
    fn terminal_statuses_are_recognized() {
        for terminal in ["VERSION_STORED", "FAILED", "CANCELLED"] {
            assert_eq!(next_order_transition(terminal), None);
        }
    }
}
