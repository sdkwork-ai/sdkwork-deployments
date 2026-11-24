//! Pure certificate-planning rules: scope validation, wildcard apex completion,
//! and CA budget constants.
//!
//! Deliberately free of I/O so one implementation drives the API validation
//! path, the order orchestrator, and the unit tests. The database only ever
//! sees the planned identifier list this module returns.

use std::collections::BTreeSet;

use crate::dto::{wildcard_apex, CertificateScope, ChallengePresentation, ValidationMethod};

/// Upper bound on identifiers per certificate.
///
/// This is the **product** ceiling and must equal the ACME engine's own
/// identifier ceiling: an order the engine refuses is an order the API should
/// have refused, and a request that passes validation only to fail at issuance
/// burns the CA's order budget and the operator's time. The storage DDL keeps a
/// wider `position` bound because it is a row-count bound, not a product rule;
/// the API gate below is what actually applies this number.
pub const MAX_CERTIFICATE_IDENTIFIERS: usize = 8;

/// Shape of one certificate identifier, as persisted in
/// `deploy_certificate_identifier.identifier_type`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CertificateIdentifierType {
    Exact,
    Wildcard,
}

impl CertificateIdentifierType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "EXACT",
            Self::Wildcard => "WILDCARD",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "EXACT" => Some(Self::Exact),
            "WILDCARD" => Some(Self::Wildcard),
            _ => None,
        }
    }

    /// Position rank used for the deterministic identifier ordering the
    /// `(certificate_id, position)` unique constraint depends on. Wildcards
    /// lead so the primary identifier of a wildcard certificate is the wildcard
    /// itself.
    pub const fn position_rank(self) -> u8 {
        match self {
            Self::Wildcard => 0,
            Self::Exact => 1,
        }
    }
}

/// One planned identifier: the normalized hostname and its shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedCertificateIdentifier {
    pub hostname: String,
    pub identifier_type: CertificateIdentifierType,
}

/// Stable, non-sensitive planning failures.
///
/// Each variant maps to a machine-readable `code()` so the API can return a
/// bounded problem code without leaking hostname inventory or provider detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CertificatePlanError {
    /// No identifier was requested.
    EmptyIdentifierSet,
    /// A hostname is not a safe ASCII DNS name.
    InvalidHostname,
    /// The same hostname appears twice ignoring ASCII case.
    DuplicateHostname,
    /// A wildcard is not a single leading label (`*`, `*.*.a.example.com`,
    /// `a.*.example.com`).
    MultiLevelWildcard,
    /// A wildcard identifier was sent for a single-domain scope.
    ScopeRequiresWildcard,
    /// A wildcard scope was requested without any wildcard identifier.
    WildcardScopeRequiresWildcardIdentifier,
    /// A wildcard identifier's apex is not a claim owned by this tenant.
    WildcardApexMissing,
    /// `HTTP_01` cannot authorize a wildcard.
    Http01NotAllowedForWildcard,
    /// A manual presentation was requested for HTTP-01, which only the platform
    /// can serve (the edge webroot is not operator-addressable).
    ManualRequiresDns01,
    /// DNS-01 was requested as managed but no DNS provider account covers every
    /// identifier.
    NoDnsProviderConfigured,
    /// HTTP-01 was requested as managed but the deployment serves no webroot.
    Http01WebrootNotConfigured,
    /// A single-domain scope was planned with anything other than exactly one
    /// exact identifier.
    SingleDomainRequiresOneIdentifier,
    /// The planned identifier set exceeds [`MAX_CERTIFICATE_IDENTIFIERS`].
    TooManyIdentifiers,
}

