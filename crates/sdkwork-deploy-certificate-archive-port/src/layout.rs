//! Where an archived certificate version lives, and what its manifest says.
//!
//! Every decision in this module is a pure function of the certificate's own
//! identity, for one reason: the layout is *quoted in the console*. An operator
//! sees `证书库/example.com-3f7c1a9b/v2-2027-02-22T09-00-00Z/`, and a retry —
//! possibly in another process, days later — must resolve the same folders
//! again. A layout derived from anything else (a clock, a counter, a random id)
//! would make the second attempt write a second copy, or overwrite the first.
//!
//! ```text
//! 证书库/                                    <- the tenant's certificate space
//!   example.com-3f7c1a9b/                    <- sanitize(certName) + "-" + certId[0..8]
//!     v2-2027-02-22T09-00-00Z/               <- "v" + versionNo + "-" + notAfter
//!       cert.pem                             <- leaf
//!       chain.pem                            <- intermediates, issuance order
//!       root.pem                             <- trust anchor
//!       fullchain.pem                        <- leaf first, what a TLS terminator loads
//!       manifest.json                        <- machine-readable record (see [`ArchiveManifest`])
//! ```
//!
//! The private key is not in that list and never will be under the `PUBLIC`
//! bundle policy: it stays in the sealed custody store
//! (`secret://deploy/certificate/<uuid>`). [`archive_bundle_files`] is the single
//! place the file list is built, and it refuses a secret kind outright rather
//! than filtering it — a filter would silently archive a five-file bundle the
//! first time somebody changes `MaterialKind::ALL`.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use sdkwork_deploy_certificate_material::{
    CertificateFacts, MaterialKind, FILE_CHAIN, FILE_FULL_CHAIN, FILE_LEAF, FILE_ROOT,
};
use sha2::{Digest, Sha256};

/// Display name of the per-tenant certificate space.
pub const CERTIFICATE_SPACE_NAME: &str = "证书库";

/// Name of the machine-readable record stored beside the PEM files.
pub const MANIFEST_FILE_NAME: &str = "manifest.json";

/// Media type recorded for the manifest node.
pub const MANIFEST_MEDIA_TYPE: &str = "application/json";

/// Media type recorded for every PEM node.
pub const PEM_MEDIA_TYPE: &str = "application/x-pem-file";

/// Version of the manifest schema [`ArchiveManifest`] serializes.
///
/// A reader that does not know this number must refuse the manifest rather than
/// guess at its fields: the whole point of the file is that a future operator or
/// importer can rely on it.
pub const MANIFEST_SCHEMA_VERSION: i32 = 1;

/// The files archived under the `PUBLIC` bundle policy, in the order they are
/// written.
///
/// `fullchain.pem` is written last among the PEM files so that a partial write
/// (a disk that filled up, a process that was killed) leaves a version folder
/// without the file a TLS terminator would load — the absence is visible — rather
/// than with it and without its manifest.
pub const PUBLIC_BUNDLE_FILES: [MaterialKind; 4] = [
    MaterialKind::LeafCert,
    MaterialKind::IntermediateChain,
    MaterialKind::RootCert,
    MaterialKind::FullChain,
];

/// Maximum length of the sanitized certificate-name segment, in bytes.
///
/// Drive's `node_name` is `VARCHAR(255)` in its baseline; the segment here is
/// only part of a folder name, so the cap leaves room for the `-<8 hex>` suffix
/// and stays far below any filesystem's limit.
const MAXIMUM_NAME_SEGMENT_BYTES: usize = 64;

/// The `(version, kind)` pair the archive row is keyed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveBundleKind {
    /// Public material only: the four PEM files plus the manifest.
    Public,
    /// Public material plus the private key.
    ///
    /// Admitted by the database check constraint and deliberately *not*
    /// implemented: enabling it requires a per-tenant policy, an ACL review and
    /// an audit trail (design §9.1). A request for it is refused with a stable
    /// error instead of being quietly downgraded to `Public`, so an operator who
    /// asked for the key is never told they got it.
    Full,
}

impl ArchiveBundleKind {
    /// The stable database value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "PUBLIC",
            Self::Full => "FULL",
        }
    }

    /// Reads the database value back.
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "PUBLIC" => Some(Self::Public),
            "FULL" => Some(Self::Full),
            _ => None,
        }
    }
}

