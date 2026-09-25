//! The one part of Deploy that calls sdkwork-iam.
//!
//! Everything IAM-specific stops at this file: the account center's row shape, its
//! error vocabulary, its scope resolution, and its credential envelope are all
//! translated into the port's vocabulary here, so the service and route layers
//! never depend on how the account center stores an account.
//!
//! The dependency is one-way. Deploy depends on
//! `sdkwork-iam-provider-account-service`; sdkwork-iam knows nothing about Deploy —
//! which is why the account is referenced by an opaque id
//! (`deploy_dns_zone.provider_account_id`) rather than by a foreign key.

use async_trait::async_trait;
use sqlx::PgPool;

use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};
use sdkwork_iam_provider_account_service::{
    create_account, find_account, list_accounts, load_bound_account, resolve_account_scope,
    resolve_credential_material, upsert_active_credential, AccountVisibility, NewProviderAccount,
    NewProviderCredential, ProviderAccount, ProviderAccountError, ScopeCaller,
    ACCOUNT_TYPE_LONG_TERM_KEY, DEFAULT_CREDENTIAL_NAME,
};

use crate::{
    assert_bindable, dns_provider, resolve_dns_family, select_by_precedence, CloudAccount,
    CloudAccountPage, CloudAccountRegistration, DeployCloudAccountPort, DnsAccountCredential,
    ListCloudAccountsCommand, RegisterCloudAccountCommand, ACCOUNT_SCOPE_ORGANIZATION,
    ACCOUNT_SCOPE_TENANT, ACCOUNT_SCOPE_USER, ACCOUNT_STATUS_ACTIVE, CAPABILITY_DNS,
    CREDENTIAL_KIND_ACCESS_KEY_PAIR,
};

/// How many accounts one probe or listing reads.
///
/// The account center paginates by table columns and cannot filter by capability —
/// `capability_codes` is a JSON array in a text column, and a malformed row would
/// turn one bad account into a failed query for everybody. The capability filter
/// therefore runs here, which means the window has to be wide enough to contain the
/// caller's accounts: a tenant with more than this many cloud accounts is not a
/// shape this control plane expects, and the console shows a page of them anyway.
const ACCOUNT_WINDOW: i64 = 200;

/// The account center, reached through the IAM provider-account service.
#[derive(Clone)]
pub struct IamCloudAccountPort {
    pg: PgPool,
}

impl IamCloudAccountPort {
    pub fn new(pg: PgPool) -> Self {
        Self { pg }
    }

