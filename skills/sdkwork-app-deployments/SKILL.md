---
name: sdkwork-app-deployments
description: Use when any SDKWork application operation is required: resolving or creating the application, building and packaging, uploading an immutable artifact, recording the application source as an archive or Git directory, creating a release, deploying, verifying, or rolling back with explicit approval.
owner: sdkwork-deployments maintainers
module: sdkwork-deploy
standards: APPLICATION_IDENTITY_SPEC.md, SDKWORK_DEPLOY_SPEC.md, PACKAGING_SPEC.md, SDK_SPEC.md, AGENT_SKILLS_SPEC.md
---

# SDKWork Application Lifecycle

One skill for the whole application lifecycle: resolve the application, build and package, upload the
immutable artifact, record its source, create a release, deploy, verify, and roll back. Keep build
declarations in `agent.app.config.json`, concrete environment values in `etc/`, artifact bytes in
Drive, and release/deployment state in SDKWork Deploy.

## When to use

Use this skill for any SDKWork application operation:

- create, retrieve, or update the Deploy application; activate or pause it;
- list or change app domains, environment variables, platform targets, resource links, composition,
  health checks, database profiles and migrations, environments and promotions, signing identities,
  usage events, or remix settings;
- declare source specs or source repositories, or set and validate the application source;
- build, package, or upload an application artifact;
- create a release, start a deployment, verify deployment evidence, or roll back with explicit
  approval.

Do not use this skill for:

- publishing an app scaffold as a Deploy app template — use `sdkwork-app-scaffold`;
- registering the commerce product listing or app-store catalog — that is a commerce boundary;
- platform-level domain zones, TLS certificates, or gateway administration — that is the Deploy
  control-plane surface, not an app operation.

## Inputs

- The nearest `AGENTS.md`, `agent.app.config.json`, applicable component `specs/`, and
  `etc/sdkwork.deployment.config.json`.
- Application id, slug, or name. Stop when the match is ambiguous; never create a second app to work
  around resolution.
- `devApp.build.targets[]` entries: `{ id, command, packageId, outputs[] }`, where each output is
  `{ path, type: "file" | "directory", fileName? }`.
- Source mode: `archive` (default) or `git`, with `repoProvider`, `repoUrl`, optional `repoRef`,
  optional `subPath`, optional `credentialSecretRef`, optional `cloneMode`.
- Release and deployment inputs: `platformTargetId`, `semanticVersion`, `deploymentKind`,
  `deploymentTarget`, and environment.
- An authenticated `deployClient` and `driveClient` from the approved bootstrap or injected provider.
- Explicit confirmation for a production action, a public listing, or when more than one app, target,
  or profile matches.

### Authentication

The scripts use the shared runtime at `tools/agent-skills-sdk/`. Log in once per profile, or inject
`SDKWORK_AUTH_TOKEN` plus `SDKWORK_ACCESS_TOKEN`. The public application access token
(`SDKWORK_ACCESS_TOKEN`) is isolation material; protected operations require a session.

```bash
# Diagnose base URLs, tokens, and SDK resolution from the target app root
node skills/sdkwork-app-deployments/scripts/run.mjs doctor --cwd <app-root> --check-sdk-import

# Password on stdin; the session is cached at ~/.sdkwork/agent-skills/<profile>.json (0600)
node skills/sdkwork-app-deployments/scripts/run.mjs login \
  --cwd <app-root> --iam-base-url "$SDKWORK_IAM_APP_API_BASE_URL" \
  --account you@sdkwork.com --password-stdin --profile standalone.development
```

Full credential, refresh, logout, and base-URL contract:
[references/auth.md](references/auth.md).

In this repository the app root that declares all four SDKs is
`apps/sdkwork-deployments-pc`, so pass `--cwd apps/sdkwork-deployments-pc` when
running from the repository root.

