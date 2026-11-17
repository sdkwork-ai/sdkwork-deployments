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
    /// The repository is registered but has not been validated.
    GitRepositoryNotValidated,
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
                return missing(AppSourceReadinessReason::GitRepositoryNotValidated);
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

pub const SOURCE_REPOSITORY_STATUS_VALIDATED: &str = "VALIDATED";

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
                "PENDING",
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
}