    /// Writes the secret for one account, shaped for its DNS family.
    async fn write_credential(
        &self,
        account_id: &str,
        family: &str,
        command: &RegisterCloudAccountCommand,
    ) -> DeployServiceResult<()> {
        let credential_kind = dns_provider::credential_kind_for(family).ok_or_else(|| {
            DeployServiceError::validation(format!(
                "`{family}` is not one of {}",
                dns_provider::ALL.join(", ")
            ))
        })?;
        let secret = command.secret_access_key.trim().to_owned();
        if secret.is_empty() {
            return Err(DeployServiceError::validation(
                "the DNS provider secret is required",
            ));
        }
        // A key pair is written as a pair or not at all: the account center refuses
        // a half-filled one, and a credential no presenter can use is worse than a
        // refusal because it looks configured.
        let (access_key_id, secret_access_key, session_token, secret_text) =
            if credential_kind == CREDENTIAL_KIND_ACCESS_KEY_PAIR {
                let identifier = command.access_key_id.trim().to_owned();
                if identifier.is_empty() {
                    return Err(DeployServiceError::validation(format!(
                        "accessKeyId is required for {family}"
                    )));
                }
                (
                    Some(identifier),
                    Some(secret),
                    command
                        .session_token
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned),
                    None,
                )
            } else {
                (None, None, None, Some(secret))
            };
        let credential = NewProviderCredential {
            provider_account_id: account_id.to_owned(),
            credential_kind: credential_kind.to_owned(),
            credential_name: DEFAULT_CREDENTIAL_NAME.to_owned(),
            access_key_id,
            secret_access_key,
            session_token,
            secret_text,
            expires_at: None,
            actor_id: command.actor_id.to_string(),
        };
        upsert_active_credential(&self.pg, &credential)
            .await
            .map_err(map_error)?;
        Ok(())
    }

    /// Loads a pinned account for work that has no caller.
    ///
    /// `load_bound_account` needs a caller to decide visibility, and an issuance
    /// worker has none: it holds a tenant and a certificate. Rather than skipping
    /// the check, a personal account is loaded *as its owner* — the pin was already
    /// authorised against that owner when the certificate stored it, so this
    /// replays that decision instead of making a new one. Tenant- and
    /// platform-scoped accounts carry no owner and load directly.
    async fn load_pinned(
        &self,
        tenant_id: i64,
        account_id: &str,
    ) -> DeployServiceResult<ProviderAccount> {
        let tenant = tenant_id.to_string();
        if let Ok(account) = load_bound_account(&self.pg, &tenant, None, account_id).await {
            return Ok(account);
        }
        // Read the row without the visibility gate for one purpose only: to learn
        // whether an owner exists. The row is not returned until `load_bound_account`
        // accepts it.
        let Some(candidate) = find_account(&self.pg, &tenant, account_id)
            .await
            .map_err(map_error)?
        else {
            return Err(DeployServiceError::not_found("cloud account not found"));
        };
        let Some(owner) = candidate.owner_user_id.as_deref() else {
            return Err(DeployServiceError::not_found("cloud account not found"));
        };
        load_bound_account(&self.pg, &tenant, Some(owner), account_id)
            .await
            .map_err(map_error)
    }
}

