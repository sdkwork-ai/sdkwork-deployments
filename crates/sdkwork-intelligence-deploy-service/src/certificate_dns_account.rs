//! Resolving a DNS-01 presenter through the cloud account center.
//!
//! The chain, in order:
//!
//! 1. the account pinned on the certificate,
//! 2. the account pinned on the zone that owns the hostname,
//! 3. the account the center would choose for that zone's declared provider,
//! 4. the deployment-level provider configuration (the fallback),
//!
//! Steps 1 and 2 are statements an operator made on purpose; step 3 is a
//! convenience that exists because a tenant which registered an Aliyun key should
//! not have to also pin it on every zone; step 4 is what a deployment that owns one
//! DNS zone and never used the account center keeps working with.
//!
//! Nothing here writes anything. A resolution that falls through to step 3 does not
//! record the account it chose, because the next renewal must be free to choose
//! again — an account that gets re-registered, disabled, or replaced is exactly the
//! case a stored derived pin would break.

use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_deploy_cloud_account_port::DeployCloudAccountPort;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};

use crate::certificate_dns::DeployDnsProviderCredential;
use crate::certificate_issuance::{
    CertificateDns01Context, CertificateDns01PresenterPort, CertificateDns01Selector,
};
use crate::repository::{DeployRepositoryPort, DnsChallengeZone};

/// Resolves a presenter from the account center, then from the fallback.
pub struct AccountBackedDns01PresenterResolver {
    repository: Arc<dyn DeployRepositoryPort>,
    accounts: Arc<dyn DeployCloudAccountPort>,
    /// The deployment-level configuration, tried last because it knows nothing
    /// about tenants: it is the right answer for an installation that owns one zone,
    /// and the wrong one to prefer over an account an operator deliberately chose.
    fallback: Arc<dyn CertificateDns01PresenterPort>,
}

impl AccountBackedDns01PresenterResolver {
    pub fn new(
        repository: Arc<dyn DeployRepositoryPort>,
        accounts: Arc<dyn DeployCloudAccountPort>,
        fallback: Arc<dyn CertificateDns01PresenterPort>,
    ) -> Self {
        Self {
            repository,
            accounts,
            fallback,
        }
    }

    /// The account to present with, and where the decision came from.
    ///
    /// Returning the id alongside the answer is what makes the log line useful: "no
    /// account was found" is only actionable once it is clear whether a pin was
    /// consulted at all.
    async fn select_account(
        &self,
        selector: &CertificateDns01Selector<'_>,
        zone: &DnsChallengeZone,
    ) -> DeployServiceResult<Option<String>> {
        if let Some(pinned) = selector.certificate_provider_account_id {
            return Ok(Some(pinned.to_owned()));
        }
        if let Some(pinned) = zone.provider_account_id.as_deref() {
            return Ok(Some(pinned.to_owned()));
        }
        // A zone that never declared a provider leaves nothing to look up: asking the
        // center for "the account for `None`" would return whatever the tenant
        // happens to have, and binding an Aliyun key to a zone nobody described is
        // worse than falling through to the configuration that does know.
        let Some(declared) = zone
            .dns_provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        // A declared provider this build cannot drive is treated as "no account",
        // not as a failure. The zone's declaration is then irrelevant to the
        // fallback, which used to be — and still is — the only way such a zone could
        // ever be presented.
        match self
            .accounts
            .find_existing_account(selector.tenant_id, None, declared, None)
            .await
        {
            Ok(account) => Ok(account.map(|account| account.id)),
            Err(DeployServiceError::Validation(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[async_trait]
impl CertificateDns01PresenterPort for AccountBackedDns01PresenterResolver {
    async fn resolve(
        &self,
        selector: CertificateDns01Selector<'_>,
    ) -> DeployServiceResult<Option<CertificateDns01Context>> {
        let Some(zone) = self
            .repository
            .retrieve_dns_challenge_zone(selector.tenant_id, selector.hostname)
            .await?
        else {
            // A hostname this tenant keeps no zone row for cannot be presented by an
            // account: the apex the record has to go under is not knowable from the
            // account center. Only the deployment-level configuration knows one.
            return self.fallback.resolve(selector).await;
        };

        let Some(account_id) = self.select_account(&selector, &zone).await? else {
            return self.fallback.resolve(selector).await;
        };

        let credential = self
            .accounts
            .resolve_dns_credential(
                selector.tenant_id,
                &account_id,
                zone.dns_provider.as_deref(),
            )
            .await?;
        let credential = DeployDnsProviderCredential::from_account_credential(
            &credential.dns_provider,
            &credential.access_key_id,
            &credential.secret_access_key,
        )?;
        let presenter = crate::certificate_dns::build_dns01_presenter(
            &credential,
            zone.provider_zone_ref.as_deref(),
        )
        .map_err(|error| {
            DeployServiceError::Internal(format!("DNS provider presenter failed: {error}"))
        })?;
        // The account's own provider wins over the zone's declaration: the account is
        // where the secret actually lives, and a zone whose declaration disagrees is
        // a misconfiguration the operator can see in the response rather than a
        // silent reason to ignore the credential they just bound.
        tracing::info!(
            hostname = selector.hostname,
            zone_apex = %zone.zone_apex,
            provider = credential.kind().as_str(),
            account = account_id,
            "resolved DNS-01 presenter from the cloud account center"
        );
        Ok(Some(CertificateDns01Context {
            presenter,
            zone_apex: zone.zone_apex.clone(),
        }))
    }
}
