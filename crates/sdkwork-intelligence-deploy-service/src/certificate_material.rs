//! Preparing issued certificate material for storage.
//!
//! Two jobs, and both have to happen before anything is written:
//!
//! 1. **Prove the description matches the bytes.** The issuance worker sends the
//!    four digests and the subject/issuer window it believes describe the
//!    certificate. Those values become the row the rest of the system reasons
//!    about — what a renewal compares against, what an audit reads — so they are
//!    re-derived here from the PEM and compared, rather than trusted. A
//!    disagreement fails the request: storing either side alone would leave the
//!    control plane holding metadata that describes a certificate it does not
//!    have.
//! 2. **Seal the one file that must not be readable.** The private key leaves this
//!    module as ciphertext under a key that never reaches the database.
//!
//! Both are done in the service, not in the repository, because the repository's
//! job is SQL: it should not be the component that decides whether a private key
//! is allowed to be written in the clear.

use chrono::{DateTime, Utc};
use sdkwork_deploy_certificate_material::{
    assemble_with_root_source, CertificateFile, MaterialError, MaterialKeyProvider, MaterialKind,
    SealedCertificateFile, TrustAnchorBundle,
};
use sdkwork_deploy_contract::{
    CertificateMaterialPayload, DeployServiceError, DeployServiceResult,
};

use crate::DeployService;

/// The certificate metadata the issuance worker declares alongside the bytes.
///
/// Mirrors `sdkwork-webserver-acme-service`'s `CertificateEvidence`; every field
/// is checked against the material rather than stored as sent.
#[derive(Clone, Copy, Debug)]
pub struct DeclaredCertificateEvidence<'a> {
    pub serial_sha256: &'a str,
    pub fingerprint_sha256: &'a str,
    pub spki_sha256: &'a str,
    pub chain_sha256: &'a str,
    pub issuer: &'a str,
    pub subject: &'a str,
    pub key_algorithm: &'a str,
    pub not_before: &'a str,
    pub not_after: &'a str,
}

/// Verifies the declared evidence, seals the bundle, and returns the five files
/// ready to be written.
///
/// `certificate_version_uuid` binds every sealed file to the version it belongs
/// to, so a blob cannot be moved between rows without failing authentication.
pub fn seal_issued_material(
    payload: &CertificateMaterialPayload,
    declared: &DeclaredCertificateEvidence<'_>,
    certificate_version_uuid: &str,
    key_provider: Option<&dyn MaterialKeyProvider>,
    anchors: Option<&TrustAnchorBundle>,
) -> DeployServiceResult<Vec<SealedCertificateFile>> {
    let (bundle, root_source) = assemble_with_root_source(
        payload.certificate_chain_pem.as_bytes(),
        payload.private_key_pem.as_bytes(),
        payload.root_pem.as_bytes(),
        anchors,
    )
    .map_err(unusable_material)?;
    let facts = bundle.facts();

    // The digests first: if the worker and the control plane disagree about which
    // certificate this is, nothing else about the request is worth reporting.
    for (field, declared_value, derived) in [
        (
            "serialSha256",
            declared.serial_sha256,
            facts.serial_sha256.as_str(),
        ),
        (
            "fingerprintSha256",
            declared.fingerprint_sha256,
            facts.fingerprint_sha256.as_str(),
        ),
        (
            "spkiSha256",
            declared.spki_sha256,
            facts.spki_sha256.as_str(),
        ),
        (
            "chainSha256",
            declared.chain_sha256,
            facts.chain_sha256.as_str(),
        ),
    ] {
        if !declared_value.eq_ignore_ascii_case(derived) {
            return Err(DeployServiceError::validation(format!(
                "{field} does not describe the material that was sent: declared \
                 '{declared_value}', derived '{derived}'"
            )));
        }
    }

    for (field, declared_value, derived) in [
        ("issuer", declared.issuer, facts.issuer.as_str()),
        ("subject", declared.subject, facts.subject.as_str()),
        (
            "keyAlgorithm",
            declared.key_algorithm,
            facts.key_algorithm.as_str(),
        ),
    ] {
        if declared_value != derived {
            return Err(DeployServiceError::validation(format!(
                "{field} does not describe the material that was sent: declared \
                 '{declared_value}', derived '{derived}'"
            )));
        }
    }

    // Compared as instants, not as strings: the worker renders them itself, so
    // `Z` and `+00:00` are both correct ways to write the same moment and must
    // not read as a disagreement.
    for (field, declared_value, derived) in [
        ("notBefore", declared.not_before, facts.not_before.as_str()),
        ("notAfter", declared.not_after, facts.not_after.as_str()),
    ] {
        if !same_instant(declared_value, derived) {
            return Err(DeployServiceError::validation(format!(
                "{field} does not describe the material that was sent: declared \
                 '{declared_value}', derived '{derived}'"
            )));
        }
    }

    let files = bundle
        .into_files()
        .iter()
        .map(|file| {
            SealedCertificateFile::seal(
                file.kind,
                &file.content,
                certificate_version_uuid,
                key_provider,
            )
        })
        .collect::<Result<Vec<_>, MaterialError>>()
        .map_err(unusable_material)?;

    // Every kind must be present, or a delivery step would silently load an
    // incomplete material root.
    debug_assert_eq!(files.len(), MaterialKind::ALL.len());

    tracing::debug!(
        root_source = ?root_source,
        certificate_version_uuid,
        "sealed issued certificate material"
    );
    Ok(files)
}

