use serde::{Deserialize, Serialize};

/// How many host shapes one certificate aggregate is sold and validated as.
///
/// The scope is a property of the certificate, not of an individual identifier:
/// `deploy_certificate_identifier` keeps recording the per-identifier
/// `EXACT`/`WILDCARD` shape, while this value states the product intent the
/// operator chose and drives validation, planning, and quota accounting.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CertificateScope {
    /// Exactly one exact FQDN identifier.
    #[default]
    SingleDomain,
    /// One leading-label wildcard plus the apex it hangs off. A wildcard SAN does
    /// not cover the apex, so the aggregate always plans two identifiers.
    Wildcard,
}

impl CertificateScope {
    pub const ALL: [Self; 2] = [Self::SingleDomain, Self::Wildcard];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SingleDomain => "SINGLE_DOMAIN",
            Self::Wildcard => "WILDCARD",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "SINGLE_DOMAIN" => Some(Self::SingleDomain),
            "WILDCARD" => Some(Self::Wildcard),
            _ => None,
        }
    }

    /// Whether an issuance using this scope may present an HTTP-01 challenge.
    /// Wildcards require DNS-01 because the CA must prove control of every
    /// name the wildcard could expand to.
    pub const fn allows_http01(self) -> bool {
        matches!(self, Self::SingleDomain)
    }
}

impl std::fmt::Display for CertificateScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// ACME challenge method selected for an issuance.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValidationMethod {
    /// Prefer HTTP-01 when the edge proof path is healthy, otherwise DNS-01.
    /// Wildcard scopes always resolve to DNS-01 regardless of this value.
    #[default]
    Auto,
    /// `/.well-known/acme-challenge/<token>` served by the edge. Exact names only.
    Http01,
    /// `_acme-challenge.<host>` TXT record, presented manually by an operator or
    /// automatically through a configured DNS provider credential.
    Dns01,
}

impl ValidationMethod {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Http01, Self::Dns01];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Http01 => "HTTP_01",
            Self::Dns01 => "DNS_01",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "AUTO" => Some(Self::Auto),
            "HTTP_01" => Some(Self::Http01),
            "DNS_01" => Some(Self::Dns01),
            _ => None,
        }
    }

    /// Resolves the concrete ACME challenge type for a certificate scope.
    ///
    /// A wildcard scope can never resolve to HTTP-01: the returned method is the
    /// one the challenge orchestrator must present, not merely the operator's
    /// preference.
    pub const fn resolve_for_scope(self, scope: CertificateScope) -> Self {
        match (scope, self) {
            (CertificateScope::Wildcard, _) => Self::Dns01,
            (CertificateScope::SingleDomain, Self::Auto) => Self::Http01,
            (_, method) => method,
        }
    }
}

impl std::fmt::Display for ValidationMethod {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Who owns a domain zone, and therefore what the zone means.
///
/// The domain inventory holds two kinds of row that look alike but answer
/// different questions, and a reader that cannot tell them apart will show a
/// platform publishing zone as if the operator had registered it:
///
/// * [`ZoneScope::User`] — a root domain an operator defined, with an owner
///   (`deploy_dns_zone.user_id`) and a DNS zone they control.
/// * [`ZoneScope::Platform`] — the tenant-level `app.<suffix>` zone the
///   deployment provisions so apps get default publishing hostnames. It has no
///   owner (`user_id IS NULL`), is visible to every tenant member, and its apex
///   is *derived* from a suffix rather than declared by anyone.
///
/// See `TECH-cloud-site-publishing-control-plane.md` §`deploy_dns_zone`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoneScope {
    /// An operator-defined root domain owned by `user_id`.
    #[default]
    #[serde(rename = "USER")]
    User,
    /// A platform-provisioned `app.<suffix>` zone (`user_id IS NULL`).
    #[serde(rename = "PLATFORM")]
    Platform,
}

impl ZoneScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "USER",
            Self::Platform => "PLATFORM",
        }
    }

    /// The scope of a zone carrying this `user_id`.
    pub const fn for_owner(user_id: Option<i64>) -> Self {
        match user_id {
            Some(_) => Self::User,
            None => Self::Platform,
        }
    }
}

