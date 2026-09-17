//! Certificate facts derived from the bundle, and the proofs that it is
//! internally consistent.
//!
//! Every digest here matches `sdkwork-webserver-acme-service`'s
//! `CertificateEvidence` definition on purpose:
//!
//! | Digest | Covers |
//! | --- | --- |
//! | `serial_sha256` | the leaf's raw serial bytes |
//! | `fingerprint_sha256` | the leaf's DER |
//! | `spki_sha256` | the leaf's `SubjectPublicKeyInfo` DER |
//! | `chain_sha256` | the certificate chain **PEM text bytes** exactly as the CA returned them |
//!
//! The control plane recomputes them from the material it is handed. A mismatch
//! means the worker and the control plane disagree about what was issued, which
//! must be refused rather than persisted as fact.
//!
//! `chain_sha256` deserves a note. The issuance worker hashes the whole blob the
//! CA returned — leaf first, then the intermediates — because that is the string
//! it was handed; it never splits the leaf out before hashing. So this crate
//! hashes its input verbatim too, which is why the input here is the *chain* and
//! not a leaf-plus-intermediates pair. Splitting it into files is a derivation
//! that happens afterwards and must not change the digest.

use std::fmt::Write as _;

use chrono::{TimeZone, Utc};
use sha2::{Digest, Sha256};
use x509_parser::prelude::*;

use crate::{MaterialError, MaterialResult};

/// `id-ecPublicKey` — the SPKI algorithm OID for ECDSA keys.
pub const OID_EC_PUBLIC_KEY: &str = "1.2.840.10045.2.1";
/// `rsaEncryption` — the SPKI algorithm OID for RSA keys.
pub const OID_RSA_ENCRYPTION: &str = "1.2.840.113549.1.1.1";

const PEM_BEGIN: &[u8] = b"-----BEGIN ";
const CERTIFICATE_BEGIN: &[u8] = b"-----BEGIN CERTIFICATE-----";
const CERTIFICATE_END: &[u8] = b"-----END CERTIFICATE-----";

/// Lowercase hex SHA-256, the form every digest column stores.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        // Writing into a String cannot fail; ignoring the result keeps this
        // infallible without an unwrap.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The facts `deploy_certificate_version` stores, derived from the bytes rather
/// than accepted from the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateFacts {
    pub issuer: String,
    pub subject: String,
    /// DNS names from the leaf's `subjectAltName`, lowercased and in order.
    pub san_list: Vec<String>,
    pub serial_sha256: String,
    pub fingerprint_sha256: String,
    pub spki_sha256: String,
    pub chain_sha256: String,
    /// `RSA` or `ECDSA`.
    pub key_algorithm: String,
    pub not_before: String,
    pub not_after: String,
}

