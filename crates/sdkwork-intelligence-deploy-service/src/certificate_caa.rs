//! CAA pre-issuance authorization (RFC 8659) and its ACME account-binding
//! extensions (RFC 8657).
//!
//! A CAA check asks one question: does the DNS policy of an identifier authorize
//! *this* CA to issue for it? The answer decides whether an order may be opened
//! at all, so the rules are implemented literally rather than approximately —
//! see the RFC citations on each function.
//!
//! Two layers are deliberately separated:
//!
//! - [`CertificateAuthorityAuthorizationPort`] performs single-name DNS lookups.
//!   The concrete adapter lives in the service host, next to the resolver, and
//!   knows nothing about CAA policy.
//! - [`evaluate_caa_policy`] is pure. It takes the records for the relevant
//!   RRset and returns a decision, so every branch of RFC 8659 §4 is reachable
//!   from a unit test without a network.
//!
//! [`observe_relevant_caa_rrset`] bridges them with the §3 tree walk.
//!
//! ## Hostname-only identifiers
//!
//! RFC 8659 is defined over DNS names. IP-address identifiers have no CAA
//! policy; callers must not invoke this module for them. [`caa_base_name`]
//! rejects an address so the mistake surfaces as an error rather than as a
//! lookup for a nonsense name.

use std::net::IpAddr;
use std::str::FromStr;

use async_trait::async_trait;
use sdkwork_deploy_contract::{
    DeployServiceError, DeployServiceResult, CAA_DECISION_LOOKUP_FAILED, CAA_DECISION_PERMITTED,
    CAA_DECISION_UNAUTHORIZED_CA,
};

/// CAA property tags defined by RFC 8659 §4.
pub const CAA_TAG_ISSUE: &str = "issue";
pub const CAA_TAG_ISSUEWILD: &str = "issuewild";
pub const CAA_TAG_IODEF: &str = "iodef";

/// Property tags this implementation supports. RFC 8659 §4.5 requires a CA to
/// refuse issuance when the relevant RRset carries a critical property whose tag
/// it does not support, so this list is the definition of "supported".
pub const CAA_SUPPORTED_TAGS: &[&str] = &[CAA_TAG_ISSUE, CAA_TAG_ISSUEWILD, CAA_TAG_IODEF];

/// Parameters understood on an `issue`/`issuewild` value (RFC 8657 §4).
pub const CAA_PARAM_ACCOUNT_URI: &str = "accounturi";
pub const CAA_PARAM_VALIDATION_METHODS: &str = "validationmethods";

/// ACME validation-method tokens, as they appear in a `validationmethods`
/// parameter (RFC 8657 §4.3).
pub const ACME_METHOD_HTTP_01: &str = "http-01";
pub const ACME_METHOD_DNS_01: &str = "dns-01";

/// How far the RFC 8659 §3 tree walk may climb.
///
/// The RFC climbs to, but not including, the root. No real identifier has more
/// than a handful of labels, and every label costs a DNS query, so the walk is
/// bounded to keep a hostile or degenerate name from becoming an amplification
/// vector. A name that exceeds the bound is treated as unevaluable, never as
/// permitted.
pub const MAX_CAA_TREE_WALK_LABELS: usize = 16;

/// One CAA resource record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaaRecord {
    /// RFC 8659 §4.1 bit 0.
    pub critical: bool,
    /// Property tag, compared case-insensitively.
    pub tag: String,
    /// Property value, verbatim.
    pub value: String,
}

/// The CAA RRset published at exactly one name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaaRrset {
    /// The name publishes a CAA RRset.
    Present(Vec<CaaRecord>),
    /// The name publishes no CAA RRset (NXDOMAIN, or an empty answer).
    Absent,
    /// The lookup could not be completed: timeout, SERVFAIL, transport error.
    Unavailable,
}

/// Single-name CAA lookups.
#[async_trait]
pub trait CertificateAuthorityAuthorizationPort: Send + Sync {
    /// Looks up the CAA RRset published at exactly `name`.
    ///
    /// `name` is an absolute DNS name. Implementations must distinguish "no
    /// records" ([`CaaRrset::Absent`]) from "could not answer"
    /// ([`CaaRrset::Unavailable`]): the first permits issuance, the second makes
    /// the check unevaluable.
    async fn lookup_caa_rrset(&self, name: &str) -> DeployServiceResult<CaaRrset>;
}

/// A host with no CAA resolver wired. Every lookup is unevaluable.
///
/// This is the safe default for an environment that has not configured DNS
/// resolution: the control plane records that the check could not run and leaves
/// enforcement to the CA, rather than either claiming authorization it has not
/// established or blocking all certificate management on a missing resolver.
pub struct UnconfiguredCertificateAuthorityAuthorization;

#[async_trait]
impl CertificateAuthorityAuthorizationPort for UnconfiguredCertificateAuthorityAuthorization {
    async fn lookup_caa_rrset(&self, _name: &str) -> DeployServiceResult<CaaRrset> {
        Ok(CaaRrset::Unavailable)
    }
}

/// The name the RFC 8659 §3 tree walk starts at for `identifier`.
///
/// RFC 8659 §3 evaluates `RelevantCAASet(X)`, where `X` is the identifier with
/// any wildcard label removed. A wildcard claim therefore shares a starting name
/// with its apex: `*.example.com` and `example.com` both start at
/// `example.com`. The walk from there is identical, and the wildcard distinction
/// is applied later by property selection (RFC 8659 §4.3).
pub fn caa_base_name(identifier: &str) -> DeployServiceResult<String> {
    let identifier = identifier.trim().trim_end_matches('.');
    let name = identifier.strip_prefix("*.").unwrap_or(identifier);
    let name = name.to_ascii_lowercase();
    if name.is_empty()
        || name.contains('*')
        || IpAddr::from_str(&name).is_ok()
        || name.split('.').any(|label| label.is_empty())
    {
        return Err(DeployServiceError::validation(
            "CAA can only be evaluated for a DNS hostname",
        ));
    }
    Ok(name)
}

