//! In-memory account center for tests and local development.
//!
//! Deliberately not a permissive stub: it applies the same rules the IAM adapter
//! does — scope visibility, `active` status, a DNS-capable vendor, an advertised
//! `dns` capability, and a credential that actually exists — so a test that passes
//! against it is exercising the caller's logic rather than a fake that answers yes
//! to everything. What it does not do is persist, encrypt, or enforce uniqueness
//! across processes.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};

use crate::{
    assert_bindable, dns_provider, resolve_dns_family, select_by_precedence, CloudAccount,
    CloudAccountPage, CloudAccountRegistration, DeployCloudAccountPort, DnsAccountCredential,
    ListCloudAccountsCommand, RegisterCloudAccountCommand, ACCOUNT_SCOPE_ORGANIZATION,
    ACCOUNT_SCOPE_PLATFORM, ACCOUNT_SCOPE_TENANT, ACCOUNT_SCOPE_USER, ACCOUNT_STATUS_ACTIVE,
    CAPABILITY_DNS, CLOUD_ACCOUNT_SCOPES,
};

/// One seeded account plus what the projection deliberately does not carry.
#[derive(Clone, Debug)]
pub struct MemoryCloudAccountSeed {
    pub id: String,
    pub tenant_id: i64,
    pub scope_type: String,
    /// Required for [`ACCOUNT_SCOPE_USER`].
    pub owner_user_id: Option<String>,
    pub vendor_code: String,
    pub account_code: String,
    pub display_name: String,
    pub environment: String,
    pub is_default: bool,
    /// Capabilities the account advertises. Empty means "unspecified", which the
    /// account center treats as reusable for every capability.
    pub capability_codes: Vec<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
}

impl Default for MemoryCloudAccountSeed {
    fn default() -> Self {
        Self {
            id: String::new(),
            tenant_id: 1,
            scope_type: ACCOUNT_SCOPE_TENANT.to_owned(),
            owner_user_id: None,
            vendor_code: "aliyun".to_owned(),
            account_code: "aliyun-dns-main".to_owned(),
            display_name: "阿里云 DNS 主账号".to_owned(),
            environment: "production".to_owned(),
            is_default: false,
            capability_codes: vec![CAPABILITY_DNS.to_owned()],
            access_key_id: Some("LTAI-memory".to_owned()),
            secret_access_key: Some("memory-secret".to_owned()),
        }
    }
}

impl MemoryCloudAccountSeed {
    /// A tenant-wide account for one vendor, enabled and carrying a credential.
    pub fn tenant(id: impl Into<String>, tenant_id: i64, vendor_code: &str) -> Self {
        Self {
            id: id.into(),
            tenant_id,
            vendor_code: vendor_code.to_owned(),
            ..Self::default()
        }
    }

    /// A personal account owned by one user.
    pub fn owned(
        id: impl Into<String>,
        tenant_id: i64,
        owner_user_id: impl Into<String>,
        vendor_code: &str,
    ) -> Self {
        Self {
            scope_type: ACCOUNT_SCOPE_USER.to_owned(),
            owner_user_id: Some(owner_user_id.into()),
            ..Self::tenant(id, tenant_id, vendor_code)
        }
    }

    pub fn code(mut self, account_code: impl Into<String>) -> Self {
        self.account_code = account_code.into();
        self
    }

    pub fn default_for_scope(mut self) -> Self {
        self.is_default = true;
        self
    }

    pub fn with_capabilities(mut self, capability_codes: &[&str]) -> Self {
        self.capability_codes = capability_codes
            .iter()
            .map(|code| (*code).to_owned())
            .collect();
        self
    }

    pub fn without_credential(mut self) -> Self {
        self.access_key_id = None;
        self.secret_access_key = None;
        self
    }
}

#[derive(Clone, Debug)]
struct AccountState {
    projection: CloudAccount,
    tenant_id: i64,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
}

#[derive(Default)]
struct State {
    accounts: Vec<AccountState>,
    next_id: u64,
}

/// An in-memory account center, seedable from tests.
#[derive(Clone, Default)]
pub struct MemoryCloudAccountPort {
    inner: Arc<Mutex<State>>,
}

