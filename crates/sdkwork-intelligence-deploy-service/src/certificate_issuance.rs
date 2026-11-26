//! Certificate issuance execution (PLAN-2026-0003 §9).
//!
//! The control plane already accepts certificate intent: `create_certificate`
//! writes a `PENDING` row, the renewal sweep writes `renewal_status = PLANNED`,
//! and `open_certificate_order` records a nine-state order behind the CAA
//! pre-flight. What was missing is the executor — something that consumes
//! `deploy_certificate_order` and turns that intent into a stored version. This
//! module is that executor.
//!
//! Two design rules carry most of the weight here:
//!
//! * **The state machine is driven one legal step at a time** through
//!   [`advance_certificate_order`](crate::repository::DeployRepositoryPort::advance_certificate_order),
//!   never by writing a status directly. `next_order_transition` is therefore the
//!   only description of the order lifecycle, and a worker that resumes a
//!   half-finished order cannot invent a shortcut.
//! * **The challenge method is a policy decision, not a retry knob.** A wildcard
//!   identifier can only be proven by DNS-01, so the scope wins over the requested
//!   method; that is the same rule the database `CHECK` enforces, restated at the
//!   point where the decision is actually taken.
//!
//! The engine behind issuance is a port. The control plane does not know which CA
//! is configured, and — more usefully — a test can drive the whole state machine
//! against a controlled CA without any of this code changing.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, SecondsFormat, Utc};
use sdkwork_deploy_contract::{
    CertificateMaterialPayload, CertificateScope, DeployServiceError, DeployServiceResult,
    ValidationMethod, CHALLENGE_TYPE_DNS_01, CHALLENGE_TYPE_HTTP_01, ORDER_ERROR_CONFLICT,
    ORDER_ERROR_DATABASE_UNAVAILABLE, ORDER_ERROR_FORBIDDEN, ORDER_ERROR_INTERNAL,
    ORDER_ERROR_NOT_FOUND, ORDER_ERROR_QUOTA_EXCEEDED, ORDER_ERROR_VALIDATION_FAILED,
    ORDER_STATUS_ACCOUNT_READY, ORDER_STATUS_CHALLENGE_PRESENTING,
    ORDER_STATUS_CHALLENGE_VALIDATING, ORDER_STATUS_ORDER_PENDING, ORDER_STATUS_REQUESTED,
};
use sdkwork_webserver_acme_service::{Dns01Presenter, IssuedCertificateMaterial};

use crate::certificate_archive::CertificateArchiveAttempt;
use crate::certificate_material::{seal_issued_material, DeclaredCertificateEvidence};
use crate::repository::CertificateOrderClaim;
use crate::DeployService;

/// The DNS-01 presentation context for one issuance.
///
/// The presenter is resolved by the caller because only the caller knows which
/// provider owns the zone and holds the credential; the issuance engine never
/// reads a secret store.
pub struct CertificateDns01Context {
    pub presenter: Arc<dyn Dns01Presenter>,
    /// The hosted zone apex that owns the `_acme-challenge` records.
    pub zone_apex: String,
    /// The account-center id the resolution answered through, when an account
    /// did. `None` is the deployment-level configuration's answer, which has
    /// no account-center row to name — a caller that reports which account
    /// served a request carries this instead of re-deriving it, so the report
    /// and the credential that was used cannot disagree.
    pub provider_account_id: Option<String>,
}

/// One issuance request, owned so it can cross an `async` trait boundary.
pub struct CertificateIssuanceRequest {
    pub hostnames: Vec<String>,
    pub cert_name: String,
    pub key_algorithm: String,
    /// `None` selects HTTP-01, which is exact-identifier only. A wildcard on that
    /// path is refused by the engine before an order is created, which is why the
    /// caller must not send one.
    pub dns01: Option<CertificateDns01Context>,
}

/// One challenge response a two-phase order has to present.
///
/// Only the fields the console can act on: the identity to prove, and — for
/// DNS-01 — the exact TXT record name and value. An HTTP-01 token is deliberately
/// not surfaced, because that proof is presented by the edge, never by hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCertificateChallenge {
    pub identifier: String,
    pub record_name: Option<String>,
    pub record_value: Option<String>,
}

/// A CA order created by [`CertificateIssuancePort::prepare`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCertificateOrder {
    /// The CA's order URL; the only state needed to resume the order.
    pub order_url: String,
    pub challenges: Vec<PreparedCertificateChallenge>,
    /// The identifiers the CA already considers authorized, which need no
    /// presentation at all.
    pub already_valid: Vec<String>,
}

