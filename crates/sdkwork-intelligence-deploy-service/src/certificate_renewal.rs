//! Certificate validity windows and the renewal decision rule.
//!
//! Two questions this module answers, and neither may be answered approximately.
//!
//! 1. **Is this certificate valid right now?** The window is the X.509
//!    `notBefore`/`notAfter` of the version being served, so the answer comes
//!    from the certificate that was actually issued rather than from a status
//!    column that someone has to remember to update.
//! 2. **Should renewal start yet?** Getting this wrong is expensive in a way
//!    that only shows up in production: renewing too eagerly re-issues on every
//!    sweep tick, and against a real CA that consumes the account's issuance
//!    rate limit — which is shared, so the damage lands on *other* certificates
//!    of the same tenant.
//!
//! ## The window rule
//!
//! Renewal becomes due at
//!
//! ```text
//! max( not_before + lifetime/3 , not_after - renew_before_days )
//! ```
//!
//! The first term is a floor, not a preference. Without it, a lead time longer
//! than the certificate's own lifetime puts `not_after - renew_before_days`
//! before `not_before`: the certificate is "due" the instant it is issued and the
//! scheduler orders a replacement on every tick, forever. A 7-day certificate
//! with the default 30-day lead time is exactly this case — and it is a real
//! configuration, not a thought experiment, because short-lived certificates are
//! the direction the ecosystem is moving.
//!
//! The floor also makes short-lived certificates behave without a second knob: a
//! 7-day certificate renews after roughly 2.3 days, a 90-day certificate with the
//! default lead time renews on day 60 (what operators expect from
//! `certbot renew`), and a deliberately long lead time on a long-lived
//! certificate simply shifts renewal to the one-third mark.
//!
//! ## Bounds
//!
//! - The lead time is bounded by [`MINIMUM_RENEW_BEFORE_DAYS`] and
//!   [`MAXIMUM_RENEW_BEFORE_DAYS`], mirroring the CHECK constraint on
//!   `deploy_certificate.renew_before_days`.
//! - [`SWEEP_LOOKAHEAD_DAYS`] is the coarse filter the SQL sweep uses. It equals
//!   the maximum lead time, which is provably safe: being due implies
//!   `not_after <= now + lead`, and `lead <= MAXIMUM_RENEW_BEFORE_DAYS`. The
//!   lifetime floor only ever makes the due instant *later*, so it can never
//!   push a due certificate outside the coarse window. See
//!   [`tests::the_coarse_sweep_filter_never_excludes_a_due_certificate`].
//! - [`RENEWAL_OVERDUE_GRACE_DAYS`] closes the other end. A certificate that
//!   expired longer ago than this is left for an operator: by then the CA
//!   account, the DNS credential, or the ownership of the name has usually
//!   changed, and retrying automatically only spends the tenant's shared
//!   issuance budget on a call that cannot succeed.

use chrono::{DateTime, Duration, Utc};
use sdkwork_deploy_contract::{
    CertificateOrderResponse, DeployServiceError, DeployServiceResult,
    RequestCertificateOrderRequest, CAA_DECISION_UNAUTHORIZED_CA,
    CERTIFICATE_VALIDITY_PHASE_EXPIRED, CERTIFICATE_VALIDITY_PHASE_EXPIRING_SOON,
    CERTIFICATE_VALIDITY_PHASE_NOT_YET_VALID, CERTIFICATE_VALIDITY_PHASE_VALID,
};

use crate::{DeployService, ExpiredCertificateSweep};

// The numbers themselves live in sdkwork-deploy-core because the contract crate
// needs the default when deserializing `renewBeforeDays`, and two crates that
// must agree on one window rule cannot each own a copy of it.
pub use sdkwork_deploy_core::{
    CERTIFICATE_DEFAULT_RENEW_BEFORE_DAYS as DEFAULT_RENEW_BEFORE_DAYS,
    CERTIFICATE_MAXIMUM_RENEW_BEFORE_DAYS as MAXIMUM_RENEW_BEFORE_DAYS,
    CERTIFICATE_MINIMUM_RENEW_BEFORE_DAYS as MINIMUM_RENEW_BEFORE_DAYS,
};