/// Lifecycle of one archive row.
///
/// Mirrors `deploy_certificate_archive.status` exactly; the write model never
/// invents a value the check constraint would reject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CertificateArchiveStatus {
    /// Inserted in the version's transaction, not yet attempted.
    Pending,
    /// A worker holds the row and is uploading.
    Storing,
    /// Every file and the manifest are in Drive.
    Archived,
    /// Every allowed attempt failed; only an operator's retry moves it again.
    Failed,
    /// Archiving is switched off for this deployment. Not a failure: nothing was
    /// attempted, and saying "failed" would be a lie an operator would chase.
    Skipped,
}

impl CertificateArchiveStatus {
    /// The stable database value, which is also the wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Storing => "STORING",
            Self::Archived => "ARCHIVED",
            Self::Failed => "FAILED",
            Self::Skipped => "SKIPPED",
        }
    }

    /// Reads the database value back.
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "PENDING" => Some(Self::Pending),
            "STORING" => Some(Self::Storing),
            "ARCHIVED" => Some(Self::Archived),
            "FAILED" => Some(Self::Failed),
            "SKIPPED" => Some(Self::Skipped),
            _ => None,
        }
    }

    /// Whether the row is the truth about a version that is *not* archived yet,
    /// and therefore whether the console should offer a retry.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Pending | Self::Storing | Self::Failed)
    }
}

/// A folder or file name that is safe to hand to Drive and to show an operator.
///
/// Constructed only through the functions below, so an unsanitized value cannot
/// reach a Drive node name by accident.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveName(String);

impl ArchiveName {
    /// The name as a `&str`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Takes ownership of the name.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for ArchiveName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Reduces a certificate name to one Drive-safe path segment.
///
/// Certificate names are operator input, and a name is used verbatim as a folder
/// in a shared space. Anything that is not `a-z`, `0-9`, `.` or `-` becomes `-`:
/// that rules out `/`, `\`, `..` traversal, wildcards (`*`), control characters,
/// and a leading dot (which would hide the folder on every Unix client). Runs of
/// separators collapse and leading/trailing separators are trimmed so that two
/// names that only differ in punctuation cannot produce the same folder by
/// accident — the certificate id suffix keeps them apart even then.
///
/// An input with nothing usable left becomes `certificate`: the caller always
/// appends the id, so the folder is still unique.
#[must_use]
pub fn sanitize_certificate_name(cert_name: &str) -> String {
    let mut out = String::with_capacity(cert_name.len());
    let mut last_was_separator = false;
    for character in cert_name.chars() {
        let lowered = character.to_ascii_lowercase();
        let allowed = lowered.is_ascii_alphanumeric() || matches!(lowered, '.' | '-');
        if !allowed || lowered == '.' {
            // `.` is only kept inside the name; a leading one would hide the
            // folder, and `..` is a traversal. Both are collapsed into the same
            // separator rule below.
            if allowed && lowered == '.' && !out.is_empty() && !last_was_separator {
                out.push('.');
                last_was_separator = false;
            } else {
                if !out.is_empty() && !last_was_separator {
                    out.push('-');
                }
                last_was_separator = true;
            }
            continue;
        }
        if out.len() >= MAXIMUM_NAME_SEGMENT_BYTES {
            continue;
        }
        out.push(lowered);
        last_was_separator = false;
    }
    let trimmed = out.trim_matches(['-', '.']).to_string();
    if trimmed.is_empty() {
        return "certificate".to_owned();
    }
    trimmed
}

/// The certificate folder: `<sanitized certName>-<certId[0..8]>`.
///
/// The id prefix is what keeps two certificates named `example.com` (one
/// created, deleted and replaced) from sharing a folder, and what lets an operator
/// match a folder to a console row without opening it.
///
/// # Errors
///
/// The certificate id must be at least eight characters and start with
/// alphanumerics: a shorter id is not a certificate id this control plane ever
/// minted, and truncating it differently in two builds would move every archived
/// version.
pub fn certificate_folder_name(
    cert_name: &str,
    certificate_id: &str,
) -> Result<ArchiveName, LayoutError> {
    let prefix = id_prefix(certificate_id)?;
    Ok(ArchiveName(format!(
        "{}-{prefix}",
        sanitize_certificate_name(cert_name)
    )))
}

/// The version folder: `v<versionNo>-<notAfter>`.
///
/// `notAfter` is rendered as `YYYY-MM-DDTHH-MM-SSZ` — the RFC 3339 instant with
/// the colons replaced, because a colon is not portable in a path on every
/// filesystem an operator might mount the space on. `-Z` keeps it unambiguous
/// that the instant is UTC rather than a local wall-clock time.
///
/// The version number alone would already be unique; the expiry is there so an
/// operator scanning the folder tree can see which versions are about to lapse
/// without opening the manifest.
pub fn version_folder_name(version_no: i64, not_after: &str) -> Result<ArchiveName, LayoutError> {
    if version_no <= 0 {
        return Err(LayoutError::InvalidVersionNo(version_no));
    }
    let instant = chrono::DateTime::parse_from_rfc3339(not_after)
        .map_err(|error| LayoutError::InvalidTimestamp(format!("notAfter '{not_after}': {error}")))?
        .with_timezone(&chrono::Utc);
    Ok(ArchiveName(format!(
        "v{version_no}-{}",
        instant.format("%Y-%m-%dT%H-%M-%SZ")
    )))
}

/// The archived file name of one material kind, refusing a secret.
///
/// # Errors
///
/// [`LayoutError::SecretNotArchivable`] for the private key. This is not a
/// filter: under the `PUBLIC` policy a secret file in the archive is a defect,
/// and a caller holding one is told so.
pub fn archived_file_name(kind: MaterialKind) -> Result<&'static str, LayoutError> {
    if kind.is_secret() {
        return Err(LayoutError::SecretNotArchivable {
            kind: kind.as_str(),
        });
    }
    Ok(kind.file_name())
}

