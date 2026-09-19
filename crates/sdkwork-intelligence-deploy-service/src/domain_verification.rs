//! Domain ownership normalization and verification port.

use std::{net::IpAddr, str::FromStr};

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult, DomainVerifyResponse};
use sdkwork_utils_rust::crypto::sha256_digest;
use sdkwork_utils_rust::encoding::base64url_encode;

pub const DOMAIN_VERIFICATION_METHOD_DNS_TXT: &str = "DNS_TXT";
const DOMAIN_VERIFICATION_RECORD_LABEL: &str = "_sdkwork-verification";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainVerificationChallenge {
    pub verification_id: Option<String>,
    pub hostname: String,
    pub record_name: Option<String>,
    /// The same record reduced against its zone, i.e. what the provider's
    /// "host"/"主机记录" field wants. `None` when the zone is unknown or the
    /// record is not inside it.
    pub record_relative_name: Option<String>,
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
            record_relative_name: self.record_relative_name.clone(),
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

/// The owner label a DNS provider's console asks for, i.e. the record name
/// reduced against the zone that owns it.
///
/// Providers label the field "host" / "name" / "主机记录" and append their own
/// zone, so handing them the fully qualified name publishes
/// `_sdkwork-verification.birdcoder.com.birdcoder.com`, which never resolves.
/// The reduction is the same one [`crate::relative_name_for_hostname`] performs;
/// it is repeated here for the verification label specifically because that
/// label carries a leading underscore, which [`normalize_domain_hostname`]
/// rejects — `idna::domain_to_ascii_strict` only accepts hostnames, and an
/// underscore is legal in a DNS owner name but not in a hostname (RFC 1035
/// §2.3.1 permits it; the IDNA/hostname profiles do not).
///
/// Returns `None` rather than guessing when the record is not inside `zone`:
/// a prefix the operator cannot use fails silently at the provider, which is
/// worse than showing no row at all.
pub fn dns_txt_relative_name(record_name: &str, zone_apex: &str) -> Option<String> {
    // Compare on the DNS owner-name alphabet, not the hostname alphabet: fold
    // case and the UTS #46 full stops, drop a trailing dot, and keep `_`.
    let record = fold_dns_owner_name(record_name)?;
    let zone = fold_dns_owner_name(zone_apex)?;
    let suffix = format!(".{zone}");
    let relative = record.strip_suffix(&suffix)?;
    (!relative.is_empty()).then(|| relative.to_owned())
}

/// Folds a DNS *owner* name for comparison: lowercases, maps the UTS #46 full
/// stops, strips one trailing dot, and validates the 253/63 length limits.
///
/// Deliberately stricter than a hostname check in exactly one direction — it
/// accepts a leading underscore, which every ownership-verification label uses
/// — and looser in none.
fn fold_dns_owner_name(raw: &str) -> Option<String> {
    let folded = raw
        .trim()
        .replace(['\u{3002}', '\u{ff0e}', '\u{ff61}'], ".");
    let body = folded
        .strip_suffix('.')
        .unwrap_or(&folded)
        .to_ascii_lowercase();
    if body.is_empty() || body.len() > 253 || body.contains('*') {
        return None;
    }
    let labels = body.split('.');
    if labels
        .into_iter()
        .any(|label| label.is_empty() || label.len() > 63)
    {
        return None;
    }
    Some(body)
}

/// Builds the TXT record value the operator must publish, following RFC 8555
/// §8.4 the way every mainstream CA and DNS provider does.
///
/// The wire format is `base64url(sha256(secret))`: 43 characters over the
/// base64url alphabet with no padding, carrying the full 256 bits of the digest
/// rather than the 122 bits a bare UUIDv4 spells out. It is deliberately opaque
/// — no brand prefix, no structuring — because the value's only job is to be
/// unique and unguessable, and a decorative prefix invites operators to edit it.
///
/// The secret is passed in rather than generated here so the caller keeps the
/// single source of entropy; only the digest ever reaches the database, exactly
/// as the verifier later recomputes it from the published TXT contents.
pub fn dns_txt_record_value(secret: &str) -> String {
    base64url_encode(&sha256_digest(secret.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::{
        dns_txt_record_name, dns_txt_record_value, dns_txt_relative_name,
        normalize_domain_hostname, normalize_zone_apex, relative_name_for_hostname,
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

    #[test]
    fn reduces_the_verification_label_to_the_providers_host_field() {
        // The operator's console appends its own zone, so the bare label is what
        // belongs in the "host"/"主机记录" box.
        assert_eq!(
            dns_txt_relative_name("_sdkwork-verification.birdcoder.com", "birdcoder.com")
                .as_deref(),
            Some("_sdkwork-verification")
        );
        // Deeper zones keep their intermediate labels: the record is still one
        // label inside `eu.example.com`.
        assert_eq!(
            dns_txt_relative_name("_sdkwork-verification.eu.example.com", "eu.example.com")
                .as_deref(),
            Some("_sdkwork-verification")
        );
        // A record the zone does not own must refuse, never guess: a wrong
        // prefix fails silently at the provider.
        assert_eq!(
            dns_txt_relative_name("_sdkwork-verification.other.example.com", "birdcoder.com"),
            None
        );
        // The apex itself is not a relative name.
        assert_eq!(
            dns_txt_relative_name("birdcoder.com", "birdcoder.com"),
            None
        );
    }

    #[test]
    fn dns_txt_record_values_follow_the_base64url_digest_shape() {
        // RFC 8555 §8.4: base64url(sha256(secret)), no padding. A 32-byte digest
        // is 43 base64url characters.
        assert_eq!(
            dns_txt_record_value("hello"),
            "LPJNul-wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ"
        );
        let value = dns_txt_record_value("6f1a3c2e-0000-4000-8000-000000000000");
        assert_eq!(value.len(), 43);
        assert!(!value.contains('='), "{value}");
        assert!(
            value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "{value}"
        );
        // Distinct secrets must not collide.
        assert_ne!(dns_txt_record_value("a"), dns_txt_record_value("b"));
    }
}