/// Splits a certificate file into its individual blocks, **preserving each
/// block's original bytes**.
///
/// Slicing the input rather than re-encoding the blocks is deliberate: a stored
/// `cert.pem` that has been through a PEM encoder is a different file from the
/// one the CA signed, and the recorded digest would no longer describe it.
///
/// Only `CERTIFICATE` blocks are accepted, and nothing but whitespace may appear
/// between them. A file that also carries a private key is therefore rejected
/// here rather than being silently stored with the key in a public column.
pub fn split_certificate_blocks(pem: &[u8]) -> MaterialResult<Vec<Vec<u8>>> {
    let mut blocks = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = find(&pem[cursor..], CERTIFICATE_BEGIN) {
        let start = cursor + offset;
        ensure_whitespace(&pem[cursor..start])?;
        let after_begin = &pem[start + CERTIFICATE_BEGIN.len()..];
        let Some(offset) = find(after_begin, CERTIFICATE_END) else {
            return Err(MaterialError::Malformed(
                "a CERTIFICATE block has no matching end marker".to_owned(),
            ));
        };
        let end_marker = start + CERTIFICATE_BEGIN.len() + offset + CERTIFICATE_END.len();
        // Carry the rest of the end marker's line so the block keeps its own
        // trailing newline; a file whose last block has none still round-trips.
        let end = match pem[end_marker..].iter().position(|byte| *byte == b'\n') {
            Some(offset) => end_marker + offset + 1,
            None => pem.len(),
        };
        blocks.push(pem[start..end].to_vec());
        cursor = end;
    }
    ensure_whitespace(&pem[cursor..])?;
    if blocks.is_empty() {
        return Err(MaterialError::Malformed(
            "no CERTIFICATE block found".to_owned(),
        ));
    }
    Ok(blocks)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn ensure_whitespace(gap: &[u8]) -> MaterialResult<()> {
    if gap.iter().all(u8::is_ascii_whitespace) {
        return Ok(());
    }
    // Naming the offending label turns "this file has junk in it" into "you sent
    // me a private key where a certificate was expected".
    let label = find(gap, PEM_BEGIN)
        .map(|start| {
            let rest = &gap[start + PEM_BEGIN.len()..];
            let end = rest
                .iter()
                .position(|byte| *byte == b'-')
                .unwrap_or(rest.len());
            String::from_utf8_lossy(&rest[..end]).into_owned()
        })
        .unwrap_or_else(|| "non-PEM content".to_owned());
    Err(MaterialError::Malformed(format!(
        "expected a CERTIFICATE block, found '{label}'"
    )))
}

/// The DER of one block, decoded out of its own text.
fn block_der(block: &[u8]) -> MaterialResult<Vec<u8>> {
    let text = std::str::from_utf8(block)
        .map_err(|_| MaterialError::Malformed("certificate PEM is not UTF-8".to_owned()))?;
    let body: Vec<u8> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("-----"))
        .flat_map(|line| line.trim().bytes())
        .collect();
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &body).map_err(|error| {
        MaterialError::Malformed(format!("certificate PEM body is not base64: {error}"))
    })
}

/// Every `CERTIFICATE` block in a PEM blob, as DER, in file order.
pub fn certificate_ders(pem: &[u8]) -> MaterialResult<Vec<Vec<u8>>> {
    split_certificate_blocks(pem)?
        .iter()
        .map(|block| block_der(block))
        .collect()
}

/// Exactly one certificate, which is what a root file must hold.
pub fn exactly_one_certificate_der(pem: &[u8], what: &str) -> MaterialResult<Vec<u8>> {
    let mut ders = certificate_ders(pem)?;
    if ders.len() != 1 {
        return Err(MaterialError::Inconsistent(format!(
            "{what} must hold exactly one certificate, found {}",
            ders.len()
        )));
    }
    Ok(ders.remove(0))
}

fn parse_certificate(der: &[u8]) -> MaterialResult<X509Certificate<'_>> {
    let (_, certificate) = X509Certificate::from_der(der).map_err(|error| {
        MaterialError::Malformed(format!("certificate is not valid X.509: {error}"))
    })?;
    Ok(certificate)
}

/// Whether a certificate issued itself: issuer equals subject *and* the
/// signature over it verifies under its own key.
///
/// Both halves are required. A certificate can carry a copied subject and still
/// be signed by someone else, and a self-issued certificate can still be signed
/// by a different key during a rollover.
pub fn is_self_signed_der(der: &[u8]) -> MaterialResult<bool> {
    let certificate = parse_certificate(der)?;
    if certificate.subject().to_string() != certificate.issuer().to_string() {
        return Ok(false);
    }
    Ok(certificate.verify_signature(None).is_ok())
}

fn key_algorithm_of(certificate: &X509Certificate<'_>) -> MaterialResult<String> {
    let oid = certificate.public_key().algorithm.algorithm.to_id_string();
    match oid.as_str() {
        OID_EC_PUBLIC_KEY => Ok("ECDSA".to_owned()),
        OID_RSA_ENCRYPTION => Ok("RSA".to_owned()),
        other => Err(MaterialError::Malformed(format!(
            "unsupported certificate public key algorithm {other}"
        ))),
    }
}

fn san_list_of(certificate: &X509Certificate<'_>) -> MaterialResult<Vec<String>> {
    let extension = certificate.subject_alternative_name().map_err(|error| {
        MaterialError::Malformed(format!("subjectAltName is unreadable: {error}"))
    })?;
    let Some(extension) = extension else {
        return Ok(Vec::new());
    };
    let mut names = Vec::new();
    for name in &extension.value.general_names {
        if let GeneralName::DNSName(value) = name {
            names.push(value.to_ascii_lowercase());
        }
    }
    Ok(names)
}