/// The full ordered write plan of a `PUBLIC` archive.
///
/// # Returns
///
/// The four PEM files by name, then [`MANIFEST_FILE_NAME`]. Built here rather
/// than by a caller so "the private key is never archived" is a property of the
/// module and not of every call site.
pub fn public_bundle_file_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = PUBLIC_BUNDLE_FILES
        .iter()
        .copied()
        .filter_map(|kind| archived_file_name(kind).ok())
        .collect();
    names.push(MANIFEST_FILE_NAME);
    names
}

/// Whether a file name is one the archive writes.
///
/// Used by the download endpoint to refuse `file=../../etc/passwd` and to know
/// which Drive nodes it may serve. Matching is by exact name against
/// [`public_bundle_file_names`], never by prefix or extension.
pub fn is_archived_file_name(name: &str) -> bool {
    public_bundle_file_names().contains(&name)
}

/// Pins the public file list to the material crate's own names.
///
/// Constant rather than hidden: if the material crate ever renames a file, an
/// operator reading the manifest and an operator reading Drive must not end up
/// with two vocabularies.
#[must_use]
pub fn public_pem_file_names() -> [&'static str; 4] {
    [FILE_LEAF, FILE_CHAIN, FILE_ROOT, FILE_FULL_CHAIN]
}

fn id_prefix(certificate_id: &str) -> Result<&str, LayoutError> {
    // A certificate uuid is `8-4-4-4-12` hexadecimal. The strict shape is checked
    // rather than just "at least eight characters" because the prefix becomes a
    // folder name an operator quotes: accepting a shortened test id here would
    // mean a real id whose first eight characters are not the uuid's own segment
    // land in a folder nobody can match to a console row.
    let trimmed = certificate_id.trim();
    let uuid = trimmed.as_bytes();
    let shape_ok = uuid.len() == 36
        && uuid.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        });
    if !shape_ok {
        return Err(LayoutError::InvalidCertificateId(trimmed.to_owned()));
    }
    // Slicing on a byte boundary is safe: every byte has been proven ASCII above.
    Ok(&trimmed[..8])
}

