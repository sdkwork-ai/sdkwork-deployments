//! Syncing a Zone's DNS resolution records through its cloud account.
//!
//! The read half of the zone's DNS relationship: where
//! [`crate::certificate_dns_account`] publishes challenge TXT records, this
//! reads back the whole inventory the provider already holds — the A / AAAA /
//! CNAME answers a hostname page renders — and stores it as one replaceable
//! snapshot. The provider is never consulted on a page render; the sync is
//! the only provider gesture, and the snapshot instant travels with every row.

use std::sync::Arc;

use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult, DomainDnsSyncResponse};
use sdkwork_webserver_acme_service::{DnsRecordChange, DnsZoneRecord};

use crate::certificate_issuance::CertificateDns01Selector;
use crate::repository::{
    DeployRepositoryPort, DnsRecordSnapshotRow, DomainDnsRecordFilter, DomainDnsSnapshotWrite,
    DomainHostnameAsset,
};

/// Store-only page over the snapshot, optionally restricted to one hostname.
pub(crate) async fn list_zone_dns_records(
    repository: &Arc<dyn DeployRepositoryPort>,
    tenant_id: i64,
    zone_id: &str,
    filter: &DomainDnsRecordFilter,
    page: i32,
    page_size: i32,
) -> DeployServiceResult<sdkwork_deploy_contract::DomainDnsRecordPage> {
    repository
        .list_domain_zone_dns_records(tenant_id, zone_id, filter, page, page_size)
        .await
}

/// The account-id literal a snapshot carries when the deployment-level
/// configuration answered instead of an account-center row. It matches the
/// account-id shape rule so the column stays uniform; it is metadata about
/// *which credential* answered, never a reference to look up.
pub const DEPLOYMENT_CONFIG_ACCOUNT: &str = "deployment-config";

/// The owner match a snapshot's `hostnameId` stamp applies.
///
/// An exact hostname matches only its own owner. A wildcard hostname
/// (`*.base`) matches its literal star record, its base owner, and every
/// owner exactly one label beneath the base — the names a single-label DNS
/// wildcard actually answers for; a two-label descendant has its own name and
/// is not the wildcard's answer.
pub fn hostname_matches_record(hostname: &str, record_name: &str) -> bool {
    match hostname.strip_prefix("*.") {
        Some(base) => {
            record_name == hostname
                || record_name == base
                || record_name
                    .strip_suffix(&format!(".{base}"))
                    .is_some_and(|prefix| !prefix.is_empty() && !prefix.contains('.'))
        }
        None => record_name == hostname,
    }
}

/// The hostname a record resolves: the exact hostname row that owns the name
/// outright wins, and a wildcard row answers only what no exact row claims.
/// The order matters for a zone that registered both `api.example.com` and
/// `*.example.com` — the record at `api.example.com` is the exact row's
/// answer, whatever the wildcard would also cover.
pub(crate) fn matched_domain<'a>(
    assets: &'a [DomainHostnameAsset],
    record_name: &str,
) -> Option<i64> {
    assets
        .iter()
        .find(|asset| asset.hostname_ascii == record_name)
        .or_else(|| {
            assets.iter().find(|asset| {
                asset.hostname_type == "WILDCARD"
                    && asset.hostname_ascii != record_name
                    && hostname_matches_record(&asset.hostname_ascii, record_name)
            })
        })
        .map(|asset| asset.domain_id)
}

