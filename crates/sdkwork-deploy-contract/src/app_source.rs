//! Remix source and remix-state DTOs.
//!
//! Two resources, deliberately separate:
//!
//! * the **source** ([`AppSourceResponse`]) — *what* a remix downloads, set by
//!   [`UpdateAppSourceRequest`];
//! * the **remix state** ([`AppRemixResponse`]) — *whether* remix is enabled, and
//!   the download instructions the client follows when it is.
//!
//! Keeping them apart is what makes the precondition legible: enabling remix is a
//! transition with a condition (the source must be ready), whereas replacing the
//! source is an edit that never touches the switch. A single merged resource would
//! have to encode "these fields only apply when that flag is true" on the wire.
//!
//! Readiness itself is not authored by any request in this module — it is derived
//! by `sdkwork_deploy_core::evaluate_app_source_readiness` from the current state
//! of the referenced repository or artifact. The DTOs therefore carry it as an
//! output-only field, plus the closed-set reason code that explains a `MISSING`
//! verdict.

use serde::{Deserialize, Serialize};

/// Re-exported so consumers reach the two wire enums through the contract, next to
/// the DTOs that carry them.
pub use sdkwork_deploy_core::{AppSourceKind, AppSourceReadiness};

/// `PUT /apps/{appId}/source` — replace the application's remix source.
///
/// A full replacement rather than a patch: a Git source and an archive source have
/// disjoint fields, so a partial edit would leave the row temporarily describing
/// both (or neither). The operation is idempotent; sending the same body twice
/// leaves the same row.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAppSourceRequest {
    /// `GIT` or `ARCHIVE`; the matching object must be present and the other one
    /// must be absent.
    pub kind: AppSourceKind,
    /// Required when `kind = GIT`.
    #[serde(default)]
    pub git: Option<AppSourceGitInput>,
    /// Required when `kind = ARCHIVE`.
    #[serde(default)]
    pub archive: Option<AppSourceArchiveInput>,
}

/// The Git half of [`UpdateAppSourceRequest`].
///
/// `repoUrl` may name a directory inside the repository through `subPath`, which
/// is the "monorepo, remix only this package" case. `repoRef` selects the branch,
/// tag or commit to download; absent means the repository's default branch.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSourceGitInput {
    /// `GITHUB` | `GITEE` | `GITLAB` | `SELF_HOSTED`.
    #[serde(rename = "repoProvider")]
    pub repo_provider: String,
    #[serde(rename = "repoUrl")]
    pub repo_url: String,
    /// Branch, tag or commit. Absent means the repository's default branch.
    #[serde(rename = "repoRef", default)]
    pub repo_ref: Option<String>,
    /// Repository-relative directory. Absent (or `/`, `.`, `./`) means the
    /// repository root.
    #[serde(rename = "subPath", default)]
    pub sub_path: Option<String>,
    /// Where the clone credential lives. The secret itself never travels here.
    #[serde(rename = "credentialSecretRef", default)]
    pub credential_secret_ref: Option<String>,
    /// `FULL` | `SHALLOW`; absent defaults to `SHALLOW`.
    #[serde(rename = "cloneMode", default)]
    pub clone_mode: Option<String>,
}

/// The archive half of [`UpdateAppSourceRequest`].
///
/// The artifact is what the existing upload flow produces
/// (`uploadSessions.create` → upload → `uploadSessions.complete` →
/// `artifacts.create`), so this request references a finished artifact instead of
/// re-implementing upload here. The artifact must be attributed to this
/// application; a foreign or retired artifact is refused.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSourceArchiveInput {
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
}

/// The Git half of [`AppSourceResponse`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSourceGitResponse {
    #[serde(rename = "repoProvider")]
    pub repo_provider: String,
    #[serde(rename = "repoUrl")]
    pub repo_url: String,
    /// The ref the remix client must check out. Always present: it falls back to
    /// the repository's default branch, so a client never has to know that rule.
    #[serde(rename = "repoRef")]
    pub repo_ref: String,
    /// Absent for the repository root.
    #[serde(rename = "subPath", skip_serializing_if = "Option::is_none")]
    pub sub_path: Option<String>,
    #[serde(rename = "cloneMode")]
    pub clone_mode: String,
    /// Where the clone credential lives (`deploy_source_repository.
    /// credential_secret_ref`), if the repository needs one.
    #[serde(
        rename = "credentialSecretRef",
        skip_serializing_if = "Option::is_none"
    )]
    pub credential_secret_ref: Option<String>,
    /// `deploy_source_repository.repo_status` — `VALIDATED` is what makes a Git
    /// source ready.
    #[serde(rename = "repoStatus")]
    pub repo_status: String,
}