The SDKs are published to npm. Install `@sdkwork/deployments-app-sdk`,
`@sdkwork/deployments-backend-sdk`, `@sdkwork/drive-app-sdk`, and
`@sdkwork/iam-app-sdk` in the target app; the runtime uses the installed
packages, runs on plain Node for published builds, and enables the TypeScript
transform only for a local sdkwork-space source checkout. `doctor` reports
`runtimeMode`, the resolved entry, and installed-versus-contract SDK versions.

## Workflow

### Scripts and generic API calls

Both skills share one runtime. Prefer the dedicated workflow below; use the generic dispatcher for
an operation that has no dedicated step. It still calls the generated SDK transport, never raw HTTP.

```bash
# Any app-surface operation from the generated registry
node skills/sdkwork-app-deployments/scripts/run.mjs api --cwd <app-root> \
  --operation apps.source.update --path-param appId=<id> --input source.json

# Upload + register an immutable artifact (Drive uploader + artifact.create)
node skills/sdkwork-app-deployments/scripts/run.mjs upload --cwd <app-root> \
  --app-id <id> --file dist/app.zip --package-type 1

# Discover operations (app and backend surfaces)
node skills/sdkwork-app-deployments/scripts/run.mjs ops --api backend --filter appTemplates
```

The generated operation map is [references/api-map.md](references/api-map.md). Operation ids that
exist on both surfaces (for example `appTemplates.list`) require `--api app|backend`.

### Application operation map

| Operation | SDK call |
| --- | --- |
| List / create / retrieve / update | `deployClient.app.list`, `.create`, `.retrieve`, `.update` |
| Activate / pause | `deployClient.app.activate`, `.pause` |
| Domains / env variables / health checks | `deployClient.app.domains.list`, `deployClient.envVariable.*`, `deployClient.monitor.*` |
| Database profiles / migrations | `deployClient.appDatabase.*` |
| Environments / promotions | `deployClient.appEnvironment.*` |
| Signing identities / usage events | `deployClient.signing.*`, `deployClient.usage.list` |
| Platform targets / resource links / composition / remix | `deployClient.app.platformTargets.*`, `.resourceLinks.*`, `.composition.update`, `.remix.*` |
| Source | `deployClient.app.source.retrieve`, `.update`, `.delete`, `.validate` |
| Source specs / repositories | `deployClient.app.sourceSpecs.*`, `.sourceRepositories.*` |
| Build / package | `deployClient.build.*`, `deployClient.package.*` |
| Upload session / artifact | `driveClient.uploader.uploadArchive`, `deployClient.uploadSession.*`, `deployClient.artifact.*` |
| Release / release history | `deployClient.release.create`, `.list`, `.retrieve` |
| Deployment / status | `deployClient.deployment.create`, `.list`, `.retrieve` |

The publish-and-deliver path below is the default. Use the map directly for a management-only
request.

### 1. Resolve the application

Resolve by exact id, slug, or name. For a new app, call `app.create` with `name` and `appKind`;
never infer an app from a fuzzy name match. Reuse an existing app when the lookup is exact.

### 2. Build and upload the artifact

1. Run only the selected `devApp.build.targets[].command`. If the targets are empty or the repo uses
   a legacy target list, stop and ask for a declared target; never infer `dist` or `build`.
2. Package each declared output. A `directory` output is zipped with its contents at the archive
   root; a `file` output is uploaded byte-for-byte.
3. Compute the SHA-256 hex digest of the final bytes.
4. Upload with `driveClient.uploader.uploadArchive` (`appResourceType: "deploy.artifact"`,
   `appResourceId: <appId>`, `scene: "deployment-package"`).
5. Register the artifact with `deployClient.artifact.create`. Pass the upload session, space, and node
   ids from the upload result; `siteId` carries the application id in the current contract.

Read [references/source-and-artifact.md](references/source-and-artifact.md) for the exact calls.

### 3. Record the application source