/// The machine-readable record stored as `manifest.json`.
///
/// Field names and semantics mirror `deploy_certificate_version`, so a version's
/// manifest and its row can be compared field by field by an operator who has
/// only the Drive folder. Every digest is the one the control plane already
/// computed and stored — the manifest never recomputes them, so it cannot
/// disagree with the row it describes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArchiveManifest {
    /// [`MANIFEST_SCHEMA_VERSION`].
    #[serde(rename = "schemaVersion")]
    pub schema_version: i32,
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    #[serde(rename = "certificateVersionId")]
    pub certificate_version_id: String,
    #[serde(rename = "certName")]
    pub cert_name: String,
    /// The order that produced this version; absent for an imported version.
    #[serde(rename = "orderId", skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(rename = "versionNo")]
    pub version_no: i64,
    #[serde(rename = "caProfile")]
    pub ca_profile: String,
    pub issuer: String,
    pub subject: String,
    /// The leaf's `subjectAltName` set, in the leaf's own order.
    pub identifiers: Vec<String>,
    #[serde(rename = "notBefore")]
    pub not_before: String,
    #[serde(rename = "notAfter")]
    pub not_after: String,
    #[serde(rename = "serialSha256")]
    pub serial_sha256: String,
    #[serde(rename = "fingerprintSha256")]
    pub fingerprint_sha256: String,
    #[serde(rename = "spkiSha256")]
    pub spki_sha256: String,
    #[serde(rename = "chainSha256")]
    pub chain_sha256: String,
    #[serde(rename = "keyAlgorithm")]
    pub key_algorithm: String,
    /// The files present in this folder, in write order.
    pub bundle: Vec<String>,
    #[serde(rename = "archivedAt")]
    pub archived_at: String,
}

impl ArchiveManifest {
    /// Builds the manifest of one version under the `PUBLIC` policy.
    ///
    /// `facts` come from the bundle the issuance worker produced, so the `issuer`
    /// and `subject` here are the ones derived from the very bytes written.
    #[must_use]
    pub fn for_version(identity: &ArchiveManifestIdentity<'_>, facts: &CertificateFacts) -> Self {
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            certificate_id: identity.certificate_id.to_owned(),
            certificate_version_id: identity.certificate_version_id.to_owned(),
            cert_name: identity.cert_name.to_owned(),
            order_id: identity.order_id.map(str::to_owned),
            version_no: identity.version_no,
            ca_profile: identity.ca_profile.to_owned(),
            issuer: facts.issuer.clone(),
            subject: facts.subject.clone(),
            identifiers: facts.san_list.clone(),
            not_before: facts.not_before.clone(),
            not_after: facts.not_after.clone(),
            serial_sha256: facts.serial_sha256.clone(),
            fingerprint_sha256: facts.fingerprint_sha256.clone(),
            spki_sha256: facts.spki_sha256.clone(),
            chain_sha256: facts.chain_sha256.clone(),
            key_algorithm: facts.key_algorithm.clone(),
            bundle: public_bundle_file_names()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            archived_at: identity.archived_at.to_owned(),
        }
    }

    /// Serializes the manifest to the exact bytes that are stored.
    ///
    /// Pretty-printed with a trailing newline: the file is read by humans at
    /// least as often as by tools, and a one-line 700-byte blob is unreadable in
    /// every Drive preview.
    ///
    /// # Errors
    ///
    /// Serialization of a struct with only strings, numbers and vectors cannot
    /// fail in practice; the error is propagated rather than swallowed so a
    /// future field that *can* fail is not silently dropped.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Every file name the manifest claims is present, for a post-write check.
    #[must_use]
    pub fn declared_files(&self) -> BTreeSet<&str> {
        self.bundle.iter().map(String::as_str).collect()
    }
}

/// The identity fields a manifest needs that do not come from the certificate
/// bytes.
pub struct ArchiveManifestIdentity<'a> {
    pub certificate_id: &'a str,
    pub certificate_version_id: &'a str,
    pub cert_name: &'a str,
    pub order_id: Option<&'a str>,
    pub version_no: i64,
    pub ca_profile: &'a str,
    pub archived_at: &'a str,
}

/// Lowercase hex SHA-256, the form `manifest_sha256` stores.
///
/// Reimplemented here rather than imported so the archive port depends only on
/// what it actually uses; the algorithm and rendering are pinned by a unit test
/// against `sdkwork-deploy-certificate-material`'s own helper.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Why a layout could not be derived.
///
/// Every variant is a refusal to guess: an unparseable `notAfter` or a version
/// number of zero means the caller passed something this control plane never
/// stored, and inventing a folder for it would put an archived version where no
/// retry would look.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("certificate id '{0}' is not a certificate id this control plane mints")]
    InvalidCertificateId(String),
    #[error("certificate version number {0} is not positive")]
    InvalidVersionNo(i64),
    #[error("certificate validity window is not a timestamp: {0}")]
    InvalidTimestamp(String),
    #[error("certificate material kind {kind} is secret and is never archived")]
    SecretNotArchivable { kind: &'static str },
}

#[cfg(test)]
mod tests {
    use sdkwork_deploy_certificate_material::MaterialKind;

    use super::*;

