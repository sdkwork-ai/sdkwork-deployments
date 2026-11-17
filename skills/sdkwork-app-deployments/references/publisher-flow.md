# Application Publisher Flow

Use the public composed facade rather than generated transport internals:

```ts
import { createDeployApplicationPublisher } from "@sdkwork/deployments-app-sdk/application-publisher";

const publisher = createDeployApplicationPublisher({
  deployClient,
  driveClient,
});

const result = await publisher.publish({
  app: {
    kind: "resolveOrCreate", // or { kind: "existing", appId }
    name: applicationName,
    appKind: "SPA_WEB",
    slug: applicationSlug, // optional
  },
  artifact: {
    file: packageFile,
    packageType: 1,
    fileName: archiveName,
    contentType: "application/zip",
    checksumSha256,
    source: applicationKey,
  },
  release: {
    platformTargetId,
    semanticVersion: versionTag,
  },
  deployment: {
    platformTargetId,
    deploymentKind: "ARTIFACT_RELEASE",
    deploymentTarget: "WEB_NODE",
    environment,
  },
  onProgress(progress) {
    reportProgress(progress);
  },
});
```

Contract notes:

- `app.kind: "resolveOrCreate"` requires `name` and `appKind`; `{ kind: "existing", appId }`
  reuses an application by exact id. Resolution never matches by fuzzy name.
- `appKind` is `STATIC_WEB`, `SPA_WEB`, `API_SERVICE`, `WECHAT_MINIPROGRAM`, `DOUYIN_MINIPROGRAM`,
  `IOS_APP`, `ANDROID_APP`, `HARMONYOS_APP`, or `DESKTOP_APP`.
- `artifact.packageType` is `1` archive, `2` static site, `3` Docker image, `4` jar, `5` war.
- `release.packageId` defaults to the artifact registered in this call; set it only when reusing an
  existing artifact. `release.semanticVersion` is required.
- `deployment` is optional. When present, the publisher creates it against the new release and
  fills `releaseId` and `idempotencyKey`.
- The publisher always creates a release. To upload an artifact without a release, use the
  upload-and-source path in this skill (Workflow steps 2-3) and the `source-and-artifact.md`
  reference.

Obtain `deployClient` and `driveClient` from the target application's approved bootstrap or injected
SDK provider. `packageFile` must implement the Drive uploader blob contract and represent the final
non-empty archive. Compute `checksumSha256` from those exact bytes before calling `publish`.

The result contains stable evidence:

```ts
const {
  app,
  upload,
  artifact,
  release,
  deployment,
} = result;
```

When a deployment was requested, retrieve it through the generated facade:

```ts
const current = await deployClient.deployment.retrieve(
  app.id,
  deployment.id,
);
```

Do not map numeric deployment statuses to names unless the active API contract defines that mapping.
Preserve the raw status and timestamps in completion evidence.

Rollback is a separate confirmed action. There is no `deployments.rollback` operation: create a new
deployment against the release you want to return to, and read the lineage back from the
response-only `rollbackFromDeploymentId` field.

```ts
const rollback = await deployClient.deployment.create(
  app.id,
  {
    platformTargetId,
    releaseId: previousReleaseId,
    deploymentKind: 'ARTIFACT_RELEASE',
    deploymentTarget: 'WEB_NODE',
    environment: 'production',
    idempotencyKey,
  },
  { idempotencyKey },
);
```

Never call rollback speculatively or use it as a deployment verification step.
