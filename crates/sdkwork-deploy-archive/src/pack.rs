//! Directory → deterministic ZIP with git-style ignore selection.

use std::fs::{self, File};
use std::io::Cursor;
use std::path::{Component, Path};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::{
    ArchiveError, DEFAULT_MAX_DEPTH, DEFAULT_MAX_ENTRIES, DEFAULT_MAX_FILE_BYTES,
    DEFAULT_MAX_TOTAL_BYTES,
};

/// Packaging rules. The defaults match the template publish path: `.git` and
/// machine-local secret carriers are excluded, every nested `.gitignore` is
/// honored, hidden source files stay included, and the archive is bounded.
#[derive(Clone, Debug)]
pub struct PackOptions {
    /// Include `.git`/`.svn`/`.hg` metadata. Off by default.
    pub include_vcs_metadata: bool,
    /// Drop `.env`, local overrides, registry tokens, and key material.
    pub exclude_sensitive: bool,
    /// Additional gitignore-style patterns relative to the package root.
    pub custom_excludes: Vec<String>,
    pub max_entries: usize,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    pub max_depth: usize,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            include_vcs_metadata: false,
            exclude_sensitive: true,
            custom_excludes: Vec::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }
}

/// Files omitted from an otherwise eligible package.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkippedReason {
    Symlink,
    SpecialFile,
    Sensitive,
    CustomRule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedEntry {
    /// Posix path relative to the package root.
    pub path: String,
    pub reason: SkippedReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedEntry {
    pub path: String,
    pub bytes: u64,
}

/// One deterministic ZIP plus the facts artifact registration needs.
#[derive(Clone, Debug)]
pub struct PackedArchive {
    pub bytes: Vec<u8>,
    pub entries: Vec<PackedEntry>,
    pub total_bytes: u64,
    pub checksum_sha256: String,
    pub skipped: Vec<SkippedEntry>,
}

/// Pack one directory into a ZIP honoring its `.gitignore` cascade.
///
/// The archive is byte-deterministic for the same input: entries are sorted by
/// their posix path, timestamps are fixed, and directory entries are omitted
/// because extraction recreates parents from file paths.
pub fn pack_directory(
    root: impl AsRef<Path>,
    options: &PackOptions,
) -> Result<PackedArchive, ArchiveError> {
    let root = root.as_ref();
    if !root.is_dir() {
        return Err(ArchiveError::NotDirectory(root.display().to_string()));
    }
    let root = fs::canonicalize(root)?;
    let custom = build_custom_rules(&root, &options.custom_excludes)?;
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();

    let mut walker = WalkBuilder::new(&root);
    walker
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .threads(1);
    for entry in walker.build() {
        let entry = entry.map_err(|error| {
            ArchiveError::Io(std::io::Error::other(format!(
                "walking {}: {error}",
                root.display()
            )))
        })?;
        if entry.depth() == 0 {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| ArchiveError::UnsafePath(path.display().to_string()))?;
        let rel = posix_path(relative)?;
        if !options.include_vcs_metadata && has_vcs_component(&rel) {
            continue;
        }
        let file_type = entry.file_type().ok_or_else(|| {
            ArchiveError::Io(std::io::Error::other(format!(
                "cannot determine the file type of {}",
                path.display()
            )))
        })?;
        if file_type.is_symlink() {
            skipped.push(SkippedEntry {
                path: rel,
                reason: SkippedReason::Symlink,
            });
            continue;
        }
        if !file_type.is_file() {
            if file_type.is_dir() {
                continue;
            }
            skipped.push(SkippedEntry {
                path: rel,
                reason: SkippedReason::SpecialFile,
            });
            continue;
        }
        if options.exclude_sensitive && is_sensitive_source_path(&rel) {
            skipped.push(SkippedEntry {
                path: rel,
                reason: SkippedReason::Sensitive,
            });
            continue;
        }
        if custom
            .matched_path_or_any_parents(Path::new(&rel), false)
            .is_ignore()
        {
            skipped.push(SkippedEntry {
                path: rel,
                reason: SkippedReason::CustomRule,
            });
            continue;
        }
        let depth = relative.components().count();
        if depth > options.max_depth {
            return Err(ArchiveError::TooDeep {
                path: rel,
                actual: depth,
                limit: options.max_depth,
            });
        }
        let metadata = entry.metadata().map_err(|error| {
            ArchiveError::Io(std::io::Error::other(format!(
                "stat {}: {error}",
                path.display()
            )))
        })?;
        if metadata.len() > options.max_file_bytes {
            return Err(ArchiveError::FileTooLarge {
                path: rel,
                actual: metadata.len(),
                limit: options.max_file_bytes,
            });
        }
        candidates.push((rel, path.to_path_buf(), metadata.len()));
    }
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    if candidates.len() > options.max_entries {
        return Err(ArchiveError::TooManyEntries {
            actual: candidates.len(),
            limit: options.max_entries,
        });
    }
    let total_bytes = candidates.iter().map(|(_, _, bytes)| *bytes).sum::<u64>();
    if total_bytes > options.max_total_bytes {
        return Err(ArchiveError::TotalTooLarge {
            actual: total_bytes,
            limit: options.max_total_bytes,
        });
    }
    if candidates.is_empty() {
        return Err(ArchiveError::Empty);
    }

    let timestamp = zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).unwrap_or_default();
    let file_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(timestamp)
        .unix_permissions(0o644);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let mut entries = Vec::with_capacity(candidates.len());
    for (rel, absolute, expected_bytes) in &candidates {
        writer.start_file(rel.as_str(), file_options)?;
        let mut source = File::open(absolute)?;
        let copied = std::io::copy(&mut source, &mut writer)?;
        if copied != *expected_bytes {
            return Err(ArchiveError::Io(std::io::Error::other(format!(
                "{} changed while it was packed",
                absolute.display()
            ))));
        }
        entries.push(PackedEntry {
            path: rel.clone(),
            bytes: *expected_bytes,
        });
    }
    let bytes = writer.finish()?.into_inner();
    let checksum_sha256 = sha256_hex(&bytes);
    Ok(PackedArchive {
        bytes,
        entries,
        total_bytes,
        checksum_sha256,
        skipped,
    })
}