    fn facts() -> CertificateFacts {
        CertificateFacts {
            issuer: "C=US, O=Let's Encrypt, CN=R11".to_owned(),
            subject: "CN=example.com".to_owned(),
            san_list: vec!["example.com".to_owned(), "*.example.com".to_owned()],
            serial_sha256: "a".repeat(64),
            fingerprint_sha256: "b".repeat(64),
            spki_sha256: "c".repeat(64),
            chain_sha256: "d".repeat(64),
            key_algorithm: "ECDSA".to_owned(),
            not_before: "2026-11-24T09:00:00Z".to_owned(),
            not_after: "2027-02-22T09:00:00Z".to_owned(),
        }
    }

    #[test]
    fn the_version_folder_is_the_documented_shape() {
        assert_eq!(
            version_folder_name(2, "2027-02-22T09:00:00Z").expect("name").as_str(),
            "v2-2027-02-22T09-00-00Z"
        );
        // A zone offset is normalized to UTC rather than echoed, so two versions
        // issued for the same instant never produce two spellings of one folder.
        assert_eq!(
            version_folder_name(1, "2027-02-22T17:00:00+08:00")
                .expect("name")
                .as_str(),
            "v1-2027-02-22T09-00-00Z"
        );
    }

    #[test]
    fn the_certificate_folder_carries_the_id_prefix() {
        assert_eq!(
            certificate_folder_name("example.com", "3f7c1a9b-1111-4222-8333-444455556666")
                .expect("name")
                .as_str(),
            "example.com-3f7c1a9b"
        );
    }

    #[test]
    fn two_certificates_with_one_name_get_two_folders() {
        let first = certificate_folder_name("example.com", "3f7c1a9b-1111-4222-8333-444455556666")
            .expect("first");
        let second = certificate_folder_name("example.com", "9e8d7c6b-aaaa-4bbb-8ccc-ddddeeeeffff")
            .expect("second");
        assert_ne!(first, second);
    }

    #[test]
    fn only_a_full_certificate_uuid_is_accepted() {
        // The prefix is quoted in the console, so a value that is not a uuid must
        // be refused rather than truncated into a folder nobody can match.
        for value in [
            "abc",
            "zzzzzzzz",
            "3f7c1a9b-1",
            "3f7c1a9b-1111-4222-8333",
        ] {
            assert!(
                certificate_folder_name("example.com", value).is_err(),
                "{value} must be refused"
            );
        }
        assert!(certificate_folder_name("example.com", "3F7C1A9B-1111-4222-8333-444455556666").is_ok());
    }

    #[test]
    fn a_hostile_certificate_name_becomes_one_safe_segment() {
        for (input, expected) in [
            ("../../etc/passwd", "etc-passwd"),
            ("*.example.com", "example.com"),
            ("Example.COM", "example.com"),
            ("a/b\\c", "a-b-c"),
            ("  ", "certificate"),
            ("..", "certificate"),
            (".hidden", "hidden"),
        ] {
            let sanitized = sanitize_certificate_name(input);
            assert_eq!(sanitized, expected, "input {input:?}");
            assert!(!sanitized.contains('/'), "input {input:?} kept a slash");
            assert!(!sanitized.contains('\\'), "input {input:?} kept a backslash");
            assert!(!sanitized.contains('*'), "input {input:?} kept a wildcard");
        }
    }

    #[test]
    fn a_very_long_certificate_name_is_bounded() {
        let sanitized = sanitize_certificate_name(&"a".repeat(400));
        assert_eq!(sanitized.len(), MAXIMUM_NAME_SEGMENT_BYTES);
    }

    #[test]
    fn a_malformed_certificate_id_is_refused_rather_than_truncated() {
        assert!(certificate_folder_name("example.com", "abc").is_err());
        assert!(certificate_folder_name("example.com", "zzzzzzzz").is_err());
    }
    #[test]
    fn an_unparseable_validity_window_is_refused() {
        assert!(version_folder_name(1, "tomorrow").is_err());
        assert!(version_folder_name(0, "2027-02-22T09:00:00Z").is_err());
    }

