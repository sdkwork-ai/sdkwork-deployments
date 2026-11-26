//! Archiving an issued certificate version into sdkwork-drive.
//!
//! This module is the orchestration half of design §4.6: it claims a due archive
//! row, opens the version's sealed material, builds the `PUBLIC` bundle and manifest
//! through [`sdkwork_deploy_certificate_archive_port`], writes it to Drive through
//! the archive store port, and records the result on the row. The layout, the
//! manifest and the retry ladder are port-crate decisions; what lives here is *when*
//! an attempt happens and what a failure is allowed to affect.
//!
//! # The one rule that shapes every function here
//!
//! **An archive failure never fails an issuance.** A certificate that was issued and
//! stored is a delivered certificate; Drive being unreachable is an operations
//! problem, not an issuance failure. So the inline entry point
//! ([`DeployService::archive_certificate_version`]) returns an
//! [`CertificateArchiveAttempt`] and never an error — the caller is
//! `store_issued_version`, whose success is what moves an order to `VERSION_STORED`.
//!
//! # Bounded, like every external await
//!
//! The attempt runs inside `tokio::time::timeout` with the configured ceiling
//! (§4.6.3: 30 s). It shares the issuance worker's lease, so an unbounded attempt
//! could push a worker past its lease and let a second worker re-claim the same
//! order — the duplicate issuance the lease exists to prevent.
//!
//! # Two callers, one row
//!
//! The row is claimed twice in practice: once inline right after the version
//! commits, and again by the backlog drain, which picks up whatever the inline
//! attempt left behind. Both go through `claim_due_certificate_archive`, a
//! conditional update, so the version is uploaded once no matter how many processes
//! are looking at it.

use std::sync::Arc;
use std::time::Duration;

use sdkwork_deploy_certificate_archive_port::bundle_policy::public_bundle_files;
use sdkwork_deploy_certificate_archive_port::layout::ArchiveBundleKind;
use sdkwork_deploy_certificate_archive_port::{
    ArchiveRetryDecision, ArchiveRetryPolicy, CertificateArchiveScope, CertificateArchiveStore,
    StoreCertificateVersionArchive,
};
use sdkwork_deploy_certificate_material::CertificateFacts;
use sdkwork_deploy_contract::{
    CertificateArchiveClaim, CertificateArchiveOutcome, CertificateArchiveRetryResult,
    CertificateArchiveStatus, DeployServiceError, DeployServiceResult,
};

use crate::DeployService;

/// The stable code recorded when Drive refused or failed the write.
pub const CERTIFICATE_ARCHIVE_WRITE_FAILED: &str = "CERTIFICATE_ARCHIVE_WRITE_FAILED";

/// The stable code recorded when the attempt exceeded its ceiling.
pub const CERTIFICATE_ARCHIVE_TIMEOUT: &str = "CERTIFICATE_ARCHIVE_TIMEOUT";

/// The stable code recorded when archiving is switched off on this deployment.
pub const CERTIFICATE_ARCHIVE_DISABLED: &str = "CERTIFICATE_ARCHIVE_DISABLED";

/// What one archive pass did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CertificateArchiveAttempt {
    /// No archive store is installed. The version's row was written `SKIPPED`.
    Disabled,
    /// Nothing was due: another worker holds the row, or its backoff has not run.
    NotDue,
    /// The files and the manifest are in Drive.
    Archived,
    /// This attempt failed; another is scheduled.
    Failed {
        attempt_count: i32,
        next_attempt_in_seconds: i64,
    },
    /// This attempt failed and the budget is spent.
    Exhausted { attempt_count: i32 },
}

impl CertificateArchiveAttempt {
    /// Whether the pass did nothing and should stay out of the log.
    pub fn is_quiet(self) -> bool {
        matches!(self, Self::NotDue | Self::Disabled)
    }