impl CertificatePlanError {
    /// Stable machine code for API problem details.
    pub const fn code(self) -> &'static str {
        match self {
            Self::EmptyIdentifierSet => "CERTIFICATE_IDENTIFIERS_REQUIRED",
            Self::InvalidHostname => "CERTIFICATE_HOSTNAME_INVALID",
            Self::DuplicateHostname => "CERTIFICATE_HOSTNAME_DUPLICATE",
            Self::MultiLevelWildcard => "CERTIFICATE_WILDCARD_DEPTH_UNSUPPORTED",
            Self::ScopeRequiresWildcard => "CERTIFICATE_SCOPE_WILDCARD_REQUIRED",
            Self::WildcardScopeRequiresWildcardIdentifier => {
                "CERTIFICATE_SCOPE_WILDCARD_IDENTIFIER_REQUIRED"
            }
            Self::WildcardApexMissing => "CERTIFICATE_WILDCARD_APEX_CLAIM_REQUIRED",
            Self::Http01NotAllowedForWildcard => "CERTIFICATE_WILDCARD_REQUIRES_DNS01",
            Self::ManualRequiresDns01 => "CERTIFICATE_MANUAL_REQUIRES_DNS01",
            Self::NoDnsProviderConfigured => "CERTIFICATE_NO_DNS_PROVIDER",
            Self::Http01WebrootNotConfigured => "CERTIFICATE_HTTP01_WEBROOT_MISSING",
            Self::SingleDomainRequiresOneIdentifier => "CERTIFICATE_SINGLE_DOMAIN_IDENTIFIER_COUNT",
            Self::TooManyIdentifiers => "CERTIFICATE_IDENTIFIER_LIMIT_EXCEEDED",
        }
    }

    /// Operator-facing sentence. Never includes the offending hostname, so a
    /// rejection cannot be used to probe another tenant's inventory.
    pub const fn message(self) -> &'static str {
        match self {
            Self::EmptyIdentifierSet => "at least one hostname identifier is required",
            Self::InvalidHostname => "every identifier must be a safe ASCII DNS hostname",
            Self::DuplicateHostname => "identifiers must be unique ignoring ASCII case",
            Self::MultiLevelWildcard => {
                "a certificate may contain only a single leading-label wildcard, such as *.example.com"
            }
            Self::ScopeRequiresWildcard => {
                "a wildcard identifier requires the WILDCARD certificate scope"
            }
            Self::WildcardScopeRequiresWildcardIdentifier => {
                "the WILDCARD certificate scope requires a wildcard identifier"
            }
            Self::WildcardApexMissing => {
                "the apex of a wildcard identifier must be an active verified hostname of this tenant"
            }
            Self::Http01NotAllowedForWildcard => {
                "wildcard identifiers require the DNS_01 validation method"
            }
            Self::ManualRequiresDns01 => {
                "a manual presentation requires the DNS_01 validation method"
            }
            Self::NoDnsProviderConfigured => {
                "no DNS provider account covers every identifier; bind one or use the manual \
                 DNS-01 presentation"
            }
            Self::Http01WebrootNotConfigured => {
                "this deployment serves no webroot for HTTP-01; use the DNS_01 validation method"
            }
            Self::SingleDomainRequiresOneIdentifier => {
                "a single-domain certificate covers exactly one hostname"
            }
            Self::TooManyIdentifiers => {
                "a certificate may cover at most 8 hostname identifiers"
            }
        }
    }
}

/// Normalizes a hostname and classifies it as exact or wildcard.
///
/// Canonicalization follows ADR-20260723 §2: lower-case ASCII, no trailing dot.
/// A single leading `*.` is preserved; any other `*` position is rejected.
fn normalize_and_classify(
    raw: &str,
) -> Result<(String, CertificateIdentifierType), CertificatePlanError> {
    let trimmed = raw.trim().trim_end_matches('.');
    let lowered = trimmed.to_ascii_lowercase();
    if lowered.is_empty() || lowered.len() > 253 {
        return Err(CertificatePlanError::InvalidHostname);
    }
    let (identifier_type, base) = match lowered.strip_prefix("*.") {
        Some(base) => (CertificateIdentifierType::Wildcard, base),
        None => (CertificateIdentifierType::Exact, lowered.as_str()),
    };
    if base.is_empty() || base.contains('*') {
        return Err(CertificatePlanError::MultiLevelWildcard);
    }
    if base.starts_with('.') || base.ends_with('.') || base.contains("..") {
        return Err(CertificatePlanError::InvalidHostname);
    }
    let valid = base.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    });
    if !valid || !base.contains('.') {
        return Err(CertificatePlanError::InvalidHostname);
    }
    let hostname = match identifier_type {
        CertificateIdentifierType::Wildcard => format!("*.{base}"),
        CertificateIdentifierType::Exact => base.to_owned(),
    };
    Ok((hostname, identifier_type))
}