#[async_trait]
impl DeployCloudAccountPort for IamCloudAccountPort {
    async fn list_accounts(
        &self,
        command: ListCloudAccountsCommand,
    ) -> DeployServiceResult<CloudAccountPage> {
        let caller_user = command.user_id.map(|id| id.to_string());
        // "My accounts" is expressed as both a scope and an owner, so a level the
        // caller did not ask for cannot leak in through the owner filter alone.
        let (scope_type, owner_user_id) = if command.mine {
            let Some(caller) = caller_user.clone() else {
                return Err(DeployServiceError::validation(
                    "mine requires an authenticated user",
                ));
            };
            (Some(ACCOUNT_SCOPE_USER.to_owned()), Some(caller))
        } else {
            (command.scope_type.clone(), None)
        };
        let visibility = AccountVisibility {
            tenant_id: command.tenant_id.to_string(),
            user_id: caller_user,
            // Deploy carries no organization dimension: its cloud account contract
            // has no organization field, so the organization layer is left off
            // rather than switched on with an id nobody supplied. Asking the
            // account center to include a layer without saying which organization
            // would be asking it to guess.
            organization_id: None,
            include_platform: command.include_platform,
            // The tenant-wide layer is always included, because "the tenant's global
            // accounts" is precisely what the console has to offer next to the
            // caller's own. The gate is this route's own permission
            // (`deploy.cloudAccounts.read`), which the caller already passed; the
            // account center is not where that decision gets re-litigated.
            include_tenant_shared: true,
            include_organization_shared: false,
            scope_type,
            owner_user_id,
        };
        // Narrowed here rather than at the account centre. The centre's filter takes
        // **one exact `vendorCode`**, and one family is written under several, so
        // asking it for the canonical code would drop the accounts a tenant
        // registered under `tencent` / `qcloud` — precisely the accounts a
        // certificate can still be bound to, which makes the loss invisible on this
        // side and a silent "you have no account for this provider" in the console.
        let family = command.narrowing_family()?;
        let (accounts, _) = list_accounts(
            &self.pg,
            &visibility,
            None,
            None,
            None,
            command.search.as_deref(),
            ACCOUNT_WINDOW,
            0,
        )
        .await
        .map_err(map_error)?;

        let mut items: Vec<CloudAccount> = accounts
            .into_iter()
            .filter(|account| match command.capability_code.as_deref() {
                Some(code) => account.serves_capability(code),
                None => true,
            })
            .map(to_cloud_account)
            // Recognised from each account's own vendor code, with the same function
            // the certificate path binds by, so this list and `ensure_bindable` cannot
            // disagree about which family an account drives. Applied before `total`
            // below, which is what makes the page and the count describe the same set.
            .filter(|account| family.is_none_or(|family| account.dns_provider() == Some(family)))
            .collect();
        // Narrowest scope first, then defaults, so the console lists what a resolver
        // would pick ahead of the wider fallbacks.
        items.sort_by(|left, right| {
            scope_rank(&left.scope_type)
                .cmp(&scope_rank(&right.scope_type))
                .then_with(|| right.is_default.cmp(&left.is_default))
                .then_with(|| left.account_code.cmp(&right.account_code))
        });
        let total = items.len() as i64;
        let page = command.page.max(1);
        let page_size = command.page_size.clamp(1, 200);
        let offset = ((page - 1) as usize) * (page_size as usize);
        let items = items
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
        // The whole visible set is asked for because the provider family, not the
        // vendor code, is the key: a tenant may name its account `aliyun_dns` or
        // `tencent` and both spellings drive the same family.
        let visibility = AccountVisibility {
            tenant_id: tenant_id.to_string(),
            user_id: user_id.map(|id| id.to_string()),
            // Same reasoning as the listing: Deploy has no organization to name, and
            // a zone's resolution must see every level the tenant made visible.
            organization_id: None,
            include_platform: true,
            include_tenant_shared: true,
            include_organization_shared: false,
            scope_type: None,
            owner_user_id: None,
        };
        let (accounts, _) = list_accounts(
            &self.pg,
            &visibility,
            None,
            None,
            Some(ACCOUNT_STATUS_ACTIVE),
            None,
            ACCOUNT_WINDOW,
            0,
        )
        .await
        .map_err(map_error)?;
        let candidates: Vec<CloudAccount> = accounts
            .into_iter()
            .filter(|account| {
                account.serves_capability(CAPABILITY_DNS)
                    && dns_provider::family_for_vendor_code(&account.vendor_code) == Some(family)
            })
            .map(to_cloud_account)
            .filter(|account| {
                environment
                    .as_deref()
                    .is_none_or(|wanted| account.environment == wanted)
            })
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
        let environment = command
            .environment
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("production")
            .to_ascii_lowercase();

        // Reuse before create: not minting a second account for a provider the
        // caller already has is the entire point of this path.
        if let Some(mut existing) = self
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
                self.write_credential(&existing.id, family, &command)
                    .await?;
                // The row now carries a credential. Mirrored locally rather than
                // re-read, because the account's own tenant owns the credential row
                // and a platform-scope account does not live in the caller's tenant.
                existing.credential_configured = true;
                existing.credential_count += 1;
            }
            return Ok(CloudAccountRegistration {
                account: existing,
                reused: true,
                credential_applied,
            });
        }

        // The caller's organization travels twice, and deliberately so: as part of
        // `ScopeCaller` it decides whether an `organization`-scope account may be
        // kept at all, and as `requested_organization_id` it is the value a shared
        // (`tenant` / `user`) row records. The account center pins the first and
        // honours the second, so passing both keeps Deploy's behaviour unchanged
        // for shared scopes while letting the center enforce its own rule for the
        // narrow one.
        let caller_tenant = command.tenant_id.to_string();
        let caller_user = command.user_id.map(|id| id.to_string());
        let caller_organization = command.organization_id.to_string();
        let caller = ScopeCaller::new(
            &caller_tenant,
            caller_user.as_deref(),
            Some(caller_organization.as_str()),
        )
        // Deploy is an in-process consumer, not an end user: the authorization
        // decision for "may this operator register a tenant-wide DNS account" was
        // already made one layer up by the route's own permission
        // (`deploy.cloudAccounts.create`). The account centre cannot re-litigate it
        // from a browser session's grant list, so the shared-scope right is asserted
        // here — the same posture `list_accounts` takes by passing
        // `include_tenant_shared: true` outright.
        .managing_shared(true);
        let resolved = resolve_account_scope(
            command.scope_type.as_deref(),
            command.owner_user_id.as_deref(),
            &caller,
            Some(caller_organization.as_str()),
        )
        .map_err(map_error)?;
        let vendor_code = dns_provider::vendor_code_for(family).ok_or_else(|| {
            DeployServiceError::validation(format!("`{family}` does not map to a cloud vendor"))
        })?;
        let account = create_account(
            &self.pg,
            &NewProviderAccount {
                // Cloned rather than moved: `caller` still borrows this string for
                // the duration of the insert.
                tenant_id: caller_tenant.clone(),
                organization_id: resolved.organization_id,
                scope_type: resolved.scope_type,
                owner_user_id: resolved.owner_user_id,
                vendor_code: vendor_code.to_owned(),
                account_code,
                display_name: display_name.to_owned(),
                // A DNS automation account is a durable access-key pair, so it is a
                // long-term key rather than a temporary credential. The account
                // centre classifies `accountType` by identity shape, not by the
                // relationship the account has to the tenant. A value it does not
                // recognize is a CHECK violation at insert time, so this must track
                // IAM's vocabulary rather than a local literal.
                account_type: ACCOUNT_TYPE_LONG_TERM_KEY.to_owned(),
                environment,
                external_account_id: None,
                capability_codes: vec![CAPABILITY_DNS.to_owned()],
                region_code: None,
                is_default: command.is_default,
                actor_id: command.actor_id.to_string(),
            },
            &caller,
        )
        .await
        .map_err(map_error)?;
        // The account exists before its secret does, which is the order that leaves
        // something recoverable: an account with no credential is a visible,
        // fixable state, whereas a credential with no account has nowhere to live.
        self.write_credential(&account.id, family, &command).await?;
        let mut projection = to_cloud_account(account);
        projection.credential_configured = true;
        projection.credential_count = 1;
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
        let account = load_bound_account(
            &self.pg,
            &tenant_id.to_string(),
            user_id.map(|id| id.to_string()).as_deref(),
            account_id,
        )
        .await
        .map_err(map_error)?;
        let projection = to_cloud_account(account);
        assert_bindable(&projection)?;
        Ok(projection)
    }

    async fn resolve_dns_credential(
        &self,
        tenant_id: i64,
        account_id: &str,
        dns_provider: Option<&str>,
    ) -> DeployServiceResult<DnsAccountCredential> {
        let account = self.load_pinned(tenant_id, account_id.trim()).await?;
        if account.status != ACCOUNT_STATUS_ACTIVE {
            return Err(DeployServiceError::validation(format!(
                "cloud account `{account_id}` is {}; an active account is required to resolve a credential",
                account.status
            )));
        }
        let projection = to_cloud_account(account.clone());
        let family = resolve_dns_family(&projection, dns_provider)?;
        // The credential kind is not constrained: the row is the truth about what
        // was stored, and asking for a kind the row does not have would report
        // "no credential" for a secret that is actually there.
        let material = resolve_credential_material(&self.pg, &account.tenant_id, &account.id, None)
            .await
            .map_err(map_error)?;
        let missing = |field: &str| {
            DeployServiceError::validation(format!(
                "the credential behind cloud account `{account_id}` is missing {field}; \
                 it is not usable for {family}"
            ))
        };
        let (access_key_id, secret_access_key) = match family {
            dns_provider::ALIYUN_DNS | dns_provider::DNSPOD => (
                material
                    .access_key_id
                    .clone()
                    .ok_or_else(|| missing("an AccessKeyId / LoginId"))?,
                material
                    .secret_access_key
                    .clone()
                    .ok_or_else(|| missing("an AccessKeySecret / ApiToken"))?,
            ),
            _ => (
                String::new(),
                material
                    .secret_text
                    .clone()
                    .or_else(|| material.secret_access_key.clone())
                    .ok_or_else(|| missing("an API token"))?,
            ),
        };
        Ok(DnsAccountCredential {
            account_id: account.id,
            dns_provider: family.to_owned(),
            access_key_id,
            secret_access_key,
        })
    }
}