/// Issues one certificate through a CA.
///
/// A port rather than a direct dependency, so the control plane carries no
/// opinion about the CA and the executor can be exercised against a controlled
/// one.
#[async_trait]
pub trait CertificateIssuancePort: Send + Sync {
    async fn issue(
        &self,
        request: CertificateIssuanceRequest,
    ) -> DeployServiceResult<IssuedCertificateMaterial>;

    /// Phase one: create the order and return every challenge response as data.
    ///
    /// Nothing is told to the CA here — no `set_ready`. For a manual request the
    /// caller persists the returned challenges, parks the order, and resumes with
    /// [`Self::complete`] once an operator has published them.
    ///
    /// Defaulted to an "unsupported" error so a port that only knows the
    /// single-shot path keeps compiling; a deployment that offers manual issuance
    /// must install an engine that implements both phases.
    async fn prepare(
        &self,
        request: CertificateIssuanceRequest,
    ) -> DeployServiceResult<PreparedCertificateOrder> {
        let _ = request;
        Err(DeployServiceError::Internal(
            "this deployment cannot prepare a two-phase certificate order".to_owned(),
        ))
    }

    /// Phase two: resume `order_url` and drive it to validated, sealed material.
    ///
    /// `managed_records` are the provider records the caller persisted in phase
    /// one; they are withdrawn after the exchange. A manual presentation passes an
    /// empty slice because the operator owns those records.
    async fn complete(
        &self,
        order_url: &str,
        request: CertificateIssuanceRequest,
        managed_records: &[sdkwork_webserver_acme_service::Dns01RecordHandle],
    ) -> DeployServiceResult<IssuedCertificateMaterial> {
        let _ = (order_url, request, managed_records);
        Err(DeployServiceError::Internal(
            "this deployment cannot complete a two-phase certificate order".to_owned(),
        ))
    }

    /// The CA's suggested renewal window (RFC 9773 ARI) for an issued leaf, when
    /// the CA publishes one.
    ///
    /// Defaulted to `Ok(None)` so a port without ARI support (a test double, an
    /// internal CA) keeps compiling and simply leaves the local window rule in
    /// charge. `None` is also the honest answer for a CA that does not implement
    /// ARI, which is not an error.
    async fn renewal_window(
        &self,
        cert_pem: &str,
    ) -> DeployServiceResult<Option<sdkwork_webserver_acme_service::AriRenewalWindow>> {
        let _ = cert_pem;
        Ok(None)
    }
}

/// What a presenter resolver is told about one DNS-01 presentation.
///
/// A struct rather than positional arguments because the resolution chain grows:
/// the certificate's own pin, then its zone's, then the account center, then the
/// deployment-level configuration. Each of those needed one more fact, and a
/// growing tail of `Option` parameters is where call sites start passing them in
/// the wrong order.
pub struct CertificateDns01Selector<'a> {
    pub tenant_id: i64,
    /// The certificate's own pin. Wins over its zone's, which is what lets one zone
    /// issue through several vendor accounts.
    pub certificate_provider_account_id: Option<&'a str>,
    /// The hostname whose `_acme-challenge` record must be published. The zone that
    /// owns it is a lookup away, and is what the record's apex comes from.
    pub hostname: &'a str,
}

/// Resolves the presenter that can publish a `_acme-challenge` TXT record for one
/// hostname.
///
/// `None` is a normal answer, not an error: a tenant may simply not have an
/// integrated DNS provider, and the certificate must then still be issuable by
/// another route rather than the whole order failing.
#[async_trait]
pub trait CertificateDns01PresenterPort: Send + Sync {
    async fn resolve(
        &self,
        selector: CertificateDns01Selector<'_>,
    ) -> DeployServiceResult<Option<CertificateDns01Context>>;
}

/// The default: no tenant has an integrated DNS provider.
///
/// Deliberately not a fabricated success. Reporting "no provider" lets the
/// caller take the honest branch, whereas a stub presenter would pretend to have
/// published a record the CA then never sees.
pub struct UnconfiguredCertificateDns01Presenter;

#[async_trait]
impl CertificateDns01PresenterPort for UnconfiguredCertificateDns01Presenter {
    async fn resolve(
        &self,
        _selector: CertificateDns01Selector<'_>,
    ) -> DeployServiceResult<Option<CertificateDns01Context>> {
        Ok(None)
    }
}

/// What one issuance batch did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CertificateOrderBatchResult {
    pub claimed: i64,
    pub stored: i64,
    pub failed: i64,
    /// Orders the worker stopped working on because it no longer held their lease.
    ///
    /// Counted apart from `failed` on purpose: these are not the order's fault, and
    /// failing them would destroy work another worker is actively doing.
    pub abandoned: i64,
}