/// Resolves the concrete ACME challenge method for a scope.
///
/// `HTTP_01` combined with a wildcard scope is a caller error, not a silent
/// downgrade: returning the rejected method as an error is what keeps the UI
/// honest instead of showing a method the order will not use.
pub fn resolve_validation_method(
    scope: CertificateScope,
    requested: ValidationMethod,
) -> Result<ValidationMethod, CertificatePlanError> {
    match (scope, requested) {
        (CertificateScope::Wildcard, ValidationMethod::Http01) => {
            Err(CertificatePlanError::Http01NotAllowedForWildcard)
        }
        _ => Ok(requested.resolve_for_scope(scope)),
    }
}

/// Builds the canonical identifier plan for a certificate request.
///
/// For a wildcard scope every wildcard identifier's apex is added as a second
/// exact identifier when it is not already present, because a wildcard SAN does
/// not cover the bare domain. The caller is responsible for confirming that the
/// added apex is an active verified claim of the same tenant; when it is not,
/// the caller returns [`CertificatePlanError::WildcardApexMissing`].
pub fn plan_certificate_identifiers(
    scope: CertificateScope,
    requested_hostnames: &[String],
) -> Result<Vec<PlannedCertificateIdentifier>, CertificatePlanError> {
    if requested_hostnames.is_empty() {
        return Err(CertificatePlanError::EmptyIdentifierSet);
    }
    let mut planned = Vec::with_capacity(requested_hostnames.len() + 1);
    let mut seen = BTreeSet::new();
    for raw in requested_hostnames {
        let (hostname, identifier_type) = normalize_and_classify(raw)?;
        if !seen.insert(hostname.clone()) {
            return Err(CertificatePlanError::DuplicateHostname);
        }
        planned.push(PlannedCertificateIdentifier {
            hostname,
            identifier_type,
        });
    }

    let wildcard_count = planned
        .iter()
        .filter(|identifier| identifier.identifier_type == CertificateIdentifierType::Wildcard)
        .count();
    match scope {
        CertificateScope::SingleDomain => {
            if wildcard_count > 0 {
                return Err(CertificatePlanError::ScopeRequiresWildcard);
            }
            // §5.2 rule 3: a single-domain certificate covers exactly one exact
            // FQDN. Accepting a longer set would let the single-domain product
            // ship a multi-SAN certificate under a scope that claims otherwise,
            // and every consumer keyed on the scope — quota accounting, UI
            // grouping, the identifier count shown to the operator — would lie
            // with it.
            if planned.len() != 1 {
                return Err(CertificatePlanError::SingleDomainRequiresOneIdentifier);
            }
        }
        CertificateScope::Wildcard => {
            if wildcard_count == 0 {
                return Err(CertificatePlanError::WildcardScopeRequiresWildcardIdentifier);
            }
            let apexes = planned
                .iter()
                .filter(|identifier| {
                    identifier.identifier_type == CertificateIdentifierType::Wildcard
                })
                .filter_map(|identifier| wildcard_apex(&identifier.hostname).map(str::to_owned))
                .collect::<Vec<_>>();
            for apex in apexes {
                if seen.insert(apex.clone()) {
                    planned.push(PlannedCertificateIdentifier {
                        hostname: apex,
                        identifier_type: CertificateIdentifierType::Exact,
                    });
                }
            }
        }
    }

    if planned.len() > MAX_CERTIFICATE_IDENTIFIERS {
        return Err(CertificatePlanError::TooManyIdentifiers);
    }
    // Deterministic ordering: the `(certificate_id, position)` unique constraint
    // requires a stable sequence, and wildcards lead the list.
    planned.sort_by(|left, right| {
        left.identifier_type
            .position_rank()
            .cmp(&right.identifier_type.position_rank())
            .then_with(|| left.hostname.cmp(&right.hostname))
    });
    Ok(planned)
}

/// What this deployment can actually present for one request.
///
/// Computed by the caller from live configuration rather than assumed here, so
/// the decision function stays pure and the same matrix is used by the API
/// validation path and by the order orchestrator.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChallengeAvailability {
    /// A DNS provider account covers every identifier in the request.
    pub dns01_provider_available: bool,
    /// The deployment has a webroot the edge already serves.
    pub http01_webroot_configured: bool,
}

/// The challenge a request resolved to, and who presents it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationPlan {
    pub presentation: ChallengePresentation,
    pub method: ValidationMethod,
}

