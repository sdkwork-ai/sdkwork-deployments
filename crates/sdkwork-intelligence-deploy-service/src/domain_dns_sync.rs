//! ⚠️ UNLANDED: no `mod` declaration names this file inside the crate, so rustc
//! never compiles it. It arrived with the `wip/deploy-certificate` work as the
//! specification for a later landing — wire it up by declaring the module, not
//! by deleting the file.
//!
//! Keeping a root domain's provider-side records in step with the hostnames
//! this control plane manages.
//!
//! A zone bound to a cloud account is an operator's statement that this
//! deployment may write that zone's records. Everything in this module exists
//! to honour one half of that statement: **what this deployment published, it
//! must also be able to take back**.
//!
//! # The record this module owns
//!
//! Only one record per hostname is derived from the platform's own data: the
//! ownership proof `_sdkwork-verification.<host>` whose value is
//! [`crate::dns_txt_record_value`] of the hostname's live verification attempt.
//! A subdomain's *service* record is deliberately not written here — a bare
//! hostname has no target until an application binding names one, and a
//! deployment that picked one for the operator would be inventing a route. The
//! service record belongs to the binding flow, which knows the app.
//!
//! # Why a ledger, and why in `deploy_domain.metadata`
//!
//! Withdrawing needs more than the record name. Cloudflare addresses a record
//! by the provider's own id, which only the publish response carries, and a
//! rename *invalidates the attempt* (`update_domain_hostname` resets
//! verification to `PENDING` and expires the active challenge), so the value
//! cannot be re-derived from the attempt row afterwards either. The publish
//! response is therefore the only place that knowledge ever exists, and it is
//! recorded at the moment it is obtained.
//!
//! It lives under a key in `deploy_domain.metadata` rather than in a column of
//! its own because the baseline DDL is applied to existing databases by
//! migration, and `metadata` is already the row's extension point. The block is
//! additive: a row that never had a record has no key, and clearing the record
//! removes the key rather than writing a null shape.
//!
//! # Failure policy
//!
//! Every provider failure here is **logged and swallowed**. The record name and
//! value are still shown to the operator by the verification response, so a
//! domain must not become unverifiable because a provider is down or a key was
//! revoked — the same rule `PLAN-2026-0003` §8.3 states for ACME DNS-01, where
//! an adapter failure downgrades to manual presentation instead of failing the
//! request. A zone with no account resolves no presenter at all, which is not a
//! failure: the manual path is the pre-existing behaviour and stays byte for
//! byte the same.

use std::sync::Arc;

use sdkwork_deploy_contract::OwnershipReach;
use sdkwork_webserver_acme_service::{
    AcmeServiceError, Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, DnsAccountVerification,
};
use serde_json::{Map, Value};

use crate::{CertificateDns01Selector, DeployService};

/// Key under `deploy_domain.metadata` holding the record this deployment
/// published for the hostname.
pub const DOMAIN_DNS_RECORD_METADATA_KEY: &str = "dnsOwnershipRecord";

/// The provider-side record one hostname currently has, as this deployment
/// published it.
///
/// Absent means "this deployment published nothing for this hostname", which is
/// the state of every hostname in a zone with no cloud account and of every
/// hostname that was never verified. It is not the same as "the provider holds
/// no record": an operator may have written one by hand, which is exactly what
/// the manual path asks them to do and which this ledger deliberately does not
/// claim to know about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainDnsRecord {
    /// The zone apex the record was published under, as the provider was told.
    /// Kept because withdrawal must name the same zone the publish used, even
    /// if the operator has since re-pointed the zone at another provider.
    pub zone_apex: String,
    /// Absolute owner name, for example `_sdkwork-verification.www.example.com`.
    pub record_name: String,
    /// The TXT value published, `base64url(sha256(attempt id))`.
    pub record_value: String,
    /// Provider-assigned record id, when the provider returned one. Cloudflare
    /// addresses deletion by this and nothing else.
    pub provider_record_ref: Option<String>,
}