/// Translates the account center's error vocabulary into the deploy one.
///
/// Each kind keeps its own meaning rather than collapsing into an internal error:
/// a validation refusal is the operator's typo, a conflict is an account that
/// already exists, and only `Unavailable` and `Cipher` are actually this process's
/// problem — and those two must not be retried as if the input were wrong.
fn map_error(error: ProviderAccountError) -> DeployServiceError {
    match error {
        ProviderAccountError::Validation(message) => DeployServiceError::validation(message),
        ProviderAccountError::NotFound(message) => DeployServiceError::not_found(message),
        ProviderAccountError::Conflict(message) => DeployServiceError::conflict(message),
        ProviderAccountError::Unavailable(_) => DeployServiceError::DatabaseUnavailable,
        ProviderAccountError::Cipher(message) => DeployServiceError::Internal(format!(
            "cloud account credential custody failed: {message}"
        )),
    }
}

fn to_cloud_account(account: ProviderAccount) -> CloudAccount {
    CloudAccount {
        id: account.id,
        scope_type: account.scope_type,
        owner_user_id: account.owner_user_id,
        vendor_code: account.vendor_code,
        account_code: account.account_code,
        display_name: account.display_name,
        environment: account.environment,
        status: account.status,
        is_default: account.is_default,
        capability_codes: account.capability_codes,
        credential_configured: account.credential_configured,
        credential_count: account.credential_count,
    }
}

