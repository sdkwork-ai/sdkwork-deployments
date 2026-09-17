//! Domain ownership normalization and verification port.

use std::{net::IpAddr, str::FromStr};

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult, DomainVerifyResponse};

pub const DOMAIN_VERIFICATION_METHOD_DNS_TXT: &str = "DNS_TXT";
const DOMAIN_VERIFICATION_RECORD_LABEL: &str = "_sdkwork-verification";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainVerificationChallenge {
    pub verification_id: Option<String>,
    pub hostname: String,
    pub record_name: Option<String>,
    pub verified: bool,
    pub proof_sha256: Option<String>,
    pub token: Option<String>,
    pub expires_at: Option<String>,
}

impl DomainVerificationChallenge {
    pub fn response(&self) -> DomainVerifyResponse {
        DomainVerifyResponse {
            verified: self.verified,
            method: DOMAIN_VERIFICATION_METHOD_DNS_TXT.to_owned(),
            verification_id: self.verification_id.clone(),
            record_name: self.record_name.clone(),
            token: if self.verified {
                None
            } else {
                self.token.clone()
            },
            expires_at: self.expires_at.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainVerificationObservation {
    pub matched: bool,
    pub observed_sha256: Option<String>,
    pub verifier_identity: String,
}

#[async_trait]
pub trait DomainOwnershipVerifierPort: Send + Sync {
    async fn verify_dns_txt(
        &self,
        hostname: &str,
        expected_sha256: &str,
    ) -> DeployServiceResult<DomainVerificationObservation>;
}

pub struct UnconfiguredDomainOwnershipVerifier;

#[async_trait]
impl DomainOwnershipVerifierPort for UnconfiguredDomainOwnershipVerifier {
    async fn verify_dns_txt(
        &self,
        _hostname: &str,
        _expected_sha256: &str,
    ) -> DeployServiceResult<DomainVerificationObservation> {
        Ok(DomainVerificationObservation {
            matched: false,
            observed_sha256: None,
            verifier_identity: "unconfigured".to_owned(),
        })
    }
}

pub fn normalize_domain_hostname(value: &str) -> DeployServiceResult<String> {
    let value = value.trim();
    let (wildcard, domain) = match value.strip_prefix("*.") {
        Some(domain) => (true, domain),
        None => (false, value),
    };
    let domain = domain.strip_suffix('.').unwrap_or(domain);
    if domain.is_empty() || domain.contains('*') {
        return Err(DeployServiceError::validation("hostname is invalid"));
    }

    let ascii = idna::domain_to_ascii_strict(domain)
        .map_err(|_| DeployServiceError::validation("hostname is invalid"))?
        .to_ascii_lowercase();
    if ascii.is_empty()
        || ascii.len() > 231
        || !ascii.contains('.')
        || IpAddr::from_str(&ascii).is_ok()
        || ascii
            .split('.')
            .any(|label| label.is_empty() || label.len() > 63)
    {
        return Err(DeployServiceError::validation("hostname is invalid"));
    }

    Ok(if wildcard {
        format!("*.{ascii}")
    } else {
        ascii
    })
}

pub fn normalize_zone_apex(value: &str) -> DeployServiceResult<String> {
    let hostname = normalize_domain_hostname(value)?;
    if hostname.starts_with("*.") || psl::domain_str(&hostname) != Some(hostname.as_str()) {
        return Err(DeployServiceError::validation(
            "zone apex must be a registrable root domain",
        ));
    }
    Ok(hostname)
}

/// Folds a fully-qualified hostname back into the relative name that
/// [`crate::DeployAppApi::create_domain_hostname`] expects inside `apex`.
///
/// This is the exact inverse of the repository's
/// `hostname_from_relative_name`: `*.shop` in `example.com` round-trips through
/// `*.shop.example.com` and back. Keeping the fold here — next to the
/// normalization that defines what a hostname even is — means a caller holding
/// a certificate SAN list never has to re-derive it, which is where the
/// `*.*.shop` double-wildcard defect came from.
pub fn relative_name_for_hostname(apex: &str, hostname: &str) -> DeployServiceResult<String> {
    let apex = normalize_domain_hostname(apex)?;
    let hostname = normalize_domain_hostname(hostname)?;
    if hostname == apex {
        return Ok("@".to_owned());
    }
    let suffix = format!(".{apex}");
    hostname
        .strip_suffix(&suffix)
        .map(str::to_owned)
        .ok_or_else(|| {
            DeployServiceError::validation("hostname must remain inside the selected domain zone")
        })
}

pub fn dns_txt_record_name(hostname: &str) -> DeployServiceResult<String> {
    let hostname = hostname.strip_prefix("*.").unwrap_or(hostname);
    let record_name = format!("{DOMAIN_VERIFICATION_RECORD_LABEL}.{hostname}");
    if record_name.len() > 253 {
        return Err(DeployServiceError::validation(
            "hostname is too long for domain verification",
        ));
    }
    Ok(record_name)
}

#[cfg(test)]
mod tests {
    use super::{
        dns_txt_record_name, normalize_domain_hostname, normalize_zone_apex,
        relative_name_for_hostname,
    };

    #[test]
    fn normalizes_idna_case_trailing_dot_and_wildcard() {
        assert_eq!(
            normalize_domain_hostname(" WWW.Example.COM. ").unwrap(),
            "www.example.com"
        );
        assert_eq!(
            normalize_domain_hostname("*.BUECHER.example").unwrap(),
            "*.buecher.example"
        );
        assert_eq!(
            dns_txt_record_name("*.example.com").unwrap(),
            "_sdkwork-verification.example.com"
        );
    }

    #[test]
    fn rejects_ip_single_label_and_ambiguous_wildcards() {
        for hostname in ["127.0.0.1", "localhost", "*.*.example.com", ""] {
            assert!(normalize_domain_hostname(hostname).is_err(), "{hostname}");
        }
    }

    #[test]
    fn folds_fully_qualified_hostnames_back_to_relative_names() {
        // `@` is the apex; a leading `*.` survives the fold as its own label so
        // the repository rebuilds the identical hostname from it.
        assert_eq!(
            relative_name_for_hostname("example.com", "example.com").unwrap(),
            "@"
        );
        assert_eq!(
            relative_name_for_hostname("example.com", "www.example.com").unwrap(),
            "www"
        );
        assert_eq!(
            relative_name_for_hostname("example.com", "*.example.com").unwrap(),
            "*"
        );
        assert_eq!(
            relative_name_for_hostname("example.com", "*.shop.example.com").unwrap(),
            "*.shop"
        );
        // Both sides are normalized, so case and trailing dots fold too.
        assert_eq!(
            relative_name_for_hostname("Example.COM.", "WWW.Example.Com.").unwrap(),
            "www"
        );
        for (apex, hostname) in [
            ("example.com", "example.com.evil.test"),
            ("example.com", "*.com"),
            ("example.com", "other.test"),
        ] {
            assert!(
                relative_name_for_hostname(apex, hostname).is_err(),
                "{hostname} must not fold into {apex}"
            );
        }
    }

    #[test]
    fn relative_names_round_trip_through_the_claim_shape() {
        // The certificate wizard hands over a fully-qualified SAN list and the
        // claim layer folds it back; the property that keeps the double-wildcard
        // defect from returning is that the fold is exactly invertible.
        for apex in ["example.com", "example.co.uk"] {
            for hostname in [
                apex.to_owned(),
                format!("www.{apex}"),
                format!("*.{apex}"),
                format!("*.shop.{apex}"),
            ] {
                let relative = relative_name_for_hostname(apex, &hostname).unwrap();
                let rebuilt = if relative == "@" {
                    apex.to_owned()
                } else {
                    format!("{relative}.{apex}")
                };
                assert_eq!(rebuilt, hostname);
            }
        }
    }

    #[test]
    fn accepts_only_public_suffix_registrable_zone_apexes() {
        assert_eq!(normalize_zone_apex("Example.COM.").unwrap(), "example.com");
        assert_eq!(
            normalize_zone_apex("example.co.uk").unwrap(),
            "example.co.uk"
        );
        for hostname in ["www.example.com", "co.uk", "*.example.com"] {
            assert!(normalize_zone_apex(hostname).is_err(), "{hostname}");
        }
    }
}