/// How far ahead of expiry the SQL sweep looks, in days.
///
/// Equals [`MAXIMUM_RENEW_BEFORE_DAYS`] by design: that is the tightest bound
/// that provably cannot exclude a certificate the precise rule calls due.
pub const SWEEP_LOOKAHEAD_DAYS: i64 = MAXIMUM_RENEW_BEFORE_DAYS as i64;

/// How long after expiry a certificate is still worth renewing automatically.
///
/// Past this the sweep stops. The bound exists because automatic retries are not
/// free: every attempt spends the tenant's shared CA rate limit, so a
/// permanently broken certificate would otherwise degrade issuance for every
/// other certificate under the same account.
pub const RENEWAL_OVERDUE_GRACE_DAYS: i64 = 30;

/// The lifetime floor's divisor. Renewal never starts before this fraction of the
/// certificate's own life has elapsed. See the module documentation.
const EARLIEST_RENEWAL_LIFETIME_DIVISOR: i64 = 3;

/// First retry delay after a failed renewal.
const RENEWAL_RETRY_BASE_SECONDS: i64 = 300;

/// Longest retry delay. See [`renewal_retry_delay`].
pub const MAXIMUM_RENEWAL_RETRY_SECONDS: i64 = 86_400;

/// Shifts available to the retry backoff; caps the doubling before the ceiling.
const MAXIMUM_RENEWAL_RETRY_SHIFT: u32 = 20;

const SECONDS_PER_DAY: i64 = 86_400;

/// Rejects a lead time the database would reject anyway.
///
/// Checked before the value reaches SQL so the caller gets a `422` naming the
/// field instead of a constraint violation naming a constraint.
pub fn validate_renew_before_days(days: i32) -> Result<(), String> {
    if !(MINIMUM_RENEW_BEFORE_DAYS..=MAXIMUM_RENEW_BEFORE_DAYS).contains(&days) {
        return Err(format!(
            "renewBeforeDays must be between {MINIMUM_RENEW_BEFORE_DAYS} and \
             {MAXIMUM_RENEW_BEFORE_DAYS}, got {days}"
        ));
    }
    Ok(())
}

/// The validity window of one certificate version.
///
/// Constructed only through [`ValidityWindow::new`], so a window that cannot
/// describe a certificate — one that ends before it begins — cannot exist to be
/// reasoned about. The database enforces the same rule
/// (`chk_deploy_certificate_version_validity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidityWindow {
    not_before: DateTime<Utc>,
    not_after: DateTime<Utc>,
}

impl ValidityWindow {
    pub fn new(not_before: DateTime<Utc>, not_after: DateTime<Utc>) -> Result<Self, String> {
        if not_after <= not_before {
            return Err(format!(
                "certificate validity window must end after it begins, got \
                 {not_before} to {not_after}"
            ));
        }
        Ok(Self {
            not_before,
            not_after,
        })
    }

    /// Parses the pair as stored on `deploy_certificate_version`.
    ///
    /// Offsets are honoured rather than assumed: `Z` and `+08:00` are the same
    /// instant, and a CA is free to return either.
    pub fn from_rfc3339(not_before: &str, not_after: &str) -> Result<Self, String> {
        fn parse(value: &str, what: &str) -> Result<DateTime<Utc>, String> {
            DateTime::parse_from_rfc3339(value)
                .map(|parsed| parsed.with_timezone(&Utc))
                .map_err(|error| format!("certificate {what} is not an RFC 3339 instant: {error}"))
        }
        Self::new(
            parse(not_before, "notBefore")?,
            parse(not_after, "notAfter")?,
        )
    }

    pub fn not_before(&self) -> DateTime<Utc> {
        self.not_before
    }

    pub fn not_after(&self) -> DateTime<Utc> {
        self.not_after
    }

    /// Total lifetime; always positive because [`ValidityWindow::new`] refuses
    /// the alternative.
    pub fn lifetime(&self) -> Duration {
        self.not_after - self.not_before
    }

    /// Whether `at` falls inside the window.
    ///
    /// Both endpoints count. RFC 5280 section 4.1.2.5 includes `notBefore` and
    /// `notAfter` in the validity period, so a certificate is still usable at the
    /// very instant named by `notAfter`.
    pub fn contains(&self, at: DateTime<Utc>) -> bool {
        at >= self.not_before && at <= self.not_after
    }

