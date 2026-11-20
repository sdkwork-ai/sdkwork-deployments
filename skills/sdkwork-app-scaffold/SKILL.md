---
name: sdkwork-app-scaffold
description: Use when any SDKWork app scaffold lifecycle operation is required: creating or updating the scaffold listing, packaging from an archive or Git directory, uploading the artifact, creating a version, submitting for review, checking status, or withdrawing.
owner: sdkwork-deployments maintainers
module: sdkwork-deploy
standards: APPLICATION_IDENTITY_SPEC.md, SDKWORK_DEPLOY_SPEC.md, PACKAGING_SPEC.md, SDK_SPEC.md, AGENT_SKILLS_SPEC.md
---

# SDKWork App Scaffold Lifecycle

One skill for the whole scaffold lifecycle: prepare the package, upload it, create or update the
scaffold listing, add an immutable version, submit for review, track status, and withdraw or update
the listing. The scaffold is the deploy-owned design-time generator input; the physical API is
`scaffolds`.

## When to use

Use this skill for any app scaffold operation:

- create, retrieve, list, or update a scaffold listing;
- package a scaffold from an archive or a Git repository directory and upload it;
- add, list, or inspect scaffold versions;
- submit a scaffold for review, or check its review status;
- withdraw a scaffold, or change its visibility, pricing, or media;
- read the marketplace view of a published scaffold.

Do not use this skill for:

- uploading or deploying an application artifact — use `sdkwork-app-deployments`;
- creating or promoting a commerce product listing, SKU, or price — that is the app-store boundary;
- modifying a published version — create a new version instead.

## Inputs

- Source app id (the application the scaffold is generated from).
- Scaffold listing: `scaffoldKey`, `displayName`, `summary`, optional `description`, `visibility`,
  `pricingModel`, `priceMinor` (int64 string), `currency`, and media references
  (`iconMediaRef`, `coverMediaRef`, `screenshots[]`). Media references are Drive references, not
  local paths.
- Category: `categoryKey` or category uuid, resolved against
  `deployClient.app.scaffolds.categories.list()`.
- Version: `version`, `changelog`, `platformTargets[]`, optional `sourceAppVersion`.
- Source mode: `archive` (default) or `git`; Git inputs are `repoUrl`, optional `repoRef`, optional
  `subPath`, and optional clone credential.
- An authenticated `deployClient` and `driveClient` from the approved bootstrap or injected provider.
- Explicit confirmation for a `PUBLIC` or `PAID` listing, and for a withdrawal.

### Authentication

This skill reuses the application skill's auth runtime. Log in once per profile, or inject
`SDKWORK_AUTH_TOKEN` plus `SDKWORK_ACCESS_TOKEN`.

```bash
node skills/sdkwork-app-scaffold/scripts/run.mjs doctor --cwd <app-root> --check-sdk-import
node skills/sdkwork-app-scaffold/scripts/run.mjs login \
  --cwd <app-root> --iam-base-url "$SDKWORK_IAM_APP_API_BASE_URL" \
  --account you@sdkwork.com --password-stdin --profile standalone.development
```

Full contract: [../sdkwork-app-deployments/references/auth.md](../sdkwork-app-deployments/references/auth.md).

In this repository the app root that declares all four SDKs is
`apps/sdkwork-deployments-pc`, so pass `--cwd apps/sdkwork-deployments-pc` when
running from the repository root.

The SDKs are published to npm. The runtime uses the installed packages from the
target app; published builds run on plain Node, and the TypeScript transform is
enabled only for a local sdkwork-space source checkout.

## Workflow

### Scripts and generic API calls

Both skills share one runtime (`tools/agent-skills-sdk/`). Use the generic dispatcher for scaffold
operations that have no dedicated step; it calls the generated SDK transport, never raw HTTP.

```bash
# Upload the scaffold package (Drive uploader + artifact.create)
node skills/sdkwork-app-scaffold/scripts/run.mjs upload --cwd <app-root> \
  --app-id <source-app-id> --file dist/scaffold.zip --package-type 1

# Create a listing, add a version, or submit for review
node skills/sdkwork-app-scaffold/scripts/run.mjs api --cwd <app-root> \
  --operation scaffolds.create --input scaffold.json
node skills/sdkwork-app-scaffold/scripts/run.mjs api --cwd <app-root> \
  --operation scaffolds.versions.create --path-param scaffoldUuid=<uuid> --input version.json
node skills/sdkwork-app-scaffold/scripts/run.mjs api --cwd <app-root> \
  --operation scaffolds.submit --path-param scaffoldUuid=<uuid>

# Backend review surface (admin): operation ids collide, so pass --api backend
node skills/sdkwork-app-scaffold/scripts/run.mjs api --cwd <app-root> --api backend \
  --operation scaffolds.list
```

The generated operation map is
[../sdkwork-app-deployments/references/api-map.md](../sdkwork-app-deployments/references/api-map.md).

### Scaffold operation map

| Operation | SDK call |
| --- | --- |
| Categories | `deployClient.app.scaffolds.categories.list` |
| List / retrieve scaffold | `deployClient.app.scaffolds.list`, `.retrieve` |
| Create / update listing | `deployClient.app.scaffolds.create`, `.update` |
| Upload package | `driveClient.uploader.uploadArchive`, `deployClient.artifact.create` |
| Versions | `deployClient.app.scaffolds.versions.create`, `.list` |
| Submit for review | `deployClient.app.scaffolds.submit` |
| Withdraw | `deployClient.app.scaffolds.delete` |
| Marketplace read | `deployClient.app.marketplace.scaffolds.list`, `.retrieve` |