fn timestamp_to_rfc3339(timestamp: i64) -> MaterialResult<String> {
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .map(|value| value.to_rfc3339())
        .ok_or_else(|| MaterialError::Malformed("certificate timestamp is out of range".to_owned()))
}

/// Derives the stored facts from the certificate chain the CA returned.
///
/// The leaf is the first block; `chain_sha256` covers the input **verbatim**, so
/// callers must hand over exactly the bytes the CA returned. Re-encoding the PEM
/// would change the digest and make the row disagree with the issuance worker.
pub fn facts_from_chain(certificate_chain_pem: &[u8]) -> MaterialResult<CertificateFacts> {
    let ders = certificate_ders(certificate_chain_pem)?;
    let leaf_der = ders.first().ok_or_else(|| {
        MaterialError::Malformed("the certificate chain holds no certificate".to_owned())
    })?;
    let leaf = parse_certificate(leaf_der)?;
    Ok(CertificateFacts {
        issuer: leaf.issuer().to_string(),
        subject: leaf.subject().to_string(),
        san_list: san_list_of(&leaf)?,
        serial_sha256: sha256_hex(leaf.raw_serial()),
        fingerprint_sha256: sha256_hex(leaf_der),
        spki_sha256: sha256_hex(leaf.public_key().raw),
        chain_sha256: sha256_hex(certificate_chain_pem),
        key_algorithm: key_algorithm_of(&leaf)?,
        not_before: timestamp_to_rfc3339(leaf.validity().not_before.timestamp())?,
        not_after: timestamp_to_rfc3339(leaf.validity().not_after.timestamp())?,
    })
}

/// The `SubjectPublicKeyInfo` DER of a private key, which is what proves the key
/// belongs to the leaf.
///
/// rcgen 0.14 no longer exposes the SPKI DER directly, so it is read back out of
/// the `PUBLIC KEY` PEM rcgen renders. This is the same route
/// `sdkwork-webserver-acme-service` takes, which is what keeps
/// `sha256(spki)` comparable between the two sides.
pub fn public_key_spki_der(private_key_pem: &[u8]) -> MaterialResult<Vec<u8>> {
    let text = std::str::from_utf8(private_key_pem)
        .map_err(|_| MaterialError::Malformed("private key is not UTF-8 encoded PEM".to_owned()))?;
    let key_pair = rcgen::KeyPair::from_pem(text).map_err(|error| {
        MaterialError::Malformed(format!("private key could not be parsed: {error}"))
    })?;
    let public_pem = key_pair.public_key_pem();
    let body: String = public_pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .map(str::trim)
        .collect();
    base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        body.trim().as_bytes(),
    )
    .map_err(|error| {
        MaterialError::Malformed(format!("public key PEM body is not base64: {error}"))
    })
}

