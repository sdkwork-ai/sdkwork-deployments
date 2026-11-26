# sdkwork-deploy-certificate-archive-port

Archives the **public** half of an issued certificate version into
sdkwork-drive, and decides where it goes.

When the certificate issuance executor stores a version, the tenant gains files it
can actually download. They live in a dedicated per-tenant Drive space named
`证书库`, laid out as:

```text
证书库/                                  <- the tenant's certificate space
  example.com-3f7c1a9b/                  <- sanitize(certName) + "-" + certId[0..8]
    v2-2027-02-22T09-00-00Z/             <- "v" + versionNo + "-" + notAfter
      cert.pem
      chain.pem
      root.pem
      fullchain.pem
      manifest.json
```

The **private key is never in that list.** It stays in the sealed custody store
(`secret://deploy/certificate/<uuid>`), and `bundle_policy` refuses
`MaterialKind::PrivateKey` rather than filtering it out, so no call site can
archive a key by accident. A `FULL` bundle policy — which the design permits only
under an explicit per-tenant decision — is refused, including at startup, rather
than silently downgraded to `PUBLIC`.

## Three layers, each testable alone

| Layer | Decides | I/O |
| --- | --- | --- |
| `layout` | folder names, file names, `manifest.json` | none |
| `retry` | when a row gets another attempt, and when it stops | none |
| `in_process` / `memory` | how the bytes reach Drive | PostgreSQL |

`layout` and `retry` are pure functions. That is not tidiness: the layout is
quoted to operators in the console, and a retry may happen in another process days
later. A name derived from anything but the certificate's own identity would make
the retry write a second copy, or overwrite the first.

## Retry

`30s → 2m → 10m → 1h → 6h`, five attempts, then `FAILED` until an operator asks
again. Each step is at least four times the previous, so a Drive outage is
survived rather than amplified. `attempt_count` and `next_attempt_at` live on the
`deploy_certificate_archive` row (migration `0019`), so the schedule survives a
restart and two workers cannot both retry the same row.

## Configuration

| Variable | Default | Values |
| --- | --- | --- |
| `SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE` | unset = `off` | `off`, `drive`, `memory` (dev only) |
| `SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE_BUNDLE` | `PUBLIC` | `PUBLIC`; `FULL` is refused today |
| `SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE_SPACE_NAME` | `证书库` | any non-empty string |
| `SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE_MAX_ATTEMPTS` | `5` | 1–20 |
| `SDKWORK_DEPLOY_CERTIFICATE_ARCHIVE_ATTEMPT_TIMEOUT_MILLIS` | `30000` | 1000–120000 |

**Unset is `off`, and `off` is `SKIPPED`** — never `FAILED` (an operator would
debug a Drive nobody configured) and never `ARCHIVED` (a false claim that the
files are downloadable). An unrecognised mode fails startup instead of falling
back.

## Drive constants that look odd, and why

- The space type is `certificate`, a first-class Drive space type, so the folder
  tree has its own ACL surface instead of mixing certificate bundles into the
  generic app-upload space.
- The space is app-owned as `app:sdkwork-deploy:system`, which is the owner Drive
  derives from `UploaderActor::System`. A mismatch there is a permission denial at
  runtime, so the pair is pinned by a unit test.
- Folders are created through `DriveNodeService::create_node` and bytes through
  `DriveUploaderService` (prepare → put → mark part → complete), so Drive keeps
  the writer ACL, content policy and node-version activation.
- The upload `task_id` is `cert-archive-<versionId>-<fileName>`: Drive returns the
  existing item for a task id it has seen, which is what makes a retried file
  reuse its node instead of creating a numbered sibling.

## Verification

```sh
cargo test -p sdkwork-deploy-certificate-archive-port
```

47 unit tests cover the folder and file naming (including path-traversal and
wildcard inputs), the manifest schema and round-trip, the retry ladder, the
selection rules, and the private-key invariant from several directions.

Not covered here: the transactional path (the `PENDING` row written inside the
version transaction) and a live Drive upload both need a database, and are
verified only by `cargo check` plus the repository's own gates.
