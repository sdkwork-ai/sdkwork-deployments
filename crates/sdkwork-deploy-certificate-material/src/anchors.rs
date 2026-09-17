//! Local trust anchors.
//!
//! A CA's chain file normally stops at its intermediate and omits the root — the
//! root belongs in the client's trust store, so the CA has no reason to send it.
//! Let's Encrypt, for instance, returns `leaf + R10/R11` and nothing above that.
//! The bundle this crate stores should still hold the anchor the chain terminates
//! at, so there has to be somewhere to get it from.
//!
//! Three sources are tried, in order, and none of them is allowed to invent an
//! anchor:
//!
//! 1. the caller hands the root over with the material;
//! 2. the chain already ends at a self-signed certificate, which *is* the root;
//! 3. a local bundle — the same thing an OS or a TLS client calls
//!    `ca-certificates.crt` — supplies it, matched by issuer name **and**
//!    signature.
//!
//! If none applies the request is refused. Storing an empty `root.pem` would
//! record a chain nobody can validate while looking like success.

use x509_parser::prelude::*;

use crate::facts::{certificate_ders, is_self_signed_der, split_certificate_blocks};
use crate::{MaterialError, MaterialResult};

/// A parsed local bundle of trust anchors.
///
/// Parsed once at load rather than per request: real bundles carry well over a
/// hundred certificates, and re-decoding that on every certificate issuance
/// would be pure waste.
#[derive(Clone, Debug)]
pub struct TrustAnchorBundle {
    anchors: Vec<Anchor>,
}

#[derive(Clone, Debug)]
struct Anchor {
    /// The block's original bytes, so a matched anchor is stored as it arrived.
    pem: Vec<u8>,
    der: Vec<u8>,
}

impl TrustAnchorBundle {
    /// Parses a PEM bundle.
    ///
    /// A bundle with no certificate in it is an error rather than an empty set:
    /// an operator who points the service at the wrong file should find out at
    /// startup, not at the first request that silently cannot resolve a root.
    pub fn from_pem(pem: &[u8]) -> MaterialResult<Self> {
        let blocks = split_certificate_blocks(pem)?;
        let anchors = blocks
            .into_iter()
            .map(|pem| {
                let der = certificate_ders(&pem)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| MaterialError::Malformed("empty anchor block".to_owned()))?;
                Ok(Anchor { pem, der })
            })
            .collect::<MaterialResult<Vec<_>>>()?;
        if anchors.is_empty() {
            return Err(MaterialError::Malformed(
                "the trust anchor bundle holds no certificate".to_owned(),
            ));
        }
        Ok(Self { anchors })
    }

    /// How many anchors the bundle holds.
    pub fn len(&self) -> usize {
        self.anchors.len()
    }

    /// Whether the bundle holds no anchor.
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }

    /// Finds the anchor that issued `subject_der`, if the bundle has one.
    ///
    /// Matching on the issuer *name* alone is not enough: any certificate can be
    /// minted with a copied subject, so the anchor must also carry the key that
    /// actually signed the child, and must be self-signed itself. A bundle
    /// holding a different root that happens to share the name is skipped rather
    /// than accepted.
    pub fn find_issuer_of(&self, subject_der: &[u8]) -> MaterialResult<Option<Vec<u8>>> {
        let (_, subject) = X509Certificate::from_der(subject_der).map_err(|error| {
            MaterialError::Malformed(format!("certificate is not valid X.509: {error}"))
        })?;
        let issuer = subject.issuer().to_string();
        for anchor in &self.anchors {
            let (_, anchor_cert) = X509Certificate::from_der(&anchor.der).map_err(|error| {
                MaterialError::Malformed(format!("trust anchor is not valid X.509: {error}"))
            })?;
            if anchor_cert.subject().to_string() != issuer {
                continue;
            }
            if !is_self_signed_der(&anchor.der)? {
                continue;
            }
            if subject
                .verify_signature(Some(anchor_cert.public_key()))
                .is_ok()
            {
                return Ok(Some(anchor.pem.clone()));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::tests::ecdsa_bundle;
    use crate::facts::sha256_hex;

    #[test]
    fn finds_the_anchor_that_signed_the_chain() {
        let fixture = ecdsa_bundle();
        let bundle = TrustAnchorBundle::from_pem(fixture.root.as_bytes()).expect("bundle");
        assert_eq!(bundle.len(), 1);
        assert!(!bundle.is_empty());

        let ders = certificate_ders(fixture.ca_chain().as_bytes()).expect("chain");
        // The last block of the chain is issued by the root, so that is what the
        // lookup has to resolve.
        let top = ders.last().expect("top");
        let found = bundle.find_issuer_of(top).expect("lookup").expect("found");
        assert_eq!(found, fixture.root.as_bytes());
    }

    #[test]
    fn ignores_an_anchor_that_only_shares_the_issuer_name() {
        // A second self-signed CA with the same subject but a different key. Name
        // matching alone would pick it; the signature check is what rejects it.
        let fixture = ecdsa_bundle();
        let impostor = crate::bundle::tests::self_signed_ca("SDKWork Test Root CA");
        let bundle = TrustAnchorBundle::from_pem(&impostor).expect("bundle");

        let ders = certificate_ders(fixture.ca_chain().as_bytes()).expect("chain");
        let top = ders.last().expect("top");
        assert!(
            bundle.find_issuer_of(top).expect("lookup").is_none(),
            "an anchor with the right name but the wrong key must not be accepted"
        );
    }

    #[test]
    fn prefers_the_real_anchor_when_a_decoy_comes_first() {
        let fixture = ecdsa_bundle();
        let impostor = crate::bundle::tests::self_signed_ca("SDKWork Test Root CA");
        let mixed = format!(
            "{}{}",
            String::from_utf8(impostor).expect("utf8"),
            fixture.root
        );
        let bundle = TrustAnchorBundle::from_pem(mixed.as_bytes()).expect("bundle");
        assert_eq!(bundle.len(), 2);

        let ders = certificate_ders(fixture.ca_chain().as_bytes()).expect("chain");
        let found = bundle
            .find_issuer_of(ders.last().expect("top"))
            .expect("lookup")
            .expect("found");
        assert_eq!(sha256_hex(&found), sha256_hex(fixture.root.as_bytes()));
    }

    #[test]
    fn refuses_a_bundle_with_no_certificate() {
        let error = TrustAnchorBundle::from_pem(b"not pem at all\n").expect_err("must reject");
        assert!(
            format!("{error}").contains("expected a CERTIFICATE block"),
            "unexpected error: {error}"
        );
    }
}