/// The identity a caller acts as, as one comparable value.
///
/// A personal account is only visible to its owner, and an owner id is a string in
/// the account center while a caller id is numeric here, so the two are compared in
/// one representation to keep the comparison in a single place.
#[derive(Clone, PartialEq, Eq)]
struct Caller {
    tenant_id: i64,
    user_id: Option<String>,
}

impl Caller {
    fn new(tenant_id: i64, user_id: Option<i64>) -> Self {
        Self {
            tenant_id,
            user_id: user_id.map(|id| id.to_string()),
        }
    }
}

impl MemoryCloudAccountPort {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds an account. Returns the same handle, so seeds chain.
    pub fn with_account(self, seed: MemoryCloudAccountSeed) -> Self {
        let credential_present = seed.secret_access_key.is_some();
        {
            let mut state = self.inner.lock().expect("memory account center lock");
            state.accounts.push(AccountState {
                projection: CloudAccount {
                    id: seed.id,
                    scope_type: seed.scope_type,
                    owner_user_id: seed.owner_user_id,
                    vendor_code: seed.vendor_code,
                    account_code: seed.account_code,
                    display_name: seed.display_name,
                    environment: seed.environment,
                    status: ACCOUNT_STATUS_ACTIVE.to_owned(),
                    is_default: seed.is_default,
                    capability_codes: seed
                        .capability_codes
                        .iter()
                        .map(|code| code.to_ascii_lowercase())
                        .collect(),
                    credential_configured: credential_present,
                    credential_count: i64::from(credential_present),
                },
                tenant_id: seed.tenant_id,
                access_key_id: seed.access_key_id,
                secret_access_key: seed.secret_access_key,
            });
        }
        self
    }

    /// Whether an account is visible to `caller`.
    ///
    /// The same three rules the IAM `account_is_visible_to` states: platform
    /// accounts are global, personal accounts belong to exactly one user inside its
    /// own tenant, and every other (tenant) account belongs to its tenant.
    fn visible(account: &AccountState, caller: &Caller) -> bool {
        match account.projection.scope_type.as_str() {
            ACCOUNT_SCOPE_PLATFORM => true,
            ACCOUNT_SCOPE_USER => {
                account.tenant_id == caller.tenant_id
                    && caller.user_id.is_some()
                    && account.projection.owner_user_id == caller.user_id
            }
            _ => account.tenant_id == caller.tenant_id,
        }
    }

    fn visible_accounts(&self, caller: &Caller) -> Vec<AccountState> {
        let state = self.inner.lock().expect("memory account center lock");
        state
            .accounts
            .iter()
            .filter(|account| Self::visible(account, caller))
            .cloned()
            .collect()
    }

    fn find(&self, account_id: &str) -> Option<AccountState> {
        let state = self.inner.lock().expect("memory account center lock");
        state
            .accounts
            .iter()
            .find(|account| account.projection.id == account_id)
            .cloned()
    }
}