/// The archive half of [`AppSourceResponse`].
///
/// This is the artifact's download descriptor: the deploy app-api never proxies
/// file bytes, so the client resolves `driveNodeId` through the Drive app API
/// (`downloadGrants.create` / `nodes.downloadUrls.retrieve`) and verifies the
/// bytes against `checksumSha256`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSourceArchiveResponse {
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    /// The upload session that produced the artifact, so the console can link back
    /// to the upload it came from.
    #[serde(rename = "uploadSessionId")]
    pub upload_session_id: String,
    #[serde(rename = "packageType")]
    pub package_type: i32,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
    #[serde(rename = "contentLength", with = "sdkwork_utils_rust::serde_int64")]
    pub content_length: i64,
    #[serde(rename = "checksumSha256", skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
    #[serde(rename = "driveNodeId")]
    pub drive_node_id: String,
    #[serde(rename = "driveSpaceId", skip_serializing_if = "Option::is_none")]
    pub drive_space_id: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

/// The application's configured remix source plus the derived verdict.
///
/// `sourceKind`/`git`/`archive` are absent when nothing has been configured yet —
/// a fresh application answers `200` with `readiness: MISSING` and
/// `readinessReason: SOURCE_NOT_CONFIGURED` rather than `404`, because "no source"
/// is a valid state of the resource and the console renders it as an empty form.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSourceResponse {
    #[serde(rename = "appId")]
    pub app_id: String,
    /// Copy of `deploy_app.remix_enabled`, so the settings page needs one read.
    #[serde(rename = "remixEnabled")]
    pub remix_enabled: bool,
    #[serde(rename = "sourceKind", skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<AppSourceKind>,
    pub readiness: AppSourceReadiness,
    /// Closed-set code explaining a `MISSING` verdict; absent when `READY`.
    #[serde(rename = "readinessReason", skip_serializing_if = "Option::is_none")]
    pub readiness_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<AppSourceGitResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<AppSourceArchiveResponse>,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// `PUT /apps/{appId}/remix` — enable or disable remix.
///
/// Enabling requires a `READY` source and is refused with `409` otherwise; the
/// refusal carries the same `readinessReason` codes as [`AppSourceResponse`] so the
/// console can turn it into a sentence without a second request. Disabling is
/// always accepted: an operator must be able to switch remix off even when the
/// source has since become unusable.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAppRemixRequest {
    pub enabled: bool,
}

/// One download instruction set, matching the source kind.
///
/// A tagged pair rather than a free-form object: the remix client branches on
/// `mode`, and an object carrying both halves would leave "clone this" and
/// "download that" both looking applicable.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRemixDownloadResponse {
    pub mode: AppSourceKind,
    /// Present when `mode = GIT`: clone `repoUrl` at `repoRef`, then read `subPath`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<AppSourceGitResponse>,
    /// Present when `mode = ARCHIVE`: download the artifact bytes from Drive using
    /// `driveNodeId`, then extract the archive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<AppSourceArchiveResponse>,
}

/// The remix state of an application, and how to download its source.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRemixResponse {
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "remixEnabled")]
    pub remix_enabled: bool,
    #[serde(rename = "sourceKind", skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<AppSourceKind>,
    pub readiness: AppSourceReadiness,
    #[serde(rename = "readinessReason", skip_serializing_if = "Option::is_none")]
    pub readiness_reason: Option<String>,
    /// Present only when remix is enabled **and** the source is ready. A disabled
    /// app or an unusable source therefore returns the state without a download,
    /// which is exactly the distinction a "Remix" button needs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download: Option<AppRemixDownloadResponse>,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}
