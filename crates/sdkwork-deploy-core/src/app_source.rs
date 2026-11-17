//! Remix source rules for a deployment application.
//!
//! "Remix" means: download the application's **source** and rebuild it locally.
//! Two facts decide whether that is possible, and both live here as pure rules so
//! the HTTP layer, the service layer and the repository layer cannot disagree:
//!
//! 1. **Which source** the application remixes. Exactly one of
//!    [`AppSourceKind::Git`] or [`AppSourceKind::Archive`]; a Git source may live
//!    in a directory below the repository root ([`normalize_app_source_sub_path`]).
//! 2. **Whether that source is usable today** — [`evaluate_app_source_readiness`].
//!
//! # Readiness is derived, never stored
//!
//! `READY` is a statement about the referenced rows *now*: a repository that is
//! still `PENDING`, an artifact that was retired, or a path that no longer
//! satisfies the rules makes a previously-ready source unusable. A stored status
//! column would keep saying `READY` until something remembered to refresh it, so
//! the schema stores only the selection and this module derives the verdict from
//! the referenced row's current state on every read.
//!
//! # Why the reason is a closed set
//!
//! The console has to tell the operator *why* remix is refused, and a free-text
//! message cannot be branched on. [`AppSourceReadinessReason`] is the closed set
//! of causes this module can prove, rendered on the wire as a stable code.

use serde::{Deserialize, Serialize};

/// The two source kinds a remix can download.
///
/// A closed set rather than a string: the kind decides which pointer column is
/// required (`deploy_app_source.git_repository_id` or `archive_artifact_id`) and
/// which download instructions the API returns. A third value would be a schema
/// change, and this enum is where that shows up first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppSourceKind {
    /// A Git repository, optionally narrowed to a directory inside it.
    #[default]
    Git,
    /// An uploaded archive (zip/tar) stored as a deploy artifact.
    Archive,
}

impl AppSourceKind {
    pub const ALL: [Self; 2] = [Self::Git, Self::Archive];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Git => "GIT",
            Self::Archive => "ARCHIVE",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }
}

/// Whether the application's source can be downloaded.
///
/// Defaults to `Missing`: a response assembled by accident must never claim that a
/// source is downloadable. The only way to produce `Ready` is
/// [`evaluate_app_source_readiness`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppSourceReadiness {
    /// The selected source exists and is usable: remix may be enabled/downloaded.
    Ready,
    /// No source is configured, or the configured one cannot be used right now.
    #[default]
    Missing,
}

impl AppSourceReadiness {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Missing => "MISSING",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "READY" => Some(Self::Ready),
            "MISSING" => Some(Self::Missing),
            _ => None,
        }
    }
}

/// Why a source is (not) `READY`.
///
/// `Ready` carries no reason; every `MISSING` verdict carries exactly one, which
/// is what lets the console replace an opaque "source missing" with an actionable
/// sentence ("the repository has not been validated yet").
///
/// The three Git repository states are kept apart on purpose, because they ask
/// the operator for three different actions: `PENDING` asks for
/// `apps.source.validate`, `INVALID` asks for a fix first (the failing detail is
/// `git.lastErrorCode`), and `NOT_VALIDATED` covers a status this module does not
/// produce itself (`ARCHIVED`, or a row written by an older deployment) where no
/// probe has ever run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppSourceReadinessReason {
    /// No source row has been written for the application.
    NotConfigured,
    /// The row names a kind but carries no matching pointer.
    KindMismatch,
    /// The referenced repository row was soft-deleted.
    GitRepositoryUnusable,
    /// The repository provider is not one this module supports.
    GitProviderUnsupported,
    /// The repository URL is empty, too long, or uses an unsupported scheme.
    GitRepositoryUrlInvalid,
    /// The repository is registered but has never been validated.
    GitRepositoryNotValidated,
    /// The repository is waiting for `apps.source.validate` to run.
    GitRepositoryPending,
    /// The last validation attempt failed; the recorded error code says why.
    GitRepositoryInvalid,
    /// The recorded ref is not a usable Git ref.
    GitReferenceInvalid,
    /// The recorded sub-path is not a normalized repository-relative directory.
    GitSubPathInvalid,
    /// The referenced archive artifact is missing or retired.
    ArchiveUnavailable,
}

impl AppSourceReadinessReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "SOURCE_NOT_CONFIGURED",
            Self::KindMismatch => "SOURCE_KIND_MISMATCH",
            Self::GitRepositoryUnusable => "GIT_REPOSITORY_UNUSABLE",
            Self::GitProviderUnsupported => "GIT_PROVIDER_UNSUPPORTED",
            Self::GitRepositoryUrlInvalid => "GIT_REPOSITORY_URL_INVALID",
            Self::GitRepositoryNotValidated => "GIT_REPOSITORY_NOT_VALIDATED",
            Self::GitRepositoryPending => "GIT_REPOSITORY_PENDING",
            Self::GitRepositoryInvalid => "GIT_REPOSITORY_INVALID",
            Self::GitReferenceInvalid => "GIT_REFERENCE_INVALID",
            Self::GitSubPathInvalid => "GIT_SUB_PATH_INVALID",
            Self::ArchiveUnavailable => "ARCHIVE_UNAVAILABLE",
        }
    }
}

/// The repository facts readiness is decided from.
///
/// Borrowed rather than owned: the caller already holds the row it just read, and
/// copying the strings would let the verdict describe a different snapshot than
/// the one the response renders.
#[derive(Clone, Copy, Debug)]
pub struct AppSourceGitFacts<'a> {
    pub repo_provider: &'a str,
    pub repo_url: &'a str,
    /// `deploy_source_repository.repo_status` (`PENDING` / `VALIDATED` / …).
    pub repo_status: &'a str,
    /// Whether the repository row is soft-deleted. A soft-deleted repository is
    /// still reachable through the foreign key, so the caller has to report it.
    pub repo_deleted: bool,
    /// Recorded ref; `None` means the repository's default branch.
    pub source_ref: Option<&'a str>,
    /// Repository-relative directory; `None` means the repository root.
    pub sub_path: Option<&'a str>,
}