impl CertificateOrderBatchResult {
    pub fn is_idle(&self) -> bool {
        self.claimed == 0
    }
}

/// How one claimed order ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrderFulfilment {
    /// A new version was stored.
    Stored,
    /// The lease was lost to another worker, which now owns the order.
    Abandoned,
}

/// A worker id that can be recorded as a lease holder.
///
/// Checked here rather than only in the worker binary, because the id is written
/// into `lease_owner` and compared for equality: an id carrying whitespace or a
/// control character would either fail to match what was written or, worse, match a
/// different row's holder.
fn validate_worker_id(worker_id: &str) -> DeployServiceResult<()> {
    if worker_id.is_empty()
        || worker_id.len() > 128
        || !worker_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(DeployServiceError::validation(
            "certificate issuance worker id is invalid",
        ));
    }
    Ok(())
}

/// Which ACME challenge can actually prove these identifiers.
///
/// Scope beats method: a wildcard can never be proven over HTTP-01, because the
/// CA must follow a TXT record to authorise a name that does not resolve to a
/// host, so a `WILDCARD` + `HTTP_01` request resolves to DNS-01 rather than
/// failing at the CA. `AUTO` means "prefer the edge proof path", which is
/// HTTP-01 for an exact name.
pub fn resolve_challenge_type(scope: CertificateScope, method: ValidationMethod) -> &'static str {
    match (scope, method) {
        (CertificateScope::Wildcard, _) => CHALLENGE_TYPE_DNS_01,
        (CertificateScope::SingleDomain, ValidationMethod::Dns01) => CHALLENGE_TYPE_DNS_01,
        (CertificateScope::SingleDomain, _) => CHALLENGE_TYPE_HTTP_01,
    }
}

/// How long a manual order waits for an operator to publish its records.
///
/// Capped well below the CA's own order lifetime: an order nobody acted on is
/// dead weight, and LE counts every rejected attempt against the account.
const MANUAL_PRESENTATION_TTL_HOURS: i64 = 72;

/// Builds the two-phase request for a manual certificate.
///
/// The presenter publishes nothing — the operator does. The zone apex is derived
/// from the identifiers rather than looked up, because no provider is contacted in
/// this mode; it only has to be an apex the `_acme-challenge` record name sits
/// under, and the shortest identifier is the closest honest answer. A certificate
/// whose identifiers span two registrable zones is rejected by the planning rules
/// before it reaches an order.
fn manual_issuance_request(
    certificate: &sdkwork_deploy_contract::CertificateResponse,
) -> DeployServiceResult<CertificateIssuanceRequest> {
    let zone_apex = certificate
        .identifiers
        .iter()
        .map(|hostname| hostname.trim_start_matches("*."))
        .min_by_key(|hostname| hostname.matches('.').count())
        .ok_or_else(|| {
            DeployServiceError::validation("certificate has no hostname identifiers to issue for")
        })?
        .to_owned();
    Ok(CertificateIssuanceRequest {
        hostnames: certificate.identifiers.clone(),
        cert_name: certificate.cert_name.clone(),
        key_algorithm: certificate.preferred_key_algorithm.clone(),
        dns01: Some(CertificateDns01Context {
            presenter: Arc::new(sdkwork_webserver_acme_service::ManualDns01Presenter),
            zone_apex,
            provider_account_id: None,
        }),
    })
}

