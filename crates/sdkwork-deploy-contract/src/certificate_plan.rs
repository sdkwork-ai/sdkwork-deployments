//! Pure certificate-planning rules: scope validation, wildcard apex completion,
//! and CA budget constants.
//!
//! Deliberately free of I/O so one implementation drives the API validation
//! path, the order orchestrator, and the unit tests. The database only ever
//! sees the planned identifier list this module returns.

use std::collections::BTreeSet;

use crate::dto::{wildcard_apex, CertificateScope, ValidationMethod};

/// Upper bound on identifiers per certificate.
///
/// Matches `deploy_certificate_identifier.position BETWEEN 0 AND 99`; the plan
/// aligns the product cap with the storage cap instead of letting the two
/// drift.
pub const MAX_CERTIFICATE_IDENTIFIERS: usize = 100;

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
            Self::SingleDomainRequiresOneIdentifier => {
                "a single-domain certificate covers exactly one hostname"
            }
            Self::TooManyIdentifiers => {
                "a certificate may cover at most 100 hostname identifiers"
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
