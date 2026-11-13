//! Safe ZIP extraction with traversal, symlink, and size guards.

use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{
    ArchiveError, DEFAULT_MAX_DEPTH, DEFAULT_MAX_ENTRIES, DEFAULT_MAX_FILE_BYTES,
    DEFAULT_MAX_TOTAL_BYTES,
};

/// Extraction rules. Defaults refuse overwrites and are bounded like packing.
#[derive(Clone, Debug)]
pub struct UnpackOptions {
    pub overwrite: bool,
    /// Optional SHA-256 over the complete ZIP bytes.
    pub expected_sha256: Option<String>,
    pub max_entries: usize,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    pub max_depth: usize,
}

impl Default for UnpackOptions {
    fn default() -> Self {
        Self {
            overwrite: false,
            expected_sha256: None,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnpackedEntry {
    pub path: String,
    pub bytes: u64,
}

#[derive(Clone, Debug)]
pub struct UnpackedArchive {
    pub entries: Vec<UnpackedEntry>,
    pub total_bytes: u64,
}

/// Extract one ZIP into `destination`.
///
/// Every entry must be a regular file with a relative, non-symlink path. Parent
/// directories are created as needed; an existing target is refused unless
/// [`UnpackOptions::overwrite`] is set.
pub fn unpack_archive(
    bytes: &[u8],
    destination: impl AsRef<Path>,
    options: &UnpackOptions,
) -> Result<UnpackedArchive, ArchiveError> {
    if let Some(expected) = options.expected_sha256.as_deref() {
        let actual = sha256_hex(bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(ArchiveError::ChecksumMismatch {
                expected: expected.to_owned(),
                actual,
            });
        }
    }
    let destination = destination.as_ref();
    fs::create_dir_all(destination)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    if archive.len() > options.max_entries {
        return Err(ArchiveError::TooManyEntries {
            actual: archive.len(),
            limit: options.max_entries,
        });
    }
    let mut entries = Vec::with_capacity(archive.len());
    let mut total_bytes = 0u64;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let raw_name = file.name().to_owned();
        let relative = safe_relative_path(&raw_name)?;
        let depth = relative.components().count();
        if depth > options.max_depth {
            return Err(ArchiveError::TooDeep {
                path: raw_name,
                actual: depth,
                limit: options.max_depth,
            });
        }
        if file.is_dir() {
            ensure_safe_parent(destination, &relative)?;
            fs::create_dir_all(destination.join(&relative))?;
            continue;
        }
        if is_symlink_mode(file.unix_mode()) {
            return Err(ArchiveError::Symlink(raw_name));
        }
        let declared = file.size();
        if declared > options.max_file_bytes {
            return Err(ArchiveError::FileTooLarge {
                path: raw_name,
                actual: declared,
                limit: options.max_file_bytes,
            });
        }
        let target = destination.join(&relative);
        ensure_safe_parent(destination, &relative)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut open = OpenOptions::new();
        open.write(true);
        if options.overwrite {
            open.create(true).truncate(true);
        } else {
            open.create_new(true);
        }
        let mut output = open.open(&target).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                ArchiveError::AlreadyExists(raw_name.clone())
            } else {
                ArchiveError::Io(error)
            }
        })?;
        // Bound the read independently of the header's declared size so a lying
        // ZIP cannot expand past the per-file cap.
        let limit = options.max_file_bytes.saturating_add(1);
        let copied = std::io::copy(&mut file.by_ref().take(limit), &mut output)?;
        if copied > options.max_file_bytes {
            return Err(ArchiveError::FileTooLarge {
                path: raw_name,
                actual: copied,
                limit: options.max_file_bytes,
            });
        }
        total_bytes = total_bytes
            .checked_add(copied)
            .ok_or(ArchiveError::TotalTooLarge {
                actual: u64::MAX,
                limit: options.max_total_bytes,
            })?;
        if total_bytes > options.max_total_bytes {
            return Err(ArchiveError::TotalTooLarge {
                actual: total_bytes,
                limit: options.max_total_bytes,
            });
        }
        if let Some(mode) = file.unix_mode() {
            apply_mode(&target, mode);
        }
        entries.push(UnpackedEntry {
            path: raw_name,
            bytes: copied,
        });
    }
    Ok(UnpackedArchive {
        entries,
        total_bytes,
    })
}

fn safe_relative_path(name: &str) -> Result<PathBuf, ArchiveError> {
    if name.is_empty() || name.contains('\\') {
        return Err(ArchiveError::UnsafePath(name.to_owned()));
    }
    let trimmed = name.trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(ArchiveError::UnsafePath(name.to_owned()));
    }
    let mut output = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => output.push(part),
            _ => return Err(ArchiveError::UnsafePath(name.to_owned())),
        }
    }
    if output.as_os_str().is_empty() {
        return Err(ArchiveError::UnsafePath(name.to_owned()));
    }
    Ok(output)
}

/// Reject a parent path that already contains a symlink: a link planted inside
/// the destination must not redirect a later entry outside it.
fn ensure_safe_parent(destination: &Path, relative: &Path) -> Result<(), ArchiveError> {
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let mut current = destination.to_path_buf();
    for component in parent.components() {
        let Component::Normal(part) = component else {
            return Err(ArchiveError::UnsafePath(relative.display().to_string()));
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ArchiveError::Symlink(current.display().to_string()));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(ArchiveError::UnsafePath(current.display().to_string()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(ArchiveError::Io(error)),
        }
    }
    Ok(())
}

fn is_symlink_mode(mode: Option<u32>) -> bool {
    matches!(mode, Some(mode) if mode & 0o170000 == 0o120000)
}

#[cfg(unix)]
fn apply_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777));
}

#[cfg(not(unix))]
fn apply_mode(_path: &Path, _mode: u32) {}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