#[async_trait]
impl DeployCloudAccountPort for MemoryCloudAccountPort {
    async fn list_accounts(
        &self,
        command: ListCloudAccountsCommand,
    ) -> DeployServiceResult<CloudAccountPage> {
        if command.mine && command.user_id.is_none() {
            return Err(DeployServiceError::validation(
                "mine requires an authenticated user",
            ));
        }
        let caller = Caller::new(command.tenant_id, command.user_id);
        let page = command.page.max(1);
        let page_size = command.page_size.clamp(1, 200);
        let mut accounts: Vec<CloudAccount> = self
            .visible_accounts(&caller)
            .into_iter()
            .filter(|account| {
                let projection = &account.projection;
                if !command.include_platform && projection.scope_type == ACCOUNT_SCOPE_PLATFORM {
                    return false;
                }
                if command.mine && projection.scope_type != ACCOUNT_SCOPE_USER {
                    return false;
                }
                if let Some(scope) = command.scope_type.as_deref() {
                    if !projection.scope_type.eq_ignore_ascii_case(scope.trim()) {
                        return false;
                    }
                }
                if let Some(vendor) = command.vendor_code.as_deref() {
                    if !projection.vendor_code.eq_ignore_ascii_case(vendor.trim()) {
                        return false;
                    }
                }
                if let Some(capability) = command.capability_code.as_deref() {
                    if !projection.serves_capability(capability) {
                        return false;
                    }
                }
                match command.search.as_deref().map(str::trim) {
                    Some(needle) if !needle.is_empty() => {
                        let needle = needle.to_ascii_lowercase();
                        projection
                            .display_name
                            .to_ascii_lowercase()
                            .contains(&needle)
                            || projection
                                .account_code
                                .to_ascii_lowercase()
                                .contains(&needle)
                    }
                    _ => true,
                }
            })
            .map(|account| account.projection)
            .collect();
        // Narrowest scope first, then defaults, so the console lists what a resolver
        // would actually pick ahead of the wider fallbacks.
        accounts.sort_by(|left, right| {
            scope_rank(&left.scope_type)
                .cmp(&scope_rank(&right.scope_type))
                .then_with(|| right.is_default.cmp(&left.is_default))
                .then_with(|| left.account_code.cmp(&right.account_code))
        });
        let total = accounts.len() as i64;
        let offset = ((page - 1) as usize) * (page_size as usize);
        let items = accounts
            .into_iter()
            .skip(offset)
            .take(page_size as usize)
            .collect();
        Ok(CloudAccountPage {
            items,
            total,
            page,
            page_size,
        })
    }

