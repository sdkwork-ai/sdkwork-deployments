# Agent Skills SDK Runtime

Shared runtime for the `sdkwork-deployments` skills. One auth implementation and one generic API
dispatcher serve both the application and scaffold skills.

## Why This Exists

The skills must authenticate a user session and call the Deploy API through the generated SDK. They
must not carry raw HTTP wrappers, per-skill copies of the token logic, or generated `dist` deep
imports. This runtime is the single implementation.

## Distribution

Every SDK in the sdkwork-space module set (`@sdkwork/deployments-app-sdk`,
`@sdkwork/deployments-backend-sdk`, `@sdkwork/drive-app-sdk`, `@sdkwork/iam-app-sdk`, and the other
module SDKs) is published to npm. The runtime is npm-first:

- It resolves each **public package export** from the target app root. Published packages resolve to
  built JavaScript and the CLI runs on plain Node.
- A local sdkwork-space checkout resolves the same export to `src/index.ts`. The launcher detects
  that layout and enables the TypeScript transform plus resolver for development only.
- It never deep-imports generated `dist` paths and never vendors the SDK.
- `doctor` reports `runtimeMode` (`npm-javascript` or `workspace-typescript`), the resolved entry,
  the installed version, and a version check against this repository's SDK contract. The generated
  operation registry is bound to that contract, so an app should install a compatible SDK version.

## Files

| File | Purpose |
| --- | --- |
| `run.mjs` | Launches the CLI with `--experimental-transform-types` and the TS resolver |
| `register-ts.mjs`, `ts-resolver.mjs` | Resolve the TS-source SDK packages that export `src/index.ts` |
| `cli.mjs` | `login`, `refresh`, `logout`, `api`, `upload`, `ops`, `doctor` |
| `runtime.mjs` | SDK client creation, generic operation dispatch, artifact upload, IAM login |
| `env.mjs` | Base URLs, env files, session cache, token resolution, path substitution |
| `operations.json` | Generated operation registry for the app and backend surfaces |
| `generate-operations.mjs` | Regenerates `operations.json` from the authored OpenAPI contracts |
| `generate-api-map.mjs` | Regenerates the skill's `references/api-map.md` |
| `runtime.test.mjs` | Unit tests for env, operation resolution, token extraction, and problem mapping |

## Quick Start

```bash
# 1. Diagnose base URLs, tokens, and SDK resolution from the target app root
node skills/sdkwork-app-deployments/scripts/run.mjs doctor --cwd <app-root> --check-sdk-import

# 2. Log in once per profile (password on stdin; never on argv)
node skills/sdkwork-app-deployments/scripts/run.mjs login \
  --cwd <app-root> --iam-base-url "$SDKWORK_IAM_APP_API_BASE_URL" \
  --account you@sdkwork.com --password-stdin --profile standalone.development

# 3. Call any operation through the SDK
node skills/sdkwork-app-deployments/scripts/run.mjs api \
  --cwd <app-root> --operation apps.source.update --path-param appId=... --input source.json

# 4. Upload and register an artifact
node skills/sdkwork-app-deployments/scripts/run.mjs upload \
  --cwd <app-root> --app-id ... --file dist/app.zip --package-type 1
```

Both skills have a `scripts/run.mjs` launcher that delegates here, so the commands are identical.
From this repository, `--cwd apps/sdkwork-deployments-pc` is the app root that declares all four
SDKs.

## Auth Model

- `login` calls IAM `auth.sessions.create` (password grant) with the public application access token
  and stores `authToken`, `accessToken`, and `refreshToken` in
  `~/.sdkwork/agent-skills/<profile>.json` (mode `0600`).
- Deploy app/backend and Drive clients use `authMode: "dual-token"`; the IAM client uses
  `authMode: "apikey"`.
- CI can skip login with `SDKWORK_AUTH_TOKEN` + `SDKWORK_ACCESS_TOKEN`.
- `refresh` renews; `logout` revokes with the refresh token and clears the cache.

Full contract: `skills/sdkwork-app-deployments/references/auth.md`.

## Generic API Dispatcher

`operations.json` is keyed `<api>:<operationId>` because six operation ids exist on both surfaces
(for example `appTemplates.list`). `api` accepts an operation id (with `--api` when ambiguous) or an
explicit `--method`/`--path`. The request goes through `client.http.request` on the generated SDK, so
auth, envelope unwrapping, and `ProblemDetail` handling stay in the SDK.

## Security

- Tokens never appear on the command line and are never printed by the runtime.
- The session cache is user-private runtime state; do not copy it into the repository.
- The public application access token is isolation material, not a user session.
