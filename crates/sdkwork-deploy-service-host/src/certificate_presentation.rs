//! TXT visibility check for manual DNS-01 presentations, over system DNS.
//!
//! Manual issuance only works if the platform can tell "the operator published it"
//! from "the operator thinks they published it". Getting that wrong spends one of
//! the CA's failed-validation allowances (Let's Encrypt: five per account per
//! hostname per hour), so the release command asks this adapter before recording
//! anything.

use std::time::Duration;

use async_trait::async_trait;
use hickory_resolver::proto::rr::RecordType;
use hickory_resolver::{name_server::TokioConnectionProvider, TokioResolver};
use sdkwork_deploy_contract::DeployServiceResult;
use sdkwork_intelligence_deploy_service::certificate_presentation::{
    dns_txt_matches, CertificatePresentationInspectorPort, PresentationReport,
    PresentationResolverReport,
};

/// One lookup ceiling. A manual release is a user-facing command, so it must fail
/// inside a request timeout rather than hang a worker.
const PRESENTATION_LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Reads TXT records through the operating system's resolver configuration.
pub struct HickoryPresentationInspector {
    resolver: TokioResolver,
}

impl HickoryPresentationInspector {
    pub fn from_system_config() -> Result<Self, String> {
        let resolver = TokioResolver::builder(TokioConnectionProvider::default())
            .map_err(|error| format!("load system DNS resolver configuration failed: {error}"))?
            .build();
        Ok(Self { resolver })
    }
}

#[async_trait]
impl CertificatePresentationInspectorPort for HickoryPresentationInspector {
    async fn inspect_dns_txt(
        &self,
        record_name: &str,
        expected: &[String],
    ) -> DeployServiceResult<PresentationReport> {
        let lookup = tokio::time::timeout(
            PRESENTATION_LOOKUP_TIMEOUT,
            self.resolver.lookup(record_name, RecordType::TXT),
        )
        .await;
        let mut observed = Vec::new();
        let mut error = None;
        match lookup {
            Err(_) => error = Some("DNS TXT lookup timed out".to_owned()),
            Ok(Err(lookup_error)) => {
                error = Some(format!("DNS TXT lookup failed: {lookup_error}"))
            }
            Ok(Ok(records)) => {
                for record in records.iter() {
                    if let hickory_resolver::proto::rr::RData::TXT(txt) = record {
                        // RFC 1035 character-strings are concatenated by the CA, so
                        // the comparison must see the joined value.
                        let joined = txt
                            .txt_data()
                            .iter()
                            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
                            .collect::<String>();
                        observed.push(joined);
                    }
                }
            }
        }
        let visible = error.is_none() && dns_txt_matches(&observed, expected);
        if error.is_some() {
            // Keep the error on the report: the caller distinguishes "checked and
            // absent" (block) from "could not check" (let the operator proceed).
            tracing::warn!(record_name, error = %error.as_deref().unwrap_or(""), "manual DNS-01 presentation lookup failed");
        }
        Ok(PresentationReport {
            visible,
            reports: vec![PresentationResolverReport {
                resolver: "system".to_owned(),
                observed,
                error,
            }],
        })
    }
}