/// Build the extra exclude rules as a gitignore rooted at the package root.
fn build_custom_rules(root: &Path, patterns: &[String]) -> Result<Gitignore, ArchiveError> {
    if patterns.is_empty() {
        return Ok(Gitignore::empty());
    }
    let mut builder = GitignoreBuilder::new(root);
    for pattern in patterns {
        builder.add_line(None, pattern)?;
    }
    Ok(builder.build()?)
}

/// `a/b/c.txt` in posix form, rejecting components a ZIP cannot carry safely.
fn posix_path(path: &Path) -> Result<String, ArchiveError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let text = part
                    .to_str()
                    .ok_or_else(|| ArchiveError::UnsafePath(path.display().to_string()))?;
                if text.contains('\\') || text == "." || text == ".." {
                    return Err(ArchiveError::UnsafePath(path.display().to_string()));
                }
                parts.push(text);
            }
            _ => return Err(ArchiveError::UnsafePath(path.display().to_string())),
        }
    }
    Ok(parts.join("/"))
}

fn has_vcs_component(path: &str) -> bool {
    path.split('/')
        .any(|part| part == ".git" || part == ".svn" || part == ".hg")
}

/// Machine-local carriers a scaffold must never publish, mirroring the browser
/// packer's policy. Checked-in per-profile environment templates stay source.
fn is_sensitive_source_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if name == ".env" || name == ".env.local" {
        return true;
    }
    if name.ends_with(".local") || name.contains(".local.") {
        return true;
    }
    if matches!(
        name.as_str(),
        ".npmrc"
            | ".yarnrc"
            | ".pypirc"
            | ".netrc"
            | ".git-credentials"
            | "id_rsa"
            | "id_ed25519"
            | "id_ecdsa"
    ) {
        return true;
    }
    matches!(
        name.rsplit('.').next().unwrap_or(""),
        "pem" | "key" | "p12" | "pfx" | "keystore" | "jks" | "secret"
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