impl DomainDnsRecord {
    /// Reads the ledger out of a `deploy_domain.metadata` document.
    ///
    /// Returns `None` for an absent key, a non-object block, or a block missing
    /// a field withdrawal could not proceed without. A partial ledger is
    /// treated as no ledger on purpose: keeping the unusable half would make
    /// every later cleanup attempt fail on a record nothing can name.
    pub fn from_metadata(metadata: &Value) -> Option<Self> {
        let block = metadata.get(DOMAIN_DNS_RECORD_METADATA_KEY)?.as_object()?;
        Some(Self {
            zone_apex: non_empty_string(block, "zoneApex")?,
            record_name: non_empty_string(block, "recordName")?,
            record_value: non_empty_string(block, "recordValue")?,
            provider_record_ref: block
                .get("providerRecordRef")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        })
    }

    /// The block to store under [`DOMAIN_DNS_RECORD_METADATA_KEY`].
    pub fn to_metadata_block(&self) -> Value {
        let mut block = Map::new();
        block.insert("zoneApex".to_owned(), Value::from(self.zone_apex.clone()));
        block.insert(
            "recordName".to_owned(),
            Value::from(self.record_name.clone()),
        );
        block.insert(
            "recordValue".to_owned(),
            Value::from(self.record_value.clone()),
        );
        if let Some(reference) = &self.provider_record_ref {
            block.insert(
                "providerRecordRef".to_owned(),
                Value::from(reference.clone()),
            );
        }
        Value::Object(block)
    }

    /// The handle a presenter withdraws with.
    ///
    /// The zone apex travels in the ledger rather than being re-resolved:
    /// withdrawal has to name the zone the record actually went into, and a
    /// zone re-pointed at another provider since then would otherwise be asked
    /// to remove a record it never held.
    pub fn withdrawal_handle(&self) -> Dns01RecordHandle {
        Dns01RecordHandle {
            zone_apex: self.zone_apex.clone(),
            record_name: self.record_name.clone(),
            record_value: self.record_value.clone(),
            provider_record_ref: self.provider_record_ref.clone(),
        }
    }
}

fn non_empty_string(block: &Map<String, Value>, field: &str) -> Option<String> {
    block
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// A presenter for one zone, resolved through the hostname that lives in it.
///
/// The concrete provider adapters are the only thing that ever speaks to a
/// provider; this type exists so the resolution — zone, then account, then
/// credential, then adapter — happens once and its result can be carried across
/// a rename or a delete, where the row that identified the zone is gone by the
/// time the record has to be withdrawn.
pub struct ZoneDnsPresenter {
    pub presenter: Arc<dyn Dns01Presenter>,
    pub zone_apex: String,
}

impl std::fmt::Debug for ZoneDnsPresenter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ZoneDnsPresenter")
            .field("zone_apex", &self.zone_apex)
            .field("provider", &self.presenter.provider_kind())
            .finish()
    }
}

/// A record this deployment published, together with the presenter that can
/// take it back.
pub struct PublishedOwnershipRecord {
    pub zone: ZoneDnsPresenter,
    pub record: DomainDnsRecord,
}

/// What a zone's DNS account established when asked, read-only, whether the
/// zone really is registered under it.
///
/// The states mirror [`DnsAccountVerification`]'s contract: an answer
/// (`checked`), a family that cannot answer (unchecked, no reason — there is
/// nothing the operator could fix), and a provider refusal that proves the
/// account cannot present for this zone (checked, with the provider's own
/// sentence as `refusal`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ZoneDnsProviderProbe {
    pub checked: bool,
    pub verified: bool,
    pub refusal: Option<String>,
}

impl std::fmt::Debug for PublishedOwnershipRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PublishedOwnershipRecord")
            .field("zone", &self.zone)
            .field("record", &self.record)
            .finish()
    }
}