/// Narrowest-first rank over the account centre's four shared scopes.
///
/// Ties are the failure mode to avoid: `organization` used to fall into a catch-all
/// arm alongside `platform`, which ranks it *wider* than `tenant` and would make an
/// organization-owned account lose to the tenant-wide one. Every level is therefore
/// named explicitly instead of being absorbed by `_`.
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
    use crate::CAPABILITY_DNS as DEPLOY_CAPABILITY_DNS;
    use crate::{ACCOUNT_SCOPE_PLATFORM, CREDENTIAL_KIND_BEARER_TOKEN};
    use sdkwork_iam_provider_account_service as iam;

    /// This crate names its own credential-kind vocabulary so a rename inside IAM
    /// cannot silently change what Deploy writes. That only holds while the two
    /// agree, so the agreement is asserted here rather than assumed.
    #[test]
    fn the_deploy_credential_kinds_are_the_ones_iam_stores() {
        assert_eq!(
            CREDENTIAL_KIND_ACCESS_KEY_PAIR,
            iam::CREDENTIAL_KIND_ACCESS_KEY_PAIR
        );
        assert_eq!(
            CREDENTIAL_KIND_BEARER_TOKEN,
            iam::CREDENTIAL_KIND_BEARER_TOKEN
        );
    }

    /// The same guard for the scope, status, and capability tokens: a value Deploy
    /// sends that IAM does not recognize would be rejected at runtime instead of at
    /// compile time, and the failure would look like a bad request rather than a
    /// version skew.
    #[test]
    fn the_deploy_scope_status_and_capability_tokens_are_the_iam_ones() {
        assert_eq!(ACCOUNT_SCOPE_PLATFORM, iam::ACCOUNT_SCOPE_PLATFORM);
        assert_eq!(ACCOUNT_SCOPE_TENANT, iam::ACCOUNT_SCOPE_TENANT);
        assert_eq!(ACCOUNT_SCOPE_USER, iam::ACCOUNT_SCOPE_USER);
        assert_eq!(ACCOUNT_STATUS_ACTIVE, iam::ACCOUNT_STATUS_ACTIVE);
        // `organization` is deliberately absent from `CLOUD_ACCOUNT_SCOPES` (Deploy
        // has no organization dimension to name), but `scope_rank` must still place
        // the level, so the token itself has to be IAM's or the rank would be
        // comparing apples to oranges the day the layer is switched on.
        assert_eq!(ACCOUNT_SCOPE_ORGANIZATION, iam::ACCOUNT_SCOPE_ORGANIZATION);
        assert!(iam::KNOWN_CAPABILITY_CODES.contains(&DEPLOY_CAPABILITY_DNS));
    }

    #[test]
    fn the_vendor_codes_deploy_sends_are_ones_iam_offers() {
        for family in dns_provider::ALL {
            let vendor = dns_provider::vendor_code_for(family).expect("a vendor per family");
            assert!(
                iam::KNOWN_VENDOR_CODES.contains(&vendor),
                "`{vendor}` is not a vendor code the account center console offers"
            );
        }
    }

    #[test]
    fn the_scope_ladder_is_narrowest_first() {
        assert!(scope_rank(ACCOUNT_SCOPE_USER) < scope_rank(ACCOUNT_SCOPE_ORGANIZATION));
        assert!(scope_rank(ACCOUNT_SCOPE_ORGANIZATION) < scope_rank(ACCOUNT_SCOPE_TENANT));
        assert!(scope_rank(ACCOUNT_SCOPE_TENANT) < scope_rank(ACCOUNT_SCOPE_PLATFORM));
        // An unknown scope must sink to the bottom rather than tie with a real level;
        // a tie is what used to make `organization` lose to `tenant`.
        assert_eq!(
            scope_rank("a-scope-iam-has-not-taught-deploy-yet"),
            scope_rank(ACCOUNT_SCOPE_PLATFORM)
        );
    }

    #[test]
    fn a_projection_carries_what_the_console_renders() {
        let account = to_cloud_account(ProviderAccount {
            id: "acct-1".to_owned(),
            uuid: "uuid-1".to_owned(),
            tenant_id: "7".to_owned(),
            organization_id: "0".to_owned(),
            scope_type: ACCOUNT_SCOPE_USER.to_owned(),
            owner_user_id: Some("42".to_owned()),
            vendor_code: "cloudflare".to_owned(),
            account_code: "cloudflare-dns-1".to_owned(),
            display_name: "我的 Cloudflare".to_owned(),
            account_type: ACCOUNT_TYPE_LONG_TERM_KEY.to_owned(),
            environment: "production".to_owned(),
            external_account_id: None,
            capability_codes: vec![CAPABILITY_DNS.to_owned()],
            region_code: None,
            is_default: false,
            status: ACCOUNT_STATUS_ACTIVE.to_owned(),
            version: 1,
            created_by: "42".to_owned(),
            updated_by: "42".to_owned(),
            created_at: String::new(),
            updated_at: String::new(),
            credential_configured: true,
            credential_count: 1,
        });
        assert_eq!(account.dns_provider(), Some(dns_provider::CLOUDFLARE));
        assert_eq!(account.owner_user_id.as_deref(), Some("42"));
        assert!(
            !account.is_tenant_global(),
            "a personal account is not shared"
        );
        assert!(account.is_usable());
        assert_eq!(account.credential_count, 1);
    }

    #[test]
    fn account_center_errors_keep_their_kind() {
        use sdkwork_deploy_contract::DeployServiceErrorKind;
        assert_eq!(
            map_error(ProviderAccountError::Conflict("duplicate".to_owned())).kind(),
            DeployServiceErrorKind::Conflict
        );
        assert_eq!(
            map_error(ProviderAccountError::NotFound("missing".to_owned())).kind(),
            DeployServiceErrorKind::NotFound
        );
        assert_eq!(
            map_error(ProviderAccountError::Validation("bad".to_owned())).kind(),
            DeployServiceErrorKind::Validation
        );
        assert_eq!(
            map_error(ProviderAccountError::Unavailable("down".to_owned())).kind(),
            DeployServiceErrorKind::DatabaseUnavailable
        );
        let cipher = map_error(ProviderAccountError::Cipher("no master key".to_owned()));
        assert_eq!(cipher.kind(), DeployServiceErrorKind::Internal);
        assert!(cipher.to_string().contains("custody"));
    }
}