/// The archive facts readiness is decided from.
///
/// `artifact_active` rather than the raw integer: the artifact status vocabulary
/// belongs to the artifact contract, and this rule only needs the boolean it
/// collapses to ("still downloadable").
#[derive(Clone, Copy, Debug)]
pub struct AppSourceArchiveFacts {
    pub artifact_active: bool,
}

/// One configured source, as much of it as readiness needs.
#[derive(Clone, Copy, Debug)]
pub struct AppSourceFacts<'a> {
    pub kind: AppSourceKind,
    pub git: Option<AppSourceGitFacts<'a>>,
    pub archive: Option<AppSourceArchiveFacts>,
}

/// The verdict plus the code that explains it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppSourceReadinessOutcome {
    pub readiness: AppSourceReadiness,
    pub reason: Option<AppSourceReadinessReason>,
}

impl AppSourceReadinessOutcome {
    /// `true` when remix may be enabled or downloaded: the readiness predicates
    /// exist so this question is answered in one place.
    pub const fn is_ready(self) -> bool {
        matches!(self.readiness, AppSourceReadiness::Ready)
    }
}

const READY: AppSourceReadinessOutcome = AppSourceReadinessOutcome {
    readiness: AppSourceReadiness::Ready,
    reason: None,
};

const fn missing(reason: AppSourceReadinessReason) -> AppSourceReadinessOutcome {
    AppSourceReadinessOutcome {
        readiness: AppSourceReadiness::Missing,
        reason: Some(reason),
    }
}

/// Decides whether the application can be remixed right now.
///
/// `source` is `None` when the application has no configured source at all — which
/// is the overwhelmingly common case, and must read as `MISSING` with
/// [`AppSourceReadinessReason::NotConfigured`] rather than as an error.
///
/// A Git source is ready only when the repository is *validated*, its provider is
/// supported, its URL is acceptable, and the optional ref/sub-path are usable. An
/// archive source is ready only when its artifact is still active. Anything the
/// rule cannot prove is a `MISSING` with the matching reason; it never guesses.
pub fn evaluate_app_source_readiness(
    source: Option<&AppSourceFacts<'_>>,
) -> AppSourceReadinessOutcome {
    let Some(source) = source else {
        return missing(AppSourceReadinessReason::NotConfigured);
    };
    match source.kind {
        AppSourceKind::Git => {
            let Some(git) = source.git else {
                return missing(AppSourceReadinessReason::KindMismatch);
            };
            if git.repo_deleted {
                return missing(AppSourceReadinessReason::GitRepositoryUnusable);
            }
            if !is_supported_repo_provider(git.repo_provider) {
                return missing(AppSourceReadinessReason::GitProviderUnsupported);
            }
            if validate_source_repo_url(git.repo_url).is_err() {
                return missing(AppSourceReadinessReason::GitRepositoryUrlInvalid);
            }
            if git.repo_status != SOURCE_REPOSITORY_STATUS_VALIDATED {
                return missing(match git.repo_status {
                    SOURCE_REPOSITORY_STATUS_PENDING => {
                        AppSourceReadinessReason::GitRepositoryPending
                    }
                    SOURCE_REPOSITORY_STATUS_INVALID => {
                        AppSourceReadinessReason::GitRepositoryInvalid
                    }
                    // `ARCHIVED`, or a status a future writer introduces: the
                    // only honest statement is that it is not validated.
                    _ => AppSourceReadinessReason::GitRepositoryNotValidated,
                });
            }
            if normalize_source_ref(git.source_ref).is_err() {
                return missing(AppSourceReadinessReason::GitReferenceInvalid);
            }
            if normalize_app_source_sub_path(git.sub_path).is_err() {
                return missing(AppSourceReadinessReason::GitSubPathInvalid);
            }
            READY
        }
        AppSourceKind::Archive => {
            let Some(archive) = source.archive else {
                return missing(AppSourceReadinessReason::KindMismatch);
            };
            if !archive.artifact_active {
                return missing(AppSourceReadinessReason::ArchiveUnavailable);
            }
            READY
        }
    }
}

/// Whether an enable-remix transition is allowed.
///
/// Exists as a named rule so the answer cannot drift between the enable
/// endpoint and any other writer: remix is enableable exactly when the source is
/// ready, and disabling is always allowed (a stuck remix must stay removable).
pub const fn remix_can_be_enabled(readiness: AppSourceReadiness) -> bool {
    matches!(readiness, AppSourceReadiness::Ready)
}

/// `deploy_source_repository.repo_status` values this module owns.
///
/// * `PENDING` — declared, not probed yet. **Every Git source write lands here**;
///   a write no longer claims the repository is usable.
/// * `VALIDATED` — a validation attempt established reachability. The only status
///   that makes a Git source `READY`.
/// * `INVALID` — the last validation attempt failed; `last_error_code` says why.
///
/// The schema also carries statuses this module does not write (`ARCHIVED` is
/// filtered by the source-event reader); those keep reading as
/// `GIT_REPOSITORY_NOT_VALIDATED`.
pub const SOURCE_REPOSITORY_STATUS_PENDING: &str = "PENDING";
pub const SOURCE_REPOSITORY_STATUS_VALIDATED: &str = "VALIDATED";
pub const SOURCE_REPOSITORY_STATUS_INVALID: &str = "INVALID";

/// The status a validation attempt writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitRepositoryValidationStatus {
    /// Not validated (a write, an SSH remote, or no probe available).
    Pending,
    /// Reachable; `last_validated_at` is stamped.
    Validated,
    /// The attempt failed; `last_error_code` explains it.
    Invalid,
}