impl DeployService {
    /// Resolves the DNS presenter that serves `hostname`'s zone, if the zone is
    /// bound to a usable cloud account.
    ///
    /// `None` covers three cases that are all "write nothing, use the manual
    /// path": the hostname's zone is unknown, the zone names no account and the
    /// account center offers none, or the account's family cannot present
    /// records. The certificate pin is deliberately not consulted — a pin is a
    /// *certificate's* statement about which account may prove its names, and a
    /// bare hostname ownership claim has no certificate behind it.
    pub(crate) async fn resolve_zone_dns_presenter(
        &self,
        tenant_id: i64,
        hostname: &str,
    ) -> Option<ZoneDnsPresenter> {
        match self
            .certificate_dns01
            .resolve(CertificateDns01Selector {
                tenant_id,
                certificate_provider_account_id: None,
                hostname,
            })
            .await
        {
            Ok(Some(context)) => Some(ZoneDnsPresenter {
                presenter: context.presenter,
                zone_apex: context.zone_apex,
            }),
            Ok(None) => {
                tracing::debug!(
                    hostname,
                    "no DNS provider account covers this hostname; records for it are published by hand"
                );
                None
            }
            Err(error) => {
                tracing::warn!(
                    hostname,
                    error = %error,
                    "resolving a DNS provider for a hostname failed; its records must be published by hand"
                );
                None
            }
        }
    }

    /// Asks the zone's DNS account, read-only, whether the zone really is
    /// registered under it.
    ///
    /// This is the half of a zone-ownership pass that a TXT lookup cannot
    /// answer: a record in DNS proves control of the name, not who hosts the
    /// zone. It never gates the TXT path — a provider outage or an
    /// unprobeable family degrades the *answer*, not the verification — and
    /// every outcome here is reported, none raised.
    pub(crate) async fn probe_zone_dns_provider(
        &self,
        tenant_id: i64,
        apex_hostname: &str,
    ) -> ZoneDnsProviderProbe {
        let Some(zone) = self
            .resolve_zone_dns_presenter(tenant_id, apex_hostname)
            .await
        else {
            // No account resolves: nothing was probed, and the manual path is
            // the honest answer rather than a failure.
            return ZoneDnsProviderProbe::default();
        };
        match zone.presenter.verify_account(&zone.zone_apex).await {
            Ok(DnsAccountVerification::Verified) => ZoneDnsProviderProbe {
                checked: true,
                verified: true,
                refusal: None,
            },
            Ok(DnsAccountVerification::Unsupported(_)) => {
                // The family has no read-only call that can prove an account.
                // Nothing failed, so there is no refusal to surface: the first
                // real write is the test, and the TXT path below runs it.
                ZoneDnsProviderProbe::default()
            }
            Err(error @ AcmeServiceError::Config(_)) => {
                // A config refusal proves the account cannot present for this
                // zone: a credential the provider does not recognise, or a zone
                // outside the account — the "域名没有注册在这个服务商" case. The
                // provider's own sentence is what an operator needs to fix the
                // account, so it travels verbatim.
                tracing::warn!(
                    zone_apex = %zone.zone_apex,
                    error = %error,
                    "the zone's DNS account cannot present for this zone; the domain \
                     may not be registered under this account's provider"
                );
                ZoneDnsProviderProbe {
                    checked: true,
                    verified: false,
                    refusal: Some(error.to_string()),
                }
            }
            Err(error) => {
                // Everything else is transient or unread: the probe says
                // nothing either way, and the TXT path is unaffected.
                tracing::warn!(
                    zone_apex = %zone.zone_apex,
                    error = %error,
                    "probing the zone's DNS account failed; treating the account as unchecked"
                );
                ZoneDnsProviderProbe::default()
            }
        }
    }

