//! The canonical five-file certificate bundle.
//!
//! The file names follow the convention every TLS terminator already speaks
//! (`cert.pem`, `privkey.pem`, `chain.pem`, `fullchain.pem`), plus `root.pem`
//! for the trust anchor. Keeping the conventional names means an operator who
//! has to debug a certificate by hand finds the same layout they would get from
//! any other ACME client.
//!
//! The input is the blob the CA returned — leaf first, then its intermediates —
//! because that is what an issuance worker actually holds and what
//! `chain_sha256` hashes. Splitting it into `cert.pem`, `chain.pem` and
//! `fullchain.pem` is a derivation done here, and it slices the input's own
//! bytes rather than re-encoding them so the stored files stay identical to the
//! ones the CA signed.

use crate::anchors::TrustAnchorBundle;
use crate::facts::{self, CertificateFacts};
use crate::{MaterialError, MaterialResult};

/// `cert.pem` — the public leaf certificate.
pub const FILE_LEAF: &str = "cert.pem";
/// `privkey.pem` — the private key; the only secret in the set.
pub const FILE_PRIVATE_KEY: &str = "privkey.pem";
/// `chain.pem` — the intermediate certificates, in issuance order.
pub const FILE_CHAIN: &str = "chain.pem";
/// `root.pem` — the trust anchor the chain terminates at.
pub const FILE_ROOT: &str = "root.pem";
/// `fullchain.pem` — leaf followed by the intermediates.
pub const FILE_FULL_CHAIN: &str = "fullchain.pem";

/// The five files a managed certificate is stored as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialKind {
    /// The public leaf certificate.
    LeafCert,
    /// The private key.
    PrivateKey,
    /// The intermediate certificates, in issuance order.
    IntermediateChain,
    /// The root certificate the chain terminates at.
    RootCert,
    /// Leaf followed by the intermediates — what a TLS terminator loads.
    ///
    /// Deliberately excludes the root. The anchor belongs in the client's trust
    /// store, not in the chain a server presents; serving it is a common
    /// misconfiguration that inflates the handshake for no benefit.
    FullChain,
}

impl MaterialKind {
    /// Every kind, in the order a bundle is written.
    pub const ALL: [MaterialKind; 5] = [
        MaterialKind::LeafCert,
        MaterialKind::PrivateKey,
        MaterialKind::IntermediateChain,
        MaterialKind::RootCert,
        MaterialKind::FullChain,
    ];

    /// The stable database value.
    pub fn as_str(self) -> &'static str {
        match self {
            MaterialKind::LeafCert => "LEAF_CERT",
            MaterialKind::PrivateKey => "PRIVATE_KEY",
            MaterialKind::IntermediateChain => "INTERMEDIATE_CHAIN",
            MaterialKind::RootCert => "ROOT_CERT",
            MaterialKind::FullChain => "FULL_CHAIN",
        }
    }

    /// The file name the material root uses.
    pub fn file_name(self) -> &'static str {
        match self {
            MaterialKind::LeafCert => FILE_LEAF,
            MaterialKind::PrivateKey => FILE_PRIVATE_KEY,
            MaterialKind::IntermediateChain => FILE_CHAIN,
            MaterialKind::RootCert => FILE_ROOT,
            MaterialKind::FullChain => FILE_FULL_CHAIN,
        }
    }

    /// The media type recorded alongside the bytes.
    pub fn media_type(self) -> &'static str {
        "application/x-pem-file"
    }

    /// Whether the file must never be stored as plaintext.
    pub fn is_secret(self) -> bool {
        matches!(self, MaterialKind::PrivateKey)
    }

    /// Reads the database value back, rejecting anything unrecognised.
    ///
    /// A stored row with a kind this build does not know is either a newer writer
    /// or corruption; either way it must not be silently skipped, because a
    /// dropped file is an incomplete material root that still looks complete.
    pub fn from_db(value: &str) -> MaterialResult<Self> {
        MaterialKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| {
                MaterialError::Malformed(format!("unknown certificate material kind '{value}'"))
            })
    }

    /// Position in the canonical bundle order.
    pub fn order(self) -> usize {
        MaterialKind::ALL
            .into_iter()
            .position(|kind| kind == self)
            .unwrap_or(usize::MAX)
    }
}