/// `Parent(X)` from RFC 8659 §3: `X` with its leftmost label removed.
///
/// Returns `None` once removal would produce the DNS root, which the tree walk
/// excludes.
pub fn caa_parent_name(name: &str) -> Option<String> {
    let (_, parent) = name.split_once('.')?;
    if parent.is_empty() {
        None
    } else {
        Some(parent.to_owned())
    }
}

/// RFC 8659 §3: climbs from `base_name` up to, but not including, the root, and
/// returns the first CAA RRset found.
///
/// A wildcard claim must be reduced by [`caa_base_name`] before calling this.
pub async fn observe_relevant_caa_rrset(
    port: &dyn CertificateAuthorityAuthorizationPort,
    base_name: &str,
) -> DeployServiceResult<CaaRrsetObservation> {
    let mut name = Some(base_name.to_owned());
    let mut labels = 0_usize;
    while let Some(current) = name {
        labels += 1;
        if labels > MAX_CAA_TREE_WALK_LABELS {
            return Ok(CaaRrsetObservation {
                rrset: CaaRrset::Unavailable,
                matched_name: None,
            });
        }
        match port.lookup_caa_rrset(&current).await? {
            // RFC 8659 §3: the walk terminates at the first non-empty RRset.
            CaaRrset::Present(records) => {
                return Ok(CaaRrsetObservation {
                    rrset: CaaRrset::Present(records),
                    matched_name: Some(current),
                })
            }
            CaaRrset::Absent => name = caa_parent_name(&current),
            // An unanswered level hides whether a higher level publishes policy,
            // so the result is unevaluable rather than absent.
            CaaRrset::Unavailable => {
                return Ok(CaaRrsetObservation {
                    rrset: CaaRrset::Unavailable,
                    matched_name: None,
                })
            }
        }
    }
    // RFC 8659 §3: no CAA RRset anywhere in the chain means CAA does not
    // restrict issuance for this name.
    Ok(CaaRrsetObservation {
        rrset: CaaRrset::Absent,
        matched_name: None,
    })
}

/// The outcome of the RFC 8659 §3 tree walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaaRrsetObservation {
    pub rrset: CaaRrset,
    /// The name the RRset was found at, for audit. `None` when nothing was found.
    pub matched_name: Option<String>,
}

/// Why CAA policy refuses issuance. Each reason is a bounded, operator-facing
/// code; the text is for humans and must not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaaRefusalReason {
    /// The relevant properties name other CAs only.
    UnauthorizedIssuer,
    /// Every relevant property has an empty or malformed issuer-domain-name,
    /// which RFC 8659 §4.2 defines as a request for no issuance.
    IssuanceForbidden,
    /// A critical property carries a tag this implementation does not support
    /// (RFC 8659 §4.5).
    CriticalUnknownProperty,
    /// The authorized properties exclude the validation method this order will
    /// use (RFC 8657 §4.3).
    ValidationMethodExcluded,
}

impl CaaRefusalReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::UnauthorizedIssuer => "CAA_UNAUTHORIZED_CA",
            Self::IssuanceForbidden => "CAA_ISSUANCE_FORBIDDEN",
            Self::CriticalUnknownProperty => "CAA_CRITICAL_UNKNOWN_PROPERTY",
            Self::ValidationMethodExcluded => "CAA_VALIDATION_METHOD_EXCLUDED",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::UnauthorizedIssuer => {
                "CAA authorizes a different certificate authority for this identifier"
            }
            Self::IssuanceForbidden => {
                "CAA forbids issuance for this identifier by every certificate authority"
            }
            Self::CriticalUnknownProperty => {
                "CAA carries a critical property this control plane does not support"
            }
            Self::ValidationMethodExcluded => {
                "CAA restricts issuance to a different ACME validation method"
            }
        }
    }
}

/// Why CAA policy could not be evaluated.
///
/// These are not authorizations. The CA performs the authoritative check; a
/// control plane that treated an unresolvable policy as a refusal would turn a
/// resolver outage into a certificate-management outage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaaIndeterminateReason {
    /// The DNS lookup could not be completed.
    LookupUnavailable,
    /// The relevant properties restrict issuance, but the configured ACME
    /// directory has no known CAA identity, so no property can be matched.
    UnknownCaIdentity,
    /// The authority pins an ACME account URI (RFC 8657 §4.2). Confirming it
    /// requires an ACME account that does not exist yet at request time, so the
    /// check is deferred to issuance.
    AccountUriPinned,
}

impl CaaIndeterminateReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::LookupUnavailable => "CAA_LOOKUP_FAILED",
            Self::UnknownCaIdentity => "CAA_CA_IDENTITY_UNKNOWN",
            Self::AccountUriPinned => "CAA_ACCOUNT_URI_PINNED",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::LookupUnavailable => "CAA could not be read from DNS",
            Self::UnknownCaIdentity => {
                "CAA restricts issuance and the configured ACME directory has no known CAA identity"
            }
            Self::AccountUriPinned => {
                "CAA pins an ACME account URI that is only knowable at issuance time"
            }
        }
    }
}

/// The result of evaluating CAA policy for one identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaaPolicyOutcome {
    Permitted,
    Refused(CaaRefusalReason),
    Indeterminate(CaaIndeterminateReason),
}