    /// The status a version in this outcome reports afterwards.
    ///
    /// `Disabled` reports `SKIPPED` because that is what the version's row says;
    /// `NotDue` reports `PENDING` because a row nobody could claim is still queued.
    pub fn archive_status(self) -> CertificateArchiveStatus {
        match self {
            Self::Disabled => CertificateArchiveStatus::Skipped,
            Self::NotDue | Self::Failed { .. } => CertificateArchiveStatus::Pending,
            Self::Archived => CertificateArchiveStatus::Archived,
            Self::Exhausted { .. } => CertificateArchiveStatus::Failed,
        }
    }
}

impl DeployService {
    /// Tries to archive one version inline, right after its row was committed.
    ///
    /// Never fails: the issuance that just succeeded must not be undone by an
    /// archive that did not. `certificate_version_uuid` is passed so the claim can
    /// exclude the row this call is about when another worker is draining the
    /// backlog at the same moment — the exclusion is an optimization, not a
    /// correctness requirement, because the claim itself is the fence.
    pub async fn archive_certificate_version(
        &self,
        certificate_version_uuid: &str,
        attempt_timeout: Duration,
    ) -> CertificateArchiveAttempt {
        let Some(store) = self.certificate_archive_store.as_ref().cloned() else {
            return CertificateArchiveAttempt::Disabled;
        };
        let attempt = self
            .claim_and_attempt(store, Some(certificate_version_uuid), attempt_timeout)
            .await;
        match attempt {
            Ok(Some(attempt)) => attempt,
            Ok(None) => CertificateArchiveAttempt::NotDue,
            Err(error) => {
                // Reaching here means the claim itself could not be read or
                // written. The row is still `PENDING` and the backlog drain will
                // find it, so this is a warning rather than a failure.
                tracing::warn!(
                    certificate_version = %certificate_version_uuid,
                    error = %error,
                    "certificate archive attempt could not run; the row stays queued"
                );
                CertificateArchiveAttempt::NotDue
            }
        }
    }

    /// Drains one due archive row, whichever certificate it belongs to.
    ///
    /// This is what the certificate worker calls every tick: the inline attempt
    /// covers the happy path, and this covers every version whose inline attempt
    /// timed out or failed, or whose process died between the commit and the
    /// attempt.
    pub async fn drain_due_certificate_archive(
        &self,
        attempt_timeout: Duration,
    ) -> DeployServiceResult<CertificateArchiveAttempt> {
        let Some(store) = self.certificate_archive_store.as_ref().cloned() else {
            return Ok(CertificateArchiveAttempt::Disabled);
        };
        Ok(self
            .claim_and_attempt(store, None, attempt_timeout)
            .await?
            .unwrap_or(CertificateArchiveAttempt::NotDue))
    }

    /// Claims a due row and attempts it.
    ///
    /// `Ok(None)` means nothing was due.
    async fn claim_and_attempt(
        &self,
        store: Arc<dyn CertificateArchiveStore>,
        exclude_version_uuid: Option<&str>,
        attempt_timeout: Duration,
    ) -> DeployServiceResult<Option<CertificateArchiveAttempt>> {
        let now = now_rfc3339();
        let Some(claim) = self
            .repository
            .claim_due_certificate_archive(&now, exclude_version_uuid)
            .await?
        else {
            return Ok(None);
        };
        let tenant_id = claim.tenant_id;
        let version_uuid = claim.certificate_version_uuid.clone();
        let outcome =
            tokio::time::timeout(attempt_timeout, self.upload_archive(store.as_ref(), &claim))
                .await;
        let attempt = match outcome {
            Ok(Ok(outcome)) => {
                let moved = self
                    .repository
                    .mark_certificate_archive_archived(tenant_id, &version_uuid, &outcome)
                    .await
                    .unwrap_or(false);
                if moved {
                    tracing::info!(
                        certificate = %claim.certificate_uuid,
                        certificate_version = %version_uuid,
                        space = %outcome.space_id,
                        folder = %outcome.version_folder_node_id,
                        "certificate version archived to drive"
                    );
                } else {
                    // Another process moved the row between the claim and this
                    // write. The bytes are in Drive either way; saying "archived"
                    // when the row says otherwise would be a second truth.
                    tracing::info!(
                        certificate_version = %version_uuid,
                        "certificate archive row was settled by another worker"
                    );
                    return Ok(Some(CertificateArchiveAttempt::NotDue));
                }
                CertificateArchiveAttempt::Archived
            }
            Ok(Err(error)) => {
                tracing::warn!(
                    certificate = %claim.certificate_uuid,
                    certificate_version = %version_uuid,
                    error = %error,
                    "certificate archive attempt failed"
                );
                self.record_archive_failure(&claim, CERTIFICATE_ARCHIVE_WRITE_FAILED)
                    .await?
            }
            Err(_elapsed) => {
                tracing::warn!(
                    certificate = %claim.certificate_uuid,
                    certificate_version = %version_uuid,
                    timeout_ms = attempt_timeout.as_millis(),
                    "certificate archive attempt exceeded its ceiling"
                );
                self.record_archive_failure(&claim, CERTIFICATE_ARCHIVE_TIMEOUT)
                    .await?
            }
        };
        Ok(Some(attempt))
    }