/// One file of a bundle, as plaintext PEM bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateFile {
    pub kind: MaterialKind,
    pub content: Vec<u8>,
}

/// A bundle whose internal consistency has already been proven.
#[derive(Clone, Debug)]
pub struct CertificateMaterialBundle {
    files: Vec<CertificateFile>,
    facts: CertificateFacts,
}

impl CertificateMaterialBundle {
    /// Every file, in [`MaterialKind::ALL`] order.
    pub fn files(&self) -> &[CertificateFile] {
        &self.files
    }

    /// The facts derived from the bytes.
    pub fn facts(&self) -> &CertificateFacts {
        &self.facts
    }

    /// One file by kind.
    pub fn file(&self, kind: MaterialKind) -> Option<&CertificateFile> {
        self.files.iter().find(|file| file.kind == kind)
    }

    /// Consumes the bundle, yielding the files.
    pub fn into_files(self) -> Vec<CertificateFile> {
        self.files
    }
}

/// Where the trust anchor came from, recorded so an operator reading a stored
/// row can tell a supplied root from one resolved out of a local bundle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootSource {
    /// Sent along with the material by the caller.
    Provided,
    /// The chain itself ended at a self-signed certificate, so that block is the
    /// anchor.
    Chain,
    /// Matched out of the configured trust anchor bundle.
    LocalBundle,
}

/// Assembles the bundle, refusing anything that is not internally consistent.
///
/// The checks, in order: the chain parses and the leaf is its first block; a
/// trust anchor can be established; the chain verifies up to that anchor and the
/// anchor is self-signed; the private key is the one the leaf publishes; the leaf
/// actually covers a name; and the validity window runs forward.
///
/// `root_pem` may be empty, in which case the anchor is resolved from the chain
/// itself or from `anchors`. When nothing can anchor the chain the call fails:
/// the alternative is a stored bundle with an empty `root.pem`, which looks like
/// success and validates nowhere.
pub fn assemble(
    certificate_chain_pem: &[u8],
    private_key_pem: &[u8],
    root_pem: &[u8],
    anchors: Option<&TrustAnchorBundle>,
) -> MaterialResult<CertificateMaterialBundle> {
    Ok(assemble_with_root_source(certificate_chain_pem, private_key_pem, root_pem, anchors)?.0)
}

