//! Cloud-account selection for Deploy domain and certificate automation.
//!
//! # Why an account center and not a second credential table
//!
//! A DNS provider credential is a *cloud account*, not a Deploy-owned resource.
//! The same Aliyun, DNSPod, or Cloudflare key is wanted by other business modules
//! too, and its custody — envelope encryption, rotation, write-only access — is
//! IAM's job (`iam_provider_account` + `iam_provider_credential`, with the
//! `platform` / `tenant` / `user` scope levels). Deploy therefore consumes that
//! center instead of keeping the credential table it used to declare;
//! `deploy_dns_provider_credential` is deprecated as of 2026-09-17 and no code
//! path reads it.
//!
//! # What this crate is
//!
//! The declared port plus its adapters, shaped like `sdkwork-deploy-drive-port`
//! and `sdkwork-deploy-content-provider-port`:
//!
//! * [`DeployCloudAccountPort`] is the vocabulary Deploy owns — "which account may
//!   present DNS-01 for this hostname", "which accounts may this caller pick",
//!   "register one from what the console collected".
//! * [`iam`] is the only part of Deploy that calls IAM. The edge is one-way:
//!   Deploy depends on IAM, IAM never depends on Deploy.
//! * [`memory`] serves tests and local development without an account center.
//! * [`selection`] chooses between them from the environment and refuses to fall
//!   back to memory in a production-like environment.
//!
//! Keeping the IAM dependency behind this port is what lets the service and route
//! layers — and the certificates that eventually consume a credential — be written
//! against `DeployCloudAccountPort` alone, so an IAM surface change cannot reach
//! them.

pub mod iam;
pub mod memory;
mod selection;

use std::fmt;

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};

pub use iam::IamCloudAccountPort;
pub use memory::MemoryCloudAccountPort;
pub use selection::{
    cloud_account_port_from_env, select_deploy_cloud_account_port, DeployCloudAccountPortSelection,
    DeployCloudAccountPortSelectionInput,
};

/// Lifecycle value that makes an account usable.
pub const ACCOUNT_STATUS_ACTIVE: &str = "active";

/// A globally shared account kept by platform operators, resolvable everywhere.
pub const ACCOUNT_SCOPE_PLATFORM: &str = "platform";
/// An application-tenant account shared by every member of the tenant.
pub const ACCOUNT_SCOPE_TENANT: &str = "tenant";
/// An organization-scoped account kept for one organization inside the tenant.
///
/// Named here so `scope_rank` can order the level, and asserted against IAM's
/// token in the tests. It is deliberately *not* a member of
/// [`CLOUD_ACCOUNT_SCOPES`]: Deploy's own account contract carries no organization
/// dimension, so there is no organization id a console could register against.
pub const ACCOUNT_SCOPE_ORGANIZATION: &str = "organization";
/// A personal account visible only to the user who created it.
pub const ACCOUNT_SCOPE_USER: &str = "user";

/// Values accepted by an account's `scopeType`, widest first.
///
/// Three wide on purpose: `platform` is published by platform operators, `tenant`
/// covers the whole tenant, and `user` is personal. `organization` is omitted
/// because [`RegisterCloudAccountCommand`] has no organization field — accepting the
/// scope while being unable to name the organization would make Deploy ask the
/// account centre to guess. Rank order for the full four-level ladder lives in
/// `scope_rank`.
pub const CLOUD_ACCOUNT_SCOPES: &[&str] = &[
    ACCOUNT_SCOPE_PLATFORM,
    ACCOUNT_SCOPE_TENANT,
    ACCOUNT_SCOPE_USER,
];

/// The capability an account must advertise to be offered for DNS automation.
pub const CAPABILITY_DNS: &str = "dns";

/// Credential shapes a DNS family's secret is stored as.
///
/// Deploy names them itself rather than importing IAM's constants, so this crate's
/// vocabulary survives an IAM rename; `iam` asserts the two still agree.
pub const CREDENTIAL_KIND_ACCESS_KEY_PAIR: &str = "access_key_pair";
pub const CREDENTIAL_KIND_BEARER_TOKEN: &str = "bearer_token";

