//! Renewal ledger storage: claiming due certificates, advancing attempts, and
//! retiring certificates whose window has closed.
//!
//! Claiming lives in SQL rather than in the service because it is a mutual
//! exclusion problem, not a business rule. Two workers reading the same due list
//! and then each opening an order would issue the same certificate twice, and a
//! duplicate issuance is not merely wasteful: it spends the tenant's shared CA
//! rate limit, which is what every *other* certificate under that account
//! competes for. So the lease is taken in the same transaction that records the
//! attempt, and a certificate another worker already holds is skipped rather than
//! reported as a failure.
//!
//! Whether a certificate is due, on the other hand, *is* a business rule, and it
//! has exactly one implementation
//! ([`sdkwork_intelligence_deploy_service::certificate_renewal`]). The SQL only
//! narrows the candidate set with conditions that are provably necessary for the
//! rule to hold; the decision comes from calling the same code the console calls
//! when it renders `daysUntilExpiry`.

use chrono::{DateTime, Utc};
use sdkwork_deploy_contract::{
    CertificateRenewalPage, CertificateRenewalResponse, DeployServiceError, DeployServiceResult,
    RENEWAL_STATUS_CANCELLED, RENEWAL_STATUS_FAILED, RENEWAL_STATUS_ORDERED,
    RENEWAL_STATUS_PLANNED, RENEWAL_TRIGGER_MANUAL, RENEWAL_TRIGGER_SCHEDULED,
};
use sdkwork_intelligence_deploy_service::certificate_renewal::{
    renewal_retry_delay, ValidityWindow, MAXIMUM_RENEWAL_FAILURE_COUNT, RENEWAL_OVERDUE_GRACE_DAYS,
    SWEEP_LOOKAHEAD_DAYS,
};
use sdkwork_intelligence_deploy_service::{CertificateRenewalClaim, ExpiredCertificateSweep};
use sqlx::postgres::PgRow;
use sqlx::Row;

use crate::support::{
    datetime_from_row, new_uuid, next_id, optional_datetime, optional_datetime_from_row,
    pagination, store_error,
};
use crate::DeployRepository;

/// Renewal status meaning "a worker holds this certificate".
const CERTIFICATE_RENEWAL_PROCESSING: &str = "PROCESSING";

/// Renewal status meaning "nothing is in flight".
const CERTIFICATE_RENEWAL_IDLE: &str = "NONE";

/// Recorded when a worker claimed an attempt and then vanished before it could
/// open an order.
///
/// The attempt is not silently deleted: an operator reading the ledger should see
/// that automation tried and was interrupted, not that nothing ever happened.
const ERROR_RENEWAL_WORKER_LOST: &str = "RENEWAL_WORKER_LOST";

/// Renewal status of one certificate version, as stored.
const VERSION_STATUS_EXPIRED: &str = "EXPIRED";

/// Candidates for the sweep: certificates that may be due, plus certificates
/// carrying work that still needs an order.
///
/// The lease is what makes "only one worker renews this" true, so only a row whose
/// previous lease has lapsed is selectable and a live worker is never preempted.
const CANDIDATE_SELECT: &str = "SELECT c.id, c.uuid, c.tenant_id, c.renew_before_days,
        c.current_version_id, c.renewal_status,
        v.uuid AS current_version_uuid,
        COALESCE(v.not_before, c.active_not_before) AS window_start,
        COALESCE(v.not_after, c.active_not_after) AS window_end
   FROM deploy_certificate c
   LEFT JOIN deploy_certificate_version v ON v.id = c.current_version_id
  WHERE c.auto_renew = TRUE
    AND c.status IN ('ACTIVE', 'EXPIRED')
    AND c.certificate_source = 'MANAGED'
    AND c.deleted_at IS NULL
    AND c.active_not_after IS NOT NULL
    AND (c.renewal_lease_expires_at IS NULL
         OR c.renewal_lease_expires_at <= CAST($1 AS TIMESTAMPTZ))
    AND (c.renewal_next_attempt_at IS NULL
         OR c.renewal_next_attempt_at <= CAST($1 AS TIMESTAMPTZ))
    -- An attempt whose order is already at the CA is real work in flight and is
    -- left alone. An attempt merely recorded but not yet ordered is *not*: a
    -- worker that died mid-claim would otherwise strand the certificate forever.
    AND NOT EXISTS (
        SELECT 1 FROM deploy_certificate_renewal r
         WHERE r.certificate_id = c.id AND r.status = 'ORDERED'
    )
    AND (
        -- Work already pending: an operator asked for this, or an earlier attempt
        -- never reached a terminal state. Neither is second-guessed by the window
        -- rule -- a manual request on a far-from-expiry certificate is a
        -- legitimate act, and a retry has already been decided.
        c.renewal_status IN ('PLANNED', 'PROCESSING')
        OR (
            -- Coarse window, and the only part of this query an index can serve:
            -- the partial index on (active_not_after, id) turns it into a bounded
            -- range scan instead of reading every certificate.
            c.active_not_after >= CAST($1 AS TIMESTAMPTZ) - make_interval(days => $2)
            AND c.active_not_after <= CAST($1 AS TIMESTAMPTZ) + make_interval(days => $3)
            -- Necessary for renewal to be due, and not itself a decision: the
            -- rule's lifetime floor only ever moves the due instant later, so a
            -- certificate failing this bound could never have been due.
            AND c.active_not_after <= CAST($1 AS TIMESTAMPTZ)
                                      + (c.renew_before_days * INTERVAL '1 day')
        )
    )
  ORDER BY c.active_not_after, c.id
  LIMIT $4";

