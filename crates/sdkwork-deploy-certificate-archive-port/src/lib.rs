//! Certificate version archives in sdkwork-drive.
//!
//! When the issuance executor stores a certificate version, the public half of
//! that version is also archived as files in Drive: a tenant should be able to
//! open `证书库/example.com-3f7c1a9b/v2-2027-02-22T09-00-00Z/` and download
//! `fullchain.pem`. The private key stays where it always was, in the sealed
//! custody store, and [`crate::bundle_policy`] is what makes that a property of
//! the type system rather than a promise in a comment.
//!
//! # Three layers, each testable alone
//!
//! | Layer | What it decides | I/O |
//! | --- | --- | --- |
//! | [`layout`] | folder names, file names, `manifest.json` | none |
//! | [`retry`] | when an archive row gets another attempt, and when it stops | none |
//! | [`in_process`] / [`memory`] | how the bytes reach Drive | PostgreSQL |
//!
//! The first two are pure functions with exhaustive unit tests, because they are
//! the parts an operator's console quotes back at them and the parts that must not
//! drift between a first attempt and a retry in another process.
//!
//! # Decoupled from issuance by construction
//!
//! The archive row is written in the version's own transaction, so "a version
//! exists" always implies "an archive intent exists". Everything after that is
//! best effort: a Drive outage leaves the row `PENDING` with a backoff, never
//! rolls back an issuance, and never turns a stored certificate into a failed
//! order. Archiving switched off reports `SKIPPED` — an honest "we did not try"
//! rather than a failure an operator would chase.

pub mod bundle_policy;
pub mod in_process;
pub mod layout;
pub mod memory;
pub mod retry;
pub mod selection;

use async_trait::async_trait;
use sdkwork_deploy_certificate_material::CertificateFacts;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};

use crate::bundle_policy::ArchiveFileContent;
use crate::layout::{ArchiveManifest, ArchiveManifestIdentity, ArchiveName};
pub use in_process::{resolve_archive_folders, InProcessCertificateArchiveStore};
pub use memory::MemoryCertificateArchiveStore;
pub use retry::{
    ArchiveRetryDecision, ArchiveRetryPolicy, RetryPolicyError, DEFAULT_MAX_ATTEMPTS,
    RETRY_BACKOFF_SECONDS,
};
pub use selection::{
    certificate_archive_config_from_env, certificate_archive_store_from_env,
    certificate_archive_store_from_selection, select_certificate_archive, CertificateArchiveConfig,
    CertificateArchiveSelection, CERTIFICATE_ARCHIVE_ATTEMPT_TIMEOUT_ENV,
    CERTIFICATE_ARCHIVE_BUNDLE_ENV, CERTIFICATE_ARCHIVE_ENV,
    CERTIFICATE_ARCHIVE_MAX_ATTEMPTS_ENV, CERTIFICATE_ARCHIVE_SPACE_NAME_ENV,
};

/// Tenant the archive belongs to.
///
/// A bare `i64` rather than the shared `DeployDriveScope`: archiving has no
/// per-request acting subject (it runs after a worker's conversation with the CA,
/// where there may be no human at all), and the Drive identity used for the upload
/// is the control plane's own app identity. Reusing the upload scope would imply an
/// operator is involved, and the first attempt made without one would fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CertificateArchiveScope {
    pub tenant_id: i64,
}

/// Everything one version's archive needs.
pub struct StoreCertificateVersionArchive {
    pub scope: CertificateArchiveScope,
    pub bundle_kind: layout::ArchiveBundleKind,
    /// The certificate's display name, already validated by the contract.
    pub cert_name: String,
    /// Certificate uuid, used for the folder suffix.
    pub certificate_id: String,
    /// Version uuid, used for the idempotency key.
    pub certificate_version_id: String,
    /// The version's sequence number.
    pub version_no: i64,
    /// The CA profile the version was issued under.
    pub ca_profile: String,
    /// The order that produced it, when one did.
    pub order_id: Option<String>,
    /// Facts derived from the issued bytes.
    pub facts: CertificateFacts,
    /// The files to write, from [`bundle_policy::public_bundle_files`].
    pub files: Vec<ArchiveFileContent>,
}

impl StoreCertificateVersionArchive {
    /// Builds the manifest for this archive at `archived_at`.
    ///
    /// # Errors
    ///
    /// Propagates a serialization failure, which means the manifest struct itself
    /// is unusable and the archive must not be attempted.
    pub fn manifest(&self, archived_at: &str) -> Result<ArchiveManifest, DeployServiceError> {
        let manifest = ArchiveManifest::for_version(
            &ArchiveManifestIdentity {
                certificate_id: &self.certificate_id,
                certificate_version_id: &self.certificate_version_id,
                cert_name: &self.cert_name,
                order_id: self.order_id.as_deref(),
                version_no: self.version_no,
                ca_profile: &self.ca_profile,
                archived_at,
            },
            &self.facts,
        );
        // The plan and the manifest must agree before anything is written; a
        // manifest that claims a different file set than the one being uploaded
        // would make the folder internally inconsistent from the first byte.
        let planned: Vec<&str> = self
            .files
            .iter()
            .map(|file| file.file_name.as_str())
            .collect();
        let declared: Vec<&str> = manifest.bundle.iter().map(String::as_str).collect();
        if planned != declared {
            return Err(DeployServiceError::Internal(format!(
                "certificate archive plan {planned:?} does not match the manifest's {declared:?}"
            )));
        }
        Ok(manifest)
    }
}