/// The DNS provider families Deploy can present a challenge through.
///
/// The strings are the same ones `deploy_dns_zone.dns_provider` already stored and
/// the ACME engine parses, so no existing row changes meaning.
pub mod dns_provider {
    /// Alibaba Cloud DNS. Two-part credential.
    pub const ALIYUN_DNS: &str = "ALIYUN_DNS";
    /// Tencent Cloud DNSPod. Two-part credential.
    pub const DNSPOD: &str = "DNSPOD";
    /// Cloudflare DNS. Single bearer token.
    pub const CLOUDFLARE: &str = "CLOUDFLARE";

    /// Every family this module can drive, in the order the console offers them.
    pub const ALL: [&str; 3] = [ALIYUN_DNS, DNSPOD, CLOUDFLARE];

    /// Whether `value` names a family this module can drive.
    pub fn is_supported(value: &str) -> bool {
        ALL.iter()
            .any(|family| family.eq_ignore_ascii_case(value.trim()))
    }

    /// Canonical uppercase spelling of a family, or `None` when unsupported.
    pub fn normalize(value: &str) -> Option<&'static str> {
        let trimmed = value.trim();
        ALL.iter()
            .copied()
            .find(|family| family.eq_ignore_ascii_case(trimmed))
    }

    /// The IAM `vendorCode` that supplies this family.
    pub fn vendor_code_for(family: &str) -> Option<&'static str> {
        match normalize(family)? {
            ALIYUN_DNS => Some("aliyun"),
            DNSPOD => Some("tencent"),
            CLOUDFLARE => Some("cloudflare"),
            _ => None,
        }
    }

    /// The family a vendor code drives, or `None` when the vendor is not a DNS one.
    ///
    /// Vendor codes are free-form (`^[a-z][a-z0-9_]{1,31}$`) and the console is
    /// free to name a DNS-specific account `aliyun_dns` or `dnspod` rather than
    /// the cloud-wide `aliyun` / `tencent`. A trailing `_dns` is therefore stripped
    /// before matching, which keeps both spellings working without teaching the
    /// vocabulary here about every name a tenant might invent.
    pub fn family_for_vendor_code(vendor_code: &str) -> Option<&'static str> {
        let code = vendor_code.trim().to_ascii_lowercase();
        if code.is_empty() {
            return None;
        }
        let code = code.strip_suffix("_dns").unwrap_or(&code);
        match code {
            "aliyun" | "ali" => Some(ALIYUN_DNS),
            "tencent" | "dnspod" | "qcloud" => Some(DNSPOD),
            "cloudflare" | "cf" => Some(CLOUDFLARE),
            _ => None,
        }
    }

    /// How a secret is shaped for this family.
    ///
    /// `access_key_pair` carries a public identifier plus a secret; `bearer_token`
    /// carries the secret alone. Cloudflare has no public half, so its token is a
    /// bearer token rather than a half-empty key pair.
    pub fn credential_kind_for(family: &str) -> Option<&'static str> {
        match normalize(family)? {
            ALIYUN_DNS | DNSPOD => Some(super::CREDENTIAL_KIND_ACCESS_KEY_PAIR),
            CLOUDFLARE => Some(super::CREDENTIAL_KIND_BEARER_TOKEN),
            _ => None,
        }
    }
}