    async fn find_existing_account(
        &self,
        tenant_id: i64,
        user_id: Option<i64>,
        dns_provider: &str,
        environment: Option<&str>,
    ) -> DeployServiceResult<Option<CloudAccount>> {
        let family = dns_provider::normalize(dns_provider).ok_or_else(|| {
            DeployServiceError::validation(format!(
                "dnsProvider `{dns_provider}` is not one of {}",
                dns_provider::ALL.join(", ")
            ))
        })?;
        let environment = environment
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase);
        let caller = Caller::new(tenant_id, user_id);
        let candidates: Vec<CloudAccount> = self
            .visible_accounts(&caller)
            .into_iter()
            .filter(|account| {
                account.projection.status == ACCOUNT_STATUS_ACTIVE
                    && account.projection.dns_provider() == Some(family)
                    && account.projection.serves_capability(CAPABILITY_DNS)
                    && environment
                        .as_deref()
                        .is_none_or(|wanted| account.projection.environment == wanted)
            })
            .map(|account| account.projection)
            .collect();
        select_by_precedence(candidates, family)
    }

    async fn register_account(
        &self,
        command: RegisterCloudAccountCommand,
    ) -> DeployServiceResult<CloudAccountRegistration> {
        let family = dns_provider::normalize(&command.dns_provider).ok_or_else(|| {
            DeployServiceError::validation(format!(
                "dnsProvider `{}` is not one of {}",
                command.dns_provider,
                dns_provider::ALL.join(", ")
            ))
        })?;
        let display_name = command.display_name.trim();
        if display_name.is_empty() {
            return Err(DeployServiceError::validation("displayName is required"));
        }
        let account_code = command.account_code.trim().to_ascii_lowercase();
        if account_code.is_empty() {
            return Err(DeployServiceError::validation("accountCode is required"));
        }
        if command.secret_access_key.trim().is_empty() {
            return Err(DeployServiceError::validation(
                "the DNS provider secret is required",
            ));
        }
        let environment = command
            .environment
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("production")
            .to_ascii_lowercase();

        // Reuse first: not creating a second account for a provider the caller
        // already has is the whole point of this path.
        if let Some(existing) = self
            .find_existing_account(
                command.tenant_id,
                command.user_id,
                family,
                Some(environment.as_str()),
            )
            .await?
        {
            let credential_applied = !existing.credential_configured;
            if credential_applied {
                let mut state = self.inner.lock().expect("memory account center lock");
                if let Some(account) = state
                    .accounts
                    .iter_mut()
                    .find(|account| account.projection.id == existing.id)
                {
                    account.access_key_id = Some(command.access_key_id.clone());
                    account.secret_access_key = Some(command.secret_access_key.clone());
                    account.projection.credential_configured = true;
                    account.projection.credential_count += 1;
                }
            }
            let account = self.find(&existing.id).expect("account just resolved");
            return Ok(CloudAccountRegistration {
                account: account.projection,
                reused: true,
                credential_applied,
            });
        }

        let scope_type = command
            .scope_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| ACCOUNT_SCOPE_TENANT.to_owned());
        if !CLOUD_ACCOUNT_SCOPES.contains(&scope_type.as_str()) {
            return Err(DeployServiceError::validation(format!(
                "scopeType must be one of {}",
                CLOUD_ACCOUNT_SCOPES.join(", ")
            )));
        }
        let owner_user_id = match scope_type.as_str() {
            ACCOUNT_SCOPE_PLATFORM => {
                return Err(DeployServiceError::validation(
                    "a platform-scope account is published by platform operators, \
                     not from a tenant console",
                ))
            }
            ACCOUNT_SCOPE_TENANT => None,
            _ => Some(
                command
                    .owner_user_id
                    .clone()
                    .or_else(|| command.user_id.map(|id| id.to_string()))
                    .ok_or_else(|| {
                        DeployServiceError::validation(
                            "ownerUserId is required for a user-scope account",
                        )
                    })?,
            ),
        };
        // Derived rather than taken from the console, so this adapter cannot end up
        // disagreeing with `dns_provider` about who drives the family.
        let vendor_code = dns_provider::vendor_code_for(family).unwrap_or("custom");
        let account_id = {
            let mut state = self.inner.lock().expect("memory account center lock");
            state.next_id += 1;
            format!("memory-account-{:04}", state.next_id)
        };
        let projection = CloudAccount {
            id: account_id.clone(),
            scope_type: scope_type.clone(),
            owner_user_id: owner_user_id.clone(),
            vendor_code: vendor_code.to_owned(),
            account_code,
            display_name: display_name.to_owned(),
            environment,
            status: ACCOUNT_STATUS_ACTIVE.to_owned(),
            is_default: command.is_default,
            capability_codes: vec![CAPABILITY_DNS.to_owned()],
            credential_configured: true,
            credential_count: 1,
        };
        {
            let mut state = self.inner.lock().expect("memory account center lock");
            state.accounts.push(AccountState {
                projection: projection.clone(),
                tenant_id: command.tenant_id,
                access_key_id: Some(command.access_key_id.clone()),
                secret_access_key: Some(command.secret_access_key.clone()),
            });
            if command.is_default {
                for other in state.accounts.iter_mut() {
                    if other.projection.scope_type == scope_type
                        && other.projection.owner_user_id == owner_user_id
                        && other.projection.id != account_id
                    {
                        other.projection.is_default = false;
                    }
                }
            }
        }
        Ok(CloudAccountRegistration {
            account: projection,
            reused: false,
            credential_applied: true,
        })
    }

    async fn ensure_bindable(
        &self,
        tenant_id: i64,
        user_id: Option<i64>,
        account_id: &str,
    ) -> DeployServiceResult<CloudAccount> {
        let account_id = account_id.trim();
        if account_id.is_empty() {
            return Err(DeployServiceError::validation(
                "providerAccountId must not be empty",
            ));
        }
        let caller = Caller::new(tenant_id, user_id);
        let account = self
            .visible_accounts(&caller)
            .into_iter()
            .find(|account| account.projection.id == account_id)
            // Absent and out-of-scope answer identically on purpose: a distinct
            // "forbidden" would let a caller probe which ids exist.
            .ok_or_else(|| DeployServiceError::not_found("cloud account not found"))?;
        assert_bindable(&account.projection)?;
        Ok(account.projection)
    }

    async fn resolve_dns_credential(
        &self,
        tenant_id: i64,
        account_id: &str,
        dns_provider: Option<&str>,
    ) -> DeployServiceResult<DnsAccountCredential> {
        let account = self
            .find(account_id.trim())
            .filter(|account| {
                account.projection.scope_type == ACCOUNT_SCOPE_PLATFORM
                    || account.tenant_id == tenant_id
            })
            .ok_or_else(|| DeployServiceError::not_found("cloud account not found"))?;
        if account.projection.status != ACCOUNT_STATUS_ACTIVE {
            return Err(DeployServiceError::validation(format!(
                "cloud account `{account_id}` is {}; an active account is required to resolve a credential",
                account.projection.status
            )));
        }
        let family = resolve_dns_family(&account.projection, dns_provider)?;
        let secret = account.secret_access_key.clone().ok_or_else(|| {
            DeployServiceError::not_found(
                "no active credential is configured for this cloud account",
            )
        })?;
        Ok(DnsAccountCredential {
            account_id: account.projection.id,
            dns_provider: family.to_owned(),
            access_key_id: account.access_key_id.unwrap_or_default(),
            secret_access_key: secret,
        })
    }
}

