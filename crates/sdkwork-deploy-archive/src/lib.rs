//! Reusable directory packaging and extraction for SDKWork Deploy.
//!
//! The component is deliberately independent of the deploy service, HTTP
//! surfaces, and Drive. It turns one directory into a deterministic ZIP using
//! git's source-selection rules (`.gitignore` cascade, negation, anchored and
//! directory patterns) and extracts a ZIP without allowing traversal, symlink,
//! or decompression-bomb escapes.
//!
//! ```no_run
//! use sdkwork_deploy_archive::{pack_directory, unpack_archive, PackOptions, UnpackOptions};
//!
//! let packed = pack_directory("apps/my-app", &PackOptions::default())?;
//! std::fs::write("my-app.zip", &packed.bytes)?;
//! let unpacked = unpack_archive(&packed.bytes, "out", &UnpackOptions::default())?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod pack;
mod unpack;

use std::io;

pub use pack::{
    pack_directory, PackOptions, PackedArchive, PackedEntry, SkippedEntry, SkippedReason,
};
pub use unpack::{unpack_archive, UnpackOptions, UnpackedArchive, UnpackedEntry};

/// Default bound on included files, matching the template host walk.
pub const DEFAULT_MAX_ENTRIES: usize = 20_000;
/// Default bound on the sum of included file bytes (2 GiB).
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Default bound on one included file (64 MiB).
pub const DEFAULT_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Default bound on the relative path depth (segments).
pub const DEFAULT_MAX_DEPTH: usize = 24;

/// Packaging/extraction failure.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive io: {0}")]
    Io(#[from] io::Error),
    #[error("archive zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("archive ignore rules: {0}")]
    Ignore(#[from] ignore::Error),
    #[error("archive root is not a directory: {0}")]
    NotDirectory(String),
    #[error("archive contains no packaging-eligible files")]
    Empty,
    #[error("archive entry count {actual} exceeds the {limit} limit")]
    TooManyEntries { actual: usize, limit: usize },
    #[error("archive path depth {actual} exceeds the {limit} limit: {path}")]
    TooDeep {
        path: String,
        actual: usize,
        limit: usize,
    },
    #[error("archive file is {actual} bytes, over the {limit} byte limit: {path}")]
    FileTooLarge {
        path: String,
        actual: u64,
        limit: u64,
    },
    #[error("archive total is {actual} bytes, over the {limit} byte limit")]
    TotalTooLarge { actual: u64, limit: u64 },
    #[error("archive entry escapes the destination: {0}")]
    UnsafePath(String),
    #[error("archive entry is a symbolic link and is refused: {0}")]
    Symlink(String),
    #[error("archive entry already exists: {0}")]
    AlreadyExists(String),
    #[error("archive checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}
