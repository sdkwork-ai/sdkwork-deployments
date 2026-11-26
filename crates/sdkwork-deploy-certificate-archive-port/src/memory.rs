//! Test double: the same contract, in process memory.
//!
//! Records what was written so a test can assert the layout, the file list and
//! the "no private key" property without a database. Instances are cheap and
//! independent; the clone shares state, which is what a worker and an assertion
//! in the same test need.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sdkwork_deploy_contract::{DeployServiceError, DeployServiceResult};

use crate::layout::{certificate_folder_name, sha256_hex, version_folder_name};
use crate::{
    ArchivedCertificateVersion, CertificateArchiveStore, StoreCertificateVersionArchive,
};

/// One file as the double recorded it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryArchivedFile {
    pub folder_node_id: String,
    pub file_name: String,
    pub content: Vec<u8>,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordedArchive {
    archived: ArchivedCertificateVersion,
    files: Vec<MemoryArchivedFile>,
}

#[derive(Clone, Default)]
pub struct MemoryCertificateArchiveStore {
    configured: bool,
    recorded: Arc<Mutex<Vec<RecordedArchive>>>,
    /// When set, `store_version` fails with this detail, so a test can prove the
    /// caller keeps issuance successful through a Drive outage.
    failure: Option<Arc<String>>,
}

impl MemoryCertificateArchiveStore {
    /// A double that reports itself configured.
    #[must_use]
    pub fn configured() -> Self {
        Self {
            configured: true,
            ..Self::default()
        }
    }

    /// A double that reports archiving switched off.
    #[must_use]
    pub fn unconfigured() -> Self {
        Self::default()
    }

    /// A store that fails every write with `detail`.
    #[must_use]
    pub fn failing(detail: impl Into<String>) -> Self {
        Self {
            configured: true,
            failure: Some(Arc::new(detail.into())),
            ..Self::default()
        }
    }

    /// Every file written so far, in write order.
    ///
    /// # Errors
    ///
    /// [`DeployServiceError::Internal`] if another thread panicked while holding
    /// the recording lock.
    pub fn files(&self) -> DeployServiceResult<Vec<MemoryArchivedFile>> {
        Ok(self
            .lock()?
            .iter()
            .flat_map(|archive| archive.files.iter().cloned())
            .collect())
    }

    /// Every archive recorded so far.
    ///
    /// # Errors
    ///
    /// [`DeployServiceError::Internal`] if another thread panicked while holding
    /// the recording lock.
    pub fn archived_versions(&self) -> DeployServiceResult<Vec<ArchivedCertificateVersion>> {
        Ok(self
            .lock()?
            .iter()
            .map(|archive| archive.archived.clone())
            .collect())
    }

    fn lock(
        &self,
    ) -> DeployServiceResult<std::sync::MutexGuard<'_, Vec<RecordedArchive>>> {
        self.recorded.lock().map_err(|_| {
            DeployServiceError::Internal("memory certificate archive poisoned".to_owned())
        })
    }
}

#[async_trait]
impl CertificateArchiveStore for MemoryCertificateArchiveStore {
    fn is_configured(&self) -> bool {
        self.configured
    }

    async fn store_version(
        &self,
        request: StoreCertificateVersionArchive,
    ) -> DeployServiceResult<ArchivedCertificateVersion> {
        if let Some(detail) = self.failure.as_deref() {
            return Err(DeployServiceError::Internal(detail.to_string()));
        }
        if !self.configured {
            return Err(DeployServiceError::Internal(
                "certificate archiving is not configured".to_owned(),
            ));
        }
        let certificate_folder = certificate_folder_name(
            &request.cert_name,
            &request.certificate_id,
        )
        .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        let version_folder = version_folder_name(request.version_no, &request.facts.not_after)
            .map_err(|error| DeployServiceError::Internal(error.to_string()))?;
        let space_id = format!("mem-certificate-space-{}", request.scope.tenant_id);
        let certificate_folder_node_id = format!("mem-node-{certificate_folder}");
        let version_folder_node_id = format!("mem-node-{certificate_folder}/{version_folder}");
        let mut manifest_node_id = String::new();
        let mut manifest_sha256 = String::new();
        let mut files = Vec::with_capacity(request.files.len());
        for file in request.files {
            let sha256 = sha256_hex(&file.content);
            let node_id = format!("{version_folder_node_id}/{}", file.file_name);
            if file.file_name == crate::layout::MANIFEST_FILE_NAME {
                manifest_node_id.clone_from(&node_id);
                manifest_sha256.clone_from(&sha256);
            }
            files.push(MemoryArchivedFile {
                folder_node_id: version_folder_node_id.clone(),
                file_name: file.file_name,
                content: file.content,
                sha256,
            });
        }
        if manifest_node_id.is_empty() {
            return Err(DeployServiceError::Internal(
                "certificate archive plan has no manifest".to_owned(),
            ));
        }
        let archived = ArchivedCertificateVersion {
            space_id,
            certificate_folder_node_id,
            version_folder_node_id,
            manifest_node_id,
            manifest_sha256,
            certificate_folder_name: certificate_folder,
            version_folder_name: version_folder,
        };
        let mut recorded = self.lock()?;
        // The real adapter and the unique constraint both make a second write for
        // one version a no-op; the double has to behave the same way or a retry
        // test would pass here and fail in production.
        if let Some(existing) = recorded
            .iter()
            .find(|archive| {
                archive.archived.certificate_folder_node_id
                    == archived.certificate_folder_node_id
                    && archive.archived.version_folder_node_id == archived.version_folder_node_id
            })
            .cloned()
        {
            return Ok(existing.archived);
        }
        recorded.push(RecordedArchive {
            archived: archived.clone(),
            files,
        });
        Ok(archived)
    }
}