/// RFC 8659 §4: decides whether `issuer_domains` may issue for an identifier.
///
/// `wildcard` selects the property set: RFC 8659 §4.3 makes every `issuewild`
/// property take precedence over every `issue` property for a wildcard, and
/// requires `issuewild` to be ignored entirely for a non-wildcard name. When a
/// wildcard has no `issuewild` property, `issue` applies.
///
/// `issuer_domains` is the CA's `caaIdentities` set; matching is
/// case-insensitive because RFC 8659 §4.2 specifies LDH-Label form without
/// fixing a case.
pub fn evaluate_caa_policy(
    wildcard: bool,
    issuer_domains: &[&str],
    acme_method: Option<&str>,
    records: &[CaaRecord],
) -> CaaPolicyOutcome {
    // RFC 8659 §4.5: a critical property with an unsupported tag forbids
    // issuance outright, before any authorizing property is considered.
    if records.iter().any(|record| {
        record.critical
            && !CAA_SUPPORTED_TAGS
                .iter()
                .any(|supported| record.tag.eq_ignore_ascii_case(supported))
    }) {
        return CaaPolicyOutcome::Refused(CaaRefusalReason::CriticalUnknownProperty);
    }

    let is_issuewild = |record: &CaaRecord| record.tag.eq_ignore_ascii_case(CAA_TAG_ISSUEWILD);
    let is_issue = |record: &CaaRecord| record.tag.eq_ignore_ascii_case(CAA_TAG_ISSUE);

    // RFC 8659 §4.3: for a wildcard, a single `issuewild` suppresses every
    // `issue`; otherwise `issue` decides. For a non-wildcard name, `issuewild`
    // never participates.
    let restricting: Vec<&CaaRecord> = if wildcard {
        let issuewild: Vec<&CaaRecord> = records.iter().filter(|r| is_issuewild(r)).collect();
        if issuewild.is_empty() {
            records.iter().filter(|r| is_issue(r)).collect()
        } else {
            issuewild
        }
    } else {
        records.iter().filter(|r| is_issue(r)).collect()
    };

    // RFC 8659 §3: an RRset with no property that restricts issuance (only
    // `iodef`, or only tags this implementation does not recognize) does not
    // restrict issuance.
    if restricting.is_empty() {
        return CaaPolicyOutcome::Permitted;
    }

    let parsed = restricting
        .iter()
        .map(|record| parse_issue_value(&record.value))
        .collect::<Vec<_>>();

    // A property can only be matched against the CA's identity, so a restricting
    // policy with no known identity is unevaluable. Absence of any authorizing
    // property is checked first so that the more specific refusal wins.
    let names_this_ca = |value: &IssueValue| {
        value.issuer.as_deref().is_some_and(|issuer| {
            issuer_domains
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(issuer))
        })
    };

    if issuer_domains.is_empty() {
        // A restricting policy exists but no identity can be matched against it.
        return CaaPolicyOutcome::Indeterminate(CaaIndeterminateReason::UnknownCaIdentity);
    }

    let mut pinned_account_uri = false;
    let mut excluded_validation_method = false;
    for value in &parsed {
        if !names_this_ca(value) {
            continue;
        }
        // RFC 8657 §4.2: an `accounturi` parameter narrows the property to one
        // ACME account. Request-time CAA cannot confirm it, so the property does
        // not authorize here; the issuance-time check owns this rule.
        if value
            .parameters
            .iter()
            .any(|(tag, _)| tag == CAA_PARAM_ACCOUNT_URI)
        {
            pinned_account_uri = true;
            continue;
        }
        // RFC 8657 §4.3: a `validationmethods` parameter restricts the property
        // to the listed ACME challenge types.
        if let Some((_, methods)) = value
            .parameters
            .iter()
            .find(|(tag, _)| tag == CAA_PARAM_VALIDATION_METHODS)
        {
            let listed = methods
                .split(',')
                .map(|method| method.trim().to_ascii_lowercase())
                .collect::<Vec<_>>();
            let permitted = match acme_method {
                Some(method) => listed.iter().any(|item| item == method),
                // The method is not known yet, so the restriction cannot be
                // shown to admit this order.
                None => false,
            };
            if !permitted {
                excluded_validation_method = true;
                continue;
            }
        }
        return CaaPolicyOutcome::Permitted;
    }

    if excluded_validation_method {
        return CaaPolicyOutcome::Refused(CaaRefusalReason::ValidationMethodExcluded);
    }
    if pinned_account_uri {
        return CaaPolicyOutcome::Indeterminate(CaaIndeterminateReason::AccountUriPinned);
    }
    // No property authorizes this CA. Report a blanket refusal when every
    // property is empty or malformed, because that is the operator's likely
    // intent and the more actionable message.
    if parsed.iter().all(|value| value.issuer.is_none()) {
        return CaaPolicyOutcome::Refused(CaaRefusalReason::IssuanceForbidden);
    }
    CaaPolicyOutcome::Refused(CaaRefusalReason::UnauthorizedIssuer)
}

/// The CAA decision recorded on an order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaaDecisionObservation {
    /// One of the contract's `CAA_DECISION_*` values.
    pub decision: &'static str,
    /// Bounded operator-facing detail, never a DNS payload.
    pub detail: String,
    /// Name the relevant RRset was found at, for audit.
    pub matched_name: Option<String>,
    /// Number of records in the relevant RRset.
    pub record_count: usize,
}

impl CaaDecisionObservation {
    /// Whether this observation forbids opening an order.
    ///
    /// Only a definite refusal blocks. An unevaluable check is recorded and
    /// surfaced but does not block, because the CA is the enforcement authority
    /// and a resolver outage must not stop all certificate management.
    pub fn blocks_order_creation(&self) -> bool {
        self.decision == CAA_DECISION_UNAUTHORIZED_CA
    }
}

/// Observes CAA for one identifier and reduces it to a recorded decision.
///
/// `acme_method` is the ACME challenge token (`http-01` / `dns-01`) this order
/// will use, or `None` when the method is not yet resolved.
pub async fn observe_identifier_caa(
    port: &dyn CertificateAuthorityAuthorizationPort,
    identifier: &str,
    issuer_domains: &[&str],
    acme_method: Option<&str>,
) -> DeployServiceResult<CaaDecisionObservation> {
    let base_name = caa_base_name(identifier)?;
    let observation = observe_relevant_caa_rrset(port, &base_name).await?;
    Ok(reduce_caa_observation(
        identifier,
        observation,
        issuer_domains,
        acme_method,
    ))
}