/// One cloud account, as Deploy needs to see it.
///
/// Never carries credential material. This is deliberately a projection of the
/// account center rather than its whole row: Deploy needs to render a choice
/// ("租户全局 / 我的账号", default badge, which vendor) and to decide whether an
/// account is usable, and nothing else.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CloudAccount {
    pub id: String,
    /// One of [`CLOUD_ACCOUNT_SCOPES`].
    pub scope_type: String,
    /// Set when `scope_type` is [`ACCOUNT_SCOPE_USER`]; `None` for shared scopes.
    pub owner_user_id: Option<String>,
    pub vendor_code: String,
    pub account_code: String,
    pub display_name: String,
    pub environment: String,
    pub status: String,
    /// Whether the account is its scope's default for its vendor and environment.
    pub is_default: bool,
    /// Capabilities the account advertises. Empty means "unspecified", which the
    /// account center treats as reusable for every capability — so an account
    /// created before capabilities were tracked stays usable.
    pub capability_codes: Vec<String>,
    /// Whether an active credential row exists behind the account.
    pub credential_configured: bool,
    pub credential_count: i64,
}

impl CloudAccount {
    /// Whether every member of the tenant can use this account, as opposed to one
    /// user having bound it to themselves.
    ///
    /// This is the split the console presents as 租户全局 / 我的账号.
    pub fn is_tenant_global(&self) -> bool {
        matches!(
            self.scope_type.as_str(),
            ACCOUNT_SCOPE_PLATFORM | ACCOUNT_SCOPE_TENANT
        )
    }

    /// Whether the account is enabled and actually carries a secret.
    pub fn is_usable(&self) -> bool {
        self.status == ACCOUNT_STATUS_ACTIVE && self.credential_configured
    }

    /// The DNS family this account can drive, from its vendor code.
    pub fn dns_provider(&self) -> Option<&'static str> {
        dns_provider::family_for_vendor_code(&self.vendor_code)
    }

    /// Whether the account advertises a capability. An empty list means
    /// "unspecified", which stays reusable for everything.
    pub fn serves_capability(&self, capability_code: &str) -> bool {
        let wanted = capability_code.trim().to_ascii_lowercase();
        self.capability_codes.is_empty()
            || self
                .capability_codes
                .iter()
                .any(|code| code.eq_ignore_ascii_case(&wanted))
    }
}

/// Which accounts a caller wants listed.
#[derive(Clone, Debug, Default)]
pub struct ListCloudAccountsCommand {
    pub tenant_id: i64,
    /// The acting end user. Enables the personal scope level; without it only the
    /// shared levels are walked.
    pub user_id: Option<i64>,
    /// Restrict to one vendor. This is how the console answers "do I already have
    /// an account for this provider".
    pub vendor_code: Option<String>,
    /// Restrict to one scope level; `None` walks every visible level.
    pub scope_type: Option<String>,
    /// Only the caller's own accounts.
    pub mine: bool,
    /// Include `platform`-scope accounts. On by default: a tenant with nothing of
    /// its own is expected to reuse the platform account.
    pub include_platform: bool,
    /// Restrict to accounts advertising this capability, e.g. [`CAPABILITY_DNS`].
    pub capability_code: Option<String>,
    pub search: Option<String>,
    pub page: i32,
    pub page_size: i32,
}

/// One page of accounts, shaped like the module's other list results.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CloudAccountPage {
    pub items: Vec<CloudAccount>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