    /// Builds and writes one version's `PUBLIC` archive.
    ///
    /// Reads the sealed material exactly once. The manifest is serialized first so
    /// the plan's last entry carries the bytes that will actually be written — a
    /// plan built with a placeholder manifest would store a file that disagrees
    /// with its own digest.
    async fn upload_archive(
        &self,
        store: &dyn CertificateArchiveStore,
        claim: &CertificateArchiveClaim,
    ) -> DeployServiceResult<CertificateArchiveOutcome> {
        // An unreadable kind is treated as the public policy: it is the only one
        // this build writes, and the only one that cannot leak a key.
        let bundle_kind =
            ArchiveBundleKind::from_db(&claim.bundle_kind).unwrap_or(ArchiveBundleKind::Public);
        // The manifest is built from the facts the row already stores rather than
        // recomputed from the bytes: the row is what the read model reports, and a
        // manifest that disagreed with it would be two truths about one version.
        let facts = CertificateFacts {
            issuer: claim.issuer.clone(),
            subject: claim.subject.clone(),
            san_list: claim.identifiers.clone(),
            serial_sha256: claim.serial_sha256.clone(),
            fingerprint_sha256: claim.fingerprint_sha256.clone(),
            spki_sha256: claim.spki_sha256.clone(),
            chain_sha256: claim.chain_sha256.clone(),
            key_algorithm: claim.key_algorithm.clone(),
            not_before: claim.not_before.clone(),
            not_after: claim.not_after.clone(),
        };
        let archived_at = now_rfc3339();
        let manifest =
            sdkwork_deploy_certificate_archive_port::layout::ArchiveManifest::for_version(
                &sdkwork_deploy_certificate_archive_port::layout::ArchiveManifestIdentity {
                    certificate_id: &claim.certificate_uuid,
                    certificate_version_id: &claim.certificate_version_uuid,
                    cert_name: &claim.cert_name,
                    order_id: claim.order_uuid.as_deref(),
                    version_no: claim.version_no,
                    ca_profile: &claim.ca_profile,
                    archived_at: &archived_at,
                },
                &facts,
            );
        let manifest_bytes = manifest.to_json_bytes().map_err(|error| {
            DeployServiceError::Internal(format!(
                "serialize certificate archive manifest failed: {error}"
            ))
        })?;
        // Opening the material is the only place the private key is decrypted.
        // `public_bundle_files` refuses to put a secret kind in the plan, so the
        // key never leaves this scope under the `PUBLIC` policy.
        let files = self
            .open_certificate_material(claim.tenant_id, &claim.certificate_version_uuid)
            .await?;
        let plan = public_bundle_files(bundle_kind, &files, &manifest_bytes).map_err(|error| {
            DeployServiceError::Internal(format!("certificate archive plan rejected: {error}"))
        })?;
        drop(files);
        let archived = store
            .store_version(StoreCertificateVersionArchive {
                scope: CertificateArchiveScope {
                    tenant_id: claim.tenant_id,
                },
                bundle_kind,
                cert_name: claim.cert_name.clone(),
                certificate_id: claim.certificate_uuid.clone(),
                certificate_version_id: claim.certificate_version_uuid.clone(),
                version_no: claim.version_no,
                ca_profile: claim.ca_profile.clone(),
                order_id: claim.order_uuid.clone(),
                facts,
                files: plan,
            })
            .await?;
        Ok(CertificateArchiveOutcome {
            space_id: archived.space_id,
            certificate_folder_node_id: archived.certificate_folder_node_id,
            version_folder_node_id: archived.version_folder_node_id,
            manifest_node_id: archived.manifest_node_id,
            manifest_sha256: archived.manifest_sha256,
        })
    }