/// Resolves the (method, presentation) pair for one certificate request.
///
/// The rules, in evaluation order:
///
/// 1. A wildcard identifier can only be proven by DNS-01, whatever was asked.
/// 2. Manual presentation exists only for DNS-01; HTTP-01 is served by the edge,
///    whose webroot an operator cannot write.
/// 3. `AUTO` resolves through the scope first, then, for a manual request, to
///    DNS-01 (a manual request is by definition "I will publish the record").
/// 4. A managed method this deployment cannot actually present is refused with a
///    stable code instead of failing later at the CA, which would burn its
///    order and failed-validation budget.
pub fn resolve_presentation(
    scope: CertificateScope,
    requested_method: ValidationMethod,
    requested_presentation: ChallengePresentation,
    availability: ChallengeAvailability,
) -> Result<PresentationPlan, CertificatePlanError> {
    if scope == CertificateScope::Wildcard && requested_method == ValidationMethod::Http01 {
        return Err(CertificatePlanError::Http01NotAllowedForWildcard);
    }
    if requested_presentation == ChallengePresentation::Manual
        && requested_method == ValidationMethod::Http01
    {
        return Err(CertificatePlanError::ManualRequiresDns01);
    }

    let method = match (requested_presentation, requested_method) {
        (ChallengePresentation::Manual, ValidationMethod::Auto) => ValidationMethod::Dns01,
        (_, method) => {
            if scope == CertificateScope::Wildcard {
                ValidationMethod::Dns01
            } else {
                method.resolve_for_scope(scope)
            }
        }
    };

    match (requested_presentation, method) {
        (ChallengePresentation::Managed, ValidationMethod::Dns01)
            if !availability.dns01_provider_available =>
        {
            Err(CertificatePlanError::NoDnsProviderConfigured)
        }
        (ChallengePresentation::Managed, ValidationMethod::Http01)
            if !availability.http01_webroot_configured =>
        {
            Err(CertificatePlanError::Http01WebrootNotConfigured)
        }
        _ => Ok(PresentationPlan {
            presentation: requested_presentation,
            method,
        }),
    }
}

/// Rolling CA budget dimension.
///
/// The limits mirror the public CA policy surface so a refusal is explainable
/// before an order is created, instead of surfacing as an opaque provider error
/// after the fact. They are configuration defaults, not protocol constants: a
/// deployment may tighten them per plan.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CertificateQuotaKind {
    /// Certificates issued for one registered domain.
    CertificatesPerDomain,
    /// Certificates reusing one identical identifier set and key algorithm.
    DuplicateCertificateSet,
    /// New orders opened by the tenant.
    NewOrders,
    /// Failed authorizations for one account and hostname.
    FailedValidations,
    /// Orders currently `PENDING` or `RUNNING` for the tenant.
    ConcurrentOrders,
}