/// Reduces a tree-walk observation to a decision. Pure, so the mapping from
/// every RFC branch to a recorded decision is unit-testable.
pub fn reduce_caa_observation(
    identifier: &str,
    observation: CaaRrsetObservation,
    issuer_domains: &[&str],
    acme_method: Option<&str>,
) -> CaaDecisionObservation {
    match observation.rrset {
        CaaRrset::Present(records) => {
            let outcome = evaluate_caa_policy(
                identifier.trim_start_matches('.').starts_with("*."),
                issuer_domains,
                acme_method,
                &records,
            );
            let (decision, detail) = match outcome {
                CaaPolicyOutcome::Permitted => (
                    CAA_DECISION_PERMITTED,
                    format!(
                        "CAA at {} permits issuance for {identifier}",
                        observation.matched_name.as_deref().unwrap_or(identifier)
                    ),
                ),
                CaaPolicyOutcome::Refused(reason) => (
                    CAA_DECISION_UNAUTHORIZED_CA,
                    format!("{} ({identifier})", reason.message()),
                ),
                CaaPolicyOutcome::Indeterminate(reason) => (
                    CAA_DECISION_LOOKUP_FAILED,
                    format!("{} ({identifier})", reason.message()),
                ),
            };
            CaaDecisionObservation {
                decision,
                detail,
                matched_name: observation.matched_name,
                record_count: records.len(),
            }
        }
        // RFC 8659 §3: no CAA RRset restricts issuance.
        CaaRrset::Absent => CaaDecisionObservation {
            decision: CAA_DECISION_PERMITTED,
            detail: format!("no CAA RRset restricts issuance for {identifier}"),
            matched_name: None,
            record_count: 0,
        },
        CaaRrset::Unavailable => CaaDecisionObservation {
            decision: CAA_DECISION_LOOKUP_FAILED,
            detail: format!(
                "{} ({identifier})",
                CaaIndeterminateReason::LookupUnavailable.message()
            ),
            matched_name: None,
            record_count: 0,
        },
    }
}

/// An `issue`/`issuewild` property value split into its parts.
///
/// `issue-value = *WSP [issuer-domain-name *WSP] [";" *WSP [parameters *WSP]]`
/// (RFC 8659 §4.2). A value that does not match the grammar must be treated
/// exactly like one with an empty issuer-domain-name, so `issuer` is `None`
/// whenever any part of the value is malformed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct IssueValue {
    issuer: Option<String>,
    parameters: Vec<(String, String)>,
}

fn parse_issue_value(raw: &str) -> IssueValue {
    // Only the first `;` separates the issuer from the parameter list; a
    // parameter value may not contain `;` (RFC 8659 §4.2 `value` charset).
    let (issuer_part, parameter_part) = match raw.split_once(';') {
        Some((issuer, parameters)) => (issuer, Some(parameters)),
        None => (raw, None),
    };
    let issuer_part = issuer_part.trim();
    let issuer = if issuer_part.is_empty() {
        None
    } else if is_ldh_name(issuer_part) {
        Some(issuer_part.to_ascii_lowercase())
    } else {
        // Malformed issuer: treat as an empty issuer-domain-name, which
        // authorizes nobody.
        None
    };

    let mut parameters = Vec::new();
    if let Some(parameter_part) = parameter_part {
        for raw_parameter in parameter_part.split(';') {
            let raw_parameter = raw_parameter.trim();
            if raw_parameter.is_empty() {
                // `";"` with trailing whitespace: a legal empty parameter list.
                continue;
            }
            let Some((tag, value)) = raw_parameter.split_once('=') else {
                // Malformed parameter list invalidates the whole value.
                return IssueValue {
                    issuer: None,
                    parameters: Vec::new(),
                };
            };
            let tag = tag.trim();
            if !is_ldh_name(tag) {
                return IssueValue {
                    issuer: None,
                    parameters: Vec::new(),
                };
            }
            parameters.push((tag.to_ascii_lowercase(), value.trim().to_owned()));
        }
    }

    IssueValue { issuer, parameters }
}

/// What the CAA pre-flight needs to know about a certificate that is about to be
/// ordered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateOrderCaaSubject {
    /// Certificate identifiers in plan order, wildcard markers preserved.
    pub identifiers: Vec<String>,
    /// Stored validation method as the contract wire value. Rows written by
    /// `create_certificate` always hold a resolved method, but the column
    /// defaults to `AUTO` and may be `AUTO` on a row that predates the scope
    /// model.
    pub validation_method: String,
    /// Directory URL of the ACME account the order will use, when the tenant has
    /// one. Absent means no CA identity can be matched against CAA.
    pub directory_url: Option<String>,
}

impl CertificateOrderCaaSubject {
    /// The ACME challenge token for the stored method, or `None` when the stored
    /// value is not a concrete method.
    pub fn acme_method(&self) -> Option<&'static str> {
        match self.validation_method.as_str() {
            "HTTP_01" => Some(ACME_METHOD_HTTP_01),
            "DNS_01" => Some(ACME_METHOD_DNS_01),
            _ => None,
        }
    }
}

/// Observes CAA for every identifier of a certificate and reduces the results to
/// the single decision recorded on the order.
pub async fn observe_order_caa(
    port: &dyn CertificateAuthorityAuthorizationPort,
    subject: &CertificateOrderCaaSubject,
) -> DeployServiceResult<CaaDecisionObservation> {
    let issuer_domains = issuer_domains_for_directory(subject.directory_url.as_deref());
    let acme_method = subject.acme_method();
    let mut observations = Vec::with_capacity(subject.identifiers.len());
    for identifier in &subject.identifiers {
        observations
            .push(observe_identifier_caa(port, identifier, issuer_domains, acme_method).await?);
    }
    Ok(combine_order_caa_observations(&observations))
}

/// The CA's `caaIdentities` set for an ACME directory URL.
///
/// The identity table lives in the ACME engine because it is CA knowledge; an
/// unknown directory yields an empty set, which makes a restricting policy
/// unevaluable instead of wrongly authorized.
pub fn issuer_domains_for_directory(directory_url: Option<&str>) -> &'static [&'static str] {
    directory_url
        .and_then(sdkwork_webserver_acme_service::acme_directory_profile)
        .map(|profile| profile.issuer_domains)
        .unwrap_or(&[])
}