/// Fills in an account the console could not find ready-made.
///
/// The three credential fields are the union of what the supported families need,
/// so the console renders one form: `access_key_id` is the public half (Aliyun
/// AccessKeyId, DNSPod LoginId) and is unused by Cloudflare, and
/// `secret_access_key` is always the secret half (Aliyun AccessKeySecret, DNSPod
/// ApiToken, Cloudflare ApiToken). [`iam`] maps them per family.
#[derive(Clone, Debug)]
pub struct RegisterCloudAccountCommand {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub actor_id: i64,
    /// The acting user. Required when the account is being registered at the
    /// personal scope level.
    pub user_id: Option<i64>,
    pub display_name: String,
    /// The account code within its scope. The console pre-fills a suggestion, the
    /// way the storage console does, because a derived code would collide the
    /// second time a tenant adds a second account for the same vendor.
    pub account_code: String,
    /// The DNS family being configured; decides the credential shape.
    pub dns_provider: String,
    /// `platform` / `tenant` / `user`. Defaults to the account center's default
    /// (`tenant`) when absent.
    pub scope_type: Option<String>,
    /// Only meaningful with `scope_type = user`; defaults to the acting user.
    pub owner_user_id: Option<String>,
    pub environment: Option<String>,
    /// Promote this account to its scope's default for its vendor.
    pub is_default: bool,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

/// What registering produced.
#[derive(Clone, Debug)]
pub struct CloudAccountRegistration {
    pub account: CloudAccount,
    /// `true` when an equivalent account already existed for this vendor and was
    /// reused, so the console reports "已存在，直接复用" rather than "已创建".
    pub reused: bool,
    /// `true` when this call wrote the secret. A reused account that already had a
    /// credential is left alone: silently rotating a credential another business
    /// module is using is not this flow's decision to make.
    pub credential_applied: bool,
}

/// Everything needed to ask one account to publish a `_acme-challenge` TXT record.
///
/// Plaintext, and deliberately neither `Serialize` nor `Clone`-friendly beyond
/// what the caller needs: it exists for the length of one issuance attempt.
pub struct DnsAccountCredential {
    pub account_id: String,
    /// The family that will present the record, canonical uppercase.
    pub dns_provider: String,
    /// Public half. Empty for families whose secret has no identifier.
    pub access_key_id: String,
    /// Secret half.
    pub secret_access_key: String,
}

impl fmt::Debug for DnsAccountCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The account and family are useful in logs; the secret is not.
        formatter
            .debug_struct("DnsAccountCredential")
            .field("account_id", &self.account_id)
            .field("dns_provider", &self.dns_provider)
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", &"<redacted>")
            .finish()
    }
}

/// The account center, as Deploy uses it.
#[async_trait]
pub trait DeployCloudAccountPort: Send + Sync {
    /// Accounts this caller may pick from.
    async fn list_accounts(
        &self,
        command: ListCloudAccountsCommand,
    ) -> DeployServiceResult<CloudAccountPage>;

    /// The account this caller already has for a provider, if any.
    ///
    /// This is the "先判断是否已存在，存在就不用创建" probe. It walks the account
    /// center's own precedence (personal, then tenant, then platform) so the answer
    /// is the account the caller would actually be given, not merely one that
    /// exists. A level offering several candidates with no default is a `Conflict`
    /// rather than a guess: picking one for the operator would silently bind a
    /// certificate to a credential nobody chose.
    async fn find_existing_account(
        &self,
        tenant_id: i64,
        user_id: Option<i64>,
        dns_provider: &str,
        environment: Option<&str>,
    ) -> DeployServiceResult<Option<CloudAccount>>;

    /// Registers an account from console input, reusing one that already matches.
    async fn register_account(
        &self,
        command: RegisterCloudAccountCommand,
    ) -> DeployServiceResult<CloudAccountRegistration>;

    /// Confirms a pinned account id may be used, and reports it.
    ///
    /// Called on every write that stores a `provider_account_id`, so a bad id
    /// fails the request that supplied it instead of the first certificate order
    /// that later needs the credential. An out-of-scope id answers `NotFound`, not
    /// `Forbidden`, so the endpoint cannot be used to probe which ids exist.
    async fn ensure_bindable(
        &self,
        tenant_id: i64,
        user_id: Option<i64>,
        account_id: &str,
    ) -> DeployServiceResult<CloudAccount>;

    /// Decrypts the credential behind a pinned account, for DNS-01 presentation.
    ///
    /// `dns_provider` is the zone's declared family and wins when present, because
    /// the zone is the statement of who actually serves those records; otherwise
    /// the family is derived from the account's vendor.
    ///
    /// There is no caller on this path: an issuance worker has a tenant and a
    /// certificate, not a session. A personal-scope account pinned on the
    /// certificate is therefore resolved as its owner, which is sound because the
    /// pin was already authorised against that owner when it was written.
    async fn resolve_dns_credential(
        &self,
        tenant_id: i64,
        account_id: &str,
        dns_provider: Option<&str>,
    ) -> DeployServiceResult<DnsAccountCredential>;
}