    /// Publishes the ownership record for a zone's apex hostname, best effort.
    ///
    /// The zone-entry twin of [`Self::publish_hostname_ownership_record`]: the
    /// apex row is resolved through the zone itself, which lets creating a zone
    /// with a cloud account and pinning an account on an existing zone share one
    /// entry point. Every failure is logged and swallowed — the manual record
    /// instructions remain the fallback — because a provider outage must not
    /// fail the create or edit that triggered it. An apex that is already
    /// verified publishes nothing, for there is no proof left to write.
    pub(crate) async fn publish_entered_zone_apex_record(
        &self,
        tenant_id: i64,
        owner: OwnershipReach,
        zone_id: &str,
    ) {
        let apex = match self
            .repository
            .zone_apex_hostname(tenant_id, owner, zone_id)
            .await
        {
            Ok(apex) => apex,
            Err(error) => {
                tracing::warn!(
                    zone = zone_id,
                    error = %error,
                    "reading the zone's apex hostname failed; its ownership record \
                     must be published by hand"
                );
                return;
            }
        };
        let Some(apex) = apex else {
            return;
        };
        self.publish_hostname_ownership_record(
            tenant_id,
            owner,
            zone_id,
            &apex.hostname_id,
            &apex.hostname,
        )
        .await;
    }

    /// Publishes the ownership record for one hostname **and records what was
    /// published**, so a later rename or delete can take it back.
    ///
    /// Opening the attempt and publishing are one operation because the value is
    /// derived from the attempt: `domain_hostname_verification_challenge`
    /// re-derives the token for a live attempt, so a second call for an attempt
    /// this deployment already opened republishes the identical value rather
    /// than a new one — which is what makes create-then-verify idempotent at the
    /// provider instead of leaving two TXT values behind.
    ///
    /// Returns whether the provider accepted the record. Every failure is
    /// logged and reported as `false`; nothing here fails the caller's request.
    pub(crate) async fn publish_hostname_ownership_record(
        &self,
        tenant_id: i64,
        owner: OwnershipReach,
        zone_id: &str,
        hostname_id: &str,
        hostname: &str,
    ) -> bool {
        // Resolved before the attempt is opened: a zone with no account must not
        // gain a verification attempt (and therefore an expiring row nobody asked
        // for) just because the operator created a hostname in it.
        let Some(zone) = self.resolve_zone_dns_presenter(tenant_id, hostname).await else {
            return false;
        };
        let challenge = match self
            .repository
            .domain_hostname_verification_challenge(tenant_id, owner, zone_id, hostname_id)
            .await
        {
            Ok(challenge) => challenge,
            Err(error) => {
                tracing::warn!(
                    hostname,
                    error = %error,
                    "reading the domain verification attempt failed; the ownership record must be published by hand"
                );
                return false;
            }
        };
        if challenge.verified {
            // Nothing is presented for a proven name: there is no record left to
            // publish, and writing one would only have to be cleaned up again.
            return false;
        }
        let (Some(record_name), Some(token)) =
            (challenge.record_name.as_deref(), challenge.token.as_deref())
        else {
            tracing::warn!(
                hostname,
                "the verification attempt carries no record to publish"
            );
            return false;
        };
        let request = match Dns01RecordRequest::new(zone.zone_apex.clone(), record_name, token) {
            Ok(request) => request,
            Err(error) => {
                // The record name is derived from the hostname and the zone is the
                // provider's own apex, so a refusal here means the two disagree —
                // publishing into the wrong zone would never resolve, and guessing
                // would hide the misconfiguration.
                tracing::warn!(
                    hostname,
                    record = record_name,
                    zone_apex = %zone.zone_apex,
                    error = %error,
                    "the ownership record does not belong to the resolved provider zone"
                );
                return false;
            }
        };
        let handle = match zone.presenter.publish(&request).await {
            Ok(handle) => handle,
            Err(error) => {
                tracing::warn!(
                    hostname,
                    record = record_name,
                    error = %error,
                    "the DNS provider refused the ownership record; it must be published by hand"
                );
                return false;
            }
        };
        let record = DomainDnsRecord {
            zone_apex: handle.zone_apex.clone(),
            record_name: handle.record_name.clone(),
            record_value: handle.record_value.clone(),
            provider_record_ref: handle.provider_record_ref.clone(),
        };
        // Stored only after the provider accepted it: the ledger is a statement
        // that a record exists out there, and a row claiming one the provider
        // never took would send every later withdrawal after a record that was
        // never created.
        if let Err(error) = self
            .repository
            .set_domain_hostname_dns_record(tenant_id, owner, zone_id, hostname_id, Some(&record))
            .await
        {
            tracing::warn!(
                hostname,
                record = record_name,
                error = %error,
                "the ownership record was published but its reference could not be stored; \
                 it will not be withdrawn when this hostname is renamed or deleted"
            );
        }
        tracing::info!(
            hostname,
            record = record_name,
            zone_apex = %record.zone_apex,
            "published the domain ownership record through the zone's DNS account"
        );
        true
    }