impl DeployService {
    /// Releases the parked manual order of one certificate so the worker validates.
    ///
    /// The caller (the app API) is expected to run the presentation self-check
    /// first: releasing an order whose records are not visible spends one of the
    /// CA's failed-validation allowances. This method is the durable half of that
    /// command and stays idempotent: a second release reports "already released"
    /// rather than failing.
    pub async fn release_manual_presentation(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<bool> {
        // Self-check first: a record the configured resolvers can see is absent
        // must not be handed to the CA, which would spend one of its
        // failed-validation allowances. "Unverifiable" (nobody answered, or a
        // lookup error) does not block: the operator asserted the record exists,
        // and refusing there would make manual mode unusable on a deployment with
        // no trusted resolver.
        if let Some(inspector) = self.certificate_presentation_inspector.as_ref() {
            let presentations = self
                .repository
                .list_certificate_presentations(tenant_id, certificate_id)
                .await?;
            for item in &presentations.items {
                let (Some(record_name), Some(record_value)) =
                    (item.record_name.as_deref(), item.record_value.as_deref())
                else {
                    continue;
                };
                let report = inspector
                    .inspect_dns_txt(record_name, &[record_value.to_owned()])
                    .await?;
                if crate::certificate_presentation::presentation_release_decision(&report)
                    == crate::certificate_presentation::PresentationReleaseDecision::Blocked
                {
                    return Err(DeployServiceError::conflict(format!(
                        "CERTIFICATE_DNS_RECORD_NOT_VISIBLE: {record_name} is not published \
                         with the expected TXT value"
                    )));
                }
            }
        }
        let ready_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        self.repository
            .release_manual_order_for_certificate(tenant_id, certificate_id, &ready_at)
            .await
    }

    /// Claims due orders and drives each to a stored version.
    ///
    /// Bounded and leased, so several workers can run this concurrently without
    /// ever issuing the same certificate twice. An order that cannot be fulfilled
    /// is failed with a stable reason code rather than left leased: the next
    /// renewal window opens a fresh order with issue budget accounted for, which
    /// is what stops a permanent failure from becoming a tight retry loop against
    /// the CA.
    pub async fn process_due_certificate_orders(
        &self,
        worker_id: &str,
        batch_size: i64,
        lease_seconds: i64,
    ) -> DeployServiceResult<CertificateOrderBatchResult> {
        validate_worker_id(worker_id)?;
        if !(1..=MAXIMUM_ISSUANCE_BATCH_SIZE).contains(&batch_size) {
            return Err(DeployServiceError::validation(format!(
                "certificate issuance batch size must be between 1 and \
                 {MAXIMUM_ISSUANCE_BATCH_SIZE}"
            )));
        }
        if !(MINIMUM_ISSUANCE_LEASE_SECONDS..=MAXIMUM_ISSUANCE_LEASE_SECONDS)
            .contains(&lease_seconds)
        {
            return Err(DeployServiceError::validation(format!(
                "certificate issuance lease must be between \
                 {MINIMUM_ISSUANCE_LEASE_SECONDS} and {MAXIMUM_ISSUANCE_LEASE_SECONDS} seconds"
            )));
        }
        // Refuse *before* claiming. Claiming is what charges `attempt_count` and
        // takes a lease, so a worker with no engine that claimed first and failed
        // afterwards would walk the whole backlog into `FAILED` — turning a missing
        // configuration into destroyed certificate requests.
        if self.certificate_issuer.is_none() {
            return Err(DeployServiceError::Internal(
                "certificate issuance is not configured for this deployment".to_owned(),
            ));
        }

        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut result = CertificateOrderBatchResult::default();
        // Close out orders that ran past their deadline before claiming new work.
        // An expired order is not claimable, so nothing else would ever move it, and
        // an operator would see a request that is neither progressing nor resolved.
        match self
            .repository
            .fail_expired_certificate_orders(&now, batch_size)
            .await
        {
            Ok(expired) if expired > 0 => {
                result.failed += expired;
                tracing::warn!(orders = expired, "certificate orders passed their deadline");
            }
            Ok(_) => {}
            // A sweep failure must not stop issuance: the orders that *can* make
            // progress are unaffected by one that cannot.
            Err(error) => {
                tracing::warn!(error = %error, "certificate order deadline sweep failed");
            }
        }

        let claims = self
            .repository
            .claim_certificate_orders(worker_id, batch_size, lease_seconds, &now)
            .await?;
        result.claimed = claims.len() as i64;

        for claim in &claims {
            match self.fulfil_certificate_order(worker_id, claim).await {
                Ok(OrderFulfilment::Stored) => {
                    result.stored += 1;
                    tracing::info!(
                        certificate = %claim.certificate_uuid,
                        order = %claim.order_uuid,
                        attempt = claim.attempt_count,
                        "certificate order stored a new version"
                    );
                }
                Ok(OrderFulfilment::Abandoned) => {
                    result.abandoned += 1;
                    tracing::info!(
                        certificate = %claim.certificate_uuid,
                        order = %claim.order_uuid,
                        "certificate order is owned by another worker; stopped work on it"
                    );
                }
                Err(error) => {
                    let code = issuance_failure_code(&error);
                    // The code is what the order keeps; the detail is what an
                    // operator needs, so it goes to the log rather than being
                    // dropped on the floor.
                    if let Err(fail_error) = self
                        .repository
                        .fail_certificate_order(claim.tenant_id, &claim.order_uuid, code)
                        .await
                    {
                        tracing::warn!(
                            order = %claim.order_uuid,
                            error = %fail_error,
                            "could not record the issuance failure on the order"
                        );
                    }
                    result.failed += 1;
                    tracing::warn!(
                        certificate = %claim.certificate_uuid,
                        order = %claim.order_uuid,
                        error_code = code,
                        error = %error,
                        "certificate order failed"
                    );
                }
            }
        }
        Ok(result)
    }

    /// First pass of a manual order: create the CA order, persist the records the
    /// operator must publish, and park the order.
    ///
    /// Returns `Abandoned`, not `Stored`: the worker did stop working on this
    /// order, but it neither stored a version nor failed the order. Failing it
    /// would destroy a request that is waiting on a human.
    async fn prepare_manual_presentation(
        &self,
        worker_id: &str,
        claim: &CertificateOrderClaim,
        certificate: &sdkwork_deploy_contract::CertificateResponse,
    ) -> DeployServiceResult<OrderFulfilment> {
        let issuer = self.certificate_issuer.as_ref().ok_or_else(|| {
            DeployServiceError::Internal(
                "certificate issuance is not configured for this deployment".to_owned(),
            )
        })?;
        // Stop at CHALLENGE_PRESENTING: the remaining transition is the one that
        // tells the CA to validate, and that belongs to the release pass.
        let held = self
            .advance_order_through(
                worker_id,
                claim,
                &[
                    ORDER_STATUS_REQUESTED,
                    ORDER_STATUS_ACCOUNT_READY,
                    ORDER_STATUS_ORDER_PENDING,
                    ORDER_STATUS_CHALLENGE_PRESENTING,
                ],
            )
            .await?;
        if !held {
            return Ok(OrderFulfilment::Abandoned);
        }
        let request = manual_issuance_request(certificate)?;
        let prepared = issuer.prepare(request).await?;
        self.repository
            .record_certificate_order_external_url(
                claim.tenant_id,
                &claim.order_uuid,
                &prepared.order_url,
            )
            .await?;
        let expires_at = (Utc::now() + chrono::Duration::hours(MANUAL_PRESENTATION_TTL_HOURS))
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        for challenge in &prepared.challenges {
            let (Some(record_name), Some(record_value)) = (
                challenge.record_name.as_deref(),
                challenge.record_value.as_deref(),
            ) else {
                // A manual certificate is DNS-01 by planning rule; if the engine
                // ever hands back a non-DNS challenge here, the order's mode and
                // its challenge disagree and continuing would strand it.
                return Err(DeployServiceError::validation(
                    "a manual certificate can only be prepared as a DNS-01 challenge",
                ));
            };
            self.repository
                .record_challenge_presentation(
                    claim.tenant_id,
                    &claim.order_uuid,
                    &challenge.identifier,
                    record_name,
                    record_value,
                    &expires_at,
                    None,
                )
                .await?;
        }
        tracing::info!(
            certificate = %claim.certificate_uuid,
            order = %claim.order_uuid,
            records = prepared.challenges.len(),
            already_valid = prepared.already_valid.len(),
            "manual certificate order prepared; awaiting operator presentation"
        );
        Ok(OrderFulfilment::Abandoned)
    }

    /// Drives one claimed order from wherever it stopped to a stored version.
    async fn fulfil_certificate_order(
        &self,
        worker_id: &str,
        claim: &CertificateOrderClaim,
    ) -> DeployServiceResult<OrderFulfilment> {
        let issuer = self.certificate_issuer.as_ref().ok_or_else(|| {
            DeployServiceError::Internal(
                "certificate issuance is not configured for this deployment".to_owned(),
            )
        })?;

        let certificate = self
            .repository
            .retrieve_certificate(claim.tenant_id, &claim.certificate_uuid)
            .await?;
        if certificate.identifiers.is_empty() {
            return Err(DeployServiceError::validation(
                "certificate has no hostname identifiers to issue for",
            ));
        }

        // Two-phase manual issuance: the first pass only prepares the challenge
        // records and parks the order. It must not advance to validation, because
        // nobody has published anything yet.
        if claim.presentation_mode == "MANUAL" && claim.external_order_url.is_none() {
            return self
                .prepare_manual_presentation(worker_id, claim, &certificate)
                .await;
        }

        // Walk the prefix of the state machine before any network call. Doing it
        // in one place keeps the "one legal step at a time" rule in a single
        // function instead of scattering status writes through the flow.
        let held = self
            .advance_order_through(
                worker_id,
                claim,
                &[
                    ORDER_STATUS_REQUESTED,
                    ORDER_STATUS_ACCOUNT_READY,
                    ORDER_STATUS_ORDER_PENDING,
                    ORDER_STATUS_CHALLENGE_PRESENTING,
                    ORDER_STATUS_CHALLENGE_VALIDATING,
                ],
            )
            .await?;
        if !held {
            return Ok(OrderFulfilment::Abandoned);
        }

        // Second pass of a manual order: the operator has published the records,
        // so the resume is the same conversation the atomic path runs, minus the
        // presentation work (nothing for the platform to withdraw).
        if claim.presentation_mode == "MANUAL" {
            let order_url = claim.external_order_url.as_deref().ok_or_else(|| {
                DeployServiceError::validation(
                    "manual certificate order has no external order url to resume",
                )
            })?;
            let request = manual_issuance_request(&certificate)?;
            let issued = issuer.complete(order_url, request, &[]).await?;
            self.repository
                .record_challenge_result(claim.tenant_id, &claim.order_uuid, None, true, None)
                .await?;
            self.store_issued_version(claim, &issued).await?;
            return Ok(OrderFulfilment::Stored);
        }

        let challenge_type = resolve_challenge_type(
            certificate.certificate_scope.clone(),
            certificate.validation_method.clone(),
        );
        let request = self
            .build_issuance_request(claim, &certificate, challenge_type)
            .await?;

        // The engine performs the whole remaining ACME conversation — account,
        // order, challenge presentation and validation, finalize, download. A
        // failure here is the CA's answer, and it is reported as such rather than
        // being retried inside the worker.
        let issued = issuer.issue(request).await?;

        // A DNS-01 or HTTP-01 challenge that the CA accepted is what moves the
        // order to FINALIZING; recording it here keeps the transition owned by the
        // same call that proved it.
        self.repository
            .record_challenge_result(claim.tenant_id, &claim.order_uuid, None, true, None)
            .await?;

        self.store_issued_version(claim, &issued).await?;
        Ok(OrderFulfilment::Stored)
    }

    /// Advances an order along `path`, one transition at a time.
    ///
    /// Returns `false` when the worker no longer holds the order's lease. Every step
    /// is fenced, so losing the lease mid-path stops the walk at the first step that
    /// no longer belongs to this worker; the caller must then abandon the order
    /// rather than fail it.
    async fn advance_order_through(
        &self,
        worker_id: &str,
        claim: &CertificateOrderClaim,
        path: &[&str],
    ) -> DeployServiceResult<bool> {
        // Resume from where the order actually is, not from the head of the path:
        // a reclaimed order must not replay transitions it already made.
        let start = match path.iter().position(|state| *state == claim.status) {
            Some(index) => index,
            // An order in a state outside this prefix (FINALIZING, say) is already
            // past it, and replaying would be a no-op at best.
            None => return Ok(true),
        };
        for window in path[start..].windows(2) {
            let (from, to) = (window[0], window[1]);
            let reached = self
                .repository
                .advance_leased_certificate_order(
                    claim.tenant_id,
                    &claim.order_uuid,
                    worker_id,
                    from,
                    to,
                )
                .await?;
            if reached != to {
                // Not an error: the order moved on without this worker, which is
                // exactly what the lease exists to allow.
                tracing::info!(
                    order = %claim.order_uuid,
                    expected = to,
                    observed = %reached,
                    "certificate order lease was lost before the transition landed"
                );
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Builds the engine request, resolving the DNS-01 presenter when the chosen
    /// challenge needs one.
    async fn build_issuance_request(
        &self,
        claim: &CertificateOrderClaim,
        certificate: &sdkwork_deploy_contract::CertificateResponse,
        challenge_type: &str,
    ) -> DeployServiceResult<CertificateIssuanceRequest> {
        let hostnames = certificate.identifiers.clone();
        let dns01 = if challenge_type == CHALLENGE_TYPE_DNS_01 {
            // Every identifier must be presentable, and they must agree on one
            // zone: a wildcard's apex and its wildcard share a zone by
            // construction, while a multi-hostname request that spans two
            // providers cannot be presented by one adapter.
            let mut context: Option<CertificateDns01Context> = None;
            for hostname in &hostnames {
                let resolved = self
                    .certificate_dns01
                    .resolve(CertificateDns01Selector {
                        tenant_id: claim.tenant_id,
                        certificate_provider_account_id: certificate.provider_account_id.as_deref(),
                        hostname,
                    })
                    .await?
                    .ok_or_else(|| {
                        DeployServiceError::validation(format!(
                            "no DNS provider credential is configured for {hostname}; \
                             a wildcard or DNS-01 certificate cannot be presented by hand \
                             through this worker"
                        ))
                    })?;
                match &context {
                    Some(existing) if existing.zone_apex != resolved.zone_apex => {
                        return Err(DeployServiceError::validation(format!(
                            "identifiers span more than one DNS zone ({} and {}); \
                             a single order cannot be presented automatically",
                            existing.zone_apex, resolved.zone_apex
                        )));
                    }
                    Some(_) => {}
                    None => context = Some(resolved),
                }
            }
            Some(context.expect("the identifier loop is never empty"))
        } else {
            None
        };

        Ok(CertificateIssuanceRequest {
            hostnames,
            cert_name: certificate.cert_name.clone(),
            key_algorithm: certificate.preferred_key_algorithm.clone(),
            dns01,
        })
    }

    /// Validates the issued material and stores it as the order's version.
    ///
    /// The SAN comparison is the one check `seal_issued_material` cannot make: it
    /// proves the bytes match the declared evidence, but only the caller knows
    /// which names were *asked for*. A CA that returns a broader or narrower SAN
    /// set has issued the wrong certificate, and storing it would silently serve
    /// a name nobody requested — or serve none of the ones that were.
    async fn store_issued_version(
        &self,
        claim: &CertificateOrderClaim,
        issued: &IssuedCertificateMaterial,
    ) -> DeployServiceResult<()> {
        let certificate = self
            .repository
            .retrieve_certificate(claim.tenant_id, &claim.certificate_uuid)
            .await?;
        let requested: std::collections::BTreeSet<String> = certificate
            .identifiers
            .iter()
            .map(|name| normalize_dns_name(name))
            .collect();
        let returned: std::collections::BTreeSet<String> = issued
            .san_list
            .iter()
            .map(|name| normalize_dns_name(name))
            .collect();
        if requested != returned {
            return Err(DeployServiceError::validation(format!(
                "issued certificate covers a different name set than requested: \
                 requested {requested:?}, issued {returned:?}"
            )));
        }

        let certificate_version_uuid = sdkwork_database_id::uuid_v4();
        // `cert_pem` is the leaf-first chain blob and `chain_pem` carries the same
        // bytes; the engine's `chainSha256` is taken over that blob verbatim, so
        // the payload must be built from the identical bytes or the digest check
        // inside `seal_issued_material` refuses it.
        let payload = CertificateMaterialPayload {
            certificate_chain_pem: issued.cert_pem.clone(),
            private_key_pem: issued.private_key_pem.clone(),
            root_pem: String::new(),
        };
        let evidence = DeclaredCertificateEvidence {
            serial_sha256: &issued.serial_sha256,
            fingerprint_sha256: &issued.fingerprint_sha256,
            spki_sha256: &issued.spki_sha256,
            chain_sha256: &issued.chain_sha256,
            issuer: &issued.issuer,
            subject: &issued.subject,
            key_algorithm: &issued.key_algorithm,
            not_before: &issued.not_before,
            not_after: &issued.not_after,
        };
        let material = seal_issued_material(
            &payload,
            &evidence,
            &certificate_version_uuid,
            self.certificate_material_key.as_deref(),
            self.certificate_trust_anchors.as_deref(),
        )?;

        self.repository
            .store_certificate_version(
                claim.tenant_id,
                &certificate_version_uuid,
                &claim.order_uuid,
                claim.requested_version_no,
                &issued.serial_sha256,
                &issued.fingerprint_sha256,
                &issued.spki_sha256,
                &issued.chain_sha256,
                &issued.issuer,
                &issued.subject,
                &issued.key_algorithm,
                &issued.not_before,
                &issued.not_after,
                &format!("secret://deploy/certificate/{certificate_version_uuid}"),
                &material,
            )
            .await?;
        self.record_ari_window(claim, issued).await;
        // Archiving is best effort by contract (��4.6.4): the version is stored and
        // the order is `VERSION_STORED` before this runs, and nothing it returns can
        // change either. The row it reads was written in the version's own
        // transaction, so a crash between the commit and this line leaves a
        // `PENDING` row the worker's drain picks up rather than a version with no
        // archive intent.
        //
        // A deployment with no archive store marks the row `SKIPPED` here, which is
        // the honest report for "switched off" �� the alternative, leaving it
        // `PENDING` forever, would show every certificate as perpetually archiving.
        if self.certificate_archive_configured() {
            let attempt = self
                .archive_certificate_version(
                    &certificate_version_uuid,
                    self.certificate_archive_attempt_timeout(),
                )
                .await;
            if !attempt.is_quiet() && !matches!(attempt, CertificateArchiveAttempt::Archived) {
                tracing::info!(
                    certificate = %claim.certificate_uuid,
                    certificate_version = %certificate_version_uuid,
                    outcome = ?attempt,
                    "certificate version stored; its drive archive is still pending"
                );
            }
        } else {
            self.skip_certificate_archive_version(claim.tenant_id, &certificate_version_uuid)
                .await;
        }
        Ok(())
    }

    /// Records the CA's suggested renewal window for the version just stored.
    ///
    /// Best effort by design: a CA without ARI, a transient lookup failure, or a
    /// persistence error must not turn a successful issuance into a failed one.
    /// The certificate keeps the local window rule until a suggestion exists, and
    /// a failure here is only ever a log line.
    async fn record_ari_window(
        &self,
        claim: &CertificateOrderClaim,
        issued: &IssuedCertificateMaterial,
    ) {
        let Some(issuance) = self.certificate_issuer.as_ref() else {
            return;
        };
        let window = match issuance.renewal_window(&issued.cert_pem).await {
            Ok(window) => window,
            Err(error) => {
                tracing::warn!(
                    certificate = %claim.certificate_uuid,
                    order = %claim.order_uuid,
                    error = %error,
                    "ARI lookup failed; the local renewal window stays in charge"
                );
                return;
            }
        };
        let Some(window) = window else {
            // The CA does not publish ARI (or has none for this certificate).
            // Leaving the columns NULL is the honest record of that.
            return;
        };
        let checked_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        if let Err(error) = self
            .repository
            .record_certificate_ari_window(
                claim.tenant_id,
                &claim.certificate_uuid,
                &window.window_start,
                &window.window_end,
                &checked_at,
            )
            .await
        {
            tracing::warn!(
                certificate = %claim.certificate_uuid,
                error = %error,
                "recorded ARI window could not be persisted"
            );
        }
    }
}

const MAXIMUM_ISSUANCE_BATCH_SIZE: i64 = 25;
const MINIMUM_ISSUANCE_LEASE_SECONDS: i64 = 30;
const MAXIMUM_ISSUANCE_LEASE_SECONDS: i64 = 1_800;

/// Lowercases and strips the trailing dot, which is the form both a CA and a
/// stored identifier should be compared in.
fn normalize_dns_name(name: &str) -> String {
    name.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// A bounded reason code for the order ledger.
///
/// The ledger keeps codes, not sentences: an operator filters on them, and a
/// message that varies per attempt would make that impossible.
fn issuance_failure_code(error: &DeployServiceError) -> &'static str {
    match error {
        DeployServiceError::Validation(_) => ORDER_ERROR_VALIDATION_FAILED,
        DeployServiceError::Conflict(_) => ORDER_ERROR_CONFLICT,
        DeployServiceError::NotFound(_) => ORDER_ERROR_NOT_FOUND,
        DeployServiceError::Forbidden(_) => ORDER_ERROR_FORBIDDEN,
        DeployServiceError::QuotaExceeded(_) => ORDER_ERROR_QUOTA_EXCEEDED,
        DeployServiceError::DatabaseUnavailable => ORDER_ERROR_DATABASE_UNAVAILABLE,
        DeployServiceError::Internal(_) => ORDER_ERROR_INTERNAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_scope_always_resolves_to_dns01() {
        // The rule the database CHECK also enforces: a wildcard SAN cannot be
        // proven over HTTP-01, so the scope must beat the requested method rather
        // than the request failing later at the CA.
        for method in ValidationMethod::ALL {
            assert_eq!(
                resolve_challenge_type(CertificateScope::Wildcard, method),
                CHALLENGE_TYPE_DNS_01,
                "wildcard with {method:?} must be DNS-01"
            );
        }
    }

    #[test]
    fn single_domain_auto_and_http01_use_the_edge_path() {
        assert_eq!(
            resolve_challenge_type(CertificateScope::SingleDomain, ValidationMethod::Auto),
            CHALLENGE_TYPE_HTTP_01
        );
        assert_eq!(
            resolve_challenge_type(CertificateScope::SingleDomain, ValidationMethod::Http01),
            CHALLENGE_TYPE_HTTP_01
        );
    }

    #[test]
    fn single_domain_dns01_honours_the_request() {
        assert_eq!(
            resolve_challenge_type(CertificateScope::SingleDomain, ValidationMethod::Dns01),
            CHALLENGE_TYPE_DNS_01
        );
    }

    #[test]
    fn dns_names_compare_ignoring_case_and_root_dot() {
        assert_eq!(normalize_dns_name("Example.COM."), "example.com");
        assert_eq!(normalize_dns_name("  a.example.com  "), "a.example.com");
    }
}