    /// Whether `at` is past the end of the window.
    pub fn is_expired(&self, at: DateTime<Utc>) -> bool {
        at > self.not_after
    }

    /// Seconds until expiry; negative once past it.
    pub fn remaining_seconds(&self, at: DateTime<Utc>) -> i64 {
        (self.not_after - at).num_seconds()
    }

    /// Whole days until expiry, floored.
    ///
    /// Floor rather than truncate: with twelve hours left a certificate is on its
    /// last day, and truncation would report that as `0` next to a certificate
    /// that expired yesterday. Floored, the two cannot be confused.
    pub fn whole_days_remaining(&self, at: DateTime<Utc>) -> i64 {
        self.remaining_seconds(at).div_euclid(SECONDS_PER_DAY)
    }

    /// The instant renewal work for this window becomes due.
    ///
    /// See the module documentation for why the lifetime floor is part of the
    /// rule rather than a refinement of it.
    pub fn renewal_due_at(&self, renew_before_days: i32) -> DateTime<Utc> {
        let one_third_elapsed = self.not_before
            + Duration::seconds(self.lifetime().num_seconds() / EARLIEST_RENEWAL_LIFETIME_DIVISOR);
        let lead_time = self.not_after - Duration::days(i64::from(renew_before_days));
        one_third_elapsed.max(lead_time)
    }

    /// Whether renewal work for this window should already have started.
    ///
    /// True for an already-expired certificate as well: an overdue certificate is
    /// the one that most needs replacing, and returning false here would make a
    /// scheduler outage longer than the lead time unrecoverable. Whether the
    /// sweep may still act on that is a separate question — see
    /// [`ValidityWindow::is_within_renewal_reach`].
    pub fn is_renewal_due(&self, renew_before_days: i32, at: DateTime<Utc>) -> bool {
        at >= self.renewal_due_at(renew_before_days)
    }

    /// Whether the sweep may still attempt this renewal automatically.
    ///
    /// Bounded on the far side by [`RENEWAL_OVERDUE_GRACE_DAYS`].
    pub fn is_within_renewal_reach(&self, at: DateTime<Utc>) -> bool {
        at <= self.not_after + Duration::days(RENEWAL_OVERDUE_GRACE_DAYS)
    }

    /// Where `at` falls in the certificate's life, for display and alerting.
    pub fn classify(&self, renew_before_days: i32, at: DateTime<Utc>) -> ValidityPhase {
        if at < self.not_before {
            ValidityPhase::NotYetValid
        } else if self.is_expired(at) {
            ValidityPhase::Expired
        } else if self.is_renewal_due(renew_before_days, at) {
            ValidityPhase::ExpiringSoon
        } else {
            ValidityPhase::Valid
        }
    }
}

/// Which part of its life a certificate currently sits in.
///
/// Derived, never stored: see the constant documentation in the contract crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidityPhase {
    /// Issued but not yet usable — a clock skew or a post-dated certificate.
    NotYetValid,
    /// Inside its window and not yet due for renewal.
    Valid,
    /// Inside its window but already inside the renewal window.
    ExpiringSoon,
    /// Past `notAfter`.
    Expired,
}

impl ValidityPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            ValidityPhase::NotYetValid => CERTIFICATE_VALIDITY_PHASE_NOT_YET_VALID,
            ValidityPhase::Valid => CERTIFICATE_VALIDITY_PHASE_VALID,
            ValidityPhase::ExpiringSoon => CERTIFICATE_VALIDITY_PHASE_EXPIRING_SOON,
            ValidityPhase::Expired => CERTIFICATE_VALIDITY_PHASE_EXPIRED,
        }
    }

    /// Whether the certificate still works for TLS right now.
    pub fn is_usable(self) -> bool {
        matches!(self, ValidityPhase::Valid | ValidityPhase::ExpiringSoon)
    }
}