/// Same as [`assemble`], also reporting where the trust anchor came from.
///
/// The provenance is not stored — the bundle has no column for it — but a caller
/// that logs issuance or explains a refusal wants to say whether the anchor was
/// sent, found in the chain, or resolved locally.
pub fn assemble_with_root_source(
    certificate_chain_pem: &[u8],
    private_key_pem: &[u8],
    root_pem: &[u8],
    anchors: Option<&TrustAnchorBundle>,
) -> MaterialResult<(CertificateMaterialBundle, RootSource)> {
    let facts = facts::facts_from_chain(certificate_chain_pem)?;
    let blocks = facts::split_certificate_blocks(certificate_chain_pem)?;
    let ders: Vec<Vec<u8>> = blocks
        .iter()
        .map(|block| facts::certificate_ders(block).map(|ders| ders[0].clone()))
        .collect::<MaterialResult<Vec<_>>>()?;

    let leaf_block = &blocks[0];
    let rest_blocks = &blocks[1..];
    let rest_ders = &ders[1..];

    // A self-signed block at the end of the chain is a trust anchor, not an
    // intermediate. A single self-signed block is both at once: that is what a
    // development certificate looks like, and its anchor really is itself.
    let (intermediate_blocks, intermediate_ders, embedded_root): (
        &[Vec<u8>],
        &[Vec<u8>],
        Option<&Vec<u8>>,
    ) = match rest_ders.last() {
        Some(last) if facts::is_self_signed_der(last)? => {
            let count = rest_blocks.len() - 1;
            (
                &rest_blocks[..count],
                &rest_ders[..count],
                Some(&rest_blocks[count]),
            )
        }
        Some(_) => (rest_blocks, rest_ders, None),
        None if !rest_ders.is_empty() => (rest_blocks, rest_ders, None),
        // One block, and it signed itself: the certificate is its own anchor.
        None if facts::is_self_signed_der(&ders[0])? => (&[], &[], Some(&blocks[0])),
        None => (&[], &[], None),
    };

    // Chain as it must verify: leaf and intermediates only, anchor excluded.
    let mut verifying: Vec<Vec<u8>> = Vec::with_capacity(intermediate_ders.len() + 1);
    verifying.push(ders[0].clone());
    verifying.extend(intermediate_ders.iter().cloned());

    let (root_pem, root_source) = resolve_root(&verifying, root_pem, embedded_root, anchors)?;
    let root_der = facts::exactly_one_certificate_der(&root_pem, "the root certificate")?;
    facts::verify_chain(&verifying, &root_der)?;

    // A leaf that publishes no name cannot terminate TLS. The CA/Browser Forum
    // has not permitted CN-only certificates for years, so this is a refusal
    // rather than a warning.
    if facts.san_list.is_empty() {
        return Err(MaterialError::Inconsistent(
            "the leaf certificate declares no subjectAltName".to_owned(),
        ));
    }

    // The whole point of storing a private key next to its certificate is that
    // they belong together; proving it here means a mis-paired key can never
    // reach the database.
    let spki_sha256 = facts::sha256_hex(&facts::public_key_spki_der(private_key_pem)?);
    if spki_sha256 != facts.spki_sha256 {
        return Err(MaterialError::Inconsistent(
            "the private key does not match the leaf certificate's public key".to_owned(),
        ));
    }

    let not_before = chrono::DateTime::parse_from_rfc3339(&facts.not_before).map_err(|error| {
        MaterialError::Malformed(format!("notBefore is not a timestamp: {error}"))
    })?;
    let not_after = chrono::DateTime::parse_from_rfc3339(&facts.not_after).map_err(|error| {
        MaterialError::Malformed(format!("notAfter is not a timestamp: {error}"))
    })?;
    if not_after <= not_before {
        return Err(MaterialError::Inconsistent(format!(
            "the leaf certificate expires at {} but is not valid before {}",
            facts.not_after, facts.not_before
        )));
    }

    let mut chain_pem = Vec::new();
    for block in intermediate_blocks {
        chain_pem.extend_from_slice(block);
    }
    let mut full_chain = leaf_block.clone();
    full_chain.extend_from_slice(&chain_pem);

    let files = vec![
        CertificateFile {
            kind: MaterialKind::LeafCert,
            content: leaf_block.clone(),
        },
        CertificateFile {
            kind: MaterialKind::PrivateKey,
            content: private_key_pem.to_vec(),
        },
        CertificateFile {
            kind: MaterialKind::IntermediateChain,
            content: chain_pem,
        },
        CertificateFile {
            kind: MaterialKind::RootCert,
            content: root_pem,
        },
        CertificateFile {
            kind: MaterialKind::FullChain,
            content: full_chain,
        },
    ];

    Ok((CertificateMaterialBundle { files, facts }, root_source))
}

