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

## HTTP API Response Envelope

All L2+ SDKWork-owned custom HTTP contracts, including `app-api`, `backend-api`, and SDKWork-owned business `open-api`, `MUST` follow `API_SPEC.md` section 4.5, section 14, and section 15:

- **Default classification:** omitted `x-sdkwork-wire-protocol` means SDKWork-owned custom API (`sdkwork-v3`); only operation-level `x-sdkwork-wire-protocol: external` plus `x-sdkwork-external-protocol-id` identifies a third-party compatibility `open-api` operation.
- **Input:** typed request bodies, section 14.1 list/search/command input, `SdkWorkListQuery`, and `q` for free-text search.
- **Success output:** `SdkWorkApiResponse` with `{ "code": 0, "data": <payload>, "traceId": "<server-uuid>" }`.
- **Error output:** HTTP 4xx/5xx `application/problem+json` (`ProblemDetail`) with numeric `code` and `traceId`; SDKWork-owned errors may include `i18nKey` and `locale` presentation metadata.
- Success `code` is numeric `int32`; HTTP 2xx JSON bodies `MUST` use `0` only. REST semantics remain on HTTP status (`201`, `202`, etc.).
- Platform error codes are numeric non-zero values per section 15.3 (`40001`, `40101`, `40401`, …).
- Single resource: `data.item`
- Lists: `data.items` + `data.pageInfo` (`PageInfo.mode` is `offset` or `cursor`)
- Commands: `data.accepted` plus optional `resourceId` / `status`
- Async accept (`202`): `data.operationId`, `data.status`, optional `pollUrl`
- Operation patterns: retrieve/list/search/create/update/delete/command/async/bulk semantics follow `API_SPEC.md` section 15.4; create uses `201`, delete uses `204` with no JSON body, and `PUT`/`PATCH` use SDK action `update`.

Vendor compatibility `open-api` routes that mirror upstream tool or provider wire (for example OpenAI `/v1/*`, Anthropic/Claude `/anthropic/v1/*`, Google/Gemini `/google/v1beta/*`, Claude Code, or Codex) `MAY` opt out only when every exempt operation declares operation-level `x-sdkwork-wire-protocol: external` and `x-sdkwork-external-protocol-id` per `API_SPEC.md` section 4.5.2. SDKWork-owned business `open-api` operations `MUST NOT` opt out. Mixed OpenAPI documents are validated per operation; one external operation never exempts SDKWork-owned operations in the same document.

Errors `MUST` use HTTP 4xx/5xx with `application/problem+json` (`ProblemDetail`) including required numeric `code` and `traceId`. Optional `i18nKey` and `locale` are display metadata only. Business failures `MUST NOT` use HTTP 2xx with non-zero `code`, string wire codes, `success`, or human `message`.

Forbidden legacy envelopes and fields: `PlusApiResult`, `AppbaseApiResult`, `StoreApiResult`, `SdkWorkResponse`, per-domain `*ApiResult`, wire field `requestId`, bare domain DTOs at the HTTP root, and top-level `{ items, pageInfo, traceId }` without `data`.

Handlers `MUST` serialize success and map errors through `sdkwork-web-framework` response mapping. Generated HTTP SDKs (`--standard-profile sdkwork-v3`) unwrap `data` by default and expose typed numeric `ProblemDetail.code` / `traceId` and returned localization metadata on errors; use `.raw` when the full envelope is required.

Before completing API contract, SDK generation, or frontend service work, run:

```bash
node <sdkwork-specs>/tools/check-api-operation-patterns.mjs --workspace <workspace-root>
node <sdkwork-specs>/tools/check-api-response-envelope.mjs --workspace <workspace-root>
```

Authority: `sdkwork-specs/API_SPEC.md` section 4.5 and sections 14–16, `SDK_SPEC.md` section 4.2, `FRONTEND_SPEC.md`, `MIGRATION_SPEC.md` section 4.2.

## Human Review Rules

Human review is required for breaking public API changes, generated SDK ownership changes,
privacy/security exceptions, and destructive filesystem or data operations.