impl DeployService {
    /// Reads back and decrypts the stored material of one certificate version.
    ///
    /// This is the only place in the service where key material leaves custody,
    /// and it is deliberately **not** reachable from an HTTP route. A management
    /// endpoint that returns a private key turns every read permission into key
    /// exfiltration; node delivery reaches this through the drive port instead,
    /// where the audience is the machine that has to terminate TLS.
    ///
    /// Every file is authenticated on the way out: the AAD proves it is still
    /// bound to this version and kind, and the digest proves the plaintext is the
    /// one that was stored. A row moved between versions, or a restored backup
    /// with a truncated column, fails here rather than being served.
    pub async fn open_certificate_material(
        &self,
        tenant_id: i64,
        certificate_version_uuid: &str,
    ) -> DeployServiceResult<Vec<CertificateFile>> {
        let stored = self
            .repository
            .retrieve_certificate_material(tenant_id, certificate_version_uuid)
            .await?;
        if stored.is_empty() {
            return Err(DeployServiceError::not_found(format!(
                "certificate version {certificate_version_uuid} has no stored material"
            )));
        }

        let mut files = Vec::with_capacity(stored.len());
        for file in &stored {
            let content = file
                .open(
                    certificate_version_uuid,
                    self.certificate_material_key.as_deref(),
                )
                .map_err(unusable_material)?;
            files.push(CertificateFile {
                kind: file.kind,
                content: content.to_vec(),
            });
        }

        // Written in one transaction, so a version either has all five files or
        // has been tampered with. `order` is a bijection onto 0..5, so comparing
        // positions checks both the count and that nothing is missing.
        files.sort_by_key(|file| file.kind.order());
        if files
            .iter()
            .enumerate()
            .any(|(index, file)| file.kind.order() != index)
        {
            return Err(DeployServiceError::Internal(format!(
                "certificate version {certificate_version_uuid} has an incomplete material set"
            )));
        }
        Ok(files)
    }
}

fn same_instant(left: &str, right: &str) -> bool {
    match (
        DateTime::parse_from_rfc3339(left),
        DateTime::parse_from_rfc3339(right),
    ) {
        (Ok(left), Ok(right)) => left.with_timezone(&Utc) == right.with_timezone(&Utc),
        _ => false,
    }
}