/// How long to wait before retrying a renewal that has failed `failure_count`
/// times.
///
/// Exponential from five minutes, capped at a day. The cap is what stops a
/// permanently broken certificate from becoming a steady stream of issuance
/// attempts against a rate limit the whole tenant shares; the low first step is
/// what keeps a transient CA error from costing a day of lead time.
pub fn renewal_retry_delay(failure_count: i32) -> Duration {
    let exponent = (failure_count.max(1) - 1).min(MAXIMUM_RENEWAL_RETRY_SHIFT as i32) as u32;
    let seconds = RENEWAL_RETRY_BASE_SECONDS.saturating_mul(1_i64 << exponent);
    Duration::seconds(seconds.min(MAXIMUM_RENEWAL_RETRY_SECONDS))
}

/// Most failures counted before the counter stops mattering.
///
/// Mirrors `chk_deploy_certificate_renewal_failures`; past the backoff ceiling an
/// ever-growing count would only be a number nobody reads.
pub const MAXIMUM_RENEWAL_FAILURE_COUNT: i32 = 1000;

/// Default number of certificates one sweep pass claims.
pub const DEFAULT_RENEWAL_BATCH_SIZE: i64 = 50;

/// Ceiling on a sweep pass, so one overdue estate cannot hold a worker for an
/// unbounded time or exhaust a CA's per-account rate limit in a single tick.
pub const MAXIMUM_RENEWAL_BATCH_SIZE: i64 = 100;

/// Shortest lease a worker may hold a certificate for. The work behind the lease
/// includes an outbound CAA lookup and an order write, so a lease short enough to
/// expire mid-claim would let a second worker duplicate the order.
pub const MINIMUM_RENEWAL_LEASE_SECONDS: i64 = 30;

/// Longest lease, so a worker that dies does not block its certificates for a
/// whole day.
pub const MAXIMUM_RENEWAL_LEASE_SECONDS: i64 = 1800;

/// What one sweep pass did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CertificateRenewalBatchResult {
    /// Certificates this worker claimed.
    pub claimed: i64,
    /// Claims that came back with an order attached.
    pub ordered: i64,
    /// Claims whose order could not be opened; each is now waiting out its
    /// backoff with a failure recorded against it.
    pub failed: i64,
}

impl CertificateRenewalBatchResult {
    /// Whether anything happened, used to keep an idle sweep from logging.
    pub fn is_idle(&self) -> bool {
        self.claimed == 0 && self.ordered == 0 && self.failed == 0
    }
}

/// The `lastErrorCode` recorded when opening an order fails.
///
/// Codes rather than messages, matching the rest of the certificate tables; the
/// detail goes to the log, where it can be searched without widening a column
/// that is meant to be stable.
fn renewal_failure_code(error: &DeployServiceError) -> &'static str {
    use sdkwork_deploy_contract::DeployServiceErrorKind;
    match error.kind() {
        DeployServiceErrorKind::NotFound => "CERTIFICATE_NOT_FOUND",
        // A CAA refusal arrives as a conflict. It is the expected reason for an
        // automatic renewal to be refused, which is why it gets its own code.
        DeployServiceErrorKind::Conflict => "ISSUANCE_REFUSED",
        DeployServiceErrorKind::Validation => "ISSUANCE_INVALID",
        DeployServiceErrorKind::Forbidden => "ISSUANCE_FORBIDDEN",
        DeployServiceErrorKind::QuotaExceeded => "ISSUANCE_QUOTA_EXCEEDED",
        DeployServiceErrorKind::DatabaseUnavailable => "STORE_UNAVAILABLE",
        DeployServiceErrorKind::Internal => "INTERNAL",
    }
}

