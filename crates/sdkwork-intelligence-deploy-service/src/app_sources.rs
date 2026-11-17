//! Remix source and remix-state service surface.
//!
//! The division of labour is deliberate:
//!
//! * **this module** turns the wire request into a validated, normalized
//!   [`UpdateAppSourceCommand`] (provider, URL, ref, sub-path, clone mode) and
//!   enforces the one precondition the feature has — remix may only be enabled
//!   when the source is ready;
//! * **`sdkwork_deploy_core::app_source`** owns the rules themselves (what a valid
//!   repository URL, ref or sub-path is, and what "ready" means), so the same
//!   functions can be unit-tested without a database and cannot drift between the
//!   write path and the read path;
//! * **the repository port** stores the selection and derives readiness from the
//!   referenced rows every time it is read.
//!
//! # Why the download is not generated here
//!
//! A remix client either clones a Git repository or downloads an uploaded archive.
//! This service does neither: it has no git binary and no tarball pipeline, and
//! inventing one would duplicate what the build runner already owns. Instead the
//! remix endpoint returns the *instructions* — repository URL + ref + sub-path for
//! Git, or the artifact's Drive node identity + checksum for an archive — and the
//! client performs the transfer. That is also why `AppRemixDownloadResponse` has no
//! signed URL: deploy does not hold one, and a fabricated one would be a lie.

use sdkwork_deploy_contract::{
    AppRemixDownloadResponse, AppRemixResponse, AppSourceArchiveResponse, AppSourceGitResponse,
    AppSourceKind, AppSourceResponse, DeployAppRequestContext, DeployServiceError,
    DeployServiceResult, UpdateAppRemixRequest, UpdateAppSourceRequest,
};
use sdkwork_deploy_core::{
    git_repository_validation_record, is_supported_clone_mode, is_supported_repo_provider,
    normalize_app_source_sub_path, normalize_source_ref, plan_git_repository_probe,
    remix_can_be_enabled, validate_source_repo_url, AppSourceReadiness,
    GitRepositoryProbeObservation, GitRepositoryProbePlan, MAXIMUM_SOURCE_REPO_URL_LENGTH,
};

use crate::git_repository_probe::GitRepositoryProbeResult;
use crate::repository::{
    GitRepositoryValidationWrite, UpdateAppSourceCommand, UpdateAppSourceGitCommand,
};
use crate::DeployService;

const DEFAULT_CLONE_MODE: &str = "SHALLOW";
const MAXIMUM_CREDENTIAL_SECRET_REF_LENGTH: usize = 512;