- Archive mode (default): `app.source.update(appId, { kind: "ARCHIVE", archive: { artifactId } })`.
- Git mode: `app.source.update(appId, { kind: "GIT", git: { repoProvider, repoUrl, repoRef, subPath,
  credentialSecretRef, cloneMode } })`, then `app.source.validate(appId)`. A Git source is ready only
  when `git.repoStatus` is `VALIDATED`.
- Read back with `app.source.retrieve(appId)`: `sourceKind` must match and `readiness` must be
  `READY`.

### 4. Release and deploy

Use the composed publisher for the atomic path: application resolution, Drive upload, artifact
registration, release creation, and deployment creation. Pass `release.platformTargetId` and
`release.semanticVersion`; `packageId` defaults to the newly registered artifact. The deployment
block is optional.

Read [references/publisher-flow.md](references/publisher-flow.md) before invoking the publisher.

### 5. Verify and report

Retrieve the deployment through the SDK and report the raw status and timestamps; run the declared
health or smoke check. Never claim success from deployment creation alone.

### 6. Roll back

Rollback is a forward fix only, and only with explicit approval of the application and deployment id.
There is no `deployments.rollback` operation: create a new deployment against the release you want to
return to, and read lineage from the response-only `rollbackFromDeploymentId` field.

## Rules

- Use only the generated SDK clients, the shared skill runtime, or the composed publisher. No raw
  HTTP, manual authorization headers, or local SDK forks.
- Run the skill scripts from the target application root, or pass `--cwd <app-root>`; that root must
  declare the SDK packages. The scripts load the public SDK entrypoints, never generated `dist`
  internals.
- Keep session tokens out of argv, tracked files, and logs. Authenticate through the login command or
  environment variables; the session cache is user-private runtime state.
- Keep backend and app login profiles isolated. Never write tokens, passwords, presigned URLs, or
  secret values into tracked files, logs, or evidence.
- A Git `repoUrl` must not embed credentials; use `credentialSecretRef`. `subPath` must stay
  repository-relative and must not be absolute or contain `..`.
- Package type values: `1` archive, `2` static site, `3` Docker image, `4` jar, `5` war.
- Reuse one idempotency key per intent. A retry of the same bytes, app, and source intent keeps its
  key; a new artifact or changed source intent gets a new key.
- Never rebuild an artifact during deployment or during a retry; deploy the reviewed immutable
  archive and digest.
- Do not run the separate `sdkwork-dev-app` PlusApp registration flow unless the request also
  requires PlusApp catalog registration.
- Stop at the failed stage and preserve every id returned by earlier completed stages.
- Request explicit confirmation before a production deployment or first production app creation when
  the request does not already name that exact action.

## Verification

```bash
# From the repository root: skill package, generated registry, and runtime
node ../sdkwork-specs/tools/check-agent-skills.mjs --root .
node tools/agent-skills-sdk/generate-operations.mjs --check
node tools/agent-skills-sdk/generate-api-map.mjs --check
node --test tools/agent-skills-sdk/runtime.test.mjs

# Runtime evidence (run through the SDK, not curl)
# deployClient.app.source.retrieve(appId)                -> sourceKind, readiness, archive/git
# deployClient.artifact.retrieve(artifactId)             -> checksumSha256, contentLength
# deployClient.release.retrieve(appId, releaseId)        -> release id and version
# deployClient.deployment.retrieve(appId, deploymentId)  -> status and timestamps
```

An operation is complete only when the artifact digest, source read-back, release, and deployment
evidence agree. A source that reads back `MISSING`, an incomplete upload, or an unverified deployment
is incomplete work.

## Output

Report:

- application id, root, and whether it was reused or created;
- operation performed, with the relevant ids (artifact, release, deployment, source spec, env var);
- build target, archive name, byte size, and SHA-256 digest;
- source mode, and for Git the provider, URL, ref, `subPath`, and `repoStatus`;
- Drive upload session and item ids;
- release version and deployment status, timestamps, and health result;
- retained rollback target, or why no rollback target is available.