/// Everything one claim needs beyond the candidate row.
struct ClaimInput<'a> {
    certificate_internal_id: i64,
    certificate_uuid: &'a str,
    tenant_id: i64,
    renew_before_days: i32,
    previous_version_uuid: Option<String>,
    previous_not_before: Option<String>,
    previous_not_after: Option<String>,
    trigger_kind: &'a str,
    worker_id: &'a str,
    lease_seconds: i64,
    now: &'a str,
}

impl DeployRepository {
    pub(super) async fn claim_due_certificate_renewals_repo(
        &self,
        worker_id: &str,
        batch_size: i64,
        lease_seconds: i64,
        now: &str,
    ) -> DeployServiceResult<Vec<CertificateRenewalClaim>> {
        let claimed_at = parse_instant(now, "renewal sweep instant")?;

        let rows = sqlx::query(CANDIDATE_SELECT)
            .bind(now)
            .bind(RENEWAL_OVERDUE_GRACE_DAYS as i32)
            .bind(SWEEP_LOOKAHEAD_DAYS as i32)
            .bind(batch_size)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("select due deploy_certificate", error))?;

        let mut claims = Vec::new();
        for row in rows {
            let certificate_internal_id: i64 = row.try_get("id").unwrap_or(0);
            let renewal_status: String = row.try_get("renewal_status").unwrap_or_default();
            let renew_before_days: i32 = row.try_get("renew_before_days").unwrap_or(0);
            let previous_not_before = optional_datetime(&row, "window_start")?;
            let previous_not_after = optional_datetime(&row, "window_end")?;

            // A certificate carrying pending work is renewed because it was asked
            // for, not because the window says so. Everything else has to earn it
            // through the one rule.
            let carrying_pending_work = matches!(
                renewal_status.as_str(),
                RENEWAL_STATUS_PLANNED | CERTIFICATE_RENEWAL_PROCESSING
            );
            if !carrying_pending_work
                && !is_due_by_window(
                    previous_not_before.as_deref(),
                    previous_not_after.as_deref(),
                    renew_before_days,
                    claimed_at,
                )
            {
                continue;
            }

            // Bound as locals rather than read inside the literal below: a
            // temporary borrowed into `ClaimInput` would otherwise be dropped at
            // the end of the statement that builds it.
            let certificate_uuid: String = row
                .try_get("uuid")
                .map_err(|error| store_error("read certificate uuid for renewal", error))?;
            let previous_version_uuid: Option<String> =
                row.try_get("current_version_uuid").map_err(|error| {
                    store_error("read certificate current version for renewal", error)
                })?;

            let input = ClaimInput {
                certificate_internal_id,
                certificate_uuid: &certificate_uuid,
                tenant_id: row.try_get("tenant_id").unwrap_or(0),
                renew_before_days,
                previous_version_uuid,
                previous_not_before,
                previous_not_after,
                // Decided here, before the claim overwrites the status: `PLANNED`
                // can only have been set by an operator's request, so it is the
                // one signal that separates a manual renewal from a scheduled one.
                trigger_kind: if renewal_status == RENEWAL_STATUS_PLANNED {
                    RENEWAL_TRIGGER_MANUAL
                } else {
                    RENEWAL_TRIGGER_SCHEDULED
                },
                worker_id,
                lease_seconds,
                now,
            };
            if let Some(claim) = self.claim_renewal_attempt(&input).await? {
                claims.push(claim);
            }
            // A `None` here means the race was lost, which is the other worker
            // doing the right thing rather than an error to report.
        }
        Ok(claims)
    }

    /// Takes the certificate lease and records the attempt, or returns `None`
    /// when another worker got there first.
    ///
    /// One transaction, because a lease without a ledger row would leave the
    /// certificate blocked with nothing to show for it, while a ledger row without
    /// a lease would let a second worker open a second order.
    async fn claim_renewal_attempt(
        &self,
        input: &ClaimInput<'_>,
    ) -> DeployServiceResult<Option<CertificateRenewalClaim>> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin certificate renewal claim", error))?;

        let claimed = sqlx::query(
            "UPDATE deploy_certificate
                SET renewal_status = $1, renewal_lease_owner = $2,
                    renewal_lease_expires_at = CAST($3 AS TIMESTAMPTZ) + make_interval(secs => $4),
                    updated_at = NOW(), version = version + 1
              WHERE id = $5
                AND auto_renew = TRUE AND status IN ('ACTIVE', 'EXPIRED')
                AND certificate_source = 'MANAGED' AND deleted_at IS NULL
                AND (renewal_lease_expires_at IS NULL
                     OR renewal_lease_expires_at <= CAST($3 AS TIMESTAMPTZ))",
        )
        .bind(CERTIFICATE_RENEWAL_PROCESSING)
        .bind(input.worker_id)
        .bind(input.now)
        .bind(input.lease_seconds as f64)
        .bind(input.certificate_internal_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("claim deploy_certificate for renewal", error))?;
        if claimed.rows_affected() == 0 {
            transaction
                .rollback()
                .await
                .map_err(|error| store_error("rollback certificate renewal claim", error))?;
            return Ok(None);
        }

        // Retire an attempt left open by a worker that never came back. Doing this
        // before the insert is what stops "at most one open attempt" from turning
        // a crash into a permanent block.
        sqlx::query(
            "UPDATE deploy_certificate_renewal
                SET status = $1, last_error_code = $2, finished_at = NOW(),
                    lease_owner = NULL, lease_expires_at = NULL,
                    updated_at = NOW(), version = version + 1
              WHERE certificate_id = $3 AND status = 'PLANNED'
                AND (lease_expires_at IS NULL OR lease_expires_at <= CAST($4 AS TIMESTAMPTZ))",
        )
        .bind(RENEWAL_STATUS_FAILED)
        .bind(ERROR_RENEWAL_WORKER_LOST)
        .bind(input.certificate_internal_id)
        .bind(input.now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("retire abandoned certificate renewal attempt", error))?;

        let attempt_no: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(attempt_no), 0) + 1
               FROM deploy_certificate_renewal WHERE certificate_id = $1",
        )
        .bind(input.certificate_internal_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error("compute certificate renewal attempt number", error))?;

        let previous_version_id: Option<i64> = match input.previous_version_uuid.as_deref() {
            Some(uuid) => sqlx::query_scalar(
                "SELECT id FROM deploy_certificate_version
                      WHERE tenant_id = $1 AND uuid = $2",
            )
            .bind(input.tenant_id)
            .bind(uuid)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("resolve previous certificate version", error))?,
            None => None,
        };

        let renewal_uuid = new_uuid();
        let inserted = sqlx::query(
            "INSERT INTO deploy_certificate_renewal
                (id, uuid, tenant_id, certificate_id, trigger_kind, status, attempt_no,
                 previous_version_id, previous_not_before, previous_not_after,
                 scheduled_at, lease_owner, lease_expires_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                     CAST($9 AS TIMESTAMPTZ), CAST($10 AS TIMESTAMPTZ),
                     CAST($11 AS TIMESTAMPTZ), $12,
                     CAST($11 AS TIMESTAMPTZ) + make_interval(secs => $13), NOW(), NOW())
             ON CONFLICT DO NOTHING",
        )
        .bind(next_id(self.id_generator())?)
        .bind(&renewal_uuid)
        .bind(input.tenant_id)
        .bind(input.certificate_internal_id)
        .bind(input.trigger_kind)
        .bind(RENEWAL_STATUS_PLANNED)
        .bind(attempt_no)
        .bind(previous_version_id)
        .bind(input.previous_not_before.as_deref())
        .bind(input.previous_not_after.as_deref())
        .bind(input.now)
        .bind(input.worker_id)
        .bind(input.lease_seconds as f64)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("insert deploy_certificate_renewal", error))?;
        if inserted.rows_affected() == 0 {
            // An open attempt appeared between the candidate read and here, so the
            // other writer owns this certificate. Undo the lease we took rather
            // than leaving it held by nobody.
            transaction
                .rollback()
                .await
                .map_err(|error| store_error("rollback certificate renewal claim", error))?;
            return Ok(None);
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit certificate renewal claim", error))?;

        Ok(Some(CertificateRenewalClaim {
            tenant_id: input.tenant_id,
            renewal_uuid,
            certificate_uuid: input.certificate_uuid.to_owned(),
            attempt_no,
            renew_before_days: input.renew_before_days,
            previous_version_uuid: input.previous_version_uuid.clone(),
            previous_not_before: input.previous_not_before.clone(),
            previous_not_after: input.previous_not_after.clone(),
        }))
    }

    /// Links a claimed attempt to the order that will do the work.
    ///
    /// Tolerant of a missing update: a caller retrying after the order was
    /// already opened must not turn a successful ordering into an error.
    pub(super) async fn mark_certificate_renewal_ordered_repo(
        &self,
        tenant_id: i64,
        renewal_uuid: &str,
        order_uuid: &str,
    ) -> DeployServiceResult<()> {
        sqlx::query(
            "UPDATE deploy_certificate_renewal r
                SET status = $1, started_at = COALESCE(r.started_at, NOW()),
                    certificate_order_id = o.id,
                    lease_owner = NULL, lease_expires_at = NULL,
                    updated_at = NOW(), version = r.version + 1
               FROM deploy_certificate_order o
              WHERE r.tenant_id = $2 AND r.uuid = $3 AND r.status = 'PLANNED'
                AND o.tenant_id = $2 AND o.uuid = $4",
        )
        .bind(RENEWAL_STATUS_ORDERED)
        .bind(tenant_id)
        .bind(renewal_uuid)
        .bind(order_uuid)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("mark deploy_certificate_renewal ordered", error))?;
        Ok(())
    }

    /// Closes an attempt that could not be ordered, and schedules the retry.
    ///
    /// The backoff and the severity are written together because they are one
    /// decision: a certificate that has failed three times should both wait longer
    /// and read as more troubled, and separating them would let the delay say
    /// "first failure" while the count said otherwise.
    pub(super) async fn fail_certificate_renewal_repo(
        &self,
        tenant_id: i64,
        renewal_uuid: &str,
        error_code: &str,
    ) -> DeployServiceResult<()> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin certificate renewal failure", error))?;

        let closed = sqlx::query(
            "UPDATE deploy_certificate_renewal
                SET status = $1, last_error_code = $2, finished_at = NOW(),
                    lease_owner = NULL, lease_expires_at = NULL,
                    updated_at = NOW(), version = version + 1
              WHERE tenant_id = $3 AND uuid = $4 AND status IN ('PLANNED', 'ORDERED')
             RETURNING certificate_id",
        )
        .bind(RENEWAL_STATUS_FAILED)
        .bind(error_code)
        .bind(tenant_id)
        .bind(renewal_uuid)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("fail deploy_certificate_renewal", error))?;
        let Some(closed) = closed else {
            transaction
                .rollback()
                .await
                .map_err(|error| store_error("rollback certificate renewal failure", error))?;
            return Ok(());
        };
        let certificate_internal_id: i64 = closed.try_get("certificate_id").unwrap_or(0);

        // Locked while reading so two failures arriving together cannot both
        // compute the same next attempt and then write the count twice.
        let failure_count: i32 = sqlx::query_scalar(
            "SELECT renewal_failure_count FROM deploy_certificate
              WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
        )
        .bind(certificate_internal_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock deploy_certificate for renewal backoff", error))?
        .unwrap_or(0);
        let next_count = failure_count
            .saturating_add(1)
            .min(MAXIMUM_RENEWAL_FAILURE_COUNT);
        let retry_after = renewal_retry_delay(next_count).num_seconds();

        sqlx::query(
            "UPDATE deploy_certificate
                SET renewal_status = $1,
                    renewal_lease_owner = NULL, renewal_lease_expires_at = NULL,
                    renewal_failure_count = $2,
                    last_renewal_at = NOW(),
                    renewal_next_attempt_at = NOW() + make_interval(secs => $3),
                    updated_at = NOW(), version = version + 1
              WHERE id = $4 AND deleted_at IS NULL",
        )
        .bind(RENEWAL_STATUS_FAILED)
        .bind(next_count)
        .bind(retry_after as f64)
        .bind(certificate_internal_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("schedule certificate renewal retry", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit certificate renewal failure", error))?;
        Ok(())
    }

    /// Retires certificates and versions whose validity window has closed.
    ///
    /// A certificate still reported `ACTIVE` after `notAfter` is worse than one
    /// reported expired: every reader — the console, an alert rule, an operator
    /// weighing whether to intervene — takes it at face value. Open attempts are
    /// cancelled only once the certificate has been expired long enough that
    /// automatic recovery has been abandoned, so a replacement already in flight
    /// is not disturbed.
    pub(super) async fn sweep_expired_certificates_repo(
        &self,
        batch_size: i64,
        now: &str,
    ) -> DeployServiceResult<ExpiredCertificateSweep> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin certificate expiry sweep", error))?;

        let certificates = sqlx::query(
            "UPDATE deploy_certificate
                SET status = 'EXPIRED', renewal_status = $1,
                    renewal_lease_owner = NULL, renewal_lease_expires_at = NULL,
                    updated_at = NOW(), version = version + 1
              WHERE id IN (
                  SELECT id FROM deploy_certificate
                   WHERE status = 'ACTIVE' AND deleted_at IS NULL
                     AND active_not_after IS NOT NULL
                     AND active_not_after < CAST($2 AS TIMESTAMPTZ)
                   ORDER BY active_not_after, id
                   LIMIT $3
              )",
        )
        .bind(CERTIFICATE_RENEWAL_IDLE)
        .bind(now)
        .bind(batch_size)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("expire deploy_certificate", error))?;

        let versions = sqlx::query(
            "UPDATE deploy_certificate_version
                SET status = $1
              WHERE id IN (
                  SELECT id FROM deploy_certificate_version
                   WHERE status = 'ACTIVE' AND not_after < CAST($2 AS TIMESTAMPTZ)
                   ORDER BY not_after, id
                   LIMIT $3
              )",
        )
        .bind(VERSION_STATUS_EXPIRED)
        .bind(now)
        .bind(batch_size)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("expire deploy_certificate_version", error))?;

        let renewals = sqlx::query(
            "UPDATE deploy_certificate_renewal r
                SET status = $1, finished_at = NOW(),
                    lease_owner = NULL, lease_expires_at = NULL,
                    updated_at = NOW(), version = r.version + 1
              WHERE r.status IN ('PLANNED', 'ORDERED')
                AND EXISTS (
                    SELECT 1 FROM deploy_certificate c
                     WHERE c.id = r.certificate_id
                       AND c.status = 'EXPIRED'
                       AND c.active_not_after IS NOT NULL
                       AND c.active_not_after
                           < CAST($2 AS TIMESTAMPTZ) - make_interval(days => $3)
                )",
        )
        .bind(RENEWAL_STATUS_CANCELLED)
        .bind(now)
        .bind(RENEWAL_OVERDUE_GRACE_DAYS as i32)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("cancel unrecoverable certificate renewal", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit certificate expiry sweep", error))?;

        Ok(ExpiredCertificateSweep {
            certificates_expired: certificates.rows_affected() as i64,
            versions_expired: versions.rows_affected() as i64,
            renewals_cancelled: renewals.rows_affected() as i64,
        })
    }

    pub(super) async fn list_certificate_renewals_repo(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        page: i32,
        page_size: i32,
    ) -> DeployServiceResult<CertificateRenewalPage> {
        let (page, page_size, offset) = pagination(page, page_size);

        // Resolved first so an unknown certificate is a 404 rather than an empty
        // page, which an operator would read as "renewal was never attempted".
        let certificate_internal_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM deploy_certificate
              WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(certificate_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve deploy_certificate for renewal history", error))?;
        let Some(certificate_internal_id) = certificate_internal_id else {
            return Err(DeployServiceError::not_found("certificate not found"));
        };

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deploy_certificate_renewal
              WHERE tenant_id = $1 AND certificate_id = $2",
        )
        .bind(tenant_id)
        .bind(certificate_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count deploy_certificate_renewal", error))?;

        let rows = sqlx::query(
            "SELECT r.uuid, c.uuid AS certificate_uuid, r.trigger_kind, r.status, r.attempt_no,
                    pv.uuid AS previous_version_uuid, rv.uuid AS resulting_version_uuid,
                    r.previous_not_before, r.previous_not_after,
                    r.new_not_before, r.new_not_after,
                    r.scheduled_at, r.started_at, r.finished_at, r.last_error_code, r.created_at
               FROM deploy_certificate_renewal r
               JOIN deploy_certificate c ON c.id = r.certificate_id
               LEFT JOIN deploy_certificate_version pv ON pv.id = r.previous_version_id
               LEFT JOIN deploy_certificate_version rv ON rv.id = r.resulting_version_id
              WHERE r.tenant_id = $1 AND r.certificate_id = $2
              ORDER BY r.created_at DESC, r.id DESC
              LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(certificate_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list deploy_certificate_renewal", error))?;
        let items = rows
            .iter()
            .map(map_certificate_renewal_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                DeployServiceError::Internal(format!("map deploy_certificate_renewal row: {error}"))
            })?;

        Ok(CertificateRenewalPage {
            items,
            total,
            page,
            page_size,
        })
    }
}

/// Whether the window rule says this certificate is due right now.
///
/// Delegated to the shared rule rather than re-expressed here: the sweep and the
/// certificate API must never be able to disagree about when renewal starts.
fn is_due_by_window(
    not_before: Option<&str>,
    not_after: Option<&str>,
    renew_before_days: i32,
    at: DateTime<Utc>,
) -> bool {
    let (Some(not_before), Some(not_after)) = (not_before, not_after) else {
        return false;
    };
    match ValidityWindow::from_rfc3339(not_before, not_after) {
        Ok(window) => {
            window.is_renewal_due(renew_before_days, at) && window.is_within_renewal_reach(at)
        }
        // A stored window the rule cannot reason about is left alone rather than
        // guessed at; the certificate stays listed and visible.
        Err(_) => false,
    }
}

fn parse_instant(value: &str, what: &str) -> DeployServiceResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|error| DeployServiceError::Internal(format!("{what} is invalid: {error}")))
}

fn map_certificate_renewal_row(row: &PgRow) -> Result<CertificateRenewalResponse, sqlx::Error> {
    Ok(CertificateRenewalResponse {
        id: row.try_get("uuid")?,
        certificate_id: row.try_get("certificate_uuid")?,
        trigger_kind: row.try_get("trigger_kind")?,
        status: row.try_get("status")?,
        attempt_no: row.try_get("attempt_no")?,
        previous_version_id: row.try_get("previous_version_uuid")?,
        resulting_version_id: row.try_get("resulting_version_uuid")?,
        previous_not_before: optional_datetime_from_row(row, "previous_not_before")?,
        previous_not_after: optional_datetime_from_row(row, "previous_not_after")?,
        new_not_before: optional_datetime_from_row(row, "new_not_before")?,
        new_not_after: optional_datetime_from_row(row, "new_not_after")?,
        scheduled_at: datetime_from_row(row, "scheduled_at")?,
        started_at: optional_datetime_from_row(row, "started_at")?,
        finished_at: optional_datetime_from_row(row, "finished_at")?,
        last_error_code: row.try_get("last_error_code")?,
        created_at: datetime_from_row(row, "created_at")?,
    })
}