impl std::fmt::Display for ZoneScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainZoneResponse {
    pub id: String,
    #[serde(rename = "apexHostname")]
    pub apex_hostname: String,
    /// `USER` for an operator-defined root domain, `PLATFORM` for a
    /// provisioned `app.<suffix>` zone. Lets a console keep the root-domain
    /// inventory honest instead of presenting both as user domains.
    pub scope: ZoneScope,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(rename = "dnsProvider", skip_serializing_if = "Option::is_none")]
    pub dns_provider: Option<String>,
    /// Cloud account pinned to serve this zone's DNS-01 challenges.
    ///
    /// Absent means "resolve one": the certificate then falls back to the zone's
    /// account, and failing that to the deployment-level DNS provider
    /// configuration. See [`CloudAccountResponse`] — the id refers to the IAM
    /// provider account center, not to a Deploy-owned row.
    #[serde(rename = "providerAccountId", skip_serializing_if = "Option::is_none")]
    pub provider_account_id: Option<String>,
    pub status: String,
    #[serde(rename = "hostnameCount")]
    pub hostname_count: i64,
    #[serde(rename = "verifiedHostnameCount")]
    pub verified_hostname_count: i64,
    #[serde(rename = "certificateCount")]
    pub certificate_count: i64,
    #[serde(rename = "bindingCount")]
    pub binding_count: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainZonePage {
    pub items: Vec<DomainZoneResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateDomainZoneRequest {
    #[serde(rename = "apexHostname")]
    pub apex_hostname: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(rename = "dnsProvider", default)]
    pub dns_provider: Option<String>,
    #[serde(rename = "providerZoneRef", default)]
    pub provider_zone_ref: Option<String>,
    /// Cloud account that will present this zone's DNS-01 challenges.
    ///
    /// Optional. Omitting it is legal and common: the zone then carries no pin, and
    /// issuance resolves an account from the account center by provider, falling
    /// back to the deployment-level provider configuration. Nothing is derived and
    /// stored, so re-registering the account later cannot leave the zone pointing
    /// at an id that no longer exists.
    #[serde(rename = "providerAccountId", default)]
    pub provider_account_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateDomainZoneRequest {
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(rename = "dnsProvider", default)]
    pub dns_provider: Option<String>,
    #[serde(rename = "providerZoneRef", default)]
    pub provider_zone_ref: Option<String>,
    /// Re-pins the zone's cloud account.
    ///
    /// Three states, because "leave it" and "unpin it" are different requests and a
    /// certificate's fallback depends on the difference: omitted leaves the current
    /// pin alone, an empty string clears it (the zone then resolves an account per
    /// issuance), and any other value must name an account this caller may bind.
    #[serde(rename = "providerAccountId", default)]
    pub provider_account_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainHostnameResponse {
    pub id: String,
    #[serde(rename = "zoneId")]
    pub zone_id: String,
    pub hostname: String,
    #[serde(rename = "relativeName")]
    pub relative_name: String,
    #[serde(rename = "hostnameType")]
    pub hostname_type: String,
    #[serde(rename = "verificationStatus")]
    pub verification_status: String,
    #[serde(rename = "verifiedAt", skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    pub status: String,
    #[serde(rename = "certificateCount")]
    pub certificate_count: i64,
    #[serde(rename = "bindingCount")]
    pub binding_count: i64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainHostnamePage {
    pub items: Vec<DomainHostnameResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateDomainHostnameRequest {
    #[serde(rename = "relativeName")]
    pub relative_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateDomainHostnameRequest {
    #[serde(rename = "relativeName")]
    pub relative_name: String,
}

pub(crate) fn default_page() -> i32 {
    1
}

pub(crate) fn default_page_size() -> i32 {
    20
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DomainVerifyResponse {
    pub verified: bool,
    pub method: String,
    #[serde(rename = "verificationId", skip_serializing_if = "Option::is_none")]
    pub verification_id: Option<String>,
    #[serde(rename = "recordName", skip_serializing_if = "Option::is_none")]
    pub record_name: Option<String>,
    /// The same record relative to its zone, which is the value a DNS
    /// provider's "host"/"主机记录" field expects. Omitted when the zone is
    /// unknown or the record is not inside it.
    #[serde(rename = "recordRelativeName", skip_serializing_if = "Option::is_none")]
    pub record_relative_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Batch ownership intent: "these are the hostnames this certificate order will
/// cover — make sure I own them".
///
/// The names are **fully qualified** hostnames (a leading `*.` is allowed on the
/// leftmost label) rather than the relative names `CreateDomainHostnameRequest`
/// speaks. A caller that already knows what it wants to issue for should not
/// have to fold the name back into the zone itself: that arithmetic is
/// zone-local and every consumer that repeats it is a place to get it wrong.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnsureDomainHostnameClaimsRequest {
    pub hostnames: Vec<String>,
}

/// One hostname's ownership state after an `ensure` pass.
///
/// `verified` is the **observation made by this call**, not a copy of the row:
/// ownership is proven by a DNS TXT lookup, so asking again is the only way to
/// learn whether the operator has published the record yet. The presentation
/// fields are populated only while a challenge is outstanding, and `dnsRecordValue`
/// is the public proof digest — never key material.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DomainHostnameClaimResponse {
    pub hostname: DomainHostnameResponse,
    pub verified: bool,
    #[serde(rename = "dnsRecordName", skip_serializing_if = "Option::is_none")]
    pub dns_record_name: Option<String>,
    /// The same record relative to the zone that owns it — the value a DNS
    /// provider's "host"/"主机记录" field expects. Omitted when the record is
    /// not inside a known zone.
    #[serde(
        rename = "dnsRecordRelativeName",
        skip_serializing_if = "Option::is_none"
    )]
    pub dns_record_relative_name: Option<String>,
    #[serde(rename = "dnsRecordType", skip_serializing_if = "Option::is_none")]
    pub dns_record_type: Option<String>,
    #[serde(rename = "dnsRecordValue", skip_serializing_if = "Option::is_none")]
    pub dns_record_value: Option<String>,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainHostnameClaimBatchResponse {
    pub items: Vec<DomainHostnameClaimResponse>,
}

/// A DNS-capable cloud account, as the Deploy console renders it.
///
/// A projection of the IAM provider account center, not a Deploy-owned resource:
/// the same Aliyun or Cloudflare key is wanted by other business modules, and its
/// custody — envelope encryption, rotation, write-only access — stays with IAM.
/// Deploy therefore never returns credential material here, only whether one is
/// configured.
///
/// `tenant_global` is the wire form of the distinction the picker shows: `true`
/// for `platform` and `tenant` accounts any member of the tenant may use, `false`
/// for an account one user bound to themselves. It is derived rather than sent by
/// IAM so the two surfaces cannot drift.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CloudAccountResponse {
    pub id: String,
    #[serde(rename = "accountCode")]
    pub account_code: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "vendorCode")]
    pub vendor_code: String,
    /// Canonical DNS provider spelling this account drives: `ALIYUN_DNS`,
    /// `DNSPOD`, or `CLOUDFLARE`. Absent when the vendor drives no DNS family this
    /// build supports, which is why such an account is never offered for binding.
    #[serde(rename = "dnsProvider", skip_serializing_if = "Option::is_none")]
    pub dns_provider: Option<String>,
    /// `platform`, `tenant`, or `user`.
    #[serde(rename = "scopeType")]
    pub scope_type: String,
    /// Whether every member of the tenant may use this account.
    #[serde(rename = "tenantGlobal")]
    pub tenant_global: bool,
    /// Set for `user`-scope accounts only.
    #[serde(rename = "ownerUserId", skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    pub status: String,
    /// Whether an active credential exists behind the account. An account without
    /// one can be listed but not bound: it looks configured and is not.
    #[serde(rename = "credentialConfigured")]
    pub credential_configured: bool,
    /// Capabilities the account advertises. Empty means unspecified, which stays
    /// reusable for everything.
    #[serde(rename = "capabilityCodes", default)]
    pub capability_codes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CloudAccountPage {
    /// Matching accounts, **narrowest first**: personal accounts, then the tenant's
    /// shared ones, then platform ones, with each level's default ahead of its other
    /// candidates.
    ///
    /// The order is part of the contract rather than an accident of the query: it is
    /// the same precedence the server itself resolves in, so `items[0]` is the
    /// account a create would reuse whenever the choice is unambiguous. Without that
    /// stated, every console would re-derive the rule and some would get it wrong.
    pub items: Vec<CloudAccountResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

/// Filters for the account picker.
///
/// Also the console's "先判断是否已存在" pre-check: filtering by `dnsProvider` and
/// finding anything at all means the credential form can be skipped in favour of
/// pinning what is already there, and a create would reuse it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListCloudAccountsQuery {
    #[serde(default = "default_page")]
    pub page: i32,
    #[serde(default = "default_page_size")]
    pub page_size: i32,
    /// The DNS family being configured. Present whenever the picker is opened from a
    /// zone or certificate form, and the reason an account list is short rather than
    /// confusing: an object-storage account is not a DNS answer.
    #[serde(
        rename = "dnsProvider",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub dns_provider: Option<String>,
    /// `platform`, `tenant`, or `user`. Absent walks every level the caller can see.
    #[serde(rename = "scopeType", default, skip_serializing_if = "Option::is_none")]
    pub scope_type: Option<String>,
    /// Only the caller's own accounts.
    #[serde(default)]
    pub mine: bool,
    /// Keyword over account code and display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
}

/// Registers a DNS cloud account from console input.
///
/// The credential halves are what the chosen family needs, so the console renders
/// the vendor's own field names for that family rather than a union of every
/// family's: `accessKeyId` is the public half (Aliyun AccessKeyId, DNSPod LoginId)
/// and is absent for Cloudflare, and `secretAccessKey` is always the secret half
/// (Aliyun AccessKeySecret, DNSPod ApiToken, Cloudflare ApiToken). A
/// `dns_provider` that names no supported family is refused rather than stored,
/// because a credential whose vendor does not match the domain it will publish for
/// fails at the first order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateCloudAccountRequest {
    #[serde(rename = "displayName")]
    pub display_name: String,
    /// The account code within its scope. Pre-filled by the console with a
    /// suggestion rather than derived server-side: a derived code collides the
    /// second time a tenant adds a second account for the same vendor.
    #[serde(rename = "accountCode", default)]
    pub account_code: Option<String>,
    /// The DNS family being configured; decides the credential shape.
    #[serde(rename = "dnsProvider")]
    pub dns_provider: String,
    /// `platform`, `tenant`, or `user`. Defaults to `tenant`.
    ///
    /// A tenant member may not publish a `platform` account — the account center
    /// refuses it — so the console offers the level as a choice only where it can
    /// succeed.
    #[serde(rename = "scopeType", default)]
    pub scope_type: Option<String>,
    #[serde(rename = "environment", default)]
    pub environment: Option<String>,
    /// Promote this account to its scope's default for its vendor.
    #[serde(rename = "isDefault", default)]
    pub is_default: bool,
    #[serde(rename = "accessKeyId", default)]
    pub access_key_id: Option<String>,
    #[serde(rename = "secretAccessKey")]
    pub secret_access_key: String,
    #[serde(rename = "sessionToken", default)]
    pub session_token: Option<String>,
    /// The operator's own statement that the credential is an active one for the
    /// declared vendor with permission to edit the zones it will publish into.
    ///
    /// The console cannot probe a credential — that would mean dispatching it to
    /// the vendor from the read path — so it asks the operator instead, and this
    /// is what records that they were asked. `false` is refused rather than
    /// silently defaulted, so a caller cannot store an unattested credential by
    /// omission. Unlike the optional fields above it is therefore *not* `default`
    /// in the accepting sense: the service reads an absent value as `false`.
    #[serde(rename = "confirmsCredential", default)]
    pub confirms_credential: bool,
}