/// Reduces per-identifier observations to the order's decision.
///
/// The CA validates every identifier, so one definite refusal refuses the whole
/// order. Otherwise the weakest observation is kept: an order whose CAA check
/// could not be completed is recorded as such even when every other identifier
/// was permitted, so the audit trail does not overstate what was established.
pub fn combine_order_caa_observations(
    observations: &[CaaDecisionObservation],
) -> CaaDecisionObservation {
    if observations.is_empty() {
        return CaaDecisionObservation {
            decision: CAA_DECISION_LOOKUP_FAILED,
            detail: "no certificate identifier was available for a CAA check".to_owned(),
            matched_name: None,
            record_count: 0,
        };
    }

    let record_count = observations.iter().map(|item| item.record_count).sum();
    let matched_name = observations
        .iter()
        .find_map(|item| item.matched_name.clone());

    if let Some(refused) = observations
        .iter()
        .find(|item| item.decision == CAA_DECISION_UNAUTHORIZED_CA)
    {
        return CaaDecisionObservation {
            decision: CAA_DECISION_UNAUTHORIZED_CA,
            detail: refused.detail.clone(),
            matched_name: refused.matched_name.clone().or(matched_name),
            record_count,
        };
    }

    if let Some(unevaluable) = observations
        .iter()
        .find(|item| item.decision == CAA_DECISION_LOOKUP_FAILED)
    {
        return CaaDecisionObservation {
            decision: CAA_DECISION_LOOKUP_FAILED,
            detail: unevaluable.detail.clone(),
            matched_name: unevaluable.matched_name.clone().or(matched_name),
            record_count,
        };
    }

    CaaDecisionObservation {
        decision: CAA_DECISION_PERMITTED,
        detail: format!(
            "CAA permits issuance for all {} identifier(s)",
            observations.len()
        ),
        matched_name,
        record_count,
    }
}