/// Proves `leaf <- intermediates <- root`, with the root anchored to itself.
///
/// `chain_ders` is the chain **with any trust anchor already removed**, so its
/// last element is the certificate that must have been issued by `root_der`.
///
/// Name chaining and signature verification are both checked. A bundle can pass
/// one and fail the other — an attacker can copy an issuer name onto a
/// self-signed certificate, and a CA can legitimately re-issue across a key
/// rollover — so neither check alone is sufficient.
pub fn verify_chain(chain_ders: &[Vec<u8>], root_der: &[u8]) -> MaterialResult<()> {
    if chain_ders.is_empty() {
        return Err(MaterialError::Inconsistent(
            "the certificate chain holds no certificate".to_owned(),
        ));
    }
    let root = parse_certificate(root_der)?;
    for (index, child_der) in chain_ders.iter().enumerate() {
        let parent_der = chain_ders
            .get(index + 1)
            .map(Vec::as_slice)
            .unwrap_or(root_der);
        let child = parse_certificate(child_der)?;
        let parent = parse_certificate(parent_der)?;
        if child.issuer().to_string() != parent.subject().to_string() {
            return Err(MaterialError::Inconsistent(format!(
                "certificate '{}' names issuer '{}' but the next certificate is '{}'",
                child.subject(),
                child.issuer(),
                parent.subject()
            )));
        }
        child
            .verify_signature(Some(parent.public_key()))
            .map_err(|error| {
                MaterialError::Inconsistent(format!(
                    "signature over '{}' does not verify against '{}': {error}",
                    child.subject(),
                    parent.subject()
                ))
            })?;
    }

    if root.subject().to_string() != root.issuer().to_string() {
        return Err(MaterialError::Inconsistent(format!(
            "root '{}' is not self-issued (issuer '{}')",
            root.subject(),
            root.issuer()
        )));
    }
    root.verify_signature(None).map_err(|error| {
        MaterialError::Inconsistent(format!(
            "root '{}' is not self-signed: {error}",
            root.subject()
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_match_the_issuance_worker_definition() {
        // The worker hashes the leaf DER for the fingerprint, the SPKI DER for
        // spki_sha256, and the whole CA blob verbatim for chain_sha256. Anchoring
        // the definition here means a refactor of either side cannot quietly
        // change what a stored digest means.
        let fixture = crate::bundle::tests::ecdsa_bundle();
        let chain = fixture.ca_chain();
        let ders = certificate_ders(chain.as_bytes()).expect("chain");
        let leaf = parse_certificate(&ders[0]).expect("parse");

        assert_eq!(
            fixture.bundle.facts().fingerprint_sha256,
            sha256_hex(&ders[0])
        );
        assert_eq!(
            fixture.bundle.facts().spki_sha256,
            sha256_hex(leaf.public_key().raw)
        );
        assert_eq!(
            fixture.bundle.facts().serial_sha256,
            sha256_hex(leaf.raw_serial())
        );
        assert_eq!(
            fixture.bundle.facts().chain_sha256,
            sha256_hex(chain.as_bytes())
        );
        // Splitting the blob into files must not disturb the digest, and the
        // stored full chain is byte-identical to what the CA returned.
        assert_eq!(
            fixture
                .bundle
                .file(crate::MaterialKind::FullChain)
                .unwrap()
                .content,
            chain.as_bytes()
        );
    }

    #[test]
    fn rejects_a_file_that_carries_a_private_key() {
        let fixture = crate::bundle::tests::ecdsa_bundle();
        let mixed = format!("{}{}", fixture.ca_chain(), fixture.private_key);
        let error = certificate_ders(mixed.as_bytes()).expect_err("must reject");
        assert!(
            format!("{error}").contains("expected a CERTIFICATE block, found 'PRIVATE KEY'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_a_pem_block_that_is_not_a_certificate() {
        let fixture = crate::bundle::tests::ecdsa_bundle();
        let error = certificate_ders(fixture.private_key.as_bytes()).expect_err("must reject");
        assert!(
            format!("{error}").contains("expected a CERTIFICATE block"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_trailing_junk_after_the_last_block() {
        let fixture = crate::bundle::tests::ecdsa_bundle();
        let mut chain = fixture.ca_chain().into_bytes();
        chain.extend_from_slice(b"garbage\n");
        let error = certificate_ders(&chain).expect_err("must reject");
        assert!(
            format!("{error}").contains("expected a CERTIFICATE block"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn splitting_preserves_each_blocks_own_bytes() {
        let fixture = crate::bundle::tests::ecdsa_bundle();
        let chain = fixture.ca_chain();
        let blocks = split_certificate_blocks(chain.as_bytes()).expect("blocks");
        assert_eq!(blocks.len(), 2, "leaf plus one intermediate");
        assert_eq!(
            blocks.concat(),
            chain.as_bytes(),
            "concatenating the blocks must reproduce the input exactly"
        );
    }

    #[test]
    fn recognises_a_self_signed_certificate() {
        let fixture = crate::bundle::tests::ecdsa_bundle();
        let root_der = exactly_one_certificate_der(fixture.root.as_bytes(), "root").expect("root");
        assert!(is_self_signed_der(&root_der).expect("root"));
        let ders = certificate_ders(fixture.ca_chain().as_bytes()).expect("chain");
        assert!(!is_self_signed_der(&ders[0]).expect("leaf"));
    }

    #[test]
    fn spki_of_the_private_key_matches_the_leaf_digest() {
        let fixture = crate::bundle::tests::ecdsa_bundle();
        let spki = public_key_spki_der(fixture.private_key.as_bytes()).expect("spki");
        assert_eq!(sha256_hex(&spki), fixture.bundle.facts().spki_sha256);
    }
}