/// What registering produced.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CloudAccountRegistrationResponse {
    pub account: CloudAccountResponse,
    /// `true` when an equivalent account already existed and was reused, so the
    /// console reports "已存在，直接复用" rather than "已创建".
    pub reused: bool,
    /// `true` when this call stored the secret. A reused account that already had a
    /// credential is left alone: silently rotating a credential another business
    /// module may be using is not this flow's decision to make.
    #[serde(rename = "credentialApplied")]
    pub credential_applied: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EnvVariableResponse {
    pub id: String,
    pub key: String,
    pub value: String,
    pub environment: String,
    #[serde(rename = "isSecret")]
    pub is_secret: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EnvVariablePage {
    pub items: Vec<EnvVariableResponse>,
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateEnvVariableRequest {
    pub key: String,
    pub value: String,
    #[serde(default = "default_environment")]
    pub environment: String,
    #[serde(rename = "isSecret", default)]
    pub is_secret: bool,
}

fn default_environment() -> String {
    "production".to_string()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateResponse {
    pub id: String,
    #[serde(rename = "certName")]
    pub cert_name: String,
    #[serde(rename = "certificateSource")]
    pub certificate_source: String,
    #[serde(rename = "caProfile")]
    pub ca_profile: String,
    #[serde(rename = "certificateScope")]
    pub certificate_scope: CertificateScope,
    #[serde(rename = "validationMethod")]
    pub validation_method: ValidationMethod,
    /// Cloud account pinned to present this certificate's DNS-01 challenges.
    ///
    /// The certificate-level pin overrides the zone's, which is what makes a
    /// single zone able to serve certificates issued through different vendor
    /// accounts. Absent means the zone decides, and failing that the
    /// deployment-level provider configuration. Only meaningful for DNS-01; an
    /// HTTP-01 certificate may carry one anyway so that switching validation
    /// method later does not require re-entering it.
    #[serde(rename = "providerAccountId", skip_serializing_if = "Option::is_none")]
    pub provider_account_id: Option<String>,
    #[serde(rename = "preferredKeyAlgorithm")]
    pub preferred_key_algorithm: String,
    pub identifiers: Vec<String>,
    #[serde(rename = "currentVersionId", skip_serializing_if = "Option::is_none")]
    pub current_version_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(rename = "notBefore", skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    #[serde(rename = "notAfter", skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
    #[serde(rename = "autoRenew")]
    pub auto_renew: bool,
    #[serde(rename = "renewalStatus")]
    pub renewal_status: String,
    /// Lead time before expiry at which renewal starts.
    ///
    /// A lead time longer than the certificate's own lifetime is harmless rather
    /// than pathological: the renewal window is floored at one third of the
    /// lifetime, so a fresh short-lived certificate can never be immediately due.
    #[serde(rename = "renewBeforeDays")]
    pub renew_before_days: i32,
    /// The instant renewal work becomes due for the version being served.
    ///
    /// Sent by the server rather than derived by each client so that every
    /// surface — console, alerting, the scheduler itself — agrees on one window
    /// rule.
    #[serde(rename = "renewalDueAt", skip_serializing_if = "Option::is_none")]
    pub renewal_due_at: Option<String>,
    /// Whole days until the served version expires, floored; negative once past.
    #[serde(rename = "daysUntilExpiry", skip_serializing_if = "Option::is_none")]
    pub days_until_expiry: Option<i64>,
    /// Which part of its validity window the served version is in right now.
    ///
    /// Computed at read time from the X.509 window, never persisted: a stored
    /// phase would be wrong from the instant the clock crossed a boundary.
    #[serde(rename = "validityPhase", skip_serializing_if = "Option::is_none")]
    pub validity_phase: Option<String>,
    /// When renewal last ran to completion, successfully or not.
    #[serde(rename = "lastRenewalAt", skip_serializing_if = "Option::is_none")]
    pub last_renewal_at: Option<String>,
    /// Consecutive failed renewal attempts; a success resets it to zero.
    ///
    /// Surfaced because the retry backoff hides a repeated failure from the
    /// schedule: without the count, a certificate failing every day looks the
    /// same as one that has never been tried.
    #[serde(rename = "renewalFailureCount")]
    pub renewal_failure_count: i32,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificatePage {
    pub items: Vec<CertificateResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

/// Apex of a leading-label wildcard hostname, or `None` when the input is not a
/// single-label wildcard.
///
/// Only one leading label is stripped: `*.example.com` yields `example.com`,
/// while `*.a.example.com` yields `a.example.com` (its own apex) and
/// `*.*.example.com` / `a.*.example.com` are rejected because a certificate can
/// only ever contain a single leading wildcard label (RFC 6125 §6.4.3).
pub fn wildcard_apex(hostname: &str) -> Option<&str> {
    let apex = hostname.strip_prefix("*.")?;
    if apex.is_empty() || apex.contains('*') {
        return None;
    }
    Some(apex)
}

/// Whether a hostname is exactly one leading-label wildcard.
///
/// `*.example.com` is valid; `*` alone, `*.*.example.com`, and
/// `a.*.example.com` are not.
pub fn is_single_label_wildcard(hostname: &str) -> bool {
    wildcard_apex(hostname).is_some()
}

/// Tenant-scoped consumption of the CA budget.
///
/// The counters mirror the public CA policy surface (registered-domain budget,
/// duplicate-certificate budget, new-order rate, failed-validation rate) so an
/// operator can see why a request was refused instead of receiving an opaque
/// provider error after the order was already created.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateQuotaResponse {
    #[serde(rename = "certificatesPerDomain")]
    pub certificates_per_domain: CertificateQuotaWindow,
    #[serde(rename = "duplicateCertificateSets")]
    pub duplicate_certificate_sets: CertificateQuotaWindow,
    #[serde(rename = "newOrders")]
    pub new_orders: CertificateQuotaWindow,
    #[serde(rename = "failedValidations")]
    pub failed_validations: CertificateQuotaWindow,
    #[serde(rename = "concurrentOrders")]
    pub concurrent_orders: CertificateQuotaWindow,
}

/// One bounded counter: what was consumed inside the rolling window, and what the
/// configured ceiling is. `retry_after_at` is present only when the window is
/// currently exhausted.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateQuotaWindow {
    pub consumed: i64,
    pub limit: i64,
    #[serde(rename = "windowSeconds")]
    pub window_seconds: i64,
    #[serde(rename = "retryAfterAt", skip_serializing_if = "Option::is_none")]
    pub retry_after_at: Option<String>,
}

impl CertificateQuotaWindow {
    pub const fn exhausted(&self) -> bool {
        self.consumed >= self.limit
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateCertificateRequest {
    #[serde(rename = "certName")]
    pub cert_name: String,
    #[serde(rename = "domainIds")]
    pub domain_ids: Vec<String>,
    /// `SINGLE_DOMAIN` (default) or `WILDCARD`. A wildcard scope plans the apex
    /// in addition to the wildcard itself, because a wildcard SAN does not
    /// cover its own apex.
    #[serde(rename = "certificateScope", default)]
    pub certificate_scope: CertificateScope,
    /// Operator preference. `AUTO` (default) resolves to HTTP-01 for a
    /// single-domain scope and always to DNS-01 for a wildcard scope.
    #[serde(rename = "validationMethod", default)]
    pub validation_method: ValidationMethod,
    #[serde(rename = "caProfile", default = "default_certificate_ca_profile")]
    pub ca_profile: String,
    #[serde(
        rename = "preferredKeyAlgorithm",
        default = "default_certificate_key_algorithm"
    )]
    pub preferred_key_algorithm: String,
    /// Whether the control plane keeps this certificate renewed on its own.
    ///
    /// Defaults to true: an operator asking for a managed certificate is asking
    /// for a name that stays covered, not for a reminder to renew it.
    #[serde(rename = "autoRenew", default = "default_certificate_auto_renew")]
    pub auto_renew: bool,
    /// Days before expiry at which renewal starts; 7..=90, default 30.
    ///
    /// Stored per certificate rather than read from `deploy_tls_policy` because a
    /// certificate may be bound to several listeners with different policies, and
    /// a renewal trigger has to resolve to exactly one window. The configured lead
    /// time is additionally floored at one third of the certificate's lifetime, so
    /// a value larger than a short-lived certificate's lifetime is safe.
    #[serde(
        rename = "renewBeforeDays",
        default = "default_certificate_renew_before_days"
    )]
    pub renew_before_days: i32,
    /// Cloud account that will present this certificate's DNS-01 challenges.
    ///
    /// Overrides the zone's pin, so one zone can issue through several vendor
    /// accounts. Omitting it is the common case and leaves the certificate deferring
    /// to its zone, then to the account center, then to the deployment-level provider
    /// configuration — decided when an order runs, not frozen at creation. Not
    /// required for an HTTP-01 certificate, which never presents a DNS record.
    #[serde(rename = "providerAccountId", default)]
    pub provider_account_id: Option<String>,
}

fn default_certificate_ca_profile() -> String {
    "LETS_ENCRYPT_PRODUCTION".to_owned()
}

fn default_certificate_key_algorithm() -> String {
    // Not a literal: the chosen default is a platform decision that the ACME engine
    // and the DDL-level check alike have to agree with, so it lives once, in the
    // shared vocabulary, and changes everywhere at once.
    sdkwork_deploy_core::CERTIFICATE_DEFAULT_KEY_ALGORITHM.to_owned()
}

fn default_certificate_auto_renew() -> bool {
    true
}

fn default_certificate_renew_before_days() -> i32 {
    sdkwork_deploy_core::CERTIFICATE_DEFAULT_RENEW_BEFORE_DAYS
}

pub fn is_deploy_package_artifact_type(package_type: i32) -> bool {
    (1..=5).contains(&package_type)
}

pub const ARTIFACT_STATUS_ACTIVE: i32 = 1;
pub const ARTIFACT_STATUS_RETAINED: i32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateArtifactRequest {
    #[serde(rename = "appId", default)]
    pub app_id: Option<String>,
    #[serde(rename = "packageType")]
    pub package_type: i32,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
    #[serde(rename = "contentLength")]
    pub content_length: i64,
    #[serde(rename = "checksumSha256", default)]
    pub checksum_sha256: Option<String>,
    #[serde(rename = "driveUploadSessionId")]
    pub drive_upload_session_id: String,
    #[serde(rename = "driveUploadItemId", default)]
    pub drive_upload_item_id: Option<String>,
    #[serde(rename = "driveSpaceId")]
    pub drive_space_id: String,
    #[serde(rename = "driveNodeId")]
    pub drive_node_id: String,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArtifactResponse {
    pub id: String,
    #[serde(rename = "appId", skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(rename = "packageType")]
    pub package_type: i32,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
    #[serde(rename = "contentLength")]
    pub content_length: i64,
    #[serde(rename = "checksumSha256", skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
    #[serde(rename = "driveNodeId")]
    pub drive_node_id: String,
    #[serde(rename = "uploadSessionId")]
    pub upload_session_id: String,
    pub status: i32,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArtifactPage {
    pub items: Vec<ArtifactResponse>,
    pub total: i64,
}

pub const UPLOAD_SESSION_STATUS_COMPLETED: i32 = 1;
pub const UPLOAD_SESSION_STATUS_CANCELLED: i32 = 2;

pub const CERTIFICATE_SOURCE_MANAGED: &str = "MANAGED";
pub const CERTIFICATE_STATUS_PENDING: &str = "PENDING";
pub const CERTIFICATE_STATUS_REVOKED: &str = "REVOKED";
pub const CERTIFICATE_RENEWAL_STATUS_NONE: &str = "NONE";
pub const CERTIFICATE_RENEWAL_STATUS_PLANNED: &str = "PLANNED";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub id: String,
    #[serde(rename = "checkType")]
    pub check_type: i32,
    pub url: String,
    pub status: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HealthCheckPage {
    pub items: Vec<HealthCheckResponse>,
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateHealthCheckRequest {
    #[serde(rename = "checkType")]
    pub check_type: i32,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NginxConfigResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "configName")]
    pub config_name: String,
    #[serde(rename = "configType")]
    pub config_type: i32,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    pub status: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NginxConfigPage {
    pub items: Vec<NginxConfigResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListNginxConfigsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    // PAGINATION_SPEC §3：query 参数使用 lower_snake_case 规范词汇。
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub config_type: Option<i32>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateNginxConfigRequest {
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "configName")]
    pub config_name: String,
    #[serde(rename = "configType")]
    pub config_type: i32,
    #[serde(rename = "configContent")]
    pub config_content: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateNginxConfigRequest {
    #[serde(rename = "configName", default)]
    pub config_name: Option<String>,
    #[serde(rename = "configContent", default)]
    pub config_content: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NginxValidateResponse {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NginxReloadResponse {
    pub reloaded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NginxStatusResponse {
    pub running: bool,
    #[serde(rename = "activeConfigs")]
    pub active_configs: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerResponse {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(rename = "sshPort")]
    pub ssh_port: i32,
    #[serde(rename = "clusterId", skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
    #[serde(rename = "clusterName", skip_serializing_if = "Option::is_none")]
    pub cluster_name: Option<String>,
    #[serde(rename = "nodeRole")]
    pub node_role: i32,
    pub status: i32,
    #[serde(rename = "sshUser", skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerPage {
    pub items: Vec<ServerResponse>,
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateServerRequest {
    pub name: String,
    pub host: String,
    #[serde(rename = "sshPort", default = "default_ssh_port")]
    pub ssh_port: i32,
    #[serde(rename = "clusterId", default)]
    pub cluster_id: Option<String>,
    #[serde(rename = "sshUser", default)]
    pub ssh_user: Option<String>,
    #[serde(rename = "sshKeyPath", default)]
    pub ssh_key_path: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateServerRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "sshPort", default)]
    pub ssh_port: Option<i32>,
    #[serde(rename = "clusterId", default)]
    pub cluster_id: Option<String>,
    #[serde(rename = "sshUser", default)]
    pub ssh_user: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<i32>,
}

fn default_ssh_port() -> i32 {
    22
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NodeClusterResponse {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub status: i32,
    #[serde(rename = "nodeCount")]
    pub node_count: i64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NodeClusterPage {
    pub items: Vec<NodeClusterResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateNodeClusterRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateNodeClusterRequest {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub status: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditLogResponse {
    pub id: String,
    pub action: String,
    pub resource: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

/// 审计日志列表过滤（PAGINATION_SPEC §4：声明即实现；wire 参数使用
/// lower_snake_case 规范词汇，禁止 camelCase 别名）。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditLogQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    #[serde(default)]
    pub target_type: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub operator_id: Option<i64>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    /// Opaque keyset continuation token（PAGINATION_SPEC §6）；提供时走
    /// cursor 模式（过滤参数不适用于 cursor 模式）。
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditLogPage {
    pub items: Vec<AuditLogResponse>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    /// Opaque keyset continuation (PAGINATION_SPEC §6). An offset page carries
    /// one as well, which is how a client obtains its first cursor and switches
    /// from `page` paging to keyset continuation; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Whether a further window exists, in either mode; absent only when the
    /// caller did not request page continuation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateDeployUploadSessionRequest {
    #[serde(rename = "appId", default)]
    pub app_id: Option<String>,
    #[serde(rename = "packageType")]
    pub package_type: i32,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
    #[serde(rename = "contentLength")]
    pub content_length: i64,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeployUploadSessionResponse {
    pub id: String,
    #[serde(rename = "appId", skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(rename = "packageType")]
    pub package_type: i32,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
    #[serde(rename = "contentLength")]
    pub content_length: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    pub status: i32,
    #[serde(rename = "driveUploadSessionId")]
    pub drive_upload_session_id: String,
    #[serde(rename = "driveUploadItemId", skip_serializing_if = "Option::is_none")]
    pub drive_upload_item_id: Option<String>,
    #[serde(rename = "driveSpaceId", skip_serializing_if = "Option::is_none")]
    pub drive_space_id: Option<String>,
    #[serde(rename = "driveNodeId", skip_serializing_if = "Option::is_none")]
    pub drive_node_id: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletedUploadPartInput {
    #[serde(rename = "partNo")]
    pub part_no: i64,
    pub etag: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompleteDeployUploadSessionRequest {
    #[serde(rename = "checksumSha256Hex")]
    pub checksum_sha256_hex: String,
    #[serde(rename = "contentLength", default)]
    pub content_length: Option<i64>,
    #[serde(rename = "contentType", default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub parts: Vec<CompletedUploadPartInput>,
}