/// Narrowest-first rank over the account centre's four shared scopes; kept in step
/// with `iam::scope_rank` so the two adapters choose the same account.
fn scope_rank(scope_type: &str) -> u8 {
    match scope_type {
        ACCOUNT_SCOPE_USER => 0,
        ACCOUNT_SCOPE_ORGANIZATION => 1,
        ACCOUNT_SCOPE_TENANT => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CREDENTIAL_KIND_ACCESS_KEY_PAIR, CREDENTIAL_KIND_BEARER_TOKEN};
    use sdkwork_deploy_contract::DeployServiceErrorKind;

    fn registration(dns_provider: &str, account_code: &str) -> RegisterCloudAccountCommand {
        RegisterCloudAccountCommand {
            tenant_id: 7,
            organization_id: 0,
            actor_id: 42,
            user_id: Some(42),
            display_name: "我的 DNS 账号".to_owned(),
            account_code: account_code.to_owned(),
            dns_provider: dns_provider.to_owned(),
            scope_type: None,
            owner_user_id: None,
            environment: None,
            is_default: false,
            access_key_id: "LTAI-new".to_owned(),
            secret_access_key: "new-secret".to_owned(),
            session_token: None,
        }
    }

    #[tokio::test]
    async fn a_personal_account_wins_over_the_tenant_default() {
        let port = MemoryCloudAccountPort::new()
            .with_account(
                MemoryCloudAccountSeed::tenant("acct-tenant", 7, "aliyun").default_for_scope(),
            )
            .with_account(MemoryCloudAccountSeed::owned(
                "acct-mine",
                7,
                "42",
                "aliyun",
            ));
        let found = port
            .find_existing_account(7, Some(42), dns_provider::ALIYUN_DNS, None)
            .await
            .expect("probe")
            .expect("an account");
        assert_eq!(found.id, "acct-mine");
    }

    #[tokio::test]
    async fn a_lone_tenant_account_is_reused_without_being_the_default() {
        let port = MemoryCloudAccountPort::new().with_account(MemoryCloudAccountSeed::tenant(
            "acct-tenant",
            7,
            "cloudflare",
        ));
        let found = port
            .find_existing_account(7, None, dns_provider::CLOUDFLARE, None)
            .await
            .expect("probe")
            .expect("an account");
        assert_eq!(found.id, "acct-tenant");
    }

    #[tokio::test]
    async fn two_undecorated_candidates_are_a_conflict_not_a_guess() {
        let port = MemoryCloudAccountPort::new()
            .with_account(MemoryCloudAccountSeed::tenant("acct-a", 7, "aliyun").code("a"))
            .with_account(MemoryCloudAccountSeed::tenant("acct-b", 7, "aliyun").code("b"));
        let error = port
            .find_existing_account(7, None, dns_provider::ALIYUN_DNS, None)
            .await
            .expect_err("must refuse to choose");
        assert_eq!(error.kind(), DeployServiceErrorKind::Conflict);
    }

    #[tokio::test]
    async fn another_tenants_account_is_invisible() {
        let port = MemoryCloudAccountPort::new().with_account(MemoryCloudAccountSeed::tenant(
            "acct-other",
            99,
            "aliyun",
        ));
        assert!(port
            .find_existing_account(7, None, dns_provider::ALIYUN_DNS, None)
            .await
            .expect("probe")
            .is_none());
        let error = port
            .ensure_bindable(7, None, "acct-other")
            .await
            .expect_err("must not bind");
        assert_eq!(error.kind(), DeployServiceErrorKind::NotFound);
    }

    #[tokio::test]
    async fn another_users_personal_account_is_invisible() {
        let port = MemoryCloudAccountPort::new().with_account(MemoryCloudAccountSeed::owned(
            "acct-theirs",
            7,
            "99",
            "aliyun",
        ));
        let error = port
            .ensure_bindable(7, Some(42), "acct-theirs")
            .await
            .expect_err("must not bind");
        assert_eq!(error.kind(), DeployServiceErrorKind::NotFound);
    }

    #[tokio::test]
    async fn registering_reuses_an_existing_account_instead_of_creating_a_second() {
        let port = MemoryCloudAccountPort::new().with_account(
            MemoryCloudAccountSeed::tenant("acct-tenant", 7, "cloudflare").without_credential(),
        );
        let outcome = port
            .register_account(registration(dns_provider::CLOUDFLARE, "cloudflare-dns-1"))
            .await
            .expect("register");
        assert!(outcome.reused, "an existing account must be reused");
        assert!(
            outcome.credential_applied,
            "a reused account with no credential takes the one just supplied"
        );
        assert_eq!(outcome.account.id, "acct-tenant");
        assert!(outcome.account.credential_configured);
    }

    #[tokio::test]
    async fn registering_leaves_an_existing_credential_alone() {
        let port = MemoryCloudAccountPort::new().with_account(MemoryCloudAccountSeed::tenant(
            "acct-tenant",
            7,
            "aliyun",
        ));
        let outcome = port
            .register_account(registration(dns_provider::ALIYUN_DNS, "aliyun-dns-2"))
            .await
            .expect("register");
        assert!(outcome.reused);
        assert!(
            !outcome.credential_applied,
            "rotating a credential another module may be using is not this flow's call"
        );
        let credential = port
            .resolve_dns_credential(7, "acct-tenant", None)
            .await
            .expect("credential");
        assert_eq!(credential.secret_access_key, "memory-secret");
    }

    #[tokio::test]
    async fn registering_creates_the_account_when_nothing_matches() {
        let port = MemoryCloudAccountPort::new();
        let outcome = port
            .register_account(registration(dns_provider::DNSPOD, "tencent-dns-1"))
            .await
            .expect("register");
        assert!(!outcome.reused);
        assert!(outcome.credential_applied);
        assert_eq!(outcome.account.scope_type, ACCOUNT_SCOPE_TENANT);
        assert_eq!(outcome.account.vendor_code, "tencent");
        assert_eq!(outcome.account.dns_provider(), Some(dns_provider::DNSPOD));
    }

    #[tokio::test]
    async fn registering_a_second_provider_does_not_reuse_the_first() {
        let port = MemoryCloudAccountPort::new().with_account(MemoryCloudAccountSeed::tenant(
            "acct-aliyun",
            7,
            "aliyun",
        ));
        let outcome = port
            .register_account(registration(dns_provider::CLOUDFLARE, "cloudflare-dns-1"))
            .await
            .expect("register");
        assert!(!outcome.reused, "a different family is a different account");
        assert_eq!(outcome.account.id, "memory-account-0001");
    }

    #[tokio::test]
    async fn an_object_storage_account_cannot_be_bound_as_a_dns_provider() {
        let port = MemoryCloudAccountPort::new().with_account(
            MemoryCloudAccountSeed::tenant("acct-oss", 7, "aliyun")
                .with_capabilities(&["object_storage"]),
        );
        let error = port
            .ensure_bindable(7, None, "acct-oss")
            .await
            .expect_err("must refuse");
        assert_eq!(error.kind(), DeployServiceErrorKind::Validation);
    }

    #[tokio::test]
    async fn a_non_dns_vendor_cannot_be_bound() {
        let port = MemoryCloudAccountPort::new()
            .with_account(MemoryCloudAccountSeed::tenant("acct-aws", 7, "aws"));
        let error = port
            .ensure_bindable(7, None, "acct-aws")
            .await
            .expect_err("must refuse");
        assert_eq!(error.kind(), DeployServiceErrorKind::Validation);
    }

    #[tokio::test]
    async fn an_account_without_a_credential_cannot_be_bound() {
        let port = MemoryCloudAccountPort::new().with_account(
            MemoryCloudAccountSeed::tenant("acct-empty", 7, "aliyun").without_credential(),
        );
        let error = port
            .ensure_bindable(7, None, "acct-empty")
            .await
            .expect_err("must refuse");
        assert_eq!(error.kind(), DeployServiceErrorKind::Validation);
    }

    #[tokio::test]
    async fn resolving_reports_the_family_and_the_secret() {
        let port = MemoryCloudAccountPort::new().with_account(MemoryCloudAccountSeed::tenant(
            "acct-cf",
            7,
            "cloudflare",
        ));
        let credential = port
            .resolve_dns_credential(7, "acct-cf", None)
            .await
            .expect("credential");
        assert_eq!(credential.dns_provider, dns_provider::CLOUDFLARE);
        assert_eq!(credential.access_key_id, "LTAI-memory");
        assert_eq!(credential.secret_access_key, "memory-secret");
    }

    #[tokio::test]
    async fn an_unsupported_declared_family_is_reported_with_its_value() {
        let port = MemoryCloudAccountPort::new().with_account(MemoryCloudAccountSeed::tenant(
            "acct-tenant",
            7,
            "aliyun",
        ));
        let error = port
            .resolve_dns_credential(7, "acct-tenant", Some("ROUTE53"))
            .await
            .expect_err("must refuse");
        assert_eq!(error.kind(), DeployServiceErrorKind::Validation);
        assert!(error.to_string().contains("ROUTE53"));
    }

    #[tokio::test]
    async fn the_secret_never_appears_in_a_debug_rendering() {
        let port = MemoryCloudAccountPort::new().with_account(MemoryCloudAccountSeed::tenant(
            "acct-cf",
            7,
            "cloudflare",
        ));
        let credential = port
            .resolve_dns_credential(7, "acct-cf", None)
            .await
            .expect("credential");
        let rendered = format!("{credential:?}");
        assert!(!rendered.contains("memory-secret"));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn vendor_codes_map_both_ways_including_the_dns_specific_spelling() {
        assert_eq!(
            dns_provider::family_for_vendor_code("aliyun"),
            Some(dns_provider::ALIYUN_DNS)
        );
        assert_eq!(
            dns_provider::family_for_vendor_code("aliyun_dns"),
            Some(dns_provider::ALIYUN_DNS)
        );
        assert_eq!(
            dns_provider::family_for_vendor_code("tencent"),
            Some(dns_provider::DNSPOD)
        );
        assert_eq!(
            dns_provider::family_for_vendor_code("dnspod"),
            Some(dns_provider::DNSPOD)
        );
        assert_eq!(
            dns_provider::family_for_vendor_code("cloudflare"),
            Some(dns_provider::CLOUDFLARE)
        );
        assert_eq!(dns_provider::family_for_vendor_code("aws"), None);
        for family in dns_provider::ALL {
            let vendor = dns_provider::vendor_code_for(family).expect("a vendor per family");
            assert_eq!(dns_provider::family_for_vendor_code(vendor), Some(family));
        }
    }

    #[test]
    fn each_family_maps_to_a_credential_shape() {
        assert_eq!(
            dns_provider::credential_kind_for(dns_provider::ALIYUN_DNS),
            Some(CREDENTIAL_KIND_ACCESS_KEY_PAIR)
        );
        assert_eq!(
            dns_provider::credential_kind_for(dns_provider::DNSPOD),
            Some(CREDENTIAL_KIND_ACCESS_KEY_PAIR)
        );
        assert_eq!(
            dns_provider::credential_kind_for(dns_provider::CLOUDFLARE),
            Some(CREDENTIAL_KIND_BEARER_TOKEN)
        );
        assert_eq!(dns_provider::credential_kind_for("ROUTE53"), None);
    }
}