/// The adapter the service host installed, or the honest absence of one.
///
/// `Unconfigured` is not a silent degradation: every operation reports that no
/// account center is reachable rather than answering "no accounts", which would
/// read to an operator as "this tenant has none" and hide a wiring fault.
#[derive(Clone)]
pub enum DeployCloudAccountPortAdapter {
    Iam(IamCloudAccountPort),
    Memory(MemoryCloudAccountPort),
    Unconfigured,
}

impl DeployCloudAccountPortAdapter {
    fn unconfigured(operation: &str) -> DeployServiceError {
        DeployServiceError::Internal(format!(
            "the cloud account center is not configured, so {operation} is unavailable; \
             set SDKWORK_IAM_PROVIDER_ACCOUNT_* and SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS=false, \
             or run against an environment that hosts sdkwork-iam"
        ))
    }
}

#[async_trait]
impl DeployCloudAccountPort for DeployCloudAccountPortAdapter {
    async fn list_accounts(
        &self,
        command: ListCloudAccountsCommand,
    ) -> DeployServiceResult<CloudAccountPage> {
        match self {
            Self::Iam(port) => port.list_accounts(command).await,
            Self::Memory(port) => port.list_accounts(command).await,
            Self::Unconfigured => Err(Self::unconfigured("listing cloud accounts")),
        }
    }

    async fn find_existing_account(
        &self,
        tenant_id: i64,
        user_id: Option<i64>,
        dns_provider: &str,
        environment: Option<&str>,
    ) -> DeployServiceResult<Option<CloudAccount>> {
        match self {
            Self::Iam(port) => {
                port.find_existing_account(tenant_id, user_id, dns_provider, environment)
                    .await
            }
            Self::Memory(port) => {
                port.find_existing_account(tenant_id, user_id, dns_provider, environment)
                    .await
            }
            Self::Unconfigured => Err(Self::unconfigured("reusing an existing cloud account")),
        }
    }

    async fn register_account(
        &self,
        command: RegisterCloudAccountCommand,
    ) -> DeployServiceResult<CloudAccountRegistration> {
        match self {
            Self::Iam(port) => port.register_account(command).await,
            Self::Memory(port) => port.register_account(command).await,
            Self::Unconfigured => Err(Self::unconfigured("registering a cloud account")),
        }
    }

    async fn ensure_bindable(
        &self,
        tenant_id: i64,
        user_id: Option<i64>,
        account_id: &str,
    ) -> DeployServiceResult<CloudAccount> {
        match self {
            Self::Iam(port) => port.ensure_bindable(tenant_id, user_id, account_id).await,
            Self::Memory(port) => port.ensure_bindable(tenant_id, user_id, account_id).await,
            Self::Unconfigured => Err(Self::unconfigured("binding a cloud account")),
        }
    }

    async fn resolve_dns_credential(
        &self,
        tenant_id: i64,
        account_id: &str,
        dns_provider: Option<&str>,
    ) -> DeployServiceResult<DnsAccountCredential> {
        match self {
            Self::Iam(port) => {
                port.resolve_dns_credential(tenant_id, account_id, dns_provider)
                    .await
            }
            Self::Memory(port) => {
                port.resolve_dns_credential(tenant_id, account_id, dns_provider)
                    .await
            }
            Self::Unconfigured => Err(Self::unconfigured("resolving a DNS provider credential")),
        }
    }
}

