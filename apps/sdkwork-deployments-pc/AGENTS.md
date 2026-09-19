# SDKWork Deployments PC

## SDKWORK Soul

Read `../../../sdkwork-specs/SOUL.md` before executing tasks in this root. Follow specs before
memory, dictionary before context, stop on ambiguity, and evidence before completion. This
browser root follows the canonical PC, React, SDK, Drive, IAM, configuration, security,
pagination, naming, frontend, TypeScript, and test standards.

## SDKWORK Standards

Resolve the standards root once and use it as the global authority for the current task:

- `../../../sdkwork-specs/README.md`
- `../../../sdkwork-specs/SOUL.md`
- `../../../sdkwork-specs/AGENTS_SPEC.md`

Read only the relevant README task-matrix row or navigation heading, then load the selected
authority sections. Do not copy root standard text into this repository.

## Spec Resolution Order

Use dynamic progressive loading for the current task:

1. Read this `AGENTS.md` routing material.
2. Read `../../../sdkwork-specs/README.md`, then only the task-specific root specs.
3. Inspect implementation files only after the dictionary and relevant specs are clear.

Language-specific specs are on-demand: only the touched language loads
`../../../sdkwork-specs/TYPESCRIPT_CODE_SPEC.md` or
`../../../sdkwork-specs/FRONTEND_CODE_SPEC.md`. Package command standardization loads
`../../../sdkwork-specs/PNPM_SCRIPT_SPEC.md` only when the current task changes package commands
or scripts; GitHub packaging work loads `../../../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md` only when
it reaches that workflow boundary. List/search work loads
`../../../sdkwork-specs/PAGINATION_SPEC.md` and `check-pagination.mjs`. Source configuration work
loads `../../../sdkwork-specs/SOURCE_CONFIG_SPEC.md` when the root owns `etc/` as deployable-root
source configuration.

## Application Identity

Read `../../sdkwork.app.config.json` for Deployments application identity, API/SDK inventory,
release metadata, packaging, or app-owned capabilities. Read `etc/` for concrete environment,
bind, upstream, runtime, and deployment values. The app manifest is not runtime configuration
authority.

## Application Identity And Configuration

Read `../../sdkwork.app.config.json` for Web Server application identity, runtime, release, and
capability metadata. `etc/` is deployable-root source configuration: environment, bind, upstream,
runtime, and deployment values. The app manifest is not runtime configuration authority.

## Local Dictionary Structure

- `AGENTS.md`: this browser-root agent entrypoint and relative SDKWork spec index.
- `package.json`, `pnpm-workspace.yaml`: language/build manifests and catalog.
- `packages/`: `*-core` isolation boundaries, `*-commons` shared types, `*-shell` hosts, and
  capability packages.
- `tools/materialize_deployments_pc.mjs`: PC package manifest/spec materializer.
- `specs/`, `tests/`: component contracts and verification assets.

## Local Dictionary

- `packages/sdkwork-deployments-pc-core`: runtime configuration and locale helpers.
- `packages/sdkwork-deployments-pc-commons`: shared registry/action types and normalization.
- `packages/sdkwork-deployments-pc-console-core`: tenant console SDK isolation boundary (Deploy App
  SDK + Drive App SDK).
- `packages/sdkwork-deployments-pc-console-*`: tenant console capability packages.
- `packages/sdkwork-deployments-pc-admin-core`: backend-admin SDK isolation boundary.
- `packages/sdkwork-deployments-pc-admin-*`: backend-admin capability packages.

## Rules

Console packages consume Deploy App SDK and Drive App SDK only through console-core. Admin
packages consume Deploy Backend SDK only through admin-core. Upload bytes are owned by Drive;
Deploy stores stable business references. Raw HTTP, manual authorization headers, local SDK
forks, generated output edits, and cross-surface business imports are forbidden. Generated SDK
output must not be hand-edited; regenerate through the owned materializers.

Upload uses the Drive App SDK `uploader.*` surface only. This root's upload identity is declared in
`specs/upload.declaration.json` and each declared entry is a distinct upload purpose. `source` is
this application's code, never a package name. `scene` is a stable workflow label, never composed
from a runtime value. `uploadProfileCode` is a standard Drive profile. Feature services, not UI
components, pass these values, and they import the declaration rather than repeating its literals.

## Required Specs By Task Type

- Agent/workflow changes: `../../../sdkwork-specs/SOUL.md`, `../../../sdkwork-specs/AGENTS_SPEC.md`,
  `../../../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`, and `../../../sdkwork-specs/TEST_SPEC.md`.
- Any code change: `../../../sdkwork-specs/CODE_STYLE_SPEC.md`,
  `../../../sdkwork-specs/NAMING_SPEC.md`, plus only the touched language/framework spec.
- API/SDK changes: `../../../sdkwork-specs/API_SPEC.md`, `../../../sdkwork-specs/SDK_SPEC.md`,
  `../../../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md`, and `../../../sdkwork-specs/TEST_SPEC.md`.
- Upload, file-storage, or app-upload changes: `../../../sdkwork-specs/DRIVE_SPEC.md` and
  `../../../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md`. Upload identity is declared in
  [`specs/upload.declaration.json`](specs/upload.declaration.json) per `DRIVE_SPEC.md` §18 and the
  declaration is the single authority for this root's `appResourceType`, `scene`, `source`,
  `uploadProfileCode`, and retention values. Upload call sites import the declaration instead of
  repeating its literals.
- List/search work: `../../../sdkwork-specs/PAGINATION_SPEC.md` and `check-pagination.mjs`.
- Source configuration changes: `../../../sdkwork-specs/SOURCE_CONFIG_SPEC.md`,
  `../../../sdkwork-specs/CONFIG_SPEC.md`, and `../../../sdkwork-specs/ENVIRONMENT_SPEC.md`.
- Security/auth changes: `../../../sdkwork-specs/IAM_SPEC.md` and
  `../../../sdkwork-specs/SECURITY_SPEC.md`.

## Code Style Rules

Read `../../../sdkwork-specs/CODE_STYLE_SPEC.md` and `../../../sdkwork-specs/NAMING_SPEC.md` before
code changes. Use `sdkwork-utils-rust` and `sdkwork-id-core` for shared helpers instead of
duplicating utility logic locally. Generated SDK output must not be hand-edited.

## Build, Test, And Verification

Choose the narrowest verification selected by the changed surface; workspace-wide checks run only
when the change crosses that boundary.

```powershell
pnpm --dir apps/sdkwork-deployments-pc typecheck
pnpm --dir apps/sdkwork-deployments-pc test
```

## Agent Execution Rules

Do not rely on memory when a relevant SDKWork spec exists. Do not replace generated SDK calls
with raw HTTP. Stop when the relative specs path, app identity, component spec, SDK family, or
provider ownership is ambiguous.

## Human Review Rules

Human review is required for breaking public API changes, generated SDK ownership changes,
privacy/security exceptions, and destructive filesystem or data operations.

