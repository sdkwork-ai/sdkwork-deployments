//! The `PUBLIC` bundle policy: which material kinds may be archived, and the
//! bytes that are handed to the store.
//!
//! The one rule of this module is the one the design calls out twice (§4.6.1 and
//! §9.1): **the private key is never archived**. Enforcing that here — at the point
//! the content vector is built — means no store implementation can receive the key
//! bytes even by mistake, because they are not in the plan at all.

use sdkwork_deploy_certificate_material::{CertificateFile, MaterialKind};

use crate::layout::{archived_file_name, ArchiveBundleKind, LayoutError, MANIFEST_MEDIA_TYPE};

/// One file of an archive: the name Drive records and the bytes to write.
///
/// The bytes are owned here rather than borrowed so a caller can hold a plan
/// while reading rows in a transaction. Under `PUBLIC` they are all public
/// material, so nothing secret is retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveFileContent {
    pub file_name: String,
    pub content: Vec<u8>,
    pub media_type: String,
}

/// Builds the `PUBLIC` write plan from an assembled bundle.
///
/// Files arrive in [`MaterialKind::ALL`] order from the material crate; this keeps
/// the archive's own order and appends the manifest last.
///
/// # Errors
///
/// * [`ArchiveBundleError::PrivateKeyPolicy`] if the requested policy is `FULL`.
/// * [`ArchiveBundleError::MissingFile`] when the bundle has no bytes for a kind
///   the archive is supposed to contain. An empty `chain.pem` is legitimate (a
///   self-signed development certificate has no intermediates) and is written as
///   an empty file; a *missing* row is not, because the manifest would then claim
///   a file nobody wrote.
/// * [`ArchiveBundleError::Layout`] if a file name cannot be derived, which in
///   practice means somebody tried to archive a secret.
pub fn public_bundle_files(
    bundle_kind: ArchiveBundleKind,
    files: &[CertificateFile],
    manifest: &[u8],
) -> Result<Vec<ArchiveFileContent>, ArchiveBundleError> {
    if bundle_kind != ArchiveBundleKind::Public {
        return Err(ArchiveBundleError::PrivateKeyPolicy);
    }
    let mut plan = Vec::with_capacity(crate::layout::PUBLIC_BUNDLE_FILES.len() + 1);
    for kind in crate::layout::PUBLIC_BUNDLE_FILES {
        let file =
            files
                .iter()
                .find(|file| file.kind == kind)
                .ok_or(ArchiveBundleError::MissingFile {
                    kind: kind.as_str(),
                })?;
        plan.push(ArchiveFileContent {
            file_name: archived_file_name(kind)?.to_owned(),
            content: file.content.clone(),
            media_type: kind.media_type().to_owned(),
        });
    }
    plan.push(ArchiveFileContent {
        file_name: crate::layout::MANIFEST_FILE_NAME.to_owned(),
        content: manifest.to_vec(),
        media_type: MANIFEST_MEDIA_TYPE.to_owned(),
    });
    Ok(plan)
}

/// Why a bundle could not be turned into an archive plan.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ArchiveBundleError {
    /// The `FULL` policy is not implemented, by design.
    #[error(
        "archiving the private key (bundle FULL) is not enabled on this deployment; \
         the certificate files are archived under the PUBLIC policy only"
    )]
    PrivateKeyPolicy,
    #[error("the issued bundle has no {kind} file to archive")]
    MissingFile { kind: &'static str },
    #[error(transparent)]
    Layout(#[from] LayoutError),
}

/// A refusal, spelled out for the operator.
///
/// Kept separate from [`ArchiveBundleError`] so the wording lives in one place and
/// the API layer can pass it through as the problem detail.
#[must_use]
pub fn private_key_refusal_detail() -> &'static str {
    "CERTIFICATE_ARCHIVE_PRIVATE_KEY_NOT_SUPPORTED: certificate archives contain the public \
     certificate files only; the private key stays in the sealed certificate store"
}

/// The material kinds a `PUBLIC` archive writes, for documentation and tests.
#[must_use]
pub fn archived_material_kinds() -> [MaterialKind; 4] {
    crate::layout::PUBLIC_BUNDLE_FILES
}

#[cfg(test)]
mod tests {
    use sdkwork_deploy_certificate_material::CertificateFile;

    use super::*;

    fn bundle() -> Vec<CertificateFile> {
        MaterialKind::ALL
            .into_iter()
            .map(|kind| CertificateFile {
                kind,
                content: format!("{} bytes", kind.as_str()).into_bytes(),
            })
            .collect()
    }

    #[test]
    fn the_private_key_never_reaches_the_plan() {
        let plan =
            public_bundle_files(ArchiveBundleKind::Public, &bundle(), b"{}\n").expect("plan");
        let names: Vec<&str> = plan.iter().map(|file| file.file_name.as_str()).collect();
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
        for file in &plan {
            assert!(
                !file.content.starts_with(b"PRIVATE_KEY"),
                "{} carries private key bytes",
                file.file_name
            );
        }
    }

    #[test]
    fn the_full_policy_is_refused_instead_of_downgraded() {
        // A caller who asked for the key must not be told they got it, and must
        // not silently receive a PUBLIC archive either: both are lies about a
        // security decision.
        assert_eq!(
            public_bundle_files(ArchiveBundleKind::Full, &bundle(), b"{}\n"),
            Err(ArchiveBundleError::PrivateKeyPolicy)
        );
    }

    #[test]
    fn a_missing_file_is_refused() {
        let files: Vec<CertificateFile> = bundle()
            .into_iter()
            .filter(|file| file.kind != MaterialKind::RootCert)
            .collect();
        let error = public_bundle_files(ArchiveBundleKind::Public, &files, b"{}\n")
            .expect_err("must refuse");
        assert_eq!(
            error,
            ArchiveBundleError::MissingFile {
                kind: MaterialKind::RootCert.as_str()
            }
        );
    }

    #[test]
    fn an_empty_intermediate_chain_is_written_rather_than_skipped() {
        // A self-signed development certificate genuinely has no intermediates.
        // The file must exist and be empty, or the manifest claims a file that is
        // not there.
        let mut files = bundle();
        for file in &mut files {
            if file.kind == MaterialKind::IntermediateChain {
                file.content.clear();
            }
        }
        let plan = public_bundle_files(ArchiveBundleKind::Public, &files, b"{}\n").expect("plan");
        let chain = plan
            .iter()
            .find(|file| file.file_name == "chain.pem")
            .expect("chain.pem");
        assert!(chain.content.is_empty());
    }

    #[test]
    fn the_manifest_is_last_and_typed_as_json() {
        let plan = public_bundle_files(ArchiveBundleKind::Public, &bundle(), b"{}").expect("plan");
        let last = plan.last().expect("manifest");
        assert_eq!(last.file_name, "manifest.json");
        assert_eq!(last.media_type, MANIFEST_MEDIA_TYPE);
        assert!(plan[..plan.len() - 1]
            .iter()
            .all(|file| file.media_type == "application/x-pem-file"));
    }

    #[test]
    fn the_refusal_detail_names_the_private_key() {
        assert!(private_key_refusal_detail().contains("private key"));
        assert_eq!(
            archived_material_kinds(),
            [
                MaterialKind::LeafCert,
                MaterialKind::IntermediateChain,
                MaterialKind::RootCert,
                MaterialKind::FullChain
            ]
        );
    }
}