/// Reads the Zone's inventory through the same presenter chain issuance uses
/// and replaces the stored snapshot.
///
/// The account resolves exactly as a challenge does: the zone's pin, then the
/// account center's choice for the declared provider, then the
/// deployment-level configuration. Nothing about the decision is recorded
/// back onto the zone, so a re-registered account is picked up by the next
/// sync rather than being stuck behind a stored one.
pub(crate) async fn sync_zone_dns_records(
    repository: &Arc<dyn DeployRepositoryPort>,
    dns01: &Arc<dyn crate::certificate_issuance::CertificateDns01PresenterPort>,
    tenant_id: i64,
    owner_user_id: Option<i64>,
    zone_id: &str,
) -> DeployServiceResult<DomainDnsSyncResponse> {
    let target = repository
        .domain_zone_dns_sync_target(tenant_id, owner_user_id, zone_id)
        .await?
        .ok_or_else(|| DeployServiceError::NotFound("domain zone not found".to_string()))?;
    let resolved = dns01
        .resolve(CertificateDns01Selector {
            tenant_id,
            certificate_provider_account_id: target.provider_account_id.as_deref(),
            hostname: &target.apex_hostname,
        })
        .await?
        .ok_or_else(|| {
            DeployServiceError::validation(
                "no DNS provider is configured for this zone; bind a cloud account \
                 to it or configure the deployment-level DNS provider",
            )
        })?;
    let records: Vec<DnsZoneRecord> = resolved
        .presenter
        .list_zone_records(&resolved.zone_apex)
        .await
        .map_err(map_dns_sync_error)?;
    let provider = resolved
        .presenter
        .provider_kind()
        .map(|kind| kind.as_str().to_owned())
        .unwrap_or_else(|| "UNKNOWN".to_owned());
    // The account the resolution actually answered through — the zone's pin
    // when one is set, the account center's choice when it picked one, or the
    // deployment-config literal when the deployment-wide credential answered.
    let account_id = resolved
        .provider_account_id
        .clone()
        .unwrap_or_else(|| DEPLOYMENT_CONFIG_ACCOUNT.to_owned());

    let hostname_assets = repository
        .list_domain_hostname_assets(tenant_id, owner_user_id, zone_id)
        .await?;
    let snapshot_rows: Vec<DnsRecordSnapshotRow> = records
        .iter()
        .map(|record| DnsRecordSnapshotRow {
            record_name: record.record_name.clone(),
            record_type: record.record_type.clone(),
            record_value: record.record_value.clone(),
            ttl_seconds: record
                .ttl_seconds
                .and_then(|value| i32::try_from(value).ok()),
            priority: record.priority.and_then(|value| i32::try_from(value).ok()),
            record_line: record.record_line.clone(),
            provider_record_ref: record.provider_record_ref.clone(),
            domain_id: matched_domain(&hostname_assets, &record.record_name),
        })
        .collect();
    let synced_at =
        sdkwork_utils_rust::datetime::format_datetime(sdkwork_utils_rust::datetime::now(), None);
    let record_count = repository
        .replace_domain_zone_dns_records(
            tenant_id,
            zone_id,
            &DomainDnsSnapshotWrite {
                dns_provider: provider.clone(),
                provider_account_id: account_id.clone(),
                synced_at: synced_at.clone(),
                records: snapshot_rows,
            },
        )
        .await?;
    Ok(DomainDnsSyncResponse {
        record_count,
        synced_at,
        zone_apex: resolved.zone_apex.clone(),
        dns_provider: provider,
        provider_account_id: account_id,
    })
}

/// Validates and converts a create/edit request into the adapter's change
/// shape. Runs before the vendor is asked, so a malformed value is a
/// validation error naming the field; an owner that already carries the zone
/// suffix is refused here, where the apex is in hand — the doubled-name
/// mistake (\`www.example.com.<zone>\`) every provider console sees.
pub(crate) fn normalize_dns_record_change(
    request: &impl DnsRecordChangeFields,
    zone_apex: &str,
) -> DeployServiceResult<DnsRecordChange> {
    let owner = request.host().trim();
    let lowered = owner.to_ascii_lowercase();
    if lowered == zone_apex.to_ascii_lowercase()
        || lowered.ends_with(&format!(".{}", zone_apex.to_ascii_lowercase()))
    {
        return Err(DeployServiceError::validation(format!(
            "host must be the zone-relative 主机记录 (for example `@` or `www`), not the absolute name `{owner}` which would create `{owner}.{zone_apex}`"
        )));
    }
    let ttl_seconds = match request.ttl_seconds() {
        None => None,
        Some(value) => Some(u32::try_from(value).map_err(|_| {
            DeployServiceError::validation("ttlSeconds must be a positive number of seconds")
        })?),
    };
    let priority = match request.priority() {
        None => None,
        Some(value) => Some(u32::try_from(value).map_err(|_| {
            DeployServiceError::validation("priority must be a non-negative number")
        })?),
    };
    DnsRecordChange::new(
        request.record_type(),
        owner,
        request.record_value(),
        ttl_seconds,
        priority,
        request.record_line().as_deref(),
    )
    .map_err(|error| DeployServiceError::validation(error.to_string()))
}

/// The two field sets a create and an edit carry; the trait form lets
/// [`normalize_dns_record_change`] read either request without duplication.
trait DnsRecordChangeFields {
    fn record_type(&self) -> &str;
    fn host(&self) -> &str;
    fn record_value(&self) -> &str;
    fn ttl_seconds(&self) -> Option<i32>;
    fn priority(&self) -> Option<i32>;
    fn record_line(&self) -> &Option<String>;
}

impl DnsRecordChangeFields for sdkwork_deploy_contract::CreateDomainDnsRecordRequest {
    fn record_type(&self) -> &str {
        &self.record_type
    }
    fn host(&self) -> &str {
        &self.host
    }
    fn record_value(&self) -> &str {
        &self.record_value
    }
    fn ttl_seconds(&self) -> Option<i32> {
        self.ttl_seconds
    }
    fn priority(&self) -> Option<i32> {
        self.priority
    }
    fn record_line(&self) -> &Option<String> {
        &self.record_line
    }
}

impl DnsRecordChangeFields for sdkwork_deploy_contract::UpdateDomainDnsRecordRequest {
    fn record_type(&self) -> &str {
        &self.record_type
    }
    fn host(&self) -> &str {
        &self.host
    }
    fn record_value(&self) -> &str {
        &self.record_value
    }
    fn ttl_seconds(&self) -> Option<i32> {
        self.ttl_seconds
    }
    fn priority(&self) -> Option<i32> {
        self.priority
    }
    fn record_line(&self) -> &Option<String> {
        &self.record_line
    }
}