impl CertificateQuotaKind {
    /// Persisted discriminator; must match
    /// `chk_deploy_certificate_quota_usage_kind`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CertificatesPerDomain => "CERTIFICATES_PER_DOMAIN",
            Self::DuplicateCertificateSet => "DUPLICATE_CERTIFICATE_SET",
            Self::NewOrders => "NEW_ORDERS",
            Self::FailedValidations => "FAILED_VALIDATIONS",
            Self::ConcurrentOrders => "CONCURRENT_ORDERS",
        }
    }

    /// Rolling window length in seconds. Instantaneous counters use one second
    /// so the bucket arithmetic stays uniform.
    pub const fn window_seconds(self) -> i64 {
        match self {
            Self::CertificatesPerDomain => 7 * 24 * 3600,
            Self::DuplicateCertificateSet => 7 * 24 * 3600,
            Self::NewOrders => 3 * 3600,
            Self::FailedValidations => 3600,
            Self::ConcurrentOrders => 1,
        }
    }

    /// Default ceiling inside the window.
    pub const fn default_limit(self) -> i64 {
        match self {
            Self::CertificatesPerDomain => 50,
            Self::DuplicateCertificateSet => 5,
            Self::NewOrders => 300,
            Self::FailedValidations => 5,
            Self::ConcurrentOrders => 8,
        }
    }

    /// Whether the counter is keyed by an additional scope value (registered
    /// domain, hostname, or identifier-set digest) beyond the tenant.
    pub const fn is_scope_keyed(self) -> bool {
        matches!(
            self,
            Self::CertificatesPerDomain | Self::DuplicateCertificateSet | Self::FailedValidations
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(
        scope: CertificateScope,
        hosts: &[&str],
    ) -> Result<Vec<PlannedCertificateIdentifier>, CertificatePlanError> {
        let owned = hosts
            .iter()
            .map(|host| (*host).to_owned())
            .collect::<Vec<_>>();
        plan_certificate_identifiers(scope, &owned)
    }

    fn hostnames(identifiers: &[PlannedCertificateIdentifier]) -> Vec<&str> {
        identifiers
            .iter()
            .map(|identifier| identifier.hostname.as_str())
            .collect()
    }

    #[test]
    fn single_domain_keeps_one_exact_identifier() {
        let planned = plan(CertificateScope::SingleDomain, &["www.example.com"]).expect("plan");
        assert_eq!(hostnames(&planned), ["www.example.com"]);
        assert_eq!(planned[0].identifier_type, CertificateIdentifierType::Exact);
    }

    #[test]
    fn wildcard_scope_always_adds_the_apex() {
        let planned = plan(CertificateScope::Wildcard, &["*.example.com"]).expect("plan");
        assert_eq!(hostnames(&planned), ["*.example.com", "example.com"]);
        assert_eq!(
            planned[0].identifier_type,
            CertificateIdentifierType::Wildcard
        );
        assert_eq!(planned[1].identifier_type, CertificateIdentifierType::Exact);
    }

    #[test]
    fn wildcard_apex_is_not_duplicated_when_already_requested() {
        let planned = plan(
            CertificateScope::Wildcard,
            &["example.com", "*.example.com"],
        )
        .expect("plan");
        assert_eq!(hostnames(&planned), ["*.example.com", "example.com"]);
    }

    #[test]
    fn wildcard_scope_handles_a_deep_wildcard_claim() {
        let planned = plan(CertificateScope::Wildcard, &["*.a.example.com"]).expect("plan");
        assert_eq!(hostnames(&planned), ["*.a.example.com", "a.example.com"]);
    }

    #[test]
    fn wildcard_scope_rejects_a_set_without_a_wildcard() {
        assert_eq!(
            plan(CertificateScope::Wildcard, &["www.example.com"]),
            Err(CertificatePlanError::WildcardScopeRequiresWildcardIdentifier)
        );
    }

    #[test]
    fn single_domain_scope_rejects_a_wildcard() {
        assert_eq!(
            plan(CertificateScope::SingleDomain, &["*.example.com"]),
            Err(CertificatePlanError::ScopeRequiresWildcard)
        );
    }

    #[test]
    fn single_domain_scope_covers_exactly_one_hostname() {
        assert_eq!(
            plan(
                CertificateScope::SingleDomain,
                &["example.com", "www.example.com"]
            ),
            Err(CertificatePlanError::SingleDomainRequiresOneIdentifier)
        );
        assert_eq!(
            plan(CertificateScope::SingleDomain, &[]),
            Err(CertificatePlanError::EmptyIdentifierSet)
        );
    }

    #[test]
    fn rejects_multi_level_and_bare_wildcards() {
        for host in ["*", "*.*.example.com", "a.*.example.com", "foo.*"] {
            let error = plan(CertificateScope::SingleDomain, &[host]);
            assert!(error.is_err(), "{host} must be rejected");
        }
    }

    #[test]
    fn rejects_duplicate_and_malformed_hostnames() {
        assert_eq!(
            plan(
                CertificateScope::Wildcard,
                &["*.example.com", "*.EXAMPLE.com"]
            ),
            Err(CertificatePlanError::DuplicateHostname)
        );
        for host in [
            "",
            "..",
            "example",
            "-bad.example.com",
            "bad-.example.com",
            "a..b.com",
        ] {
            assert!(
                plan(CertificateScope::SingleDomain, &[host]).is_err(),
                "{host} must be rejected"
            );
        }
    }

    #[test]
    fn normalizes_case_and_trailing_dot() {
        let planned = plan(CertificateScope::SingleDomain, &["WWW.Example.COM."]).expect("plan");
        assert_eq!(hostnames(&planned), ["www.example.com"]);
    }

    #[test]
    fn wildcard_cannot_use_http01() {
        assert_eq!(
            resolve_validation_method(CertificateScope::Wildcard, ValidationMethod::Http01),
            Err(CertificatePlanError::Http01NotAllowedForWildcard)
        );
        assert_eq!(
            resolve_validation_method(CertificateScope::Wildcard, ValidationMethod::Auto),
            Ok(ValidationMethod::Dns01)
        );
        assert_eq!(
            resolve_validation_method(CertificateScope::SingleDomain, ValidationMethod::Auto),
            Ok(ValidationMethod::Http01)
        );
        assert_eq!(
            resolve_validation_method(CertificateScope::SingleDomain, ValidationMethod::Dns01),
            Ok(ValidationMethod::Dns01)
        );
    }

    #[test]
    fn a_manual_request_resolves_to_dns01_and_needs_no_provider_account() {
        // The whole point of manual presentation: it must work exactly when no
        // DNS provider credential exists.
        for requested in [ValidationMethod::Auto, ValidationMethod::Dns01] {
            let plan = resolve_presentation(
                CertificateScope::SingleDomain,
                requested,
                ChallengePresentation::Manual,
                ChallengeAvailability::default(),
            )
            .expect("manual DNS-01 needs no provider account");
            assert_eq!(plan.method, ValidationMethod::Dns01);
            assert_eq!(plan.presentation, ChallengePresentation::Manual);
        }
    }

    #[test]
    fn a_manual_http01_request_is_refused_with_its_own_code() {
        assert_eq!(
            resolve_presentation(
                CertificateScope::SingleDomain,
                ValidationMethod::Http01,
                ChallengePresentation::Manual,
                ChallengeAvailability {
                    http01_webroot_configured: true,
                    ..ChallengeAvailability::default()
                },
            ),
            Err(CertificatePlanError::ManualRequiresDns01)
        );
    }

    #[test]
    fn a_wildcard_manual_request_stays_dns01() {
        let plan = resolve_presentation(
            CertificateScope::Wildcard,
            ValidationMethod::Auto,
            ChallengePresentation::Manual,
            ChallengeAvailability::default(),
        )
        .expect("wildcard manual DNS-01");
        assert_eq!(plan.method, ValidationMethod::Dns01);
        assert_eq!(plan.presentation, ChallengePresentation::Manual);
    }

    #[test]
    fn a_managed_method_the_deployment_cannot_present_is_refused() {
        assert_eq!(
            resolve_presentation(
                CertificateScope::SingleDomain,
                ValidationMethod::Dns01,
                ChallengePresentation::Managed,
                ChallengeAvailability::default(),
            ),
            Err(CertificatePlanError::NoDnsProviderConfigured)
        );
        assert_eq!(
            resolve_presentation(
                CertificateScope::SingleDomain,
                ValidationMethod::Http01,
                ChallengePresentation::Managed,
                ChallengeAvailability::default(),
            ),
            Err(CertificatePlanError::Http01WebrootNotConfigured)
        );
    }

    #[test]
    fn a_fully_available_managed_request_keeps_its_method() {
        let availability = ChallengeAvailability {
            dns01_provider_available: true,
            http01_webroot_configured: true,
        };
        let plan = resolve_presentation(
            CertificateScope::SingleDomain,
            ValidationMethod::Auto,
            ChallengePresentation::Managed,
            availability,
        )
        .expect("managed auto");
        assert_eq!(plan.method, ValidationMethod::Http01);
        assert_eq!(plan.presentation, ChallengePresentation::Managed);

        let plan = resolve_presentation(
            CertificateScope::SingleDomain,
            ValidationMethod::Dns01,
            ChallengePresentation::Managed,
            availability,
        )
        .expect("managed dns-01");
        assert_eq!(plan.method, ValidationMethod::Dns01);
    }

    #[test]
    fn presentation_tokens_round_trip() {
        for presentation in ChallengePresentation::ALL {
            assert_eq!(
                ChallengePresentation::parse(presentation.as_str()),
                Some(presentation)
            );
        }
        assert_eq!(
            ChallengePresentation::default(),
            ChallengePresentation::Managed
        );
    }

    #[test]
    fn the_product_identifier_cap_matches_the_engine_ceiling() {
        // The engine refuses an order above its own ceiling before contacting the
        // CA. If this number were larger, a request would pass validation and then
        // fail at issuance, after the account/order budget was already spent.
        assert_eq!(MAX_CERTIFICATE_IDENTIFIERS, 8);
        let four_wildcards = [
            "*.a.example.com",
            "*.b.example.com",
            "*.c.example.com",
            "*.d.example.com",
        ];
        // Each wildcard plans its apex too, so four wildcards fill the cap exactly.
        assert_eq!(
            plan(CertificateScope::Wildcard, &four_wildcards)
                .unwrap()
                .len(),
            8
        );
        let five_wildcards = [
            "*.a.example.com",
            "*.b.example.com",
            "*.c.example.com",
            "*.d.example.com",
            "*.e.example.com",
        ];
        assert_eq!(
            plan(CertificateScope::Wildcard, &five_wildcards),
            Err(CertificatePlanError::TooManyIdentifiers)
        );
    }

    #[test]
    fn an_absent_challenge_presentation_defaults_to_managed() {
        // Clients that predate the switch keep working: an omitted
        // `challengePresentation` must mean "the platform presents it", which is
        // exactly the behaviour those clients already relied on.
        let request: crate::CreateCertificateRequest = serde_json::from_value(serde_json::json!({
            "certName": "example.com",
            "domainIds": ["example.com"],
        }))
        .expect("request parses without the new field");
        assert_eq!(
            request.challenge_presentation,
            crate::ChallengePresentation::Managed
        );
        assert_eq!(request.validation_method, ValidationMethod::Auto);

        let with_manual: crate::CreateCertificateRequest =
            serde_json::from_value(serde_json::json!({
                "certName": "example.com",
                "domainIds": ["example.com"],
                "validationMethod": "DNS_01",
                "challengePresentation": "MANUAL",
            }))
            .expect("manual presentation parses");
        assert_eq!(
            with_manual.challenge_presentation,
            crate::ChallengePresentation::Manual
        );
    }

    #[test]
    fn validation_method_wire_values_match_their_stable_tokens() {
        // The wire value, `as_str`, and the DDL CHECK are one vocabulary. They
        // drifted once (`HTTP01` in JSON vs `HTTP_01` in the column), which made a
        // console request that selected DNS-01 fail to deserialize.
        for method in ValidationMethod::ALL {
            let json = serde_json::to_value(method).expect("serialize method");
            assert_eq!(json, serde_json::Value::String(method.as_str().to_owned()));
            let parsed: ValidationMethod =
                serde_json::from_value(json).expect("deserialize method");
            assert_eq!(parsed, method);
            assert_eq!(ValidationMethod::parse(method.as_str()), Some(method));
        }
        assert_eq!(
            serde_json::to_value(CertificateScope::Wildcard).unwrap(),
            serde_json::Value::String("WILDCARD".to_owned())
        );
    }

    #[test]
    fn error_codes_are_stable_and_unique() {
        let all = [
            CertificatePlanError::EmptyIdentifierSet,
            CertificatePlanError::InvalidHostname,
            CertificatePlanError::DuplicateHostname,
            CertificatePlanError::MultiLevelWildcard,
            CertificatePlanError::ScopeRequiresWildcard,
            CertificatePlanError::WildcardScopeRequiresWildcardIdentifier,
            CertificatePlanError::WildcardApexMissing,
            CertificatePlanError::Http01NotAllowedForWildcard,
            CertificatePlanError::ManualRequiresDns01,
            CertificatePlanError::NoDnsProviderConfigured,
            CertificatePlanError::Http01WebrootNotConfigured,
            CertificatePlanError::SingleDomainRequiresOneIdentifier,
            CertificatePlanError::TooManyIdentifiers,
        ];
        let codes = all
            .iter()
            .map(|error| error.code())
            .collect::<BTreeSet<_>>();
        assert_eq!(codes.len(), all.len());
    }

    #[test]
    fn quota_windows_and_limits_are_bounded() {
        let kinds = [
            CertificateQuotaKind::CertificatesPerDomain,
            CertificateQuotaKind::DuplicateCertificateSet,
            CertificateQuotaKind::NewOrders,
            CertificateQuotaKind::FailedValidations,
            CertificateQuotaKind::ConcurrentOrders,
        ];
        for kind in kinds {
            assert!(kind.window_seconds() >= 1);
            assert!(kind.default_limit() > 0);
            assert!(!kind.as_str().is_empty());
        }
        let names = kinds
            .iter()
            .map(|kind| kind.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), kinds.len());
    }
}
