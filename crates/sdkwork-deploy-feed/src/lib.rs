//! Update feed rules shared by the Deploy API and the desktop clients.
//!
//! This crate owns the protocol decisions that must be identical on both sides
//! of the wire and must never drift:
//!
//! - **Channel file naming.** electron-updater asks for `<channel>.yml` on
//!   Windows/Linux and `<channel>-mac.yml` on macOS. One function decides it.
//! - **Staged rollout bucketing.** The percentage in the operator's update
//!   configuration is applied to a deterministic bucket derived from the
//!   installation id, so the same install keeps the same decision across
//!   restarts while a rollout can still be widened one percent at a time.
//! - **Feed document shape.** electron-updater 6.x needs `version`, `files`
//!   (url/sha512/size), the legacy top-level `path`/`sha512`, `releaseDate`, and
//!   the optional `stagingPercentage`. `render_yaml` is the only serializer, so
//!   a golden test can pin the exact bytes clients receive.
//!
//! Nothing here touches the database, HTTP, or the clock: callers pass every
//! fact in, which is what makes the golden tests meaningful.

use sdkwork_deploy_contract::{DownloadFeedDocument, DownloadFeedFile};
use sdkwork_utils_rust::sha256_hash;

/// Failures that make a feed unpublishable. Each one is a configuration error
/// the operator can fix, never a transient network condition.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FeedError {
    /// The package has no SHA-512 digest; electron-updater cannot verify it.
    #[error("package has no sha512 digest; re-digest it before publishing a feed")]
    MissingSha512,
    /// The installer has no public URL to put in `files[].url`.
    #[error("installer has no public download URL")]
    MissingFileUrl,
    /// The channel key is not one of the protocol channels.
    #[error("unsupported update channel: {0}")]
    UnsupportedChannel(String),
}

/// Facts required to build one feed document.
#[derive(Clone, Debug)]
pub struct FeedInput {
    pub version: String,
    pub file_url: String,
    pub sha512: String,
    pub size: i64,
    pub release_date: String,
    pub staging_percentage: Option<i32>,
}

/// Channel keys the updater protocol accepts.
pub const UPDATE_CHANNELS: [&str; 2] = ["nightly", "stable"];

/// Whether `channel_key` is a protocol channel.
pub fn is_supported_channel(channel_key: &str) -> bool {
    UPDATE_CHANNELS.contains(&channel_key)
}

/// The metadata file name electron-updater requests for a platform/channel.
///
/// macOS uses the `-mac` suffix because one release publishes both platform
/// feeds from the same directory.
pub fn channel_filename(platform: &str, channel_key: &str) -> String {
    if platform == "macos" {
        format!("{channel_key}-mac.yml")
    } else {
        format!("{channel_key}.yml")
    }
}

/// Deterministic 0..=99 rollout bucket for one installation.
///
/// The bucket depends on the product, the offered version and the installation
/// id, so widening a rollout keeps every already-included install included.
pub fn stable_bucket(install_id: &str, product_key: &str, version: &str) -> u8 {
    let digest = sha256_hash(format!("{product_key}\u{0}{version}\u{0}{install_id}").as_bytes());
    u8::from_str_radix(digest.get(..2).unwrap_or("00"), 16).unwrap_or(0) % 100
}

/// Whether one installation is inside a rollout percentage.
pub fn is_in_rollout(install_id: &str, product_key: &str, version: &str, percentage: i32) -> bool {
    if percentage >= 100 {
        return true;
    }
    if percentage <= 0 {
        return false;
    }
    i32::from(stable_bucket(install_id, product_key, version)) < percentage
}

/// Builds the feed document for one published installer.
pub fn build_feed(input: &FeedInput) -> Result<DownloadFeedDocument, FeedError> {
    if input.sha512.trim().is_empty() {
        return Err(FeedError::MissingSha512);
    }
    if input.file_url.trim().is_empty() {
        return Err(FeedError::MissingFileUrl);
    }
    let staging = input
        .staging_percentage
        .filter(|percentage| (0..=100).contains(percentage));
    Ok(DownloadFeedDocument {
        version: input.version.clone(),
        files: vec![DownloadFeedFile {
            url: input.file_url.clone(),
            sha512: input.sha512.clone(),
            size: input.size,
        }],
        path: input.file_url.clone(),
        release_date: input.release_date.clone(),
        staging_percentage: staging,
    })
}

/// Stable revision token for a feed document (used as the HTTP ETag).
pub fn revision_of(document: &DownloadFeedDocument) -> String {
    let canonical = format!(
        "{}\u{0}{}\u{0}{}\u{0}{}",
        document.version,
        document.path,
        document.release_date,
        document.files.len()
    );
    format!("sha256:{}", sha256_hash(canonical.as_bytes()))
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Renders the YAML body electron-updater reads.
///
/// Written by hand instead of pulling a YAML serializer: the schema is six
/// fields, every value is a scalar, and a golden test pins the exact output.
pub fn render_yaml(document: &DownloadFeedDocument) -> String {
    let mut yaml = String::new();
    yaml.push_str(&format!("version: {}\n", quote(&document.version)));
    yaml.push_str("files:\n");
    for file in &document.files {
        yaml.push_str(&format!("  - url: {}\n", quote(&file.url)));
        yaml.push_str(&format!("    sha512: {}\n", quote(&file.sha512)));
        yaml.push_str(&format!("    size: {}\n", file.size));
    }
    yaml.push_str(&format!("path: {}\n", quote(&document.path)));
    // electron-updater 6.x still reads the legacy top-level digest for the
    // single-file feed shape this server publishes.
    if let Some(file) = document.files.first() {
        yaml.push_str(&format!("sha512: {}\n", quote(&file.sha512)));
    }
    yaml.push_str(&format!("releaseDate: {}\n", quote(&document.release_date)));
    if let Some(percentage) = document.staging_percentage {
        yaml.push_str(&format!("stagingPercentage: {percentage}\n"));
    }
    yaml
}