/// The shared write skeleton: resolve the zone, resolve the presenter through
/// the same chain issuance uses, hand the change to the vendor, and return the
/// vendor's answer next to the resolution context (provider + account id) the
/// snapshot row carries.
pub(crate) struct DnsRecordWriteContext {
    pub zone_id: String,
    pub zone_apex: String,
    pub provider: String,
    pub provider_account_id: String,
}

pub(crate) async fn resolve_zone_write_context(
    repository: &Arc<dyn DeployRepositoryPort>,
    dns01: &Arc<dyn crate::certificate_issuance::CertificateDns01PresenterPort>,
    tenant_id: i64,
    owner_user_id: Option<i64>,
    zone_id: &str,
) -> DeployServiceResult<(
    crate::certificate_issuance::CertificateDns01Context,
    DnsRecordWriteContext,
)> {
    let target = repository
        .domain_zone_dns_sync_target(tenant_id, owner_user_id, zone_id)
        .await?
        .ok_or_else(|| DeployServiceError::NotFound("domain zone not found".to_string()))?;
    let resolved = dns01
        .resolve(CertificateDns01Selector {
            tenant_id,
            certificate_provider_account_id: target.provider_account_id.as_deref(),
            hostname: &target.apex_hostname,
        })
        .await?
        .ok_or_else(|| {
            DeployServiceError::validation(
                "no DNS provider is configured for this zone; bind a cloud account                  to it or configure the deployment-level DNS provider",
            )
        })?;
    let provider = resolved
        .presenter
        .provider_kind()
        .map(|kind| kind.as_str().to_owned())
        .unwrap_or_else(|| "UNKNOWN".to_owned());
    let account_id = resolved
        .provider_account_id
        .clone()
        .unwrap_or_else(|| DEPLOYMENT_CONFIG_ACCOUNT.to_owned());
    let zone_apex = resolved.zone_apex.clone();
    Ok((
        resolved,
        DnsRecordWriteContext {
            zone_id: zone_id.to_owned(),
            zone_apex,
            provider,
            provider_account_id: account_id,
        },
    ))
}

/// Maps an inventory read's failure to the error an operator can act on.
///
/// A configuration mistake (no account on the deployment, an account that
/// cannot see the zone) is fixed by configuration, so it surfaces as a
/// validation error naming the fix; a provider refusal or a vendor outage
/// stays an internal failure carrying the provider's own words.
pub(crate) fn map_dns_sync_error(
    error: sdkwork_webserver_acme_service::AcmeServiceError,
) -> DeployServiceError {
    match &error {
        sdkwork_webserver_acme_service::AcmeServiceError::Config(detail) => {
            DeployServiceError::validation(format!("dns record sync refused: {detail}"))
        }
        other => DeployServiceError::Internal(format!("dns record sync failed: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{hostname_matches_record, matched_domain};
    use crate::repository::DomainHostnameAsset;

    #[test]
    fn an_exact_hostname_matches_only_its_own_owner() {
        assert!(hostname_matches_record(
            "www.example.com",
            "www.example.com"
        ));
        assert!(!hostname_matches_record("www.example.com", "example.com"));
        assert!(!hostname_matches_record(
            "www.example.com",
            "api.www.example.com"
        ));
    }

    #[test]
    fn a_wildcard_hostname_matches_its_star_record_base_and_one_label_beneath() {
        // The literal star record a provider stores for the wildcard owner.
        assert!(hostname_matches_record(
            "*.dev.example.com",
            "*.dev.example.com"
        ));
        assert!(hostname_matches_record(
            "*.dev.example.com",
            "dev.example.com"
        ));
        assert!(hostname_matches_record(
            "*.dev.example.com",
            "api.dev.example.com"
        ));
        // Two labels beneath the base is a name of its own, not the
        // single-label wildcard's answer.
        assert!(!hostname_matches_record(
            "*.dev.example.com",
            "api.staging.dev.example.com"
        ));
        assert!(!hostname_matches_record(
            "*.dev.example.com",
            "www.example.com"
        ));
    }

    fn asset(id: i64, hostname: &str, hostname_type: &str) -> DomainHostnameAsset {
        DomainHostnameAsset {
            domain_id: id,
            hostname_ascii: hostname.to_owned(),
            hostname_type: hostname_type.to_owned(),
        }
    }

    #[test]
    fn an_exact_row_wins_over_the_wildcard_that_also_covers_it() {
        let assets = vec![
            asset(1, "*.example.com", "WILDCARD"),
            asset(2, "api.example.com", "EXACT"),
        ];
        assert_eq!(matched_domain(&assets, "api.example.com"), Some(2));
        // The names only the wildcard answers still stamp with the wildcard.
        assert_eq!(matched_domain(&assets, "www.example.com"), Some(1));
        assert_eq!(matched_domain(&assets, "example.com"), Some(1));
        // A name under the zone nobody registered matches nothing.
        assert_eq!(
            matched_domain(&assets, "a.b.example.com"),
            None,
            "a two-label descendant is not the single-label wildcard's answer"
        );
    }
}