/// The DNS family a pinned account is expected to drive.
///
/// Shared by the adapters so the rule — "the zone's declared family wins, otherwise
/// ask the account's vendor" — is stated once.
pub(crate) fn resolve_dns_family(
    account: &CloudAccount,
    declared: Option<&str>,
) -> DeployServiceResult<&'static str> {
    if let Some(raw) = declared.map(str::trim).filter(|value| !value.is_empty()) {
        return dns_provider::normalize(raw).ok_or_else(|| {
            DeployServiceError::validation(format!(
                "dnsProvider `{raw}` is not one of {}",
                dns_provider::ALL.join(", ")
            ))
        });
    }
    account.dns_provider().ok_or_else(|| {
        DeployServiceError::validation(format!(
            "cloud account `{}` uses vendor `{}`, which does not drive a supported DNS provider ({})",
            account.id,
            account.vendor_code,
            dns_provider::ALL.join(", ")
        ))
    })
}

/// Picks the one account a caller would be given, out of everything visible.
///
/// The account center's own precedence, applied to an already filtered candidate
/// set: personal beats tenant, tenant beats platform; inside one level the account
/// flagged as default wins, a lone candidate wins by elimination, and several
/// candidates with no default is a `Conflict` rather than a guess — silently
/// binding a certificate to a credential nobody chose is worse than making the
/// operator choose.
///
/// `family` is used only to name the provider in the conflict message.
///
/// The filter that produced `candidates` is the caller's business: the IAM adapter
/// asks the account center for the accounts that advertise the DNS capability,
/// because a vendor code is a free-form label while a DNS family is the real key.
pub(crate) fn select_by_precedence(
    mut candidates: Vec<CloudAccount>,
    family: &str,
) -> DeployServiceResult<Option<CloudAccount>> {
    // Narrowest first. `organization` sits between the personal layer and the
    // tenant-wide one; it is unreachable while both adapters pass
    // `include_organization_shared: false`, and named here so switching that layer on
    // cannot silently change which account a resolver picks.
    for scope in [
        ACCOUNT_SCOPE_USER,
        ACCOUNT_SCOPE_ORGANIZATION,
        ACCOUNT_SCOPE_TENANT,
        ACCOUNT_SCOPE_PLATFORM,
    ] {
        let mut level: Vec<CloudAccount> = candidates
            .iter()
            .filter(|account| account.scope_type == scope)
            .cloned()
            .collect();
        if level.is_empty() {
            continue;
        }
        if let Some(index) = level.iter().position(|account| account.is_default) {
            return Ok(Some(level.swap_remove(index)));
        }
        if level.len() == 1 {
            return Ok(level.pop());
        }
        return Err(DeployServiceError::conflict(format!(
            "{} cloud accounts for {} are available in the `{scope}` scope and none is the default; \
             mark one as the default or select an account explicitly",
            level.len(),
            family,
        )));
    }
    candidates.clear();
    Ok(None)
}

/// The checks a pinned account must pass before a resource may store its id.
///
/// Shared by both adapters so the answer to "may this account present DNS-01"
/// is stated once. Each refusal names the reason, because the console shows it
/// verbatim and "选了一个不能用的账号" without the reason is the support ticket this
/// exists to prevent.
///
/// Visibility is *not* checked here: it needs the caller, and it is the account
/// centre's own rule, so each adapter applies it while loading.
pub(crate) fn assert_bindable(account: &CloudAccount) -> DeployServiceResult<()> {
    if account.status != ACCOUNT_STATUS_ACTIVE {
        return Err(DeployServiceError::conflict(format!(
            "cloud account `{}` is {}; an active account is required",
            account.id, account.status
        )));
    }
    if account.dns_provider().is_none() {
        return Err(DeployServiceError::validation(format!(
            "cloud account `{}` uses vendor `{}`, which does not drive a supported DNS provider ({})",
            account.id,
            account.vendor_code,
            dns_provider::ALL.join(", ")
        )));
    }
    if !account.serves_capability(CAPABILITY_DNS) {
        return Err(DeployServiceError::validation(format!(
            "cloud account `{}` does not advertise the `{CAPABILITY_DNS}` capability",
            account.id
        )));
    }
    if !account.credential_configured {
        return Err(DeployServiceError::validation(format!(
            "cloud account `{}` has no active credential; \
             configure one in the account center before binding it",
            account.id
        )));
    }
    Ok(())
}