impl GitRepositoryValidationStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => SOURCE_REPOSITORY_STATUS_PENDING,
            Self::Validated => SOURCE_REPOSITORY_STATUS_VALIDATED,
            Self::Invalid => SOURCE_REPOSITORY_STATUS_INVALID,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            SOURCE_REPOSITORY_STATUS_PENDING => Some(Self::Pending),
            SOURCE_REPOSITORY_STATUS_VALIDATED => Some(Self::Validated),
            SOURCE_REPOSITORY_STATUS_INVALID => Some(Self::Invalid),
            _ => None,
        }
    }
}

/// What `POST /apps/{appId}/source/validate` decided, and why.
///
/// The `as_str` values are persisted in `deploy_source_repository.last_error_code`
/// and rendered by the console, so they are a stable wire vocabulary: a code is
/// added, never repurposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitValidationErrorCode {
    /// An `git@host:path` (SSH) remote: no SSH credential is available here, so
    /// the check stays manual. Deliberately **not** a silent `VALIDATED`.
    UnsupportedScheme,
    /// The stored URL is not the shape this module accepts at all.
    UnusableUrl,
    /// The server answered `404`/`410`: nothing is published at that URL.
    RepositoryNotFound,
    /// The server answered `5xx`.
    ServerError,
    /// The server answered a status that proves neither presence nor absence
    /// (for example `400`, `405` after the GET fallback, or `429`).
    UnexpectedStatus,
    /// No answer within the probe budget.
    Timeout,
    /// The host could not be reached (DNS, TCP, TLS, redirect loop).
    NetworkFailure,
    /// The repository is reachable but demands credentials (`401`/`403`).
    ///
    /// Informational rather than a failure: the row stays `VALIDATED`, because
    /// the repository *is* reachable — the clone will need
    /// `credentialSecretRef`, which this probe cannot verify.
    CredentialsRequired,
    /// This deployment has no outbound HTTP probe wired, so nothing was checked.
    ProbeUnavailable,
}

impl GitValidationErrorCode {
    pub const ALL: [Self; 9] = [
        Self::UnsupportedScheme,
        Self::UnusableUrl,
        Self::RepositoryNotFound,
        Self::ServerError,
        Self::UnexpectedStatus,
        Self::Timeout,
        Self::NetworkFailure,
        Self::CredentialsRequired,
        Self::ProbeUnavailable,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedScheme => "GIT_VALIDATION_UNSUPPORTED_SCHEME",
            Self::UnusableUrl => "GIT_VALIDATION_URL_INVALID",
            Self::RepositoryNotFound => "GIT_VALIDATION_REPOSITORY_NOT_FOUND",
            Self::ServerError => "GIT_VALIDATION_SERVER_ERROR",
            Self::UnexpectedStatus => "GIT_VALIDATION_UNEXPECTED_STATUS",
            Self::Timeout => "GIT_VALIDATION_TIMEOUT",
            Self::NetworkFailure => "GIT_VALIDATION_NETWORK_FAILURE",
            Self::CredentialsRequired => "GIT_VALIDATION_CREDENTIALS_REQUIRED",
            Self::ProbeUnavailable => "GIT_VALIDATION_PROBE_UNAVAILABLE",
        }
    }

    /// The sentence stored next to the code and rendered by the console.
    ///
    /// Kept next to `as_str` so a new code cannot ship without an explanation,
    /// and so the wording is identical in the API response and the audit trail.
    pub const fn description(self) -> &'static str {
        match self {
            Self::UnsupportedScheme => {
                "a git@/ssh remote cannot be probed without an SSH credential; confirm \
                 reachability manually"
            }
            Self::UnusableUrl => "the stored repository URL is not a probeable http(s) URL",
            Self::RepositoryNotFound => {
                "the repository server answered 404: no repository is published at that URL"
            }
            Self::ServerError => "the repository server answered with a 5xx status",
            Self::UnexpectedStatus => {
                "the repository server answered a status that proves neither reachability nor \
                 absence"
            }
            Self::Timeout => "the repository did not answer within the 5 s probe budget",
            Self::NetworkFailure => "the repository could not be reached over the network",
            Self::CredentialsRequired => "the repository is reachable but demands credentials",
            Self::ProbeUnavailable => {
                "outbound HTTP probing is not configured on this deployment, so the repository \
                 was not checked"
            }
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }
}

/// Whether a validation attempt is even possible against this URL.
///
/// Split from the probe itself so the transport layer never has to reason about
/// URL shapes: `git@host:path` is a real Git remote that this control plane has no
/// way to authenticate, and pretending otherwise would mean either a fabricated
/// `VALIDATED` or a misleading `INVALID`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitRepositoryProbePlan {
    /// An `http(s)://` URL: probe it.
    HttpProbe,
    /// An scp-style `git@host:path` URL: SSH, cannot be probed here.
    RequiresManualConfirmation,
    /// The URL is not a Git source URL this module accepts.
    UnusableUrl,
}

/// Decides whether `repo_url` can be probed, and how.
#[must_use]
pub fn plan_git_repository_probe(repo_url: &str) -> GitRepositoryProbePlan {
    let url = repo_url.trim();
    if validate_source_repo_url(url).is_err() {
        return GitRepositoryProbePlan::UnusableUrl;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        GitRepositoryProbePlan::HttpProbe
    } else {
        // `validate_source_repo_url` only accepts one other shape: `git@`.
        GitRepositoryProbePlan::RequiresManualConfirmation
    }
}

/// What a bounded probe observed.
///
/// The status code is kept (rather than collapsed into a boolean) so the rule
/// below can tell "reachable" from "reachable but private" from "absent".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitRepositoryProbeObservation<'a> {
    /// The server answered with this status.
    Status(u16),
    /// No answer within the budget.
    TimedOut,
    /// The connection itself failed; `detail` is for the log, not for the wire.
    NetworkFailure(&'a str),
}