impl DeployService {
    /// Reads the configured source and its derived readiness.
    ///
    /// A read: no source configured is `MISSING`, not a `404`. The application
    /// itself must be reachable, which is what `assert_app_access` proves.
    pub async fn retrieve_app_source(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppSourceResponse> {
        let tenant_id = Self::tenant_id(context)?;
        Self::assert_caller_app_scope(context, app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository.retrieve_app_source(tenant_id, app_id).await
    }

    /// Creates or replaces the source.
    ///
    /// Replacing the source never changes `remix_enabled`: the write is validated
    /// to produce a *structurally* usable source (a supported provider and
    /// well-formed URL for Git; an active, application-attributed artifact for an
    /// archive), and a replacement cannot leave remix pointing at nothing.
    ///
    /// A Git write no longer claims the repository is reachable: it lands in
    /// `PENDING`, and only [`Self::validate_app_source`] can move it to
    /// `VALIDATED`. Until then readiness is `MISSING` with
    /// `GIT_REPOSITORY_PENDING`, and remix cannot be enabled. A source that later
    /// becomes unusable — an artifact retired out from under it, a repository
    /// re-probed as `INVALID` — degrades the same way, which the read path reports
    /// and the download path refuses.
    pub async fn update_app_source(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &UpdateAppSourceRequest,
    ) -> DeployServiceResult<AppSourceResponse> {
        let tenant_id = Self::tenant_id(context)?;
        Self::assert_caller_app_scope(context, app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        let command = build_update_app_source_command(request)?;
        let source = self
            .repository
            .update_app_source(tenant_id, context.actor_id, app_id, &command)
            .await?;
        self.audit_app_action(context, "app.source.update", app_id)
            .await?;
        Ok(source)
    }

    /// Probes the configured Git repository and records the verdict.
    ///
    /// This is the only writer that can produce `VALIDATED`, which is why the
    /// write path above deliberately leaves `PENDING`: "the declaration is
    /// well-formed" and "the repository is reachable" are different claims, and
    /// only this operation makes the second one.
    ///
    /// What it can and cannot establish:
    ///
    /// * an `http(s)` URL is probed once, bounded (5 s, ≤ 5 redirects, status code
    ///   only). `2xx`/`3xx` → `VALIDATED`; `401`/`403` → `VALIDATED` with
    ///   `GIT_VALIDATION_CREDENTIALS_REQUIRED` (reachable, but the clone will need
    ///   the credential reference); `404`/`410`/`5xx`/timeout/transport failure →
    ///   `INVALID` with the matching code;
    /// * a `git@host:path` remote cannot be probed without an SSH credential, so
    ///   it stays `PENDING` with `GIT_VALIDATION_UNSUPPORTED_SCHEME`. It is never
    ///   written `VALIDATED`: this service does not know that repository exists;
    /// * with no probe installed the row stays `PENDING` with
    ///   `GIT_VALIDATION_PROBE_UNAVAILABLE`.
    ///
    /// Reachable is not clonable: the probe proves the host answers for the URL,
    /// not that the ref, the sub-path or the credential are right.
    pub async fn validate_app_source(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppSourceResponse> {
        let tenant_id = Self::tenant_id(context)?;
        Self::assert_caller_app_scope(context, app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        let source = self
            .repository
            .retrieve_app_source(tenant_id, app_id)
            .await?;
        let repo_url = git_repository_url_for_validation(&source)?.to_owned();

        // Probe only when the URL shape allows it *and* a probe is installed; every
        // other combination is decided by the core rule, so the "SSH stays PENDING"
        // and "no probe stays PENDING" outcomes cannot diverge from the read path.
        let probe_result = match plan_git_repository_probe(&repo_url) {
            GitRepositoryProbePlan::HttpProbe => match self.git_repository_probe.as_ref() {
                Some(probe) => Some(probe.probe(&repo_url).await),
                None => None,
            },
            GitRepositoryProbePlan::RequiresManualConfirmation
            | GitRepositoryProbePlan::UnusableUrl => None,
        };
        let observation = probe_result.as_ref().map(probe_observation);
        let record = git_repository_validation_record(&repo_url, observation);
        let write = GitRepositoryValidationWrite {
            repo_status: record.status,
            last_error_code: record.error_code,
        };
        let source = self
            .repository
            .record_git_repository_validation(tenant_id, context.actor_id, app_id, &write)
            .await?;
        self.audit_app_action(context, "app.source.validate", app_id)
            .await?;
        Ok(source)
    }

    /// Retires the source and turns remix off.
    pub async fn delete_app_source(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<()> {
        let tenant_id = Self::tenant_id(context)?;
        Self::assert_caller_app_scope(context, app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        self.repository
            .delete_app_source(tenant_id, context.actor_id, app_id)
            .await?;
        self.audit_app_action(context, "app.source.delete", app_id)
            .await?;
        Ok(())
    }

    /// Reads the remix state, plus the download instructions when it is usable.
    pub async fn retrieve_app_remix(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
    ) -> DeployServiceResult<AppRemixResponse> {
        let tenant_id = Self::tenant_id(context)?;
        Self::assert_caller_app_scope(context, app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;
        let source = self
            .repository
            .retrieve_app_source(tenant_id, app_id)
            .await?;
        Ok(remix_response_from_source(&source))
    }

    /// Enables or disables remix.
    ///
    /// Enabling is refused with `409` unless the source is ready *right now*: a
    /// flag that claims remix while the source cannot be downloaded is a promise
    /// the next request breaks, and the operator would only discover it after the
    /// console offered the action. Disabling is unconditional — an operator must
    /// always be able to switch remix off, including (especially) when the source
    /// has become unusable.
    pub async fn update_app_remix(
        &self,
        context: &DeployAppRequestContext,
        app_id: &str,
        request: &UpdateAppRemixRequest,
    ) -> DeployServiceResult<AppRemixResponse> {
        let tenant_id = Self::tenant_id(context)?;
        Self::assert_caller_app_scope(context, app_id)?;
        self.assert_app_access(context, tenant_id, app_id).await?;

        let source = self
            .repository
            .retrieve_app_source(tenant_id, app_id)
            .await?;
        // Re-sending the state already in force is a no-op, not a transition: the
        // precondition below guards *changing* the flag, so a replay of
        // `enabled: true` for an already-enabled application still answers 200 even
        // if the source has degraded since it was enabled.
        if source.remix_enabled == request.enabled {
            return Ok(remix_response_from_source(&source));
        }
        if request.enabled {
            ensure_remix_can_be_enabled(&source)?;
        }
        self.repository
            .update_app_remix_enabled(tenant_id, context.actor_id, app_id, request.enabled)
            .await?;
        self.audit_app_action(context, "app.remix.update", app_id)
            .await?;
        let source = self
            .repository
            .retrieve_app_source(tenant_id, app_id)
            .await?;
        Ok(remix_response_from_source(&source))
    }
}

/// Refuses the enable transition unless the source is ready.
///
/// Split out so the refusal — its `409` kind and the reason code it carries — is
/// testable without a repository, which is where the interesting cases live
/// (`MISSING` with every reason).
pub(crate) fn ensure_remix_can_be_enabled(source: &AppSourceResponse) -> DeployServiceResult<()> {
    if remix_can_be_enabled(source.readiness) {
        return Ok(());
    }
    let reason = source
        .readiness_reason
        .as_deref()
        .unwrap_or("SOURCE_NOT_CONFIGURED");
    Err(DeployServiceError::conflict(format!(
        "remix cannot be enabled: the application source is {} ({reason})",
        source.readiness.as_str()
    )))
}

/// Resolves the repository URL one validation attempt should probe.
///
/// Split out because the three refusals have three different meanings and the
/// tests pin them without a database: no source at all is `404` (nothing exists
/// to validate), an archive source is `422` (the operation does not apply to it),
/// and a Git pointer with no repository facts is an internal inconsistency rather
/// than anything the caller did.
fn git_repository_url_for_validation(source: &AppSourceResponse) -> DeployServiceResult<&str> {
    match source.source_kind {
        None => Err(DeployServiceError::not_found(
            "the application has no remix source to validate",
        )),
        Some(AppSourceKind::Archive) => Err(DeployServiceError::validation(
            "the application source is an archive; only a Git source can be validated",
        )),
        Some(AppSourceKind::Git) => source
            .git
            .as_ref()
            .map(|git| git.repo_url.as_str())
            .ok_or_else(|| {
                DeployServiceError::Internal(
                    "the Git source is missing its repository facts".to_owned(),
                )
            }),
    }
}

/// Narrows a transport observation to the vocabulary the core rule interprets.
///
/// Kept as a function so the mapping is testable and so the transport enum never
/// leaks into `sdkwork_deploy_core`.
fn probe_observation(result: &GitRepositoryProbeResult) -> GitRepositoryProbeObservation<'_> {
    match result {
        GitRepositoryProbeResult::Status(status) => GitRepositoryProbeObservation::Status(*status),
        GitRepositoryProbeResult::TimedOut => GitRepositoryProbeObservation::TimedOut,
        GitRepositoryProbeResult::NetworkFailure(detail) => {
            GitRepositoryProbeObservation::NetworkFailure(detail)
        }
    }
}

/// Projects the source resource onto the remix resource.
///
/// `download` is present only when the flag is on **and** the source is ready.
/// Emitting it whenever a source exists would make a disabled application look
/// downloadable; omitting it only for an unusable source would hide the reason,
/// which is why the readiness fields travel alongside.
fn remix_response_from_source(source: &AppSourceResponse) -> AppRemixResponse {
    AppRemixResponse {
        app_id: source.app_id.clone(),
        remix_enabled: source.remix_enabled,
        source_kind: source.source_kind,
        readiness: source.readiness,
        readiness_reason: source.readiness_reason.clone(),
        download: download_for(
            source.source_kind,
            source.remix_enabled,
            source.readiness,
            source.git.clone(),
            source.archive.clone(),
        ),
        updated_at: source.updated_at.clone(),
        version: source.version.clone(),
    }
}

/// Turns the wire request into the normalized command the port accepts.
///
/// Everything this function can decide is decided here, once: the provider is
/// supported, the URL is an accepted shape, the ref and sub-path are normalized,
/// the clone mode is defaulted, and the "exactly one half" pairing is enforced. The
/// repository therefore never has to answer "is this valid?" — only "does the
/// referenced row exist and belong to this application?".
pub(crate) fn build_update_app_source_command(
    request: &UpdateAppSourceRequest,
) -> DeployServiceResult<UpdateAppSourceCommand> {
    match request.kind {
        AppSourceKind::Git => {
            if request.archive.is_some() {
                return Err(DeployServiceError::validation(
                    "archive must be absent when kind is GIT",
                ));
            }
            let git = request.git.as_ref().ok_or_else(|| {
                DeployServiceError::validation("git is required when kind is GIT")
            })?;
            let repo_provider = git.repo_provider.trim().to_ascii_uppercase();
            if !is_supported_repo_provider(&repo_provider) {
                return Err(DeployServiceError::validation(
                    "repoProvider must be GITHUB, GITEE, GITLAB or SELF_HOSTED",
                ));
            }
            let repo_url = git.repo_url.trim();
            if repo_url.len() > MAXIMUM_SOURCE_REPO_URL_LENGTH {
                return Err(DeployServiceError::validation(
                    "repoUrl must be 1..=1000 characters",
                ));
            }
            validate_source_repo_url(repo_url).map_err(DeployServiceError::validation)?;
            let repo_ref = normalize_source_ref(git.repo_ref.as_deref())
                .map_err(|error| DeployServiceError::validation(error.as_str()))?;
            let sub_path = normalize_app_source_sub_path(git.sub_path.as_deref())
                .map_err(|error| DeployServiceError::validation(error.as_str()))?;
            let clone_mode = git
                .clone_mode
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_ascii_uppercase)
                .unwrap_or_else(|| DEFAULT_CLONE_MODE.to_owned());
            if !is_supported_clone_mode(&clone_mode) {
                return Err(DeployServiceError::validation(
                    "cloneMode must be FULL or SHALLOW",
                ));
            }
            let credential_secret_ref = git
                .credential_secret_ref
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            if let Some(reference) = credential_secret_ref.as_deref() {
                if reference.len() > MAXIMUM_CREDENTIAL_SECRET_REF_LENGTH {
                    return Err(DeployServiceError::validation(
                        "credentialSecretRef must be at most 512 characters",
                    ));
                }
            }
            Ok(UpdateAppSourceCommand {
                kind: AppSourceKind::Git,
                git: Some(UpdateAppSourceGitCommand {
                    repo_provider,
                    repo_url: repo_url.to_owned(),
                    repo_ref,
                    sub_path,
                    credential_secret_ref,
                    clone_mode,
                }),
                archive_artifact_id: None,
            })
        }
        AppSourceKind::Archive => {
            if request.git.is_some() {
                return Err(DeployServiceError::validation(
                    "git must be absent when kind is ARCHIVE",
                ));
            }
            let archive = request.archive.as_ref().ok_or_else(|| {
                DeployServiceError::validation("archive is required when kind is ARCHIVE")
            })?;
            let artifact_id = archive.artifact_id.trim();
            if artifact_id.is_empty() {
                return Err(DeployServiceError::validation("artifactId is required"));
            }
            Ok(UpdateAppSourceCommand {
                kind: AppSourceKind::Archive,
                git: None,
                archive_artifact_id: Some(artifact_id.to_owned()),
            })
        }
    }
}

/// Rebuilds the download descriptor from a stored source response.
///
/// Used by the repository when it answers the remix read, so the two endpoints
/// cannot disagree about whether a download is offered.
pub(crate) fn download_for(
    source_kind: Option<AppSourceKind>,
    remix_enabled: bool,
    readiness: AppSourceReadiness,
    git: Option<AppSourceGitResponse>,
    archive: Option<AppSourceArchiveResponse>,
) -> Option<AppRemixDownloadResponse> {
    if !remix_enabled || readiness != AppSourceReadiness::Ready {
        return None;
    }
    Some(AppRemixDownloadResponse {
        mode: source_kind?,
        git,
        archive,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_deploy_contract::{
        AppSourceArchiveInput, AppSourceGitInput, DeployServiceErrorKind,
    };

    fn git_request(git: AppSourceGitInput) -> UpdateAppSourceRequest {
        UpdateAppSourceRequest {
            kind: AppSourceKind::Git,
            git: Some(git),
            archive: None,
        }
    }

    fn git_input() -> AppSourceGitInput {
        AppSourceGitInput {
            repo_provider: "GITHUB".to_owned(),
            repo_url: "https://github.com/sdkwork/app.git".to_owned(),
            repo_ref: None,
            sub_path: None,
            credential_secret_ref: None,
            clone_mode: None,
        }
    }

    fn source(readiness: AppSourceReadiness, reason: &str) -> AppSourceResponse {
        AppSourceResponse {
            app_id: "11111111-1111-4111-8111-111111111111".to_owned(),
            remix_enabled: false,
            source_kind: Some(AppSourceKind::Git),
            readiness,
            readiness_reason: Some(reason.to_owned()),
            git: None,
            archive: None,
            updated_at: None,
            version: None,
        }
    }

    #[test]
    fn a_git_source_is_defaulted_and_normalized() {
        let mut git = git_input();
        git.sub_path = Some("./apps/web/".to_owned());
        git.repo_ref = Some(" release/1.4 ".to_owned());
        let command = build_update_app_source_command(&git_request(git)).expect("valid git source");
        assert_eq!(command.kind, AppSourceKind::Git);
        assert!(command.archive_artifact_id.is_none());
        let git = command.git.expect("git half");
        assert_eq!(git.repo_provider, "GITHUB");
        assert_eq!(git.sub_path.as_deref(), Some("apps/web"));
        assert_eq!(git.repo_ref.as_deref(), Some("release/1.4"));
        assert_eq!(git.clone_mode, "SHALLOW");
    }

    #[test]
    fn a_git_source_without_a_sub_path_is_the_repository_root() {
        let command =
            build_update_app_source_command(&git_request(git_input())).expect("valid git source");
        assert!(command.git.expect("git half").sub_path.is_none());
    }

    #[test]
    fn an_archive_source_carries_only_the_artifact() {
        let request = UpdateAppSourceRequest {
            kind: AppSourceKind::Archive,
            git: None,
            archive: Some(AppSourceArchiveInput {
                artifact_id: "artifact-uuid".to_owned(),
            }),
        };
        let command = build_update_app_source_command(&request).expect("valid archive source");
        assert_eq!(command.kind, AppSourceKind::Archive);
        assert!(command.git.is_none());
        assert_eq!(
            command.archive_artifact_id.as_deref(),
            Some("artifact-uuid")
        );
    }

    #[test]
    fn a_source_must_carry_exactly_the_half_its_kind_names() {
        let both = UpdateAppSourceRequest {
            kind: AppSourceKind::Archive,
            git: Some(git_input()),
            archive: None,
        };
        assert_eq!(
            build_update_app_source_command(&both)
                .expect_err("git half on an archive source")
                .kind(),
            DeployServiceErrorKind::Validation
        );

        let neither = UpdateAppSourceRequest {
            kind: AppSourceKind::Git,
            git: None,
            archive: None,
        };
        assert_eq!(
            build_update_app_source_command(&neither)
                .expect_err("git kind without a git half")
                .kind(),
            DeployServiceErrorKind::Validation
        );
    }

    #[test]
    fn an_unsupported_provider_or_scheme_is_refused() {
        let mut provider = git_input();
        provider.repo_provider = "BITBUCKET".to_owned();
        assert_eq!(
            build_update_app_source_command(&git_request(provider))
                .expect_err("unsupported provider")
                .kind(),
            DeployServiceErrorKind::Validation
        );

        let mut url = git_input();
        url.repo_url = "ssh://github.com/sdkwork/app.git".to_owned();
        assert_eq!(
            build_update_app_source_command(&git_request(url))
                .expect_err("unsupported scheme")
                .kind(),
            DeployServiceErrorKind::Validation
        );
    }

    #[test]
    fn a_sub_path_that_escapes_the_checkout_is_refused() {
        let mut git = git_input();
        git.sub_path = Some("../secret".to_owned());
        assert_eq!(
            build_update_app_source_command(&git_request(git))
                .expect_err("traversal")
                .kind(),
            DeployServiceErrorKind::Validation
        );
    }

    #[test]
    fn enabling_remix_requires_a_ready_source() {
        let error = ensure_remix_can_be_enabled(&source(
            AppSourceReadiness::Missing,
            "SOURCE_NOT_CONFIGURED",
        ))
        .expect_err("a missing source cannot enable remix");
        assert_eq!(error.kind(), DeployServiceErrorKind::Conflict);
        assert!(
            error.to_string().contains("SOURCE_NOT_CONFIGURED"),
            "the refusal must name the readiness reason: {error}"
        );

        ensure_remix_can_be_enabled(&source(AppSourceReadiness::Ready, ""))
            .expect("a ready source may enable remix");
    }

    #[test]
    fn the_download_is_offered_only_when_enabled_and_ready() {
        let git = AppSourceGitResponse {
            repo_url: "https://github.com/sdkwork/app.git".to_owned(),
            repo_ref: "main".to_owned(),
            ..AppSourceGitResponse::default()
        };
        assert!(download_for(
            Some(AppSourceKind::Git),
            true,
            AppSourceReadiness::Ready,
            Some(git.clone()),
            None
        )
        .is_some());
        assert!(download_for(
            Some(AppSourceKind::Git),
            false,
            AppSourceReadiness::Ready,
            Some(git.clone()),
            None
        )
        .is_none());
        assert!(download_for(
            Some(AppSourceKind::Git),
            true,
            AppSourceReadiness::Missing,
            Some(git),
            None
        )
        .is_none());
    }

    fn git_response() -> AppSourceGitResponse {
        AppSourceGitResponse {
            repo_url: "https://github.com/sdkwork/app.git".to_owned(),
            repo_ref: "main".to_owned(),
            repo_status: "PENDING".to_owned(),
            ..AppSourceGitResponse::default()
        }
    }

    fn source_of_kind(
        kind: Option<AppSourceKind>,
        git: Option<AppSourceGitResponse>,
    ) -> AppSourceResponse {
        AppSourceResponse {
            source_kind: kind,
            git,
            ..source(AppSourceReadiness::Missing, "SOURCE_NOT_CONFIGURED")
        }
    }

    #[test]
    fn validating_a_git_source_answers_its_repository_url() {
        let source = source_of_kind(Some(AppSourceKind::Git), Some(git_response()));
        assert_eq!(
            git_repository_url_for_validation(&source).expect("a Git source is validatable"),
            "https://github.com/sdkwork/app.git"
        );
    }

    #[test]
    fn validating_without_a_source_is_not_found() {
        let source = source_of_kind(None, None);
        assert_eq!(
            git_repository_url_for_validation(&source)
                .expect_err("nothing to validate")
                .kind(),
            DeployServiceErrorKind::NotFound
        );
    }

    #[test]
    fn validating_an_archive_source_is_a_validation_error() {
        let source = source_of_kind(Some(AppSourceKind::Archive), None);
        assert_eq!(
            git_repository_url_for_validation(&source)
                .expect_err("an archive is not a repository")
                .kind(),
            DeployServiceErrorKind::Validation
        );
    }

    #[test]
    fn a_git_source_without_its_repository_facts_is_an_internal_inconsistency() {
        let source = source_of_kind(Some(AppSourceKind::Git), None);
        assert_eq!(
            git_repository_url_for_validation(&source)
                .expect_err("a Git pointer without a repository is inconsistent")
                .kind(),
            DeployServiceErrorKind::Internal
        );
    }

    #[test]
    fn the_transport_observation_keeps_the_status_and_the_failure_kind() {
        assert_eq!(
            probe_observation(&GitRepositoryProbeResult::Status(401)),
            GitRepositoryProbeObservation::Status(401)
        );
        assert_eq!(
            probe_observation(&GitRepositoryProbeResult::TimedOut),
            GitRepositoryProbeObservation::TimedOut
        );
        assert_eq!(
            probe_observation(&GitRepositoryProbeResult::NetworkFailure(
                "dns lookup failed".to_owned()
            )),
            GitRepositoryProbeObservation::NetworkFailure("dns lookup failed")
        );
    }
}