/// `label = (ALPHA / DIGIT) *(*("-") (ALPHA / DIGIT))` joined by `.`
/// (RFC 8659 §4.2): no leading or trailing hyphen, no empty label.
fn is_ldh_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    /// A resolver with fixed answers that records every name it was asked for.
    #[derive(Default)]
    struct FixtureResolver {
        answers: HashMap<String, CaaRrset>,
        asked: Mutex<Vec<String>>,
    }

    impl FixtureResolver {
        fn with(answers: &[(&str, CaaRrset)]) -> Self {
            Self {
                answers: answers
                    .iter()
                    .map(|(name, rrset)| ((*name).to_owned(), rrset.clone()))
                    .collect(),
                asked: Mutex::new(Vec::new()),
            }
        }

        fn asked(&self) -> Vec<String> {
            self.asked.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl CertificateAuthorityAuthorizationPort for FixtureResolver {
        async fn lookup_caa_rrset(&self, name: &str) -> DeployServiceResult<CaaRrset> {
            self.asked.lock().expect("lock").push(name.to_owned());
            Ok(self.answers.get(name).cloned().unwrap_or(CaaRrset::Absent))
        }
    }

    fn record(tag: &str, value: &str) -> CaaRecord {
        CaaRecord {
            critical: false,
            tag: tag.to_owned(),
            value: value.to_owned(),
        }
    }

    fn critical_record(tag: &str, value: &str) -> CaaRecord {
        CaaRecord {
            critical: true,
            ..record(tag, value)
        }
    }

    const LETS_ENCRYPT: &[&str] = &["letsencrypt.org"];

    // ---- RFC 8659 §3 tree walk -------------------------------------------

    #[tokio::test]
    async fn the_walk_stops_at_the_first_non_empty_rrset() {
        // "A.B.C" with CAA only at "B.C" (the RFC's own example).
        let resolver = FixtureResolver::with(&[(
            "b.c",
            CaaRrset::Present(vec![record(CAA_TAG_ISSUE, "example.com")]),
        )]);
        let observation = observe_relevant_caa_rrset(&resolver, "a.b.c")
            .await
            .expect("walk");
        assert_eq!(observation.matched_name.as_deref(), Some("b.c"));
        assert_eq!(resolver.asked(), ["a.b.c", "b.c"]);
    }

    #[tokio::test]
    async fn an_empty_chain_permits_issuance() {
        let resolver = FixtureResolver::default();
        let observation = observe_relevant_caa_rrset(&resolver, "a.b.example.com")
            .await
            .expect("walk");
        assert_eq!(observation.rrset, CaaRrset::Absent);
        assert_eq!(observation.matched_name, None);
        // Climbs to, but not including, the root.
        assert_eq!(
            resolver.asked(),
            ["a.b.example.com", "b.example.com", "example.com", "com"]
        );
    }

    #[tokio::test]
    async fn an_unanswered_level_makes_the_whole_check_unevaluable() {
        // A resolver failure below must not be read as "no policy above".
        let resolver = FixtureResolver::with(&[("b.example.com", CaaRrset::Unavailable)]);
        let observation = observe_relevant_caa_rrset(&resolver, "a.b.example.com")
            .await
            .expect("walk");
        assert_eq!(observation.rrset, CaaRrset::Unavailable);
        assert_eq!(resolver.asked(), ["a.b.example.com", "b.example.com"]);
    }

    #[test]
    fn the_wildcard_label_is_removed_before_the_walk() {
        // RFC 8659 §3 evaluates RelevantCAASet(X), so `*.example.com` and
        // `example.com` share a starting name.
        assert_eq!(caa_base_name("*.example.com").expect("base"), "example.com");
        assert_eq!(caa_base_name("Example.COM.").expect("base"), "example.com");
        assert_eq!(
            caa_base_name("*.sub.example.com").expect("base"),
            "sub.example.com"
        );
    }

    #[test]
    fn a_non_hostname_identifier_is_rejected_rather_than_queried() {
        for identifier in [
            "",
            "192.0.2.10",
            "2001:db8::1",
            "*.",
            "a..b.example.com",
            "a.*.example.com",
        ] {
            assert!(caa_base_name(identifier).is_err(), "{identifier}");
        }
    }

    #[test]
    fn parent_removal_stops_before_the_root() {
        assert_eq!(caa_parent_name("a.b.c").as_deref(), Some("b.c"));
        assert_eq!(caa_parent_name("b.c").as_deref(), Some("c"));
        assert_eq!(caa_parent_name("c"), None);
    }

    #[tokio::test]
    async fn the_walk_is_bounded() {
        // A name with more labels than the bound must fail closed as
        // unevaluable, never as permitted.
        let deep = (0..(MAX_CAA_TREE_WALK_LABELS + 4))
            .map(|index| format!("l{index}"))
            .collect::<Vec<_>>()
            .join(".")
            .to_owned();
        let resolver = FixtureResolver::default();
        let observation = observe_relevant_caa_rrset(&resolver, &deep)
            .await
            .expect("walk");
        assert_eq!(observation.rrset, CaaRrset::Unavailable);
        assert_eq!(resolver.asked().len(), MAX_CAA_TREE_WALK_LABELS);
    }

    // ---- RFC 8659 §4 property selection ----------------------------------

    #[test]
    fn an_rrset_without_a_restricting_property_does_not_restrict() {
        // Only iodef, and only an unrecognized non-critical tag.
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_HTTP_01),
                &[record(CAA_TAG_IODEF, "mailto:security@example.com")]
            ),
            CaaPolicyOutcome::Permitted
        );
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_HTTP_01),
                &[record("futuretag", "example")]
            ),
            CaaPolicyOutcome::Permitted
        );
        assert_eq!(
            evaluate_caa_policy(false, LETS_ENCRYPT, Some(ACME_METHOD_HTTP_01), &[]),
            CaaPolicyOutcome::Permitted
        );
    }

    #[test]
    fn an_issue_property_authorizes_only_the_named_issuer() {
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_HTTP_01),
                &[record(CAA_TAG_ISSUE, "letsencrypt.org")]
            ),
            CaaPolicyOutcome::Permitted
        );
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_HTTP_01),
                &[record(CAA_TAG_ISSUE, "other-ca.example")]
            ),
            CaaPolicyOutcome::Refused(CaaRefusalReason::UnauthorizedIssuer)
        );
    }

    #[test]
    fn matching_the_ca_identity_ignores_case() {
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_HTTP_01),
                &[record(CAA_TAG_ISSUE, "LetsEncrypt.ORG")]
            ),
            CaaPolicyOutcome::Permitted
        );
    }

    #[test]
    fn an_empty_or_malformed_issuer_forbids_issuance() {
        // RFC 8659 §4.2: a lone ";" requests no issuance, and a value that does
        // not match the ABNF must be treated the same way.
        for value in [";", "  ;  ", "%%%%%", "not a host name", "-leading.example"] {
            assert_eq!(
                evaluate_caa_policy(
                    false,
                    LETS_ENCRYPT,
                    Some(ACME_METHOD_HTTP_01),
                    &[record(CAA_TAG_ISSUE, value)]
                ),
                CaaPolicyOutcome::Refused(CaaRefusalReason::IssuanceForbidden),
                "{value}"
            );
        }
    }

    #[test]
    fn authorizations_are_additive() {
        // RFC 8659 §4.2: an empty issuer next to a non-empty one is the same as
        // specifying only the non-empty one.
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_HTTP_01),
                &[
                    record(CAA_TAG_ISSUE, ";"),
                    record(CAA_TAG_ISSUE, "letsencrypt.org"),
                ]
            ),
            CaaPolicyOutcome::Permitted
        );
    }

    #[test]
    fn any_recognized_ca_identity_authorizes() {
        // ZeroSSL recognizes the Sectigo/Entrust family; a zone pinning one of
        // them is correctly authorized.
        let zero_ssl: &[&str] = &["sectigo.com", "affirmtrust.com"];
        for value in ["sectigo.com", "affirmtrust.com"] {
            assert_eq!(
                evaluate_caa_policy(
                    false,
                    zero_ssl,
                    Some(ACME_METHOD_HTTP_01),
                    &[record(CAA_TAG_ISSUE, value)]
                ),
                CaaPolicyOutcome::Permitted,
                "{value}"
            );
        }
    }

    #[test]
    fn a_critical_unknown_property_forbids_issuance() {
        // RFC 8659 §4.5, ahead of any authorizing property.
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_HTTP_01),
                &[
                    record(CAA_TAG_ISSUE, "letsencrypt.org"),
                    critical_record("futuretag", "example"),
                ]
            ),
            CaaPolicyOutcome::Refused(CaaRefusalReason::CriticalUnknownProperty)
        );
        // A critical tag this implementation does support is not a refusal.
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_HTTP_01),
                &[critical_record(CAA_TAG_ISSUE, "letsencrypt.org")]
            ),
            CaaPolicyOutcome::Permitted
        );
    }

    // ---- RFC 8659 §4.3 wildcard semantics --------------------------------

    #[test]
    fn an_issuewild_property_governs_a_wildcard_and_suppresses_issue() {
        assert_eq!(
            evaluate_caa_policy(
                true,
                LETS_ENCRYPT,
                Some(ACME_METHOD_DNS_01),
                &[record(CAA_TAG_ISSUEWILD, "letsencrypt.org")]
            ),
            CaaPolicyOutcome::Permitted
        );
        // The RFC's own example: an issuewild naming another CA is not rescued
        // by a permissive `issue`.
        assert_eq!(
            evaluate_caa_policy(
                true,
                LETS_ENCRYPT,
                Some(ACME_METHOD_DNS_01),
                &[
                    record(CAA_TAG_ISSUEWILD, "ca2.example.org"),
                    record(CAA_TAG_ISSUE, ";"),
                ]
            ),
            CaaPolicyOutcome::Refused(CaaRefusalReason::UnauthorizedIssuer)
        );
    }

    #[test]
    fn a_wildcard_falls_back_to_issue_only_when_no_issuewild_exists() {
        assert_eq!(
            evaluate_caa_policy(
                true,
                LETS_ENCRYPT,
                Some(ACME_METHOD_DNS_01),
                &[record(CAA_TAG_ISSUE, "letsencrypt.org")]
            ),
            CaaPolicyOutcome::Permitted
        );
    }

    #[test]
    fn issuewild_never_applies_to_a_non_wildcard_name() {
        // RFC 8659 §4.3: "Each issuewild Property MUST be ignored when
        // processing a request for an FQDN that is not a Wildcard Domain Name."
        // `example.com` therefore keeps the permissive `issue` while
        // `*.example.com` does not.
        let records = [
            record(CAA_TAG_ISSUEWILD, "ca2.example.org"),
            record(CAA_TAG_ISSUE, "letsencrypt.org"),
        ];
        assert_eq!(
            evaluate_caa_policy(true, LETS_ENCRYPT, Some(ACME_METHOD_DNS_01), &records),
            CaaPolicyOutcome::Refused(CaaRefusalReason::UnauthorizedIssuer)
        );
        assert_eq!(
            evaluate_caa_policy(false, LETS_ENCRYPT, Some(ACME_METHOD_HTTP_01), &records),
            CaaPolicyOutcome::Permitted
        );
    }

    // ---- RFC 8657 parameters ---------------------------------------------

    #[test]
    fn a_validation_method_restriction_is_honoured() {
        let value = "letsencrypt.org; validationmethods=dns-01";
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_DNS_01),
                &[record(CAA_TAG_ISSUE, value)]
            ),
            CaaPolicyOutcome::Permitted
        );
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_HTTP_01),
                &[record(CAA_TAG_ISSUE, value)]
            ),
            CaaPolicyOutcome::Refused(CaaRefusalReason::ValidationMethodExcluded)
        );
        // A list, and case-insensitive comparison.
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_DNS_01),
                &[record(
                    CAA_TAG_ISSUE,
                    "letsencrypt.org;validationmethods=HTTP-01,DNS-01"
                )]
            ),
            CaaPolicyOutcome::Permitted
        );
    }

    #[test]
    fn an_unresolved_method_cannot_be_shown_to_satisfy_the_restriction() {
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                None,
                &[record(
                    CAA_TAG_ISSUE,
                    "letsencrypt.org; validationmethods=dns-01"
                )]
            ),
            CaaPolicyOutcome::Refused(CaaRefusalReason::ValidationMethodExcluded)
        );
    }

    #[test]
    fn an_account_uri_pin_defers_rather_than_authorizes() {
        // RFC 8657 §4.2. The account URI does not exist at request time, so the
        // property cannot authorize here; issuance owns the real check.
        assert_eq!(
            evaluate_caa_policy(
                false,
                LETS_ENCRYPT,
                Some(ACME_METHOD_HTTP_01),
                &[record(
                    CAA_TAG_ISSUE,
                    "letsencrypt.org; accounturi=https://acme-v02.api.letsencrypt.org/acme/acct/1"
                )]
            ),
            CaaPolicyOutcome::Indeterminate(CaaIndeterminateReason::AccountUriPinned)
        );
    }

    #[test]
    fn a_malformed_parameter_list_authorizes_nobody() {
        // The parameter list is part of issue-value, so a malformed list makes
        // the whole value malformed and the issuer empty.
        for value in [
            "letsencrypt.org; validationmethods",
            "letsencrypt.org; =dns-01",
            "letsencrypt.org; validation methods=dns-01",
        ] {
            assert_eq!(
                evaluate_caa_policy(
                    false,
                    LETS_ENCRYPT,
                    Some(ACME_METHOD_DNS_01),
                    &[record(CAA_TAG_ISSUE, value)]
                ),
                CaaPolicyOutcome::Refused(CaaRefusalReason::IssuanceForbidden),
                "{value}"
            );
        }
    }

    #[test]
    fn a_restricting_policy_with_no_known_ca_identity_is_unevaluable() {
        // A CA that is not in the directory table must not be treated as
        // refused, and must not be treated as authorized either.
        assert_eq!(
            evaluate_caa_policy(
                false,
                &[],
                Some(ACME_METHOD_HTTP_01),
                &[record(CAA_TAG_ISSUE, "letsencrypt.org")]
            ),
            CaaPolicyOutcome::Indeterminate(CaaIndeterminateReason::UnknownCaIdentity)
        );
        // With no restricting property the identity is irrelevant.
        assert_eq!(
            evaluate_caa_policy(false, &[], Some(ACME_METHOD_HTTP_01), &[]),
            CaaPolicyOutcome::Permitted
        );
    }

    // ---- Decision recording ----------------------------------------------

    #[tokio::test]
    async fn the_recorded_decision_only_blocks_on_a_definite_refusal() {
        let refused = FixtureResolver::with(&[(
            "example.com",
            CaaRrset::Present(vec![record(CAA_TAG_ISSUE, "other-ca.example")]),
        )]);
        let observation = observe_identifier_caa(
            &refused,
            "*.example.com",
            LETS_ENCRYPT,
            Some(ACME_METHOD_DNS_01),
        )
        .await
        .expect("observe");
        assert_eq!(observation.decision, CAA_DECISION_UNAUTHORIZED_CA);
        assert!(observation.blocks_order_creation());
        // The walk starts at the apex for a wildcard claim.
        assert_eq!(refused.asked(), ["example.com"]);

        let unavailable = FixtureResolver::with(&[("example.com", CaaRrset::Unavailable)]);
        let observation = observe_identifier_caa(
            &unavailable,
            "www.example.com",
            LETS_ENCRYPT,
            Some(ACME_METHOD_HTTP_01),
        )
        .await
        .expect("observe");
        assert_eq!(observation.decision, CAA_DECISION_LOOKUP_FAILED);
        assert!(!observation.blocks_order_creation());

        let absent = FixtureResolver::default();
        let observation = observe_identifier_caa(
            &absent,
            "www.example.com",
            LETS_ENCRYPT,
            Some(ACME_METHOD_HTTP_01),
        )
        .await
        .expect("observe");
        assert_eq!(observation.decision, CAA_DECISION_PERMITTED);
        assert!(!observation.blocks_order_creation());
        assert_eq!(observation.record_count, 0);
    }

    #[tokio::test]
    async fn the_unconfigured_port_never_claims_authorization() {
        let observation = observe_identifier_caa(
            &UnconfiguredCertificateAuthorityAuthorization,
            "www.example.com",
            LETS_ENCRYPT,
            Some(ACME_METHOD_HTTP_01),
        )
        .await
        .expect("observe");
        assert_eq!(observation.decision, CAA_DECISION_LOOKUP_FAILED);
        assert!(!observation.blocks_order_creation());
    }

    #[test]
    fn refusal_and_indeterminate_codes_are_stable_and_distinct() {
        let mut codes = vec![
            CaaRefusalReason::UnauthorizedIssuer.code(),
            CaaRefusalReason::IssuanceForbidden.code(),
            CaaRefusalReason::CriticalUnknownProperty.code(),
            CaaRefusalReason::ValidationMethodExcluded.code(),
            CaaIndeterminateReason::LookupUnavailable.code(),
            CaaIndeterminateReason::UnknownCaIdentity.code(),
            CaaIndeterminateReason::AccountUriPinned.code(),
        ];
        let total = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), total, "reason codes must be unique");
        assert!(codes.iter().all(|code| code.starts_with("CAA_")));
    }

    // ---- Order-level reduction -------------------------------------------

    fn observation(decision: &'static str, detail: &str) -> CaaDecisionObservation {
        CaaDecisionObservation {
            decision,
            detail: detail.to_owned(),
            matched_name: Some("example.com".to_owned()),
            record_count: 1,
        }
    }

    #[test]
    fn one_definite_refusal_refuses_the_whole_order() {
        // The CA validates every identifier, so a refusal on the apex must not
        // be masked by a permitted wildcard.
        let combined = combine_order_caa_observations(&[
            observation(CAA_DECISION_PERMITTED, "wildcard permitted"),
            observation(CAA_DECISION_UNAUTHORIZED_CA, "apex refused"),
        ]);
        assert_eq!(combined.decision, CAA_DECISION_UNAUTHORIZED_CA);
        assert!(combined.blocks_order_creation());
        assert!(combined.detail.contains("apex refused"));
        assert_eq!(combined.record_count, 2);
    }

    #[test]
    fn an_unevaluable_identifier_is_not_reported_as_permitted() {
        let combined = combine_order_caa_observations(&[
            observation(CAA_DECISION_PERMITTED, "wildcard permitted"),
            observation(CAA_DECISION_LOOKUP_FAILED, "apex unevaluable"),
        ]);
        assert_eq!(combined.decision, CAA_DECISION_LOOKUP_FAILED);
        assert!(!combined.blocks_order_creation());
    }

    #[test]
    fn an_all_permitted_order_records_permission() {
        let combined = combine_order_caa_observations(&[
            observation(CAA_DECISION_PERMITTED, "a permitted"),
            observation(CAA_DECISION_PERMITTED, "b permitted"),
        ]);
        assert_eq!(combined.decision, CAA_DECISION_PERMITTED);
        assert!(!combined.blocks_order_creation());
        assert!(combined.detail.contains("2 identifier"));
    }

    #[test]
    fn an_order_with_no_identifiers_is_unevaluable() {
        let combined = combine_order_caa_observations(&[]);
        assert_eq!(combined.decision, CAA_DECISION_LOOKUP_FAILED);
        assert!(!combined.blocks_order_creation());
    }

    #[test]
    fn the_acme_method_token_is_derived_from_the_stored_method() {
        let subject = |method: &str| CertificateOrderCaaSubject {
            identifiers: vec!["*.example.com".to_owned()],
            validation_method: method.to_owned(),
            directory_url: None,
        };
        assert_eq!(subject("DNS_01").acme_method(), Some("dns-01"));
        assert_eq!(subject("HTTP_01").acme_method(), Some("http-01"));
        // `AUTO` is the column default and is not a concrete method.
        assert_eq!(subject("AUTO").acme_method(), None);
        assert_eq!(subject("nonsense").acme_method(), None);
    }

    #[test]
    fn the_ca_identity_comes_from_the_directory_profile() {
        assert_eq!(
            issuer_domains_for_directory(Some("https://acme-v02.api.letsencrypt.org/directory")),
            ["letsencrypt.org"]
        );
        // An unknown or absent directory yields no identity, which makes a
        // restricting policy unevaluable rather than authorized.
        assert!(
            issuer_domains_for_directory(Some("https://acme.example.test/directory")).is_empty()
        );
        assert!(issuer_domains_for_directory(None).is_empty());
    }

    #[tokio::test]
    async fn an_order_pre_flight_checks_every_identifier_against_the_wildcard_start_name() {
        let resolver = FixtureResolver::with(&[(
            "example.com",
            CaaRrset::Present(vec![record(CAA_TAG_ISSUEWILD, "other-ca.example")]),
        )]);
        let subject = CertificateOrderCaaSubject {
            identifiers: vec!["*.example.com".to_owned(), "example.com".to_owned()],
            validation_method: "DNS_01".to_owned(),
            directory_url: Some("https://acme-v02.api.letsencrypt.org/directory".to_owned()),
        };
        let combined = observe_order_caa(&resolver, &subject)
            .await
            .expect("observe");
        assert_eq!(combined.decision, CAA_DECISION_UNAUTHORIZED_CA);
        assert!(combined.blocks_order_creation());
        // Only `issue` restricts the apex, and here it authorizes nobody, so the
        // apex identifier is refused as well while the wildcard is refused by
        // the issuewild property.
        assert_eq!(resolver.asked(), ["example.com", "example.com"]);
    }
}