/// The verdict of one probe, with the evidence that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitRepositoryProbeVerdict {
    /// `2xx`/`3xx`: the host answers for that URL.
    Reachable { status_code: u16 },
    /// `401`/`403`: the host answers, but the clone will need a credential.
    ReachableWithCredentialsRequired { status_code: u16 },
    /// The URL is not usable right now; the code says why.
    Unreachable {
        status_code: Option<u16>,
        error_code: GitValidationErrorCode,
    },
}

/// Classifies a probe observation by status code alone.
///
/// Status-code-only is a deliberate limit: the response body of a Git host is
/// HTML or protocol data, not a contract, and parsing it would make the verdict
/// depend on the provider's markup. The cost of that limit is documented on
/// [`GitRepositoryProbeVerdict`]: a `200` on the landing page proves the host
/// answers, not that `git clone` succeeds.
pub const fn classify_git_repository_probe(
    observation: GitRepositoryProbeObservation<'_>,
) -> GitRepositoryProbeVerdict {
    match observation {
        GitRepositoryProbeObservation::Status(status_code) => {
            if status_code >= 200 && status_code < 400 {
                GitRepositoryProbeVerdict::Reachable { status_code }
            } else if status_code == 401 || status_code == 403 {
                GitRepositoryProbeVerdict::ReachableWithCredentialsRequired { status_code }
            } else if status_code == 404 || status_code == 410 {
                GitRepositoryProbeVerdict::Unreachable {
                    status_code: Some(status_code),
                    error_code: GitValidationErrorCode::RepositoryNotFound,
                }
            } else if status_code >= 500 {
                GitRepositoryProbeVerdict::Unreachable {
                    status_code: Some(status_code),
                    error_code: GitValidationErrorCode::ServerError,
                }
            } else {
                GitRepositoryProbeVerdict::Unreachable {
                    status_code: Some(status_code),
                    error_code: GitValidationErrorCode::UnexpectedStatus,
                }
            }
        }
        GitRepositoryProbeObservation::TimedOut => GitRepositoryProbeVerdict::Unreachable {
            status_code: None,
            error_code: GitValidationErrorCode::Timeout,
        },
        GitRepositoryProbeObservation::NetworkFailure(_) => {
            GitRepositoryProbeVerdict::Unreachable {
                status_code: None,
                error_code: GitValidationErrorCode::NetworkFailure,
            }
        }
    }
}

/// The state a validation attempt writes, independent of how it was obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitRepositoryValidationRecord {
    pub status: GitRepositoryValidationStatus,
    pub error_code: Option<GitValidationErrorCode>,
}

/// The rule every validation write goes through: probe verdict → stored state.
///
/// Written as one total function so the state machine has exactly one
/// implementation and one test table:
///
/// | attempt | status written | `last_error_code` |
/// |---|---|---|
/// | `2xx`/`3xx` | `VALIDATED` | — |
/// | `401`/`403` | `VALIDATED` | `GIT_VALIDATION_CREDENTIALS_REQUIRED` |
/// | `404`/`410`/`5xx`/other | `INVALID` | the matching code |
/// | timeout / network failure | `INVALID` | `GIT_VALIDATION_TIMEOUT` / `..._NETWORK_FAILURE` |
/// | SSH remote | `PENDING` (unchanged) | `GIT_VALIDATION_UNSUPPORTED_SCHEME` |
/// | no probe wired | `PENDING` (unchanged) | `GIT_VALIDATION_PROBE_UNAVAILABLE` |
///
/// Note what is *not* here: a path from `PENDING` straight to `VALIDATED`. Only a
/// probe result can produce `VALIDATED`, and an SSH remote can never produce one
/// from this service.
pub const fn advance_git_repository_validation(
    verdict: GitRepositoryProbeVerdict,
) -> GitRepositoryValidationRecord {
    match verdict {
        GitRepositoryProbeVerdict::Reachable { .. } => GitRepositoryValidationRecord {
            status: GitRepositoryValidationStatus::Validated,
            error_code: None,
        },
        GitRepositoryProbeVerdict::ReachableWithCredentialsRequired { .. } => {
            GitRepositoryValidationRecord {
                status: GitRepositoryValidationStatus::Validated,
                error_code: Some(GitValidationErrorCode::CredentialsRequired),
            }
        }
        GitRepositoryProbeVerdict::Unreachable { error_code, .. } => {
            GitRepositoryValidationRecord {
                status: GitRepositoryValidationStatus::Invalid,
                error_code: Some(error_code),
            }
        }
    }
}

/// The record a validation attempt writes for `repo_url`, given what the probe saw.
///
/// `observation` is `None` when the URL cannot be (or was not) probed. This is the
/// single entry point the service calls, so the "SSH stays `PENDING`" and "no
/// probe stays `PENDING`" rules cannot drift apart between callers.
pub fn git_repository_validation_record(
    repo_url: &str,
    observation: Option<GitRepositoryProbeObservation<'_>>,
) -> GitRepositoryValidationRecord {
    match plan_git_repository_probe(repo_url) {
        GitRepositoryProbePlan::UnusableUrl => GitRepositoryValidationRecord {
            status: GitRepositoryValidationStatus::Invalid,
            error_code: Some(GitValidationErrorCode::UnusableUrl),
        },
        GitRepositoryProbePlan::RequiresManualConfirmation => GitRepositoryValidationRecord {
            // Stays PENDING: an unprobed SSH remote is not a validated one.
            status: GitRepositoryValidationStatus::Pending,
            error_code: Some(GitValidationErrorCode::UnsupportedScheme),
        },
        GitRepositoryProbePlan::HttpProbe => match observation {
            Some(observation) => {
                advance_git_repository_validation(classify_git_repository_probe(observation))
            }
            None => GitRepositoryValidationRecord {
                status: GitRepositoryValidationStatus::Pending,
                error_code: Some(GitValidationErrorCode::ProbeUnavailable),
            },
        },
    }
}