/// Where one archived version ended up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedCertificateVersion {
    /// Drive space of the tenant's certificate library.
    pub space_id: String,
    /// The per-certificate folder.
    pub certificate_folder_node_id: String,
    /// The per-version folder that holds the files.
    pub version_folder_node_id: String,
    /// The `manifest.json` node, which is what a console links to.
    pub manifest_node_id: String,
    /// SHA-256 of the manifest bytes, recorded on the row so the folder can be
    /// proven to be the one this control plane wrote.
    pub manifest_sha256: String,
    /// The folder names, for display and for logs.
    pub certificate_folder_name: ArchiveName,
    pub version_folder_name: ArchiveName,
}

/// Durable home for certificate version archives.
///
/// Implementations must be idempotent per `certificate_version_id`: a second call
/// for the same version returns the same nodes instead of writing a second copy.
#[async_trait]
pub trait CertificateArchiveStore: Send + Sync {
    /// Whether this deployment can archive at all.
    ///
    /// `false` is what makes an archive row `SKIPPED`, which is a fact and not a
    /// failure. Implementations answer from configuration, never from a live
    /// probe: the value decides a persisted status, so it must not depend on
    /// whether Drive happened to answer just then.
    fn is_configured(&self) -> bool;

    /// Writes every file of one version and answers its Drive location.
    async fn store_version(
        &self,
        request: StoreCertificateVersionArchive,
    ) -> DeployServiceResult<ArchivedCertificateVersion>;
}

#[cfg(test)]
mod tests {
    use sdkwork_deploy_certificate_material::{MaterialKind, CertificateFile};

    use super::*;
    use crate::bundle_policy::public_bundle_files;
    use crate::layout::ArchiveBundleKind;

    fn facts() -> CertificateFacts {
        CertificateFacts {
            issuer: "CN=Test CA".to_owned(),
            subject: "CN=example.com".to_owned(),
            san_list: vec!["example.com".to_owned()],
            serial_sha256: "a".repeat(64),
            fingerprint_sha256: "b".repeat(64),
            spki_sha256: "c".repeat(64),
            chain_sha256: "d".repeat(64),
            key_algorithm: "ECDSA".to_owned(),
            not_before: "2026-11-24T09:00:00Z".to_owned(),
            not_after: "2027-02-22T09:00:00Z".to_owned(),
        }
    }

    fn request(files: Vec<ArchiveFileContent>) -> StoreCertificateVersionArchive {
        StoreCertificateVersionArchive {
            scope: CertificateArchiveScope { tenant_id: 7 },
            bundle_kind: ArchiveBundleKind::Public,
            cert_name: "example.com".to_owned(),
            certificate_id: "3f7c1a9b-1111-4222-8333-444455556666".to_owned(),
            certificate_version_id: "aa11bb22-cccc-4ddd-8eee-ffff00001111".to_owned(),
            version_no: 2,
            ca_profile: "LETS_ENCRYPT_PRODUCTION".to_owned(),
            order_id: Some("order-1".to_owned()),
            facts: facts(),
            files,
        }
    }

    fn bundle_files() -> Vec<CertificateFile> {
        MaterialKind::ALL
            .into_iter()
            .map(|kind| CertificateFile {
                kind,
                content: kind.as_str().as_bytes().to_vec(),
            })
            .collect()
    }

    #[test]
    fn the_manifest_describes_the_plan_it_is_written_with() {
        let manifest_bytes = b"{}\n";
        let files = public_bundle_files(ArchiveBundleKind::Public, &bundle_files(), manifest_bytes)
            .expect("plan");
        let manifest = request(files).manifest("2026-11-24T10:22:35Z").expect("manifest");
        assert_eq!(manifest.certificate_version_id, "aa11bb22-cccc-4ddd-8eee-ffff00001111");
        assert_eq!(manifest.version_no, 2);
        assert_eq!(manifest.declared_files().len(), 5);
    }

    #[test]
    fn a_plan_that_disagrees_with_the_manifest_is_refused_before_writing() {
        let files = public_bundle_files(ArchiveBundleKind::Public, &bundle_files(), b"{}\n")
            .expect("plan")
            .into_iter()
            .filter(|file| file.file_name != "root.pem")
            .collect();
        let error = request(files)
            .manifest("2026-11-24T10:22:35Z")
            .expect_err("must refuse");
        assert!(
            format!("{error}").contains("does not match"),
            "unexpected error: {error}"
        );
    }
}
