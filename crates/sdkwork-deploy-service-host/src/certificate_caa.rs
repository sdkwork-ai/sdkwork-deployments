//! CAA resolution over the host's system DNS resolver.

use std::time::Duration;

use async_trait::async_trait;
use hickory_resolver::proto::rr::{RData, RecordType};
use hickory_resolver::{name_server::TokioConnectionProvider, TokioResolver};
use sdkwork_deploy_contract::DeployServiceResult;
use sdkwork_intelligence_deploy_service::{
    CaaRecord, CaaRrset, CertificateAuthorityAuthorizationPort,
};

/// Per-name lookup ceiling. The tree walk makes at most
/// `MAX_CAA_TREE_WALK_LABELS` lookups, so this bounds the whole check.
const CAA_LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Reads CAA RRsets through the operating system's resolver configuration.
pub struct HickoryCaaResolver {
    resolver: TokioResolver,
}

impl HickoryCaaResolver {
    pub fn from_system_config() -> Result<Self, String> {
        let resolver = TokioResolver::builder(TokioConnectionProvider::default())
            .map_err(|error| format!("load system DNS resolver configuration failed: {error}"))?
            .build();
        Ok(Self { resolver })
    }
}

#[async_trait]
impl CertificateAuthorityAuthorizationPort for HickoryCaaResolver {
    async fn lookup_caa_rrset(&self, name: &str) -> DeployServiceResult<CaaRrset> {
        // `Resolver::lookup` is the generic RecordType entry point; hickory 0.25
        // generates typed helpers for TXT/SRV/etc. but not for CAA.
        let lookup = match tokio::time::timeout(
            CAA_LOOKUP_TIMEOUT,
            self.resolver.lookup(name, RecordType::CAA),
        )
        .await
        {
            Ok(Ok(lookup)) => lookup,
            // A name with no CAA RRset is not a failure: RFC 8659 §3 continues
            // the walk at the parent.
            Ok(Err(error)) if error.is_nx_domain() || error.is_no_records_found() => {
                return Ok(CaaRrset::Absent)
            }
            // Any other resolver answer (SERVFAIL, refused, malformed) leaves the
            // policy unknown, which must not be read as "no policy".
            Ok(Err(_)) => return Ok(CaaRrset::Unavailable),
            Err(_) => return Ok(CaaRrset::Unavailable),
        };

        let records = lookup
            .iter()
            .filter_map(|rdata| match rdata {
                RData::CAA(caa) => Some(CaaRecord {
                    critical: caa.issuer_critical(),
                    // Tags are case-insensitive (RFC 8659 §4.1); the policy layer
                    // compares case-insensitively, so the wire form is kept.
                    tag: caa.tag().as_str().to_owned(),
                    // The raw value is re-parsed by the policy layer so that a
                    // malformed value can be reduced to an empty
                    // issuer-domain-name exactly as RFC 8659 §4.2 requires.
                    // hickory keeps the wire bytes even when it cannot
                    // interpret them.
                    value: String::from_utf8_lossy(caa.raw_value()).into_owned(),
                }),
                _ => None,
            })
            .collect::<Vec<_>>();

        if records.is_empty() {
            return Ok(CaaRrset::Absent);
        }
        Ok(CaaRrset::Present(records))
    }
}