/// Maps a custody failure onto the API's error vocabulary.
///
/// Material the caller sent wrongly is theirs to fix and belongs in `422`; a
/// service that cannot reach its master key is an operator problem and must not
/// be reported as a bad request, or the caller will keep retrying a payload that
/// was fine.
fn unusable_material(error: MaterialError) -> DeployServiceError {
    match error {
        MaterialError::Malformed(detail) | MaterialError::Inconsistent(detail) => {
            DeployServiceError::validation(format!("certificate material was refused: {detail}"))
        }
        MaterialError::Custody(detail) => DeployServiceError::Internal(format!(
            "certificate material custody is unavailable: {detail}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use rcgen::{date_time_ymd, CertificateParams, IsCa, KeyPair, PKCS_ECDSA_P256_SHA256};
    use sdkwork_deploy_certificate_material::{
        assemble, CertificateFacts, FileKeyProvider, Protection, MATERIAL_AAD_NAMESPACE,
    };

    use super::*;

    const VERSION_UUID: &str = "0b6f1c62-6f1e-4a2f-9d0a-2c5b8f3a1e77";

    /// The declaration, kept as owned strings so the evidence can borrow it
    /// without the test having to build a self-referential struct.
    struct Declared {
        serial_sha256: String,
        fingerprint_sha256: String,
        spki_sha256: String,
        chain_sha256: String,
        issuer: String,
        subject: String,
        key_algorithm: String,
        not_before: String,
        not_after: String,
    }

    impl Declared {
        fn from_facts(facts: &CertificateFacts) -> Self {
            Self {
                serial_sha256: facts.serial_sha256.clone(),
                fingerprint_sha256: facts.fingerprint_sha256.clone(),
                spki_sha256: facts.spki_sha256.clone(),
                chain_sha256: facts.chain_sha256.clone(),
                issuer: facts.issuer.clone(),
                subject: facts.subject.clone(),
                key_algorithm: facts.key_algorithm.clone(),
                not_before: facts.not_before.clone(),
                not_after: facts.not_after.clone(),
            }
        }

        fn evidence(&self) -> DeclaredCertificateEvidence<'_> {
            DeclaredCertificateEvidence {
                serial_sha256: &self.serial_sha256,
                fingerprint_sha256: &self.fingerprint_sha256,
                spki_sha256: &self.spki_sha256,
                chain_sha256: &self.chain_sha256,
                issuer: &self.issuer,
                subject: &self.subject,
                key_algorithm: &self.key_algorithm,
                not_before: &self.not_before,
                not_after: &self.not_after,
            }
        }
    }

    fn provider() -> FileKeyProvider {
        FileKeyProvider::from_material(&[7u8; 32], "file:test/certificate-material/master.key")
            .expect("provider")
    }

    /// A self-signed certificate presented the way the worker presents one: the
    /// CA blob, the key, and no separate root.
    fn fixture() -> (CertificateMaterialPayload, CertificateFacts) {
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("key pair");
        let mut params =
            CertificateParams::new(vec!["api.sdkwork.dev".to_owned()]).expect("params");
        params.is_ca = IsCa::ExplicitNoCa;
        params.not_before = date_time_ymd(2026, 1, 1);
        params.not_after = date_time_ymd(2026, 4, 1);
        let certificate = params.self_signed(&key).expect("self-signed certificate");
        let chain = certificate.pem();
        let private_key = key.serialize_pem();

        let bundle =
            assemble(chain.as_bytes(), private_key.as_bytes(), b"", None).expect("assemble");
        let facts = bundle.facts().clone();
        (
            CertificateMaterialPayload {
                certificate_chain_pem: chain,
                private_key_pem: private_key,
                root_pem: String::new(),
            },
            facts,
        )
    }

    #[test]
    fn seals_the_whole_bundle_and_only_the_key_is_secret() {
        let (payload, facts) = fixture();
        let declared = Declared::from_facts(&facts);
        let files = seal_issued_material(
            &payload,
            &declared.evidence(),
            VERSION_UUID,
            Some(&provider()),
            None,
        )
        .expect("seal");

        assert_eq!(files.len(), MaterialKind::ALL.len());
        let kinds: Vec<MaterialKind> = files.iter().map(|file| file.kind).collect();
        assert_eq!(kinds, MaterialKind::ALL.to_vec());
        for file in &files {
            if file.kind.is_secret() {
                assert_eq!(file.sealed.protection, Protection::EnvelopeAes256Gcm);
                assert!(!file.sealed.wrapped_dek.is_empty());
            } else {
                assert_eq!(
                    file.sealed.protection,
                    Protection::None,
                    "{} is public and must stay readable",
                    file.kind.as_str()
                );
            }
        }
    }

    #[test]
    fn refuses_a_digest_that_does_not_describe_the_material() {
        let (payload, facts) = fixture();
        let mut declared = Declared::from_facts(&facts);
        declared.serial_sha256 = "0".repeat(64);

        let error = seal_issued_material(
            &payload,
            &declared.evidence(),
            VERSION_UUID,
            Some(&provider()),
            None,
        )
        .expect_err("must refuse a mismatched digest");
        assert!(
            format!("{error}").contains("serialSha256 does not describe the material"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn refuses_a_subject_that_does_not_describe_the_material() {
        let (payload, facts) = fixture();
        let mut declared = Declared::from_facts(&facts);
        declared.subject = "CN=somewhere.else".to_owned();

        let error = seal_issued_material(
            &payload,
            &declared.evidence(),
            VERSION_UUID,
            Some(&provider()),
            None,
        )
        .expect_err("must refuse a mismatched subject");
        assert!(
            format!("{error}").contains("subject does not describe the material"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn accepts_the_same_moment_written_with_another_offset() {
        // The worker renders its own timestamps, so `Z` and `+08:00` are both
        // correct spellings of one instant and must not read as a disagreement.
        let (payload, facts) = fixture();
        let mut declared = Declared::from_facts(&facts);
        let instant = DateTime::parse_from_rfc3339(&facts.not_before).expect("instant");
        let east_eight = chrono::FixedOffset::east_opt(8 * 3600).expect("offset");
        declared.not_before = instant.with_timezone(&east_eight).to_rfc3339();
        assert_ne!(
            declared.not_before, facts.not_before,
            "the test must actually change the rendering"
        );

        seal_issued_material(
            &payload,
            &declared.evidence(),
            VERSION_UUID,
            Some(&provider()),
            None,
        )
        .expect("the same instant must be accepted");
    }

    #[test]
    fn refuses_to_seal_without_a_master_key() {
        // Not a validation failure: the payload is fine and the deployment is
        // broken, so the caller must not be told to fix its request.
        let (payload, facts) = fixture();
        let declared = Declared::from_facts(&facts);
        let error = seal_issued_material(&payload, &declared.evidence(), VERSION_UUID, None, None)
            .expect_err("must refuse to store a key without custody");
        assert_eq!(
            error.kind(),
            sdkwork_deploy_contract::DeployServiceErrorKind::Internal,
            "unexpected error: {error}"
        );
    }

    #[test]
    fn binds_every_sealed_file_to_the_version() {
        let (payload, facts) = fixture();
        let declared = Declared::from_facts(&facts);
        let files = seal_issued_material(
            &payload,
            &declared.evidence(),
            VERSION_UUID,
            Some(&provider()),
            None,
        )
        .expect("seal");

        for file in &files {
            assert!(
                file.sealed
                    .aad
                    .starts_with(MATERIAL_AAD_NAMESPACE.as_bytes()),
                "{} is not bound to the AAD namespace",
                file.kind.as_str()
            );
            let moved = file
                .open("11111111-2222-3333-4444-555555555555", Some(&provider()))
                .expect_err("a file must not open under another version");
            assert!(
                format!("{moved}").contains("different version"),
                "unexpected error: {moved}"
            );
        }
    }
}
