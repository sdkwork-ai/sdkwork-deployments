# Application Source And Artifact Contract

Two contracts are involved. The artifact carries bytes; the application source records where those
bytes (or the Git repository) live.

| Concern | Operation | Key fields |
| --- | --- | --- |
| Upload bytes | `driveClient.uploader.uploadArchive` | `appResourceType: "deploy.artifact"`, `appResourceId`, `scene`, `originalFileName`, `contentType`, `checksumSha256Hex` |
| Register artifact | `deployClient.artifact.create` | `siteId` (carries the app id), `packageType`, `fileName`, `contentType`, `contentLength`, `checksumSha256`, `driveUploadSessionId`, `driveUploadItemId`, `driveSpaceId`, `driveNodeId`, `idempotencyKey` |
| Record archive source | `deployClient.app.source.update` | `{ kind: "ARCHIVE", archive: { artifactId } }` |
| Record Git source | `deployClient.app.source.update` | `{ kind: "GIT", git: { repoProvider, repoUrl, repoRef, subPath, credentialSecretRef, cloneMode } }` |
| Probe Git source | `deployClient.app.source.validate` | path `appId`; no body |
| Read source | `deployClient.app.source.retrieve` | `sourceKind`, `readiness`, `git`, `archive` |

`packageType`: `1` archive, `2` static site, `3` Docker image, `4` jar, `5` war.

## Archive Mode

```ts
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

const bytes = await readFile(archivePath);
const digest = createHash("sha256").update(bytes).digest("hex");
const fileName = "sdkwork-demo-1.4.0-linux-x64.zip";

const file = new Blob([bytes], { type: "application/zip" });
const upload = await driveClient.uploader.uploadArchive({
  file,
  appResourceType: "deploy.artifact",
  appResourceId: appId,
  scene: "deployment-package",
  originalFileName: fileName,
  contentType: "application/zip",
  checksumSha256Hex: digest,
});

const artifact = await deployClient.artifact.create(
  {
    siteId: appId, // the artifact request still names the owning app as siteId
    packageType: 1,
    fileName,
    contentType: "application/zip",
    contentLength: String(bytes.byteLength),
    checksumSha256: digest,
    driveUploadSessionId: upload.uploadSession.id,
    driveUploadItemId: upload.uploadItem.id,
    driveSpaceId: upload.uploadItem.spaceId,
    driveNodeId: upload.uploadItem.nodeId,
    idempotencyKey,
  },
  { idempotencyKey },
);

const source = await deployClient.app.source.update(appId, {
  kind: "ARCHIVE",
  archive: { artifactId: artifact.id },
});
```

The archive root is the package root. Do not add a wrapper directory that the delivery plane would
serve as an extra path segment.

## Git Mode

```ts
const source = await deployClient.app.source.update(appId, {
  kind: "GIT",
  git: {
    repoProvider: "GITHUB", // GITHUB | GITEE | GITLAB | SELF_HOSTED
    repoUrl: "https://github.com/example/sdkwork-demo.git",
    repoRef: "main", // branch, tag, or commit; omit for the default branch
    subPath: "apps/sdkwork-demo-pc", // repository-relative directory; omit for the repository root
    cloneMode: "SHALLOW", // FULL | SHALLOW
    credentialSecretRef: "deploy/git/example-sdkwork-demo", // only when the repository is private
  },
});

const validated = await deployClient.app.source.validate(appId);
if (validated.git?.repoStatus !== "VALIDATED") {
  throw new Error(
    `Git source is not ready: ${validated.git?.repoStatus} ${validated.git?.lastErrorCode ?? ""}`,
  );
}
```

A Git write lands in `PENDING`. Only `apps.source.validate` records `VALIDATED` or `INVALID`.
`readiness` stays `MISSING` until then. When `credentialsRequired` is true, stop and ask for a
stored credential reference; never place a token in `repoUrl`.

## Read-Back

```ts
const current = await deployClient.app.source.retrieve(appId);
if (current.readiness !== "READY" || current.sourceKind !== expectedKind) {
  throw new Error(`source read-back failed: ${current.readiness} ${current.sourceKind ?? ""}`);
}
```

Treat `GIT_REPOSITORY_PENDING`, `GIT_REPOSITORY_INVALID`, `ARCHIVE_UNAVAILABLE`, and
`SOURCE_KIND_MISMATCH` as named failures to report, not as success.

## Idempotency And Retry

- Compute the key from the intent: app id + source kind + digest (archive) or app id + source kind +
  URL + ref + subPath (Git).
- Retrying the same bytes with the same key returns the same artifact instead of a duplicate.
- A rebuilt archive has different bytes and a different digest, so it needs a new key.

## Boundaries

- Obtain `deployClient` and `driveClient` from the application's approved bootstrap or injected SDK
  provider. Do not construct clients from raw base URLs.
- Do not call the Drive or Deploy HTTP endpoints directly, and do not reimplement the upload session
  protocol.
- Upload only the reviewed archive. Deployment never rebuilds.