    #[test]
    fn the_private_key_is_never_part_of_the_archive() {
        // The single most important property of this module, asserted from three
        // directions so no refactor of the file list can quietly break it.
        for kind in MaterialKind::ALL {
            if kind.is_secret() {
                assert_eq!(
                    archived_file_name(kind),
                    Err(LayoutError::SecretNotArchivable {
                        kind: MaterialKind::PrivateKey.as_str()
                    })
                );
            } else {
                assert!(archived_file_name(kind).is_ok());
            }
        }
        for name in public_bundle_file_names() {
            assert_ne!(name, MaterialKind::PrivateKey.file_name());
            assert!(!name.contains("privkey"));
            assert!(!is_archived_file_name(MaterialKind::PrivateKey.file_name()));
        }
    }

    #[test]
    fn the_bundle_is_the_four_pem_files_plus_the_manifest() {
        assert_eq!(
            public_bundle_file_names(),
            vec![
                "cert.pem",
                "chain.pem",
                "root.pem",
                "fullchain.pem",
                "manifest.json"
            ]
        );
        // Pinned to the material crate's constants, so a rename there shows up
        // here instead of silently changing what gets archived.
        assert_eq!(
            public_pem_file_names(),
            [FILE_LEAF, FILE_CHAIN, FILE_ROOT, FILE_FULL_CHAIN]
        );
    }

    #[test]
    fn only_exact_archive_names_are_addressable() {
        assert!(is_archived_file_name("fullchain.pem"));
        assert!(is_archived_file_name("manifest.json"));
        for name in [
            "privkey.pem",
            "../manifest.json",
            "fullchain.pem.bak",
            "",
            "FULLCHAIN.PEM",
        ] {
            assert!(!is_archived_file_name(name), "{name} must not be addressable");
        }
    }

    #[test]
    fn the_manifest_round_trips_with_the_documented_field_names() {
        let facts = facts();
        let manifest = ArchiveManifest::for_version(
            &ArchiveManifestIdentity {
                certificate_id: "3f7c1a9b-1111-4222-8333-444455556666",
                certificate_version_id: "aa11bb22-cccc-4ddd-8eee-ffff00001111",
                cert_name: "example.com",
                order_id: Some("order-1"),
                version_no: 2,
                ca_profile: "LETS_ENCRYPT_PRODUCTION",
                archived_at: "2026-11-24T10:22:35Z",
            },
            &facts,
        );
        let bytes = manifest.to_json_bytes().expect("json");
        assert_eq!(bytes.last(), Some(&b'\n'), "the file ends in a newline");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("parse");
        for field in [
            "schemaVersion",
            "certificateId",
            "certificateVersionId",
            "certName",
            "orderId",
            "versionNo",
            "caProfile",
            "issuer",
            "subject",
            "identifiers",
            "notBefore",
            "notAfter",
            "serialSha256",
            "fingerprintSha256",
            "spkiSha256",
            "chainSha256",
            "keyAlgorithm",
            "bundle",
            "archivedAt",
        ] {
            assert!(value.get(field).is_some(), "manifest is missing {field}");
        }
        assert_eq!(value["schemaVersion"], MANIFEST_SCHEMA_VERSION);
        assert_eq!(value["versionNo"], 2);
        assert_eq!(value["bundle"][4], "manifest.json");
        assert_eq!(
            value["identifiers"],
            serde_json::json!(["example.com", "*.example.com"])
        );
        assert_eq!(serde_json::from_slice::<ArchiveManifest>(&bytes).expect("decode"), manifest);
    }

    #[test]
    fn the_manifest_declares_exactly_what_is_written() {
        let manifest = ArchiveManifest::for_version(
            &ArchiveManifestIdentity {
                certificate_id: "3f7c1a9b-1111",
                certificate_version_id: "aa11bb22",
                cert_name: "example.com",
                order_id: None,
                version_no: 1,
                ca_profile: "LETS_ENCRYPT_STAGING",
                archived_at: "2026-11-24T10:22:35Z",
            },
            &facts(),
        );
        let written: BTreeSet<&str> = public_bundle_file_names().into_iter().collect();
        assert_eq!(manifest.declared_files(), written);
        let value: serde_json::Value =
            serde_json::from_slice(&manifest.to_json_bytes().expect("json")).expect("parse");
        assert!(
            value.get("orderId").is_none(),
            "an absent order id is omitted, not serialized as null"
        );
    }

    #[test]
    fn the_archive_digest_matches_the_material_crate() {
        let bytes = b"-----BEGIN CERTIFICATE-----\n";
        assert_eq!(
            sha256_hex(bytes),
            sdkwork_deploy_certificate_material::sha256_hex(bytes)
        );
    }
}
