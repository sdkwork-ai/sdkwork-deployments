//! Pack/unpack contract tests for the reusable archive component.

use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use sdkwork_deploy_archive::{
    pack_directory, unpack_archive, ArchiveError, PackOptions, SkippedReason, UnpackOptions,
};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, content).expect("write fixture");
}

fn paths(pack: &sdkwork_deploy_archive::PackedArchive) -> Vec<&str> {
    pack.entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect()
}

#[test]
fn gitignore_cascade_prunes_and_reincludes_like_git() {
    let root = tempdir().expect("tempdir");
    write(
        root.path(),
        ".gitignore",
        "dist/\n*.log\n!keep.log\n/root-only.txt\n",
    );
    write(root.path(), "debug.log", "ignored");
    write(root.path(), "keep.log", "kept");
    write(root.path(), "dist/bundle.js", "ignored");
    write(root.path(), "root-only.txt", "ignored");
    write(root.path(), "nested/root-only.txt", "kept");
    write(root.path(), "src/app.ts", "source");
    write(root.path(), "src/generated/out.ts", "ignored");
    write(root.path(), "src/.gitignore", "generated/\n");
    write(root.path(), ".env", "SECRET=1");
    write(root.path(), "private.pem", "secret");
    write(root.path(), ".git/config", "metadata");
    write(root.path(), ".github/workflow.yml", "hidden but source");

    let pack = pack_directory(root.path(), &PackOptions::default()).expect("pack");
    assert_eq!(
        paths(&pack),
        vec![
            ".github/workflow.yml",
            ".gitignore",
            "keep.log",
            "nested/root-only.txt",
            "src/.gitignore",
            "src/app.ts",
        ]
    );
    assert!(pack
        .skipped
        .iter()
        .any(|entry| entry.path == ".env" && entry.reason == SkippedReason::Sensitive));
    assert!(pack
        .skipped
        .iter()
        .any(|entry| entry.path == "private.pem" && entry.reason == SkippedReason::Sensitive));
    assert!(!pack.skipped.iter().any(|entry| entry.path == "debug.log"));
    assert!(!pack.bytes.is_empty());
}

#[test]
fn pack_is_deterministic_and_honors_custom_excludes() {
    let root = tempdir().expect("tempdir");
    write(root.path(), "src/a.ts", "a");
    write(root.path(), "test/a.test.ts", "test");
    let options = PackOptions {
        custom_excludes: vec!["test/".to_owned()],
        ..PackOptions::default()
    };
    let first = pack_directory(root.path(), &options).expect("first");
    let second = pack_directory(root.path(), &options).expect("second");
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.checksum_sha256, second.checksum_sha256);
    assert_eq!(paths(&first), vec!["src/a.ts"]);
    assert!(first
        .skipped
        .iter()
        .any(|entry| entry.path == "test/a.test.ts" && entry.reason == SkippedReason::CustomRule));
}

#[test]
fn pack_refuses_empty_and_over_limit_inputs() {
    let empty = tempdir().expect("empty");
    assert!(matches!(
        pack_directory(empty.path(), &PackOptions::default()),
        Err(ArchiveError::Empty)
    ));

    let root = tempdir().expect("tempdir");
    write(root.path(), "a.txt", "a");
    write(root.path(), "b.txt", "b");
    let too_many = PackOptions {
        max_entries: 1,
        ..PackOptions::default()
    };
    assert!(matches!(
        pack_directory(root.path(), &too_many),
        Err(ArchiveError::TooManyEntries { .. })
    ));
    let too_large = PackOptions {
        max_total_bytes: 1,
        ..PackOptions::default()
    };
    assert!(matches!(
        pack_directory(root.path(), &too_large),
        Err(ArchiveError::TotalTooLarge { .. })
    ));
}

#[test]
fn unpack_round_trip_and_refuses_checksum_mismatch() {
    let root = tempdir().expect("root");
    write(root.path(), "src/a.ts", "a");
    write(root.path(), "README.md", "readme");
    let packed = pack_directory(root.path(), &PackOptions::default()).expect("pack");
    let output = tempdir().expect("output");
    let unpacked = unpack_archive(
        &packed.bytes,
        output.path(),
        &UnpackOptions {
            expected_sha256: Some(packed.checksum_sha256.clone()),
            ..UnpackOptions::default()
        },
    )
    .expect("unpack");
    assert_eq!(unpacked.total_bytes, 7);
    assert_eq!(
        fs::read_to_string(output.path().join("src/a.ts")).expect("file"),
        "a"
    );

    assert!(matches!(
        unpack_archive(
            &packed.bytes,
            output.path(),
            &UnpackOptions {
                expected_sha256: Some("00".repeat(32)),
                overwrite: true,
                ..UnpackOptions::default()
            },
        ),
        Err(ArchiveError::ChecksumMismatch { .. })
    ));
}

#[test]
fn unpack_refuses_traversal_and_overwrite() {
    let root = tempdir().expect("root");
    write(root.path(), "safe.txt", "safe");
    let packed = pack_directory(root.path(), &PackOptions::default()).expect("pack");
    let output = tempdir().expect("output");
    unpack_archive(&packed.bytes, output.path(), &UnpackOptions::default()).expect("first");
    assert!(matches!(
        unpack_archive(&packed.bytes, output.path(), &UnpackOptions::default()),
        Err(ArchiveError::AlreadyExists(_))
    ));

    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("../escape.txt", SimpleFileOptions::default())
        .expect("start");
    writer.write_all(b"escape").expect("write");
    let malicious = writer.finish().expect("finish").into_inner();
    assert!(matches!(
        unpack_archive(&malicious, output.path(), &UnpackOptions::default()),
        Err(ArchiveError::UnsafePath(_))
    ));
}

#[cfg(unix)]
#[test]
fn pack_skips_symlinks_and_unpack_refuses_symlink_entries() {
    use std::os::unix::fs::symlink;

    let root = tempdir().expect("root");
    write(root.path(), "real.txt", "real");
    symlink(root.path().join("real.txt"), root.path().join("link.txt")).expect("symlink");
    let pack = pack_directory(root.path(), &PackOptions::default()).expect("pack");
    assert_eq!(paths(&pack), vec!["real.txt"]);
    assert!(pack
        .skipped
        .iter()
        .any(|entry| entry.path == "link.txt" && entry.reason == SkippedReason::Symlink));

    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().unix_permissions(0o120777);
    writer.start_file("link", options).expect("start");
    writer.write_all(b"target").expect("write");
    let bytes = writer.finish().expect("finish").into_inner();
    let output = tempdir().expect("output");
    assert!(matches!(
        unpack_archive(&bytes, output.path(), &UnpackOptions::default()),
        Err(ArchiveError::Symlink(_))
    ));
}