    /// Loads the record this deployment published for a hostname, together with
    /// the presenter that can withdraw it.
    ///
    /// Called **before** the row is renamed or deleted, because both operations
    /// destroy the row that resolves the presenter: `retrieve_dns_challenge_zone`
    /// reads the zone through the hostname's own `deploy_domain` row. Taking the
    /// presenter up front is what lets the withdrawal still name the right zone
    /// and credential afterwards.
    pub(crate) async fn load_published_ownership_record(
        &self,
        tenant_id: i64,
        owner: OwnershipReach,
        zone_id: &str,
        hostname_id: &str,
        hostname: &str,
    ) -> Option<PublishedOwnershipRecord> {
        let record = match self
            .repository
            .domain_hostname_dns_record(tenant_id, owner, zone_id, hostname_id)
            .await
        {
            Ok(Some(record)) => record,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(
                    hostname,
                    error = %error,
                    "reading the published ownership record failed; it will not be withdrawn"
                );
                return None;
            }
        };
        // A ledger whose zone no longer resolves leaves the record with the
        // provider rather than deleting it through some other zone's credential:
        // removing it with the wrong account is not possible, and pretending to
        // have cleaned up would be worse than reporting that it was not.
        let Some(zone) = self.resolve_zone_dns_presenter(tenant_id, hostname).await else {
            tracing::warn!(
                hostname,
                record = %record.record_name,
                "the zone's DNS account no longer resolves; the published ownership record is left in place"
            );
            return None;
        };
        Some(PublishedOwnershipRecord { zone, record })
    }

    /// Withdraws one published ownership record.
    ///
    /// Returns whether the provider accepted the removal. Adapters are required
    /// to tolerate a record that is already gone, so a `true` here means "the
    /// provider does not hold it any more", which is the only invariant a
    /// caller cares about.
    pub(crate) async fn withdraw_hostname_ownership_record(
        &self,
        published: &PublishedOwnershipRecord,
        hostname: &str,
    ) -> bool {
        match published
            .zone
            .presenter
            .withdraw(&published.record.withdrawal_handle())
            .await
        {
            Ok(()) => {
                tracing::info!(
                    hostname,
                    record = %published.record.record_name,
                    zone_apex = %published.record.zone_apex,
                    "withdrew the domain ownership record through the zone's DNS account"
                );
                true
            }
            Err(error) => {
                tracing::warn!(
                    hostname,
                    record = %published.record.record_name,
                    error = %error,
                    "the DNS provider refused to withdraw the ownership record; \
                     it stays behind and must be removed by hand"
                );
                false
            }
        }
    }

    /// Moves the published record for a renamed hostname: withdraw the old
    /// name's record, then publish the new name's.
    ///
    /// The publish runs after the withdrawal and is **not** conditional on it. A
    /// hold that could not be withdrawn leaves one stale record under a name
    /// this deployment no longer answers for; skipping the publish would leave
    /// the new, live hostname unverifiable, which is the worse of the two.
    pub(crate) async fn resync_renamed_hostname_ownership_record(
        &self,
        tenant_id: i64,
        owner: OwnershipReach,
        zone_id: &str,
        hostname_id: &str,
        previous: Option<PublishedOwnershipRecord>,
        new_hostname: &str,
    ) {
        if let Some(previous) = previous {
            self.withdraw_hostname_ownership_record(&previous, new_hostname)
                .await;
        }
        self.publish_hostname_ownership_record(
            tenant_id,
            owner,
            zone_id,
            hostname_id,
            new_hostname,
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_only_a_complete_record_back() {
        let block = json!({
            DOMAIN_DNS_RECORD_METADATA_KEY: {
                "zoneApex": "example.com",
                "recordName": "_sdkwork-verification.www.example.com",
                "recordValue": "token-abc",
                "providerRecordRef": "ref-1"
            }
        });
        let record = DomainDnsRecord::from_metadata(&block).expect("complete block");
        assert_eq!(record.zone_apex, "example.com");
        assert_eq!(record.record_name, "_sdkwork-verification.www.example.com");
        assert_eq!(record.record_value, "token-abc");
        assert_eq!(record.provider_record_ref.as_deref(), Some("ref-1"));

        // A record with no provider-assigned id is still withdrawable: two of the
        // three families address deletion by name and value.
        let no_ref = json!({
            DOMAIN_DNS_RECORD_METADATA_KEY: {
                "zoneApex": "example.com",
                "recordName": "_sdkwork-verification.www.example.com",
                "recordValue": "token-abc"
            }
        });
        let record = DomainDnsRecord::from_metadata(&no_ref).expect("no provider ref");
        assert_eq!(record.provider_record_ref, None);
    }

    /// A half-written block is treated as no block. Keeping the usable half would
    /// make every later cleanup fail on a record nothing can name, and the
    /// failure would surface at delete time — the one moment the operator cannot
    /// fix it.
    #[test]
    fn refuses_a_block_it_cannot_withdraw_with() {
        for block in [
            json!({ "dnsOwnershipRecord": {} }),
            json!({ "dnsOwnershipRecord": { "zoneApex": "example.com", "recordName": "_x.example.com" } }),
            json!({ "dnsOwnershipRecord": { "zoneApex": "example.com", "recordValue": "v" } }),
            json!({ "dnsOwnershipRecord": { "zoneApex": "  ", "recordName": "_x", "recordValue": "v" } }),
            json!({ "dnsOwnershipRecord": "not-an-object" }),
            json!({ "dnsOwnershipRecord": [] }),
        ] {
            assert_eq!(
                DomainDnsRecord::from_metadata(&block),
                None,
                "must not read a partial ledger: {block}"
            );
        }
        // Absent key and an unrelated metadata document are both simply "none".
        assert_eq!(DomainDnsRecord::from_metadata(&json!({})), None);
        assert_eq!(DomainDnsRecord::from_metadata(&json!({ "other": 1 })), None);
    }

    #[test]
    fn round_trips_through_the_metadata_block() {
        let record = DomainDnsRecord {
            zone_apex: "example.com".to_owned(),
            record_name: "_sdkwork-verification.api.example.com".to_owned(),
            record_value: "token-xyz".to_owned(),
            provider_record_ref: Some("cf-42".to_owned()),
        };
        let document = json!({ DOMAIN_DNS_RECORD_METADATA_KEY: record.to_metadata_block() });
        assert_eq!(DomainDnsRecord::from_metadata(&document), Some(record));
    }

    /// The handle has to carry the zone the record was published under, not one
    /// re-resolved at withdrawal time: a zone re-pointed at another provider
    /// would otherwise be asked to remove a record it never held.
    #[test]
    fn withdrawal_handle_names_the_zone_the_record_went_into() {
        let record = DomainDnsRecord {
            zone_apex: "published-under.example".to_owned(),
            record_name: "_sdkwork-verification.www.published-under.example".to_owned(),
            record_value: "token".to_owned(),
            provider_record_ref: None,
        };
        let handle = record.withdrawal_handle();
        assert_eq!(handle.zone_apex, "published-under.example");
        assert_eq!(
            handle.record_name,
            "_sdkwork-verification.www.published-under.example"
        );
        assert_eq!(handle.record_value, "token");
        assert_eq!(handle.provider_record_ref, None);
    }
}