    /// Applies the retry ladder to a failed attempt and records it.
    async fn record_archive_failure(
        &self,
        claim: &CertificateArchiveClaim,
        error_code: &str,
    ) -> DeployServiceResult<CertificateArchiveAttempt> {
        // `attempt_count` on the claim is the value before this attempt, so the
        // attempt that just failed is one more than that.
        let attempts = claim.attempt_count.saturating_add(1);
        let decision = self.certificate_archive_retry.after_failure(attempts);
        let (status, next_attempt_at, attempt) = match decision {
            ArchiveRetryDecision::Retry { delay_seconds } => (
                CertificateArchiveStatus::Pending,
                Some(rfc3339_after(delay_seconds)),
                CertificateArchiveAttempt::Failed {
                    attempt_count: attempts,
                    next_attempt_in_seconds: delay_seconds,
                },
            ),
            ArchiveRetryDecision::Exhausted => (
                CertificateArchiveStatus::Failed,
                None,
                CertificateArchiveAttempt::Exhausted {
                    attempt_count: attempts,
                },
            ),
        };
        self.repository
            .record_certificate_archive_failure(
                claim.tenant_id,
                claim.archive_row_id,
                error_code,
                attempts,
                next_attempt_at.as_deref(),
                status,
            )
            .await?;
        if matches!(decision, ArchiveRetryDecision::Exhausted) {
            tracing::warn!(
                certificate = %claim.certificate_uuid,
                certificate_version = %claim.certificate_version_uuid,
                attempts,
                error_code,
                "certificate archive attempts exhausted; an operator retry is required"
            );
        }
        Ok(attempt)
    }

    /// Whether this deployment archives certificate material at all.
    #[must_use]
    pub fn certificate_archive_configured(&self) -> bool {
        self.certificate_archive_store.is_some()
    }

    /// The retry ladder this deployment applies.
    #[must_use]
    pub fn certificate_archive_retry_policy(&self) -> ArchiveRetryPolicy {
        self.certificate_archive_retry
    }

    /// How long the attempt made inside an issuance may take.
    #[must_use]
    pub fn certificate_archive_attempt_timeout(&self) -> Duration {
        Duration::from_millis(self.certificate_archive_attempt_timeout_millis)
    }

    /// Marks one version's archive row `SKIPPED` when nothing will archive it.
    ///
    /// Called from `store_issued_version` right after the version transaction
    /// commits. Best effort: a failure here leaves the row `PENDING`, which is
    /// still true — it is queued and nothing has attempted it.
    pub(crate) async fn skip_certificate_archive_version(
        &self,
        tenant_id: i64,
        certificate_version_uuid: &str,
    ) {
        if let Err(error) = self
            .repository
            .skip_certificate_archive(
                tenant_id,
                certificate_version_uuid,
                CERTIFICATE_ARCHIVE_DISABLED,
            )
            .await
        {
            tracing::warn!(
                certificate_version = %certificate_version_uuid,
                error = %error,
                "could not mark the certificate archive row SKIPPED"
            );
        }
    }