### 1. Resolve the source app and category

Resolve the source app by exact id, slug, or name; stop when the match is ambiguous. Resolve the
category to exactly one entry by uuid or `categoryKey`; never invent a category or pick the first
entry when several match.

### 2. Acquire the scaffold package

- Archive mode (default): use the supplied archive and verify it is non-empty.
- Git mode: clone `repoUrl` at `repoRef` (shallow by default), then package the directory named by
  `subPath` as the archive root. `subPath` must stay repository-relative and must not be absolute or
  contain `..`.

The version contract carries `artifactUuid` (and optional `sourceAppVersion`), not a Git URL. Git is
an acquisition mode that produces the same immutable archive. Record `repoUrl`, `repoRef`, and
`subPath` in the evidence; when the platform must also hold that Git source, bind it on the source
app with `sdkwork-app-deployments` in Git mode.

### 3. Upload the artifact

Compute the SHA-256 hex digest and byte size of the final archive, upload it with
`driveClient.uploader.uploadArchive`, and register it with `deployClient.artifact.create`. Use
`packageType: 1`, `contentType: "application/zip"`, and the source app id for `siteId`.

### 4. Create or update the listing and its version

- Reuse an existing scaffold only when `scaffolds.list` returns exactly one exact `scaffoldKey`
  match. Otherwise create a new scaffold with `create` and an `initialVersion`.
- Add a version with `scaffolds.versions.create`, including `artifactUuid`, `checksumSha256`,
  `packageSizeBytes`, `platformTargets`, `changelog`, and `sourceAppVersion` when the source app has
  one.
- Update listing metadata with `scaffolds.update`; never change an existing version.

### 5. Submit and track review

Submit only when requested and only after the version exists:
`scaffolds.submit(scaffoldUuid)`. Status moves `DRAFT` -> `PENDING_REVIEW` -> `PUBLISHED` (or
`REJECTED`). Report the status you actually read; never infer approval. Use `scaffolds.retrieve`
and `scaffolds.versions.list` to track progress.

### 6. Update or withdraw

Update listing metadata through `scaffolds.update`. Withdraw through `scaffolds.delete` only
with explicit approval; existing installations keep the versions they already downloaded. A version
that is `PUBLISHED` stays immutable; publish a new version instead of editing it.

Read [references/scaffold-lifecycle.md](references/scaffold-lifecycle.md) for the exact calls and
field contract.

## Rules

- A scaffold is design-time generator input, not a runtime definition
  (`APPLICATION_IDENTITY_SPEC.md` §1). Exactly one scaffold family exists and it is owned by the
  deploy module (`§7`); never introduce a second scaffold store.
- The physical API resource is `scaffolds`. Use it as the deploy-owned scaffold; do not revive
  `template` as an application definition.
- Versions are immutable. Never edit or overwrite a published version; a changed package gets a new
  version.
- Archive is the default. Require an explicit request for Git mode, and never claim a Git upload when
  no artifact was produced.
- `packageSizeBytes` and `priceMinor` are int64 values serialized as strings; never convert them to
  numbers.
- `visibility` defaults to `PRIVATE` and `pricingModel` defaults to `FREE`. A `PUBLIC` or `PAID`
  listing requires explicit confirmation.
- Reuse one idempotency key per intent; a changed archive or version gets a new key.
- Use only generated SDK clients, the shared skill runtime, or the composed publisher. No raw HTTP,
  manual authorization headers, or local SDK forks.
- Run the skill scripts from the target application root, or pass `--cwd <app-root>`; that root must
  declare the SDK packages. Keep session tokens out of argv, tracked files, and logs.
- Do not submit for review while the scaffold has no version, and do not re-submit an unchanged
  scaffold as a new listing.

## Verification

```bash
# From the repository root: skill package, generated registry, and runtime
node ../sdkwork-specs/tools/check-agent-skills.mjs --root .
node tools/agent-skills-sdk/generate-operations.mjs --check
node tools/agent-skills-sdk/generate-api-map.mjs --check
node --test tools/agent-skills-sdk/runtime.test.mjs

# Runtime evidence (run through the SDK, not curl)
# deployClient.app.scaffolds.retrieve(scaffoldUuid)    -> status, latestVersionUuid
# deployClient.app.scaffolds.versions.list(scaffoldUuid) -> version, artifactUuid, checksumSha256
# deployClient.artifact.retrieve(artifactUuid)                 -> checksumSha256, contentLength
```

A lifecycle step is complete only when the scaffold read-back and the version read-back both name the
artifact and checksum you uploaded.

## Output

Report:

- source app id, `scaffoldKey`, scaffold uuid, and category uuid;
- operation performed (create, version, submit, update, withdraw, or read);
- source mode, and for Git the URL, ref, and `subPath`;
- archive file name, byte size, and SHA-256 digest;
- Drive upload session and item ids, and the artifact uuid;
- scaffold status before and after the operation, and `latestVersionUuid`;
- version string, `platformTargets`, and whether the listing is `PUBLIC` or `PAID`;
- any step deliberately left for review.