impl DeployService {
    /// Opens a certificate order behind the CAA pre-flight.
    ///
    /// Shared by the operator-facing backend API and the renewal sweep, so an
    /// automatically renewed certificate cannot skip the RFC 8659 check that a
    /// manually requested one goes through. Two order-creation paths would be a
    /// quiet way for the automatic one to become the unregulated one.
    ///
    /// The check runs before the order exists, so a policy refusal never consumes
    /// the CA's rate-limit budget, and the observation is recorded on the order so
    /// the refusal stays auditable without re-querying DNS. Only a definite
    /// refusal blocks: an unevaluable check is recorded and left to the CA, which
    /// performs the authoritative check, because a resolver outage must not stop
    /// all certificate management.
    pub async fn open_certificate_order(
        &self,
        tenant_id: i64,
        request: &RequestCertificateOrderRequest,
    ) -> DeployServiceResult<CertificateOrderResponse> {
        if request.idempotency_key.trim().is_empty() {
            return Err(DeployServiceError::validation("idempotencyKey is required"));
        }
        if let Some(challenge_type) = request.challenge_type.as_deref() {
            if !matches!(challenge_type, "HTTP_01" | "DNS_01") {
                return Err(DeployServiceError::validation(
                    "challengeType must be HTTP_01 or DNS_01",
                ));
            }
        }

        let subject = self
            .repository
            .certificate_order_caa_subject(tenant_id, &request.certificate_id)
            .await?;
        let caa = crate::observe_order_caa(self.certificate_caa.as_ref(), &subject).await?;
        if caa.blocks_order_creation() {
            return Err(DeployServiceError::conflict(format!(
                "CAA refuses issuance ({}): {}",
                CAA_DECISION_UNAUTHORIZED_CA, caa.detail
            )));
        }

        let order = self
            .repository
            .request_certificate_order(tenant_id, request)
            .await?;
        self.repository
            .record_certificate_order_caa_decision(
                tenant_id,
                &order.id,
                caa.decision,
                &Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            )
            .await?;
        Ok(order)
    }

