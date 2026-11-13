# sdkwork-deploy-archive

Reusable directory packaging and extraction for SDKWork Deploy.

The crate is independent of the HTTP, database, Drive, and service layers. It
accepts a filesystem directory, applies git-compatible source selection, and
returns deterministic ZIP bytes. The inverse operation accepts ZIP bytes and
extracts them without allowing path traversal, symbolic links, or
decompression-bomb growth.

## Pack

```rust
use sdkwork_deploy_archive::{pack_directory, PackOptions};

let archive = pack_directory("apps/my-app", &PackOptions::default())?;
std::fs::write("my-app.zip", &archive.bytes)?;
```

`pack_directory` honors every nested `.gitignore` with git's cascade and
negation rules (including directory-only, anchored, and `**` patterns), prunes
an ignored directory with its descendants, always excludes VCS metadata by
default, and excludes machine-local secret carriers such as `.env`, `.local`
overrides, registry credentials, and private-key formats. Files are sorted by
posix path; timestamps are fixed at the 1980-01-01 ZIP epoch; directory entries
are omitted. Identical inputs therefore produce identical bytes and checksum.

Hidden source files remain included, and `.gitignore` itself stays in the
package unless a rule excludes it.

Bounds default to 20,000 files, 2 GiB total, 64 MiB per file, and 24 path
segments. Exceeding a bound fails the pack instead of silently truncating it.
Symbolic links and special files are reported in `skipped`; a secret or custom
rule exclusion is reported without leaking file contents.

## Unpack

```rust
use sdkwork_deploy_archive::{unpack_archive, UnpackOptions};

let result = unpack_archive(&archive.bytes, "out", &UnpackOptions {
    expected_sha256: Some(archive.checksum_sha256),
    ..UnpackOptions::default()
})?;
```

`unpack_archive` refuses absolute paths, `..`, backslash-ambiguous names,
symlink entries, a target that already exists unless `overwrite` is set, and
files whose actual decompressed bytes exceed the configured limits.

## Verification

```bash
cargo test -p sdkwork-deploy-archive
```

The tests pin root/nested ignore cascades, negation, pruning, custom rules,
secret exclusion, deterministic output, limits, round-trip checksums,
traversal, overwrite refusal, and symlink refusal.