#[cfg(test)]
mod tests {
    use sdkwork_deploy_certificate_material::MaterialKind;

    use super::*;
    use crate::bundle_policy::public_bundle_files;
    use crate::layout::ArchiveBundleKind;
    use crate::CertificateArchiveScope;

    fn request() -> StoreCertificateVersionArchive {
        let files = MaterialKind::ALL
            .into_iter()
            .map(|kind| sdkwork_deploy_certificate_material::CertificateFile {
                kind,
                content: kind.as_str().as_bytes().to_vec(),
            })
            .collect::<Vec<_>>();
        let files = public_bundle_files(ArchiveBundleKind::Public, &files, b"{}\n").expect("plan");
        StoreCertificateVersionArchive {
            scope: CertificateArchiveScope { tenant_id: 7 },
            bundle_kind: ArchiveBundleKind::Public,
            cert_name: "example.com".to_owned(),
            certificate_id: "3f7c1a9b-1111-4222-8333-444455556666".to_owned(),
            certificate_version_id: "aa11bb22-cccc-4ddd-8eee-ffff00001111".to_owned(),
            version_no: 2,
            ca_profile: "LETS_ENCRYPT_PRODUCTION".to_owned(),
            order_id: None,
            facts: sdkwork_deploy_certificate_material::CertificateFacts {
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
            },
            files,
        }
    }

    #[tokio::test]
    async fn a_store_writes_the_documented_folder_and_never_the_private_key() {
        let store = MemoryCertificateArchiveStore::configured();
        let archived = store.store_version(request()).await.expect("store");
        assert_eq!(
            archived.certificate_folder_name.as_str(),
            "example.com-3f7c1a9b"
        );
        assert_eq!(archived.version_folder_name.as_str(), "v2-2027-02-22T09-00-00Z");
        assert!(archived.version_folder_node_id.ends_with("/v2-2027-02-22T09-00-00Z"));
        assert!(archived.manifest_node_id.ends_with("/manifest.json"));
        let names: Vec<String> = store
            .files()
            .expect("files")
            .into_iter()
            .map(|file| file.file_name)
            .collect();
        assert_eq!(
            names,
            vec![
                "cert.pem",
                "chain.pem",
                "root.pem",
                "fullchain.pem",
                "manifest.json"
            ]
        );
    }

    #[tokio::test]
    async fn a_second_write_for_one_version_does_not_duplicate_it() {
        let store = MemoryCertificateArchiveStore::configured();
        let first = store.store_version(request()).await.expect("first");
        let second = store.store_version(request()).await.expect("second");
        assert_eq!(first, second);
        assert_eq!(store.archived_versions().expect("versions").len(), 1);
        assert_eq!(store.files().expect("files").len(), 5);
    }

    #[tokio::test]
    async fn a_failing_store_reports_the_failure_the_caller_must_survive() {
        let store = MemoryCertificateArchiveStore::failing("drive is down");
        let error = store.store_version(request()).await.expect_err("must fail");
        assert!(format!("{error}").contains("drive is down"));
        assert!(store.files().expect("files").is_empty());
    }

    #[test]
    fn an_unconfigured_store_says_so_without_writing() {
        let store = MemoryCertificateArchiveStore::unconfigured();
        assert!(!store.is_configured());
    }
}