/// The `deploy_source_repository.repo_key` the remix source's repository row is
/// written under.
///
/// A reserved key rather than one the operator chooses: `deploy_source_repository`
/// is unique on `(app_id, repo_key)`, and giving the remix source a key of its own
/// keeps it from colliding with — or silently overwriting — the repositories the
/// application's builds were registered against.
pub const REMIX_SOURCE_REPOSITORY_KEY: &str = "remix";

/// Git providers this module accepts for a source repository.
pub const APP_SOURCE_REPO_PROVIDERS: [&str; 4] = ["GITHUB", "GITEE", "GITLAB", "SELF_HOSTED"];

/// Clone modes a source repository may declare.
pub const APP_SOURCE_CLONE_MODES: [&str; 2] = ["FULL", "SHALLOW"];

pub const MAXIMUM_SOURCE_REPO_URL_LENGTH: usize = 1000;
pub const MAXIMUM_SOURCE_REF_LENGTH: usize = 255;
pub const MAXIMUM_SOURCE_SUB_PATH_LENGTH: usize = 1000;

pub fn is_supported_repo_provider(value: &str) -> bool {
    APP_SOURCE_REPO_PROVIDERS.contains(&value)
}

pub fn is_supported_clone_mode(value: &str) -> bool {
    APP_SOURCE_CLONE_MODES.contains(&value)
}

/// Accepts the URL shapes a Git source may carry.
///
/// Same vocabulary as the source-repository command: `https://`, `http://`, or an
/// scp-style `git@host:path`. Embedded credentials are refused — a credential
/// belongs in `credential_secret_ref`, where it can be encrypted and rotated,
/// not in a URL that is rendered in the console and written to audit logs.
pub fn validate_source_repo_url(url: &str) -> Result<(), &'static str> {
    let url = url.trim();
    if url.is_empty() || url.len() > MAXIMUM_SOURCE_REPO_URL_LENGTH {
        return Err("repoUrl must be 1..=1000 characters");
    }
    if url.contains("://") {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err("repoUrl must use https:// or http://");
        }
        if let Some(credentials_end) = url.find("://") {
            let rest = &url[credentials_end + 3..];
            if rest.contains('@') && !rest.starts_with("git@") {
                return Err("repoUrl must not embed credentials");
            }
        }
        return Ok(());
    }
    if !url.starts_with("git@") {
        return Err("repoUrl must be an https://, http://, or git@ scp-style URL");
    }
    Ok(())
}

/// Normalizes an optional repository-relative directory.
///
/// `None`, an empty string, `.` and `/` all mean "the repository root". Everything
/// else is normalized so that the stored value is the one a client can join onto a
/// checkout without re-deciding the question:
///
/// * a leading `./` is stripped (`./apps/web` → `apps/web`);
/// * trailing `/` is stripped (`apps/web/` → `apps/web`);
/// * absolute paths, backslashes, empty segments and `.`/`..` segments are refused
///   — a source path that escapes the checkout is not a source path.
pub fn normalize_app_source_sub_path(
    value: Option<&str>,
) -> Result<Option<String>, SourceSubPathError> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let mut candidate = raw.trim();
    if candidate.is_empty() {
        return Ok(None);
    }
    if candidate.contains('\\') {
        return Err(SourceSubPathError::Backslash);
    }
    while let Some(rest) = candidate.strip_prefix("./") {
        candidate = rest;
    }
    let candidate = candidate.trim_end_matches('/');
    if candidate.is_empty() || candidate == "." {
        // `/`, `./` and `.` are the root, not an error.
        return Ok(None);
    }
    if candidate.starts_with('/') {
        return Err(SourceSubPathError::Absolute);
    }
    if candidate.len() > MAXIMUM_SOURCE_SUB_PATH_LENGTH {
        return Err(SourceSubPathError::TooLong);
    }
    for segment in candidate.split('/') {
        if segment.is_empty() {
            return Err(SourceSubPathError::EmptySegment);
        }
        if segment == "." || segment == ".." {
            return Err(SourceSubPathError::Traversal);
        }
        let valid = segment.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' || byte == b'-'
        });
        if !valid {
            return Err(SourceSubPathError::InvalidCharacter);
        }
    }
    Ok(Some(candidate.to_owned()))
}

/// Why a sub-path was refused. Rendered to the caller as a validation message by
/// the service layer; the enum keeps the wording in one place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSubPathError {
    Absolute,
    Backslash,
    EmptySegment,
    Traversal,
    TooLong,
    InvalidCharacter,
}

impl SourceSubPathError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Absolute => "subPath must be relative to the repository root",
            Self::Backslash => "subPath must use / as the separator",
            Self::EmptySegment => "subPath must not contain empty segments",
            Self::Traversal => "subPath must not contain . or .. segments",
            Self::TooLong => "subPath must be at most 1000 characters",
            Self::InvalidCharacter => "subPath segments must be letters, digits, '.', '_' or '-'",
        }
    }
}