    /// Claims due certificates and opens a renewal order for each.
    ///
    /// Bounded and leased, so several workers can run this concurrently without
    /// ever issuing the same certificate twice. A certificate whose order cannot
    /// be opened is not retried immediately: the attempt is closed and the
    /// certificate is given a backoff, because the alternatives are a tight loop
    /// against a shared rate limit or silently dropping the certificate.
    pub async fn plan_due_certificate_renewals(
        &self,
        worker_id: &str,
        batch_size: i64,
        lease_seconds: i64,
    ) -> DeployServiceResult<CertificateRenewalBatchResult> {
        if worker_id.is_empty()
            || worker_id.len() > 128
            || !worker_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
            })
        {
            return Err(DeployServiceError::validation(
                "certificate renewal worker id is invalid",
            ));
        }
        if !(1..=MAXIMUM_RENEWAL_BATCH_SIZE).contains(&batch_size) {
            return Err(DeployServiceError::validation(format!(
                "certificate renewal batch size must be between 1 and \
                 {MAXIMUM_RENEWAL_BATCH_SIZE}"
            )));
        }
        if !(MINIMUM_RENEWAL_LEASE_SECONDS..=MAXIMUM_RENEWAL_LEASE_SECONDS).contains(&lease_seconds)
        {
            return Err(DeployServiceError::validation(format!(
                "certificate renewal lease must be between \
                 {MINIMUM_RENEWAL_LEASE_SECONDS} and {MAXIMUM_RENEWAL_LEASE_SECONDS} seconds"
            )));
        }

        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let claims = self
            .repository
            .claim_due_certificate_renewals(worker_id, batch_size, lease_seconds, &now)
            .await?;

        let mut result = CertificateRenewalBatchResult {
            claimed: claims.len() as i64,
            ..CertificateRenewalBatchResult::default()
        };
        for claim in claims {
            let request = RequestCertificateOrderRequest {
                certificate_id: claim.certificate_uuid.clone(),
                // Anchored to the attempt, so retrying *this* attempt reuses the
                // order instead of opening a second one at the CA.
                idempotency_key: format!("renew:{}", claim.renewal_uuid),
                challenge_type: None,
            };
            match self.open_certificate_order(claim.tenant_id, &request).await {
                Ok(order) => {
                    self.repository
                        .mark_certificate_renewal_ordered(
                            claim.tenant_id,
                            &claim.renewal_uuid,
                            &order.id,
                        )
                        .await?;
                    result.ordered += 1;
                    tracing::info!(
                        certificate = %claim.certificate_uuid,
                        attempt = claim.attempt_no,
                        order = %order.id,
                        request_id = %order.request_sha256,
                        "certificate renewal order opened"
                    );
                }
                Err(error) => {
                    let code = renewal_failure_code(&error);
                    self.repository
                        .fail_certificate_renewal(claim.tenant_id, &claim.renewal_uuid, code)
                        .await?;
                    result.failed += 1;
                    // The code is what the ledger keeps; the detail is what an
                    // operator needs to act, so it goes to the log rather than
                    // being dropped.
                    tracing::warn!(
                        certificate = %claim.certificate_uuid,
                        attempt = claim.attempt_no,
                        error_code = code,
                        error = %error,
                        "certificate renewal order refused; backoff scheduled"
                    );
                }
            }
        }
        Ok(result)
    }

    /// Retires certificates and versions whose validity window has closed, and
    /// gives up on renewals that are past recovery.
    pub async fn sweep_expired_certificates(
        &self,
        batch_size: i64,
    ) -> DeployServiceResult<ExpiredCertificateSweep> {
        if !(1..=MAXIMUM_RENEWAL_BATCH_SIZE).contains(&batch_size) {
            return Err(DeployServiceError::validation(format!(
                "certificate expiry sweep batch size must be between 1 and \
                 {MAXIMUM_RENEWAL_BATCH_SIZE}"
            )));
        }
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.repository
            .sweep_expired_certificates(batch_size, &now)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test instant is RFC 3339")
            .with_timezone(&Utc)
    }

    fn window(lifetime_days: i64) -> ValidityWindow {
        let start = instant("2026-01-01T00:00:00Z");
        ValidityWindow::new(start, start + Duration::days(lifetime_days)).expect("valid window")
    }

    #[test]
    fn lead_time_bounds_match_the_database_constraint() {
        // These numbers are duplicated in chk_deploy_certificate_renew_before_days
        // and on deploy_certificate.renew_before_days. Asserting them here means a
        // change to either side has to be deliberate rather than accidental.
        assert_eq!(DEFAULT_RENEW_BEFORE_DAYS, 30);
        assert_eq!(MINIMUM_RENEW_BEFORE_DAYS, 7);
        assert_eq!(MAXIMUM_RENEW_BEFORE_DAYS, 90);
        assert_eq!(SWEEP_LOOKAHEAD_DAYS, 90);
        assert!(validate_renew_before_days(MINIMUM_RENEW_BEFORE_DAYS).is_ok());
        assert!(validate_renew_before_days(MAXIMUM_RENEW_BEFORE_DAYS).is_ok());
        assert!(validate_renew_before_days(MINIMUM_RENEW_BEFORE_DAYS - 1).is_err());
        assert!(validate_renew_before_days(MAXIMUM_RENEW_BEFORE_DAYS + 1).is_err());
    }

    #[test]
    fn a_certificate_is_valid_including_both_endpoints() {
        let start = instant("2026-01-01T00:00:00Z");
        let subject = ValidityWindow::new(start, instant("2026-04-01T00:00:00Z")).expect("window");
        assert!(!subject.contains(start - Duration::seconds(1)));
        assert!(subject.contains(start));
        assert!(subject.contains(subject.not_after()));
        assert!(!subject.contains(subject.not_after() + Duration::seconds(1)));
        assert!(!subject.is_expired(subject.not_after()));
        assert!(subject.is_expired(subject.not_after() + Duration::seconds(1)));
    }

    #[test]
    fn a_short_lived_certificate_is_not_due_the_moment_it_is_issued() {
        // The case the lifetime floor exists for: a 7-day certificate with the
        // default 30-day lead time. Without the floor, `not_after - 30d` lands
        // before `not_before`, so the certificate would be due at issuance and the
        // scheduler would re-order it on every tick.
        let subject = window(7);
        let due = subject.renewal_due_at(DEFAULT_RENEW_BEFORE_DAYS);
        assert!(
            due > subject.not_before(),
            "a fresh 7-day certificate must not be due immediately, but was due at {due}"
        );
        assert!(!subject.is_renewal_due(DEFAULT_RENEW_BEFORE_DAYS, subject.not_before()));
        // One third of seven days is 2 days 8 hours.
        assert_eq!(due, subject.not_before() + Duration::seconds(201_600));
        assert!(subject.is_renewal_due(
            DEFAULT_RENEW_BEFORE_DAYS,
            subject.not_before() + Duration::seconds(201_600)
        ));
    }

    #[test]
    fn a_ninety_day_certificate_with_the_default_lead_renews_on_day_sixty() {
        // What operators expect from `certbot renew`, and the reason the rule is
        // `max` rather than the floor alone.
        let subject = window(90);
        assert_eq!(
            subject.renewal_due_at(DEFAULT_RENEW_BEFORE_DAYS),
            subject.not_before() + Duration::days(60)
        );
    }

    #[test]
    fn an_exhaustive_lead_time_never_becomes_due_at_issuance() {
        // The floor must hold for every lifetime and every allowed lead time, not
        // just the two cases above.
        for lifetime_days in [1i64, 2, 3, 7, 30, 90, 100, 365, 398, 825] {
            for lead in MINIMUM_RENEW_BEFORE_DAYS..=MAXIMUM_RENEW_BEFORE_DAYS {
                let subject = window(lifetime_days);
                let due = subject.renewal_due_at(lead);
                assert!(
                    due > subject.not_before(),
                    "lifetime={lifetime_days}d lead={lead}d became due at issuance"
                );
                assert!(
                    due <= subject.not_after(),
                    "lifetime={lifetime_days}d lead={lead}d became due after expiry"
                );
            }
        }
    }

    #[test]
    fn the_coarse_sweep_filter_never_excludes_a_due_certificate() {
        // The SQL sweep filters on `active_not_after <= now + SWEEP_LOOKAHEAD_DAYS`
        // before applying the precise rule. If that coarse bound were ever too
        // tight, certificates would silently stop renewing -- so it is checked
        // against every combination rather than argued for.
        //
        // `is_renewal_due` depends on `at` only through `at >= due_at`, so it is
        // monotone: the tightest margin is at the instant renewal becomes due.
        // Checking there makes the property exact instead of sampled.
        let lookahead = Duration::days(SWEEP_LOOKAHEAD_DAYS);
        let leads = MINIMUM_RENEW_BEFORE_DAYS..=MAXIMUM_RENEW_BEFORE_DAYS;
        for lifetime_days in [1i64, 2, 3, 7, 30, 47, 90, 100, 365, 398, 825] {
            let subject = window(lifetime_days);
            for lead in leads.clone() {
                let due = subject.renewal_due_at(lead);
                assert!(
                    subject.not_after() <= due + lookahead,
                    "a due certificate escaped the coarse sweep filter: \
                     lifetime={lifetime_days}d lead={lead}d due={due}"
                );
                // The precise rule must not fire one second earlier, or the sweep
                // would renew before the window it was configured for.
                assert!(!subject.is_renewal_due(lead, due - Duration::seconds(1)));

                // Sample the rest of the life, including well past expiry, so the
                // property is not only checked at the single tightest instant.
                for offset in [
                    Duration::zero(),
                    Duration::seconds(1),
                    subject.lifetime() / 3,
                    subject.lifetime() - Duration::seconds(1),
                    Duration::days(lifetime_days),
                    Duration::days(lifetime_days + 1),
                    Duration::days(lifetime_days + 60),
                ] {
                    let at = subject.not_before() + offset;
                    if subject.is_renewal_due(lead, at) {
                        assert!(
                            subject.not_after() <= at + lookahead,
                            "a due certificate escaped the coarse sweep filter: \
                             lifetime={lifetime_days}d lead={lead}d at={at}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn an_expired_certificate_stays_due_so_an_outage_is_recoverable() {
        let subject = window(90);
        let well_past_expiry = subject.not_after() + Duration::days(1);
        assert!(subject.is_expired(well_past_expiry));
        assert!(
            subject.is_renewal_due(DEFAULT_RENEW_BEFORE_DAYS, well_past_expiry),
            "a certificate the scheduler missed must still be renewable"
        );
        assert!(subject.is_within_renewal_reach(well_past_expiry));
    }

    #[test]
    fn a_certificate_far_past_expiry_leaves_the_sweep_alone() {
        let subject = window(90);
        let boundary = subject.not_after() + Duration::days(RENEWAL_OVERDUE_GRACE_DAYS);
        assert!(subject.is_within_renewal_reach(boundary));
        assert!(!subject.is_within_renewal_reach(boundary + Duration::seconds(1)));
        // Still "due" by the rule; the sweep's reach is what stops it.
        assert!(subject.is_renewal_due(DEFAULT_RENEW_BEFORE_DAYS, boundary));
    }

    #[test]
    fn remaining_days_floor_rather_than_truncate() {
        let subject = window(90);
        assert_eq!(
            subject.whole_days_remaining(subject.not_before()),
            90,
            "a fresh 90-day certificate has 90 whole days left"
        );
        // Twelve hours before expiry is still "the last day", not zero.
        assert_eq!(
            subject.whole_days_remaining(subject.not_after() - Duration::hours(12)),
            0
        );
        // Twelve hours past expiry is one day expired, not zero, so an expired
        // certificate can never render as "expires today".
        assert_eq!(
            subject.whole_days_remaining(subject.not_after() + Duration::hours(12)),
            -1
        );
    }

    #[test]
    fn the_phase_boundaries_are_the_window_and_the_renewal_window() {
        let subject = window(90);
        let lead = DEFAULT_RENEW_BEFORE_DAYS;
        let due = subject.renewal_due_at(lead);

        assert_eq!(
            subject.classify(lead, subject.not_before() - Duration::seconds(1)),
            ValidityPhase::NotYetValid
        );
        assert_eq!(
            subject.classify(lead, subject.not_before()),
            ValidityPhase::Valid
        );
        assert_eq!(
            subject.classify(lead, due - Duration::seconds(1)),
            ValidityPhase::Valid
        );
        assert_eq!(subject.classify(lead, due), ValidityPhase::ExpiringSoon);
        assert_eq!(
            subject.classify(lead, subject.not_after()),
            ValidityPhase::ExpiringSoon
        );
        assert_eq!(
            subject.classify(lead, subject.not_after() + Duration::seconds(1)),
            ValidityPhase::Expired
        );

        assert!(ValidityPhase::Valid.is_usable());
        assert!(ValidityPhase::ExpiringSoon.is_usable());
        assert!(!ValidityPhase::NotYetValid.is_usable());
        assert!(!ValidityPhase::Expired.is_usable());
        assert_eq!(ValidityPhase::ExpiringSoon.as_str(), "EXPIRING_SOON");
    }

    #[test]
    fn retry_delay_doubles_from_five_minutes_and_stops_at_a_day() {
        assert_eq!(renewal_retry_delay(0), Duration::seconds(300));
        assert_eq!(renewal_retry_delay(1), Duration::seconds(300));
        assert_eq!(renewal_retry_delay(2), Duration::seconds(600));
        assert_eq!(renewal_retry_delay(3), Duration::seconds(1_200));
        assert_eq!(renewal_retry_delay(9), Duration::seconds(76_800));
        assert_eq!(renewal_retry_delay(10), Duration::seconds(86_400));
        // Pinned at the ceiling, and no overflow at absurd counts.
        assert_eq!(renewal_retry_delay(1_000), Duration::seconds(86_400));
        assert_eq!(renewal_retry_delay(i32::MAX), Duration::seconds(86_400));
        assert!(renewal_retry_delay(2) > renewal_retry_delay(1));
    }

    #[test]
    fn a_window_that_ends_before_it_begins_is_refused() {
        let start = instant("2026-01-01T00:00:00Z");
        assert!(ValidityWindow::new(start, start).is_err());
        assert!(ValidityWindow::new(start, start - Duration::seconds(1)).is_err());
        assert!(ValidityWindow::new(start, start + Duration::seconds(1)).is_ok());
    }

    #[test]
    fn rfc3339_offsets_are_compared_as_instants() {
        let utc = ValidityWindow::from_rfc3339("2026-01-01T00:00:00Z", "2026-04-01T00:00:00Z")
            .expect("utc window");
        let offset =
            ValidityWindow::from_rfc3339("2026-01-01T08:00:00+08:00", "2026-04-01T08:00:00+08:00")
                .expect("offset window");
        assert_eq!(utc, offset);
        assert!(ValidityWindow::from_rfc3339("2026-01-01", "2026-04-01").is_err());
    }
}