/// Establishes the trust anchor, in the precedence documented on
/// [`TrustAnchorBundle`].
fn resolve_root(
    verifying: &[Vec<u8>],
    supplied_root_pem: &[u8],
    embedded_root: Option<&Vec<u8>>,
    anchors: Option<&TrustAnchorBundle>,
) -> MaterialResult<(Vec<u8>, RootSource)> {
    if !supplied_root_pem.iter().all(u8::is_ascii_whitespace) {
        return Ok((supplied_root_pem.to_vec(), RootSource::Provided));
    }
    if let Some(block) = embedded_root {
        return Ok((block.clone(), RootSource::Chain));
    }
    if let Some(anchors) = anchors {
        let top = verifying.last().ok_or_else(|| {
            MaterialError::Inconsistent("the certificate chain holds no certificate".to_owned())
        })?;
        if let Some(found) = anchors.find_issuer_of(top)? {
            return Ok((found, RootSource::LocalBundle));
        }
    }
    Err(MaterialError::Inconsistent(
        "the certificate chain does not reach a trust anchor and none was supplied; \
         send the root with the material, or configure a local trust anchor bundle"
            .to_owned(),
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use rcgen::{
        date_time_ymd, BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose,
        PKCS_ECDSA_P256_SHA256,
    };

    use super::*;

    /// A bundle built from a real three-level chain, so the signature and name
    /// chaining checks are exercised against actual cryptography rather than
    /// hand-written PEM.
    pub(crate) struct BundleFixture {
        pub leaf: String,
        pub intermediate: String,
        pub private_key: String,
        pub root: String,
        pub bundle: CertificateMaterialBundle,
    }

    impl BundleFixture {
        /// The blob a CA returns: leaf first, then the intermediates.
        pub(crate) fn ca_chain(&self) -> String {
            format!("{}{}", self.leaf, self.intermediate)
        }
    }

    fn ca_params(common_name: &str, ca: IsCa) -> CertificateParams {
        let mut params = CertificateParams::new(Vec::<String>::new()).expect("params");
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        params.is_ca = ca;
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        params.not_before = date_time_ymd(2026, 1, 1);
        params.not_after = date_time_ymd(2036, 1, 1);
        params
    }

    /// A self-signed CA certificate, for trust-anchor fixtures.
    pub(crate) fn self_signed_ca(common_name: &str) -> Vec<u8> {
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("ca key");
        let params = ca_params(common_name, IsCa::Ca(BasicConstraints::Unconstrained));
        params
            .self_signed(&key)
            .expect("ca cert")
            .pem()
            .into_bytes()
    }

    pub(crate) fn ecdsa_bundle() -> BundleFixture {
        let root_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("root key");
        let root_params = ca_params(
            "SDKWork Test Root CA",
            IsCa::Ca(BasicConstraints::Constrained(1)),
        );
        let root_cert = root_params.self_signed(&root_key).expect("root cert");

        let intermediate_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("ca key");
        let intermediate_params = ca_params(
            "SDKWork Test Intermediate CA",
            IsCa::Ca(BasicConstraints::Unconstrained),
        );
        let issuer = rcgen::Issuer::from_params(&root_params, &root_key);
        let intermediate_cert = intermediate_params
            .signed_by(&intermediate_key, &issuer)
            .expect("intermediate cert");

        let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("leaf key");
        let mut leaf_params = CertificateParams::new(vec![
            "shop.sdkwork.com".to_owned(),
            "www.shop.sdkwork.com".to_owned(),
        ])
        .expect("leaf params");
        leaf_params
            .distinguished_name
            .push(DnType::CommonName, "shop.sdkwork.com");
        leaf_params.is_ca = IsCa::ExplicitNoCa;
        leaf_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        leaf_params.not_before = date_time_ymd(2026, 1, 1);
        leaf_params.not_after = date_time_ymd(2026, 4, 1);
        let leaf_issuer = rcgen::Issuer::from_params(&intermediate_params, &intermediate_key);
        let leaf_cert = leaf_params
            .signed_by(&leaf_key, &leaf_issuer)
            .expect("leaf cert");

        let leaf = leaf_cert.pem();
        let private_key = leaf_key.serialize_pem();
        let intermediate = intermediate_cert.pem();
        let root = root_cert.pem();
        let chain = format!("{leaf}{intermediate}");
        let bundle = assemble(
            chain.as_bytes(),
            private_key.as_bytes(),
            root.as_bytes(),
            None,
        )
        .expect("assemble");
        BundleFixture {
            leaf,
            intermediate,
            private_key,
            root,
            bundle,
        }
    }

    #[test]
    fn assembles_the_canonical_five_files() {
        let fixture = ecdsa_bundle();
        let kinds: Vec<&str> = fixture
            .bundle
            .files()
            .iter()
            .map(|file| file.kind.as_str())
            .collect();
        assert_eq!(
            kinds,
            vec![
                "LEAF_CERT",
                "PRIVATE_KEY",
                "INTERMEDIATE_CHAIN",
                "ROOT_CERT",
                "FULL_CHAIN"
            ]
        );
        assert!(fixture.bundle.file(MaterialKind::PrivateKey).is_some());
        assert!(MaterialKind::PrivateKey.is_secret());
        assert!(!MaterialKind::RootCert.is_secret());
        for file in fixture.bundle.files() {
            assert_eq!(file.kind.file_name(), file.kind.file_name());
            assert_eq!(file.kind.media_type(), "application/x-pem-file");
        }
    }

    #[test]
    fn splits_the_ca_blob_into_leaf_chain_and_fullchain() {
        let fixture = ecdsa_bundle();
        let file = |kind| {
            String::from_utf8(fixture.bundle.file(kind).expect("file").content.clone())
                .expect("utf8")
        };
        assert_eq!(file(MaterialKind::LeafCert), fixture.leaf);
        assert_eq!(file(MaterialKind::IntermediateChain), fixture.intermediate);
        assert_eq!(file(MaterialKind::RootCert), fixture.root);
        // The full chain is the root-free part of what the CA returned, byte for
        // byte — no re-encoding, so it loads in a TLS terminator unchanged.
        assert_eq!(file(MaterialKind::FullChain), fixture.ca_chain());
    }

    #[test]
    fn derives_the_expected_facts() {
        let fixture = ecdsa_bundle();
        let facts = fixture.bundle.facts();
        assert_eq!(facts.key_algorithm, "ECDSA");
        assert_eq!(
            facts.san_list,
            vec![
                "shop.sdkwork.com".to_owned(),
                "www.shop.sdkwork.com".to_owned()
            ]
        );
        assert!(facts.subject.contains("shop.sdkwork.com"));
        assert!(facts.issuer.contains("SDKWork Test Intermediate CA"));
        assert_eq!(facts.fingerprint_sha256.len(), 64);
        assert!(facts.not_after > facts.not_before);
    }

    #[test]
    fn treats_a_self_signed_certificate_as_its_own_anchor() {
        // What a development certificate looks like: no chain, no separate root,
        // and the certificate really is the anchor a client would have to trust.
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("key");
        let mut params = CertificateParams::new(vec!["dev.localhost".to_owned()]).expect("params");
        params
            .distinguished_name
            .push(DnType::CommonName, "dev.localhost");
        params.is_ca = IsCa::ExplicitNoCa;
        params.not_before = date_time_ymd(2026, 1, 1);
        params.not_after = date_time_ymd(2026, 4, 1);
        let pem = params.self_signed(&key).expect("self signed").pem();
        let private_key = key.serialize_pem();

        let bundle = assemble(pem.as_bytes(), private_key.as_bytes(), b"", None).expect("assemble");
        assert_eq!(
            String::from_utf8(bundle.file(MaterialKind::RootCert).unwrap().content.clone())
                .expect("utf8"),
            pem
        );
        assert!(
            bundle
                .file(MaterialKind::IntermediateChain)
                .expect("chain")
                .content
                .is_empty(),
            "a self-signed certificate has no intermediates"
        );
    }

    #[test]
    fn drops_a_root_that_the_ca_included_in_the_chain() {
        // Some CAs append their root. It is an anchor, not an intermediate, so it
        // must not end up in the chain a server presents.
        let fixture = ecdsa_bundle();
        let chain = format!("{}{}", fixture.ca_chain(), fixture.root);
        let bundle = assemble(chain.as_bytes(), fixture.private_key.as_bytes(), b"", None)
            .expect("assemble");
        let file = |kind| {
            String::from_utf8(bundle.file(kind).expect("file").content.clone()).expect("utf8")
        };
        assert_eq!(file(MaterialKind::IntermediateChain), fixture.intermediate);
        assert_eq!(file(MaterialKind::FullChain), fixture.ca_chain());
        assert_eq!(file(MaterialKind::RootCert), fixture.root);
    }

    #[test]
    fn refuses_a_chain_no_local_anchor_can_terminate() {
        let fixture = ecdsa_bundle();
        let error = assemble(
            fixture.ca_chain().as_bytes(),
            fixture.private_key.as_bytes(),
            b"",
            None,
        )
        .expect_err("must refuse an unanchored chain");
        assert!(
            format!("{error}").contains("does not reach a trust anchor"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn resolves_the_anchor_out_of_a_local_bundle() {
        let fixture = ecdsa_bundle();
        let anchors = TrustAnchorBundle::from_pem(fixture.root.as_bytes()).expect("anchor bundle");
        let bundle = assemble(
            fixture.ca_chain().as_bytes(),
            fixture.private_key.as_bytes(),
            b"",
            Some(&anchors),
        )
        .expect("assemble");
        assert_eq!(
            String::from_utf8(bundle.file(MaterialKind::RootCert).unwrap().content.clone())
                .expect("utf8"),
            fixture.root
        );
    }

    #[test]
    fn rejects_a_private_key_from_a_different_certificate() {
        let first = ecdsa_bundle();
        let second = ecdsa_bundle();
        let error = assemble(
            first.ca_chain().as_bytes(),
            second.private_key.as_bytes(),
            first.root.as_bytes(),
            None,
        )
        .expect_err("must refuse a mismatched key");
        assert!(
            format!("{error}").contains("does not match the leaf certificate"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_a_chain_that_does_not_reach_the_root() {
        let alpha = ecdsa_bundle();
        let beta = ecdsa_bundle();
        let error = assemble(
            alpha.ca_chain().as_bytes(),
            alpha.private_key.as_bytes(),
            beta.root.as_bytes(),
            None,
        )
        .expect_err("must refuse a foreign root");
        assert!(
            format!("{error}").contains("does not verify against"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_a_root_that_is_not_self_issued() {
        // The intermediate chains the leaf correctly and its signature verifies,
        // yet it is not an anchor: its own issuer is the root. Accepting it
        // would let anyone terminate a chain at a certificate they merely hold,
        // so the self-issued check has to be a separate gate from verification.
        let fixture = ecdsa_bundle();
        let error = assemble(
            fixture.leaf.as_bytes(),
            fixture.private_key.as_bytes(),
            fixture.intermediate.as_bytes(),
            None,
        )
        .expect_err("must refuse a non-self-issued root");
        assert!(
            format!("{error}").contains("not self-issued"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_a_leaf_presented_as_the_root() {
        // The leaf is not self-signed, so it cannot serve as the anchor however
        // it is passed; what catches it here is that the intermediate cannot be
        // verified against it.
        let fixture = ecdsa_bundle();
        let error = assemble(
            fixture.ca_chain().as_bytes(),
            fixture.private_key.as_bytes(),
            fixture.leaf.as_bytes(),
            None,
        )
        .expect_err("must refuse a leaf as root");
        assert!(
            format!("{error}").contains("names issuer"),
            "unexpected error: {error}"
        );
    }
}