/// Normalizes an optional Git ref (branch, tag or commit).
///
/// `None` and the empty string mean "the repository's default branch". The rules
/// are the subset of `git check-ref-format` that a caller can actually violate by
/// typing a ref into a form: no whitespace, no `..`, none of `~^:?*[\`, and no
/// leading `-` (which a console would otherwise hand to git as an option).
pub fn normalize_source_ref(value: Option<&str>) -> Result<Option<String>, SourceRefError> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let candidate = raw.trim();
    if candidate.is_empty() {
        return Ok(None);
    }
    if candidate.len() > MAXIMUM_SOURCE_REF_LENGTH {
        return Err(SourceRefError::TooLong);
    }
    if candidate.starts_with('-') {
        return Err(SourceRefError::LeadingDash);
    }
    if candidate.chars().any(char::is_whitespace) {
        return Err(SourceRefError::Whitespace);
    }
    if candidate.contains("..") {
        return Err(SourceRefError::DoubleDot);
    }
    if candidate
        .chars()
        .any(|character| "~^:?*[\\".contains(character))
    {
        return Err(SourceRefError::ForbiddenCharacter);
    }
    Ok(Some(candidate.to_owned()))
}

impl SourceRefError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooLong => "repoRef must be at most 255 characters",
            Self::LeadingDash => "repoRef must not start with '-'",
            Self::Whitespace => "repoRef must not contain whitespace",
            Self::DoubleDot => "repoRef must not contain '..'",
            Self::ForbiddenCharacter => "repoRef must not contain any of ~^:?*[\\",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceRefError {
    TooLong,
    LeadingDash,
    Whitespace,
    DoubleDot,
    ForbiddenCharacter,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_facts<'a>(repo_url: &'a str, repo_status: &'a str) -> AppSourceGitFacts<'a> {
        AppSourceGitFacts {
            repo_provider: "GITHUB",
            repo_url,
            repo_status,
            repo_deleted: false,
            source_ref: None,
            sub_path: None,
        }
    }

    fn git_source<'a>(git: AppSourceGitFacts<'a>) -> AppSourceFacts<'a> {
        AppSourceFacts {
            kind: AppSourceKind::Git,
            git: Some(git),
            archive: None,
        }
    }

    fn assert_missing(outcome: AppSourceReadinessOutcome, reason: AppSourceReadinessReason) {
        assert_eq!(outcome.readiness, AppSourceReadiness::Missing);
        assert_eq!(outcome.reason, Some(reason));
        assert!(!outcome.is_ready());
    }

    #[test]
    fn an_unconfigured_application_is_missing_not_an_error() {
        assert_missing(
            evaluate_app_source_readiness(None),
            AppSourceReadinessReason::NotConfigured,
        );
    }

    #[test]
    fn a_validated_git_repository_at_the_root_is_ready() {
        let outcome = evaluate_app_source_readiness(Some(&git_source(git_facts(
            "https://github.com/sdkwork/app.git",
            SOURCE_REPOSITORY_STATUS_VALIDATED,
        ))));
        assert_eq!(outcome, READY);
        assert!(outcome.is_ready());
    }

    #[test]
    fn a_validated_git_repository_with_a_sub_path_is_ready() {
        let mut git = git_facts("https://github.com/sdkwork/app.git", "VALIDATED");
        git.sub_path = Some("apps/sdkwork-app-pc");
        git.source_ref = Some("release/1.4");
        assert!(evaluate_app_source_readiness(Some(&git_source(git))).is_ready());
    }

    #[test]
    fn a_pending_repository_is_missing_until_validated() {
        assert_missing(
            evaluate_app_source_readiness(Some(&git_source(git_facts(
                "https://github.com/sdkwork/app.git",
                SOURCE_REPOSITORY_STATUS_PENDING,
            )))),
            AppSourceReadinessReason::GitRepositoryPending,
        );
    }

    #[test]
    fn an_invalid_repository_is_missing_and_distinguishable_from_pending() {
        assert_missing(
            evaluate_app_source_readiness(Some(&git_source(git_facts(
                "https://github.com/sdkwork/app.git",
                SOURCE_REPOSITORY_STATUS_INVALID,
            )))),
            AppSourceReadinessReason::GitRepositoryInvalid,
        );
    }

    #[test]
    fn a_status_this_module_does_not_write_is_still_not_validated() {
        assert_missing(
            evaluate_app_source_readiness(Some(&git_source(git_facts(
                "https://github.com/sdkwork/app.git",
                "ARCHIVED",
            )))),
            AppSourceReadinessReason::GitRepositoryNotValidated,
        );
    }

    #[test]
    fn a_soft_deleted_repository_is_missing() {
        let mut git = git_facts("https://github.com/sdkwork/app.git", "VALIDATED");
        git.repo_deleted = true;
        assert_missing(
            evaluate_app_source_readiness(Some(&git_source(git))),
            AppSourceReadinessReason::GitRepositoryUnusable,
        );
    }

    #[test]
    fn an_unusable_repository_url_is_missing() {
        assert_missing(
            evaluate_app_source_readiness(Some(&git_source(git_facts(
                "ftp://example.com/app.git",
                "VALIDATED",
            )))),
            AppSourceReadinessReason::GitRepositoryUrlInvalid,
        );
    }

    #[test]
    fn an_unsupported_provider_is_missing() {
        let mut git = git_facts("https://github.com/sdkwork/app.git", "VALIDATED");
        git.repo_provider = "BITBUCKET";
        assert_missing(
            evaluate_app_source_readiness(Some(&git_source(git))),
            AppSourceReadinessReason::GitProviderUnsupported,
        );
    }

    #[test]
    fn a_stored_path_that_is_not_normalized_is_missing() {
        let mut git = git_facts("https://github.com/sdkwork/app.git", "VALIDATED");
        git.sub_path = Some("apps/../etc");
        assert_missing(
            evaluate_app_source_readiness(Some(&git_source(git))),
            AppSourceReadinessReason::GitSubPathInvalid,
        );
    }

    #[test]
    fn an_active_artifact_is_ready_and_a_retired_one_is_not() {
        let ready = AppSourceFacts {
            kind: AppSourceKind::Archive,
            git: None,
            archive: Some(AppSourceArchiveFacts {
                artifact_active: true,
            }),
        };
        assert!(evaluate_app_source_readiness(Some(&ready)).is_ready());

        let retired = AppSourceFacts {
            archive: Some(AppSourceArchiveFacts {
                artifact_active: false,
            }),
            ..ready
        };
        assert_missing(
            evaluate_app_source_readiness(Some(&retired)),
            AppSourceReadinessReason::ArchiveUnavailable,
        );
    }

    #[test]
    fn a_kind_without_its_pointer_is_a_mismatch() {
        let missing_pointer = AppSourceFacts {
            kind: AppSourceKind::Git,
            git: None,
            archive: Some(AppSourceArchiveFacts {
                artifact_active: true,
            }),
        };
        assert_missing(
            evaluate_app_source_readiness(Some(&missing_pointer)),
            AppSourceReadinessReason::KindMismatch,
        );
    }

    #[test]
    fn remix_may_only_be_enabled_for_a_ready_source() {
        assert!(remix_can_be_enabled(AppSourceReadiness::Ready));
        assert!(!remix_can_be_enabled(AppSourceReadiness::Missing));
    }

    #[test]
    fn the_repository_root_is_expressed_as_absence() {
        for input in [
            None,
            Some(""),
            Some("   "),
            Some("."),
            Some("/"),
            Some("./"),
        ] {
            assert_eq!(
                normalize_app_source_sub_path(input),
                Ok(None),
                "input {input:?} must mean the repository root"
            );
        }
    }

    #[test]
    fn sub_paths_are_normalized() {
        assert_eq!(
            normalize_app_source_sub_path(Some("./apps/web/")),
            Ok(Some("apps/web".to_owned()))
        );
        assert_eq!(
            normalize_app_source_sub_path(Some("apps/widgets")),
            Ok(Some("apps/widgets".to_owned()))
        );
    }

    #[test]
    fn sub_paths_that_escape_the_checkout_are_refused() {
        for (input, expected) in [
            ("/etc/passwd", SourceSubPathError::Absolute),
            ("apps/../secret", SourceSubPathError::Traversal),
            ("apps//web", SourceSubPathError::EmptySegment),
            ("apps\\web", SourceSubPathError::Backslash),
            ("apps/web node", SourceSubPathError::InvalidCharacter),
        ] {
            assert_eq!(
                normalize_app_source_sub_path(Some(input)),
                Err(expected),
                "input {input} must be refused"
            );
        }
    }

    #[test]
    fn refs_are_trimmed_and_validated() {
        assert_eq!(normalize_source_ref(None), Ok(None));
        assert_eq!(normalize_source_ref(Some("  ")), Ok(None));
        assert_eq!(
            normalize_source_ref(Some(" release/1.4 ")),
            Ok(Some("release/1.4".to_owned()))
        );
        assert_eq!(
            normalize_source_ref(Some("-rf")),
            Err(SourceRefError::LeadingDash)
        );
        assert_eq!(
            normalize_source_ref(Some("a b")),
            Err(SourceRefError::Whitespace)
        );
        assert_eq!(
            normalize_source_ref(Some("a..b")),
            Err(SourceRefError::DoubleDot)
        );
    }

    #[test]
    fn repo_urls_accept_the_documented_shapes_only() {
        assert!(validate_source_repo_url("https://github.com/sdkwork/app.git").is_ok());
        assert!(validate_source_repo_url("git@github.com:sdkwork/app.git").is_ok());
        assert!(validate_source_repo_url("").is_err());
        assert!(validate_source_repo_url("ssh://github.com/app.git").is_err());
        assert!(validate_source_repo_url("https://user:token@github.com/app.git").is_err());
    }

    // -- validation state machine -------------------------------------------

    const HTTP_REPO: &str = "https://github.com/sdkwork/app.git";
    const SSH_REPO: &str = "git@github.com:sdkwork/app.git";

    /// Applies the record to a repository row and reads the derived readiness —
    /// the same two steps the HTTP layer performs, in the same order.
    fn readiness_after(record: GitRepositoryValidationRecord) -> AppSourceReadinessOutcome {
        let git = git_facts(HTTP_REPO, record.status.as_str());
        evaluate_app_source_readiness(Some(&git_source(git)))
    }

    fn record_for(observation: GitRepositoryProbeObservation<'_>) -> GitRepositoryValidationRecord {
        git_repository_validation_record(HTTP_REPO, Some(observation))
    }

    #[test]
    fn a_reachable_repository_moves_pending_to_validated_and_only_then_is_ready() {
        // The row a GIT write leaves behind.
        assert_missing(
            evaluate_app_source_readiness(Some(&git_source(git_facts(
                HTTP_REPO,
                SOURCE_REPOSITORY_STATUS_PENDING,
            )))),
            AppSourceReadinessReason::GitRepositoryPending,
        );

        // 2xx and 3xx both prove the host answers.
        for status in [200u16, 204, 302, 399] {
            let record = record_for(GitRepositoryProbeObservation::Status(status));
            assert_eq!(record.status, GitRepositoryValidationStatus::Validated);
            assert_eq!(
                record.error_code, None,
                "status {status} is plainly reachable"
            );
            let outcome = readiness_after(record);
            assert!(
                outcome.is_ready(),
                "status {status} must make the source READY"
            );
            assert_eq!(outcome.reason, None);
        }
    }

    #[test]
    fn a_credentials_challenge_is_reachable_and_says_which_credential_is_missing() {
        for status in [401u16, 403] {
            let record = record_for(GitRepositoryProbeObservation::Status(status));
            assert_eq!(
                record.status,
                GitRepositoryValidationStatus::Validated,
                "status {status} proves the repository exists"
            );
            assert_eq!(
                record.error_code,
                Some(GitValidationErrorCode::CredentialsRequired)
            );
            assert!(
                readiness_after(record).is_ready(),
                "status {status} is reachable, so the source is ready"
            );
        }
    }

    #[test]
    fn an_absent_repository_moves_pending_to_invalid() {
        for status in [404u16, 410] {
            let record = record_for(GitRepositoryProbeObservation::Status(status));
            assert_eq!(record.status, GitRepositoryValidationStatus::Invalid);
            assert_eq!(
                record.error_code,
                Some(GitValidationErrorCode::RepositoryNotFound)
            );
            assert_missing(
                readiness_after(record),
                AppSourceReadinessReason::GitRepositoryInvalid,
            );
        }
    }

    #[test]
    fn a_server_error_and_an_inconclusive_status_are_invalid() {
        for (status, expected) in [
            (500u16, GitValidationErrorCode::ServerError),
            (503, GitValidationErrorCode::ServerError),
            (400, GitValidationErrorCode::UnexpectedStatus),
            (408, GitValidationErrorCode::UnexpectedStatus),
            (429, GitValidationErrorCode::UnexpectedStatus),
        ] {
            let record = record_for(GitRepositoryProbeObservation::Status(status));
            assert_eq!(record.status, GitRepositoryValidationStatus::Invalid);
            assert_eq!(record.error_code, Some(expected), "status {status}");
        }
    }

    #[test]
    fn a_timed_out_or_unreachable_probe_is_invalid_and_names_the_failure() {
        let timed_out = record_for(GitRepositoryProbeObservation::TimedOut);
        assert_eq!(timed_out.status, GitRepositoryValidationStatus::Invalid);
        assert_eq!(timed_out.error_code, Some(GitValidationErrorCode::Timeout));

        let unreachable = record_for(GitRepositoryProbeObservation::NetworkFailure(
            "dns lookup failed",
        ));
        assert_eq!(unreachable.status, GitRepositoryValidationStatus::Invalid);
        assert_eq!(
            unreachable.error_code,
            Some(GitValidationErrorCode::NetworkFailure)
        );
        assert_missing(
            readiness_after(unreachable),
            AppSourceReadinessReason::GitRepositoryInvalid,
        );
    }

    #[test]
    fn an_ssh_remote_stays_pending_and_is_never_fabricated_as_validated() {
        let record = git_repository_validation_record(SSH_REPO, None);
        assert_eq!(
            record.status,
            GitRepositoryValidationStatus::Pending,
            "an SSH remote cannot be probed here, so it cannot be validated here"
        );
        assert_eq!(
            record.error_code,
            Some(GitValidationErrorCode::UnsupportedScheme)
        );
        let git = git_facts(SSH_REPO, record.status.as_str());
        assert_missing(
            evaluate_app_source_readiness(Some(&git_source(git))),
            AppSourceReadinessReason::GitRepositoryPending,
        );
    }

    #[test]
    fn a_deployment_without_a_probe_leaves_the_status_pending() {
        let record = git_repository_validation_record(HTTP_REPO, None);
        assert_eq!(record.status, GitRepositoryValidationStatus::Pending);
        assert_eq!(
            record.error_code,
            Some(GitValidationErrorCode::ProbeUnavailable)
        );
    }

    #[test]
    fn a_url_this_module_cannot_probe_is_invalid_rather_than_silently_accepted() {
        let record = git_repository_validation_record("ftp://example.com/app.git", None);
        assert_eq!(record.status, GitRepositoryValidationStatus::Invalid);
        assert_eq!(record.error_code, Some(GitValidationErrorCode::UnusableUrl));
    }

    #[test]
    fn revalidation_can_degrade_and_recover() {
        // An already-validated row that has since gone away.
        let degraded = record_for(GitRepositoryProbeObservation::Status(404));
        assert_eq!(degraded.status, GitRepositoryValidationStatus::Invalid);

        // An invalid row whose URL was fixed.
        let recovered = record_for(GitRepositoryProbeObservation::Status(200));
        assert_eq!(recovered.status, GitRepositoryValidationStatus::Validated);
        assert!(readiness_after(recovered).is_ready());
    }

    #[test]
    fn probe_plans_cover_exactly_the_url_shapes_the_writer_accepts() {
        for url in [
            "https://github.com/sdkwork/app.git",
            "http://127.0.0.1:1/app.git",
        ] {
            assert_eq!(
                plan_git_repository_probe(url),
                GitRepositoryProbePlan::HttpProbe
            );
        }
        assert_eq!(
            plan_git_repository_probe(SSH_REPO),
            GitRepositoryProbePlan::RequiresManualConfirmation
        );
        for url in ["ssh://github.com/app.git", "ftp://example.com/app.git", ""] {
            assert_eq!(
                plan_git_repository_probe(url),
                GitRepositoryProbePlan::UnusableUrl
            );
        }
    }

    #[test]
    fn every_validation_error_code_round_trips_and_has_an_explanation() {
        for code in GitValidationErrorCode::ALL {
            assert_eq!(GitValidationErrorCode::parse(code.as_str()), Some(code));
            assert!(
                !code.description().is_empty(),
                "{} needs an operator-facing explanation",
                code.as_str()
            );
        }
        assert_eq!(
            GitValidationErrorCode::parse("GIT_VALIDATION_UNKNOWN"),
            None
        );
    }

    #[test]
    fn validation_statuses_map_to_the_persisted_vocabulary() {
        assert_eq!(GitRepositoryValidationStatus::Pending.as_str(), "PENDING");
        assert_eq!(
            GitRepositoryValidationStatus::Validated.as_str(),
            "VALIDATED"
        );
        assert_eq!(GitRepositoryValidationStatus::Invalid.as_str(), "INVALID");
        for status in [
            GitRepositoryValidationStatus::Pending,
            GitRepositoryValidationStatus::Validated,
            GitRepositoryValidationStatus::Invalid,
        ] {
            assert_eq!(
                GitRepositoryValidationStatus::parse(status.as_str()),
                Some(status)
            );
        }
    }
}
