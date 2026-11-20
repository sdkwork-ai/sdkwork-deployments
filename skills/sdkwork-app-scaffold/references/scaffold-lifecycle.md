# Scaffold Lifecycle Contract

The scaffold is the deploy-owned design-time generator input. Its physical API is `scaffolds`;
its concept is Scaffold. A scaffold carries listing metadata, and each immutable version carries the
uploaded package.

| Step | Operation | Required fields |
| --- | --- | --- |
| Resolve category | `app.scaffolds.categories.list()` | match `categoryKey` or `id` exactly |
| List / retrieve scaffold | `app.scaffolds.list`, `.retrieve` | exact `scaffoldKey` for reuse |
| Upload package | `driveClient.uploader.uploadArchive` | `appResourceType: "deploy.artifact"`, `appResourceId`, `originalFileName`, `contentType`, `checksumSha256Hex` |
| Register package | `artifact.create` | `siteId`, `packageType: 1`, `fileName`, `contentType`, `contentLength`, `checksumSha256`, `driveUploadSessionId`, `driveUploadItemId`, `driveSpaceId`, `driveNodeId`, `idempotencyKey` |
| Create scaffold | `app.scaffolds.create` | `appUuid`, `categoryUuid`, `scaffoldKey`, `displayName`, `summary`, optional `initialVersion` |
| Update listing | `app.scaffolds.update` | listing fields only; never a version |
| Add version | `app.scaffolds.versions.create` | `version`, `artifactUuid`, `checksumSha256`, `packageSizeBytes`, `platformTargets`, `changelog` |
| Submit review | `app.scaffolds.submit` | path `scaffoldUuid`; no body |
| Track status | `app.scaffolds.retrieve`, `app.scaffolds.versions.list` | `status`, `latestVersionUuid` |
| Withdraw | `app.scaffolds.delete` | path `scaffoldUuid`; requires explicit approval |
| Marketplace read | `app.marketplace.scaffolds.list`, `.retrieve` | public view of a published scaffold |

`packageSizeBytes` and `priceMinor` are int64 strings. `scaffoldType` is `APP`, `PPT`, or `VIDEO`
(default `APP`).

## Resolve The Category

```ts
const categories = await deployClient.app.scaffolds.categories.list();
const matches = categories.items.filter(
  (category) => category.id === categoryRef || category.categoryKey === categoryRef,
);
if (matches.length !== 1) {
  throw new Error(`category must resolve to exactly one entry, got ${matches.length}`);
}
const categoryUuid = matches[0].id;
```

Never invent a category, and never pick the first entry when more than one matches.

## Upload The Scaffold Package

```ts
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

const bytes = await readFile(scaffoldArchivePath);
const digest = createHash("sha256").update(bytes).digest("hex");
const fileName = `sdkwork-demo-scaffold-1.2.0.zip`;

const upload = await driveClient.uploader.uploadArchive({
  file: new Blob([bytes], { type: "application/zip" }),
  appResourceType: "deploy.artifact",
  appResourceId: sourceAppId,
  scene: "application-scaffold",
  originalFileName: fileName,
  contentType: "application/zip",
  checksumSha256Hex: digest,
});

const artifact = await deployClient.artifact.create(
  {
    siteId: sourceAppId, // the artifact request still names the owning app as siteId
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
```

## Create Or Reuse The Listing

```ts
const existing = (await deployClient.app.scaffolds.list({
  keyword: scaffoldKey,
  pageSize: 50,
})).items.filter((item) => item.scaffoldKey === scaffoldKey);

const version = {
  version: semanticVersion,
  changelog,
  artifactUuid: artifact.id,
  checksumSha256: digest,
  packageSizeBytes: String(bytes.byteLength),
  platformTargets,
  ...(sourceAppVersion !== undefined ? { sourceAppVersion } : {}),
};

const template = existing.length === 1
  ? await deployClient.app.scaffolds.update(
      existing[0].id,
      { displayName, summary, visibility, pricingModel },
      { idempotencyKey: updateIdempotencyKey },
    )
  : await deployClient.app.scaffolds.create(
      {
        appUuid: sourceAppId,
        categoryUuid,
        scaffoldKey,
        displayName,
        summary,
        visibility,
        pricingModel,
        initialVersion: version,
      },
      { idempotencyKey: createIdempotencyKey },
    );

if (existing.length === 1) {
  await deployClient.app.scaffolds.versions.create(
    template.id,
    version,
    { idempotencyKey: versionIdempotencyKey },
  );
}
```

`list({ keyword })` is a search, not an identity lookup. Confirm the exact `scaffoldKey` before
updating; stop when zero or several templates match.

## Submit, Track, Update, Withdraw

```ts
if (submitForReview) {
  await deployClient.app.scaffolds.submit(template.id);
}

const current = await deployClient.app.scaffolds.retrieve(template.id);
const versions = await deployClient.app.scaffolds.versions.list(template.id);
const latest = versions.items.find((item) => item.templateVersion === semanticVersion);

if (!latest || latest.artifactUuid !== artifact.id || latest.checksumSha256 !== digest) {
  throw new Error("scaffold version read-back does not match the uploaded artifact");
}
```

- `DRAFT` -> `PENDING_REVIEW` -> `PUBLISHED` is the review path; `REJECTED` stays editable.
- `scaffolds.update(scaffoldUuid, { ... }, { idempotencyKey })` changes listing metadata only.
- `scaffolds.delete(scaffoldUuid)` withdraws the listing; ask for explicit approval first.
- `marketplace.scaffolds.list({ templateType: "APP" })` and `.retrieve(scaffoldUuid)` read the public
  view.

## Git Acquisition

The version contract has no Git URL field. In Git mode:

1. Clone `repoUrl` at `repoRef` (shallow by default).
2. Package the `subPath` directory as the archive root.
3. Upload that archive exactly as above.

When the Git source must also be visible on the platform, additionally bind it through
`sdkwork-app-deployments` Git mode on the source app. That is source provenance, not the scaffold
version.

## Idempotency And Boundaries

- One idempotency key per intent: create, update, version, and artifact each get their own key.
- A retry with the same bytes and the same key returns the same artifact instead of a duplicate.
- A changed archive or version needs a new key.
- Never edit a published version; add a new one.
- Never write tokens, presigned URLs, or secret values into tracked files, logs, or evidence.