    /// Re-arms the archive of a certificate's served version.
    ///
    /// The console's "retry" command. It does not upload anything itself: it
    /// clears the backoff so the worker's next pass picks the row up, which keeps
    /// one code path doing the writing and keeps an HTTP request from blocking on
    /// an object store.
    pub async fn retry_certificate_archive(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> DeployServiceResult<CertificateArchiveRetryResult> {
        // Confirm the certificate exists for this tenant before touching a row:
        // otherwise a retry for a foreign or deleted certificate would answer
        // `accepted:false`, which reads as "nothing to do" instead of "not yours".
        self.repository
            .retrieve_certificate(tenant_id, certificate_id)
            .await?;
        if !self.certificate_archive_configured() {
            return Ok(CertificateArchiveRetryResult {
                accepted: false,
                archive_status: CertificateArchiveStatus::Skipped,
            });
        }
        let now = now_rfc3339();
        let accepted = self
            .repository
            .retry_certificate_archive(tenant_id, certificate_id, &now)
            .await?;
        // Report the state the certificate is in afterwards rather than the one it
        // is expected to reach: a retry that lost a race with a worker which just
        // finished must not read as "pending" on a certificate whose files are
        // already in Drive.
        let after = self
            .repository
            .retrieve_certificate(tenant_id, certificate_id)
            .await?;
        Ok(CertificateArchiveRetryResult {
            accepted,
            archive_status: after.archive_status,
        })
    }
}

/// The RFC 3339 instant `seconds` from now, in the rendering every stored timestamp
/// here uses.
fn rfc3339_after(seconds: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(seconds))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// "Now", in the RFC 3339 rendering the repository columns expect.
fn now_rfc3339() -> String {
    rfc3339_after(0)
}

/// The manifest file name, re-exported for tests that assert the file list.
pub use sdkwork_deploy_certificate_archive_port::layout::MANIFEST_FILE_NAME as ARCHIVE_MANIFEST_FILE;

#[cfg(test)]
mod tests {
    use sdkwork_deploy_certificate_archive_port::layout::MANIFEST_FILE_NAME;

    use super::*;

    #[test]
    fn the_manifest_name_is_the_one_the_port_writes() {
        assert_eq!(ARCHIVE_MANIFEST_FILE, MANIFEST_FILE_NAME);
        assert_eq!(MANIFEST_FILE_NAME, "manifest.json");
    }

    #[test]
    fn the_retry_instants_are_rfc3339_in_utc() {
        let instant = rfc3339_after(30);
        assert!(chrono::DateTime::parse_from_rfc3339(&instant).is_ok());
        assert!(instant.ends_with('Z'));
    }

    #[test]
    fn a_quiet_pass_is_the_two_states_that_changed_nothing() {
        assert!(CertificateArchiveAttempt::NotDue.is_quiet());
        assert!(CertificateArchiveAttempt::Disabled.is_quiet());
        assert!(!CertificateArchiveAttempt::Archived.is_quiet());
        assert!(!CertificateArchiveAttempt::Failed {
            attempt_count: 1,
            next_attempt_in_seconds: 30
        }
        .is_quiet());
        assert!(!CertificateArchiveAttempt::Exhausted { attempt_count: 5 }.is_quiet());
    }

    #[test]
    fn every_outcome_reports_a_status_the_row_can_hold() {
        // The read model and the row must never disagree about what "this outcome"
        // means, so each variant's reported status is pinned here.
        assert_eq!(
            CertificateArchiveAttempt::Archived.archive_status(),
            CertificateArchiveStatus::Archived
        );
        assert_eq!(
            CertificateArchiveAttempt::Exhausted { attempt_count: 5 }.archive_status(),
            CertificateArchiveStatus::Failed
        );
        assert_eq!(
            CertificateArchiveAttempt::Failed {
                attempt_count: 1,
                next_attempt_in_seconds: 30
            }
            .archive_status(),
            CertificateArchiveStatus::Pending
        );
        assert_eq!(
            CertificateArchiveAttempt::Disabled.archive_status(),
            CertificateArchiveStatus::Skipped
        );
        assert_eq!(
            CertificateArchiveAttempt::NotDue.archive_status(),
            CertificateArchiveStatus::Pending
        );
    }
}
