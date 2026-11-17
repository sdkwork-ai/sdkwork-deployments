# Skill Authentication And SDK Access

The skills call the Deploy API through the generated app and backend SDK facades. This reference
defines how a script obtains credentials, where they live, and how a call is authenticated.

## 1. Credential Classes

| Class | Source | Purpose | May be committed |
| --- | --- | --- | --- |
| Public application access token | `SDKWORK_ACCESS_TOKEN`, `--access-token-file` | Pre-login application isolation; accepted by credential-entry routes only | Yes (public material) |
| Session auth token | login response, `SDKWORK_AUTH_TOKEN`, session cache | Authenticated user session (`Authorization: Bearer`) | No |
| Session access token | login response, `SDKWORK_ACCESS_TOKEN` after login, session cache | Session-scoped `Access-Token` header | No |
| Refresh token | login response, `SDKWORK_REFRESH_TOKEN`, session cache | Session refresh and logout | No |

A Deploy app or backend call uses `authMode: "dual-token"`: the SDK sends
`Authorization: Bearer <authToken>` and `Access-Token: <accessToken>`. The IAM login call uses
`authMode: "apikey"`: it sends the public application access token only.

Never pass a session token on the command line and never write one into a tracked file. The session
cache is the only persisted form.

## 2. Login, Refresh, Logout

Login uses the IAM app SDK (`@sdkwork/iam-app-sdk`) password grant:

```bash
node skills/sdkwork-app-deployments/scripts/run.mjs login \
  --cwd . \
  --iam-base-url "$SDKWORK_IAM_APP_API_BASE_URL" \
  --account you@sdkwork.com \
  --password-stdin \
  --profile standalone.development
```

- `--password-stdin` reads the password from stdin; `SDKWORK_LOGIN_PASSWORD` is the non-interactive
  equivalent. There is intentionally no `--password`.
- `SDKWORK_ACCESS_TOKEN` (public application access token) is required for the login request.
- The command stores the session in `~/.sdkwork/agent-skills/<profile>.json` with mode `0600` and
  prints only the profile, tenant, and expiry. It never prints tokens.
- `refresh` renews the session with the stored refresh token.
- `logout` revokes the server session with the refresh token (the IAM logout route accepts it without
  a live access token), then removes the local session file.

The session file is user-private runtime state under `~/.sdkwork/`; it must never be copied into the
repository, a bundle, or a log.

## 3. Injecting A Session

For CI or a short-lived agent, skip login and inject the session:

```bash
SDKWORK_AUTH_TOKEN=... SDKWORK_ACCESS_TOKEN=... \
  node skills/sdkwork-app-deployments/scripts/run.mjs api --operation apps.list
```

`SDKWORK_ACCESS_TOKEN` is required on every call. Without `SDKWORK_AUTH_TOKEN` (or a saved session)
every protected Deploy operation fails with a login instruction before any request is sent.

## 4. Base URLs

| Surface | Flag | Environment |
| --- | --- | --- |
| Deploy app API | `--base-url` | `SDKWORK_DEPLOY_APP_API_BASE_URL` |
| Deploy backend API | `--backend-base-url` | `SDKWORK_DEPLOY_BACKEND_API_BASE_URL` |
| Drive app API | `--drive-base-url` | `SDKWORK_DRIVE_APP_API_BASE_URL` |
| IAM app API | `--iam-base-url` | `SDKWORK_IAM_APP_API_BASE_URL` |

Each also accepts the topology key that the application already materializes
(`SDKWORK_DEPLOY_APPLICATION_APP_HTTP_URL`, `..._BACKEND_HTTP_URL`,
`SDKWORK_DRIVE_APPLICATION_APP_HTTP_URL`, `SDKWORK_APPBASE_APP_API_BASE_URL`). `--env-file` loads a
`KEY=VALUE` topology file; process environment wins over the file.

## 5. SDK Resolution

Every SDK in the sdkwork-space module set is published to npm. The runtime resolves
`@sdkwork/deployments-app-sdk`, `@sdkwork/deployments-backend-sdk`, `@sdkwork/drive-app-sdk`, and
`@sdkwork/iam-app-sdk` from `--cwd` (default: the current directory). Run from the application root
that installs those npm packages per `APP_SDK_INTEGRATION_SPEC.md`.

- A published package exports built JavaScript (`dist/*.js`), so the CLI runs on plain Node.
- A local sdkwork-space checkout exports `src/index.ts`; the launcher detects that and enables the
  TypeScript transform and resolver for development only. Published deployments never need that
  flag.
- The scripts never import generated `dist` internals and never vendor an SDK copy.
- The operation registry and API map are generated from this repository's OpenAPI contract. Install
  an SDK version compatible with the target Deploy control plane; `run.mjs doctor --check-sdk-import`
  reports `runtimeMode`, the resolved entry, each installed version, and a match/mismatch against the
  repository contract version.

## 6. Error Contract

The Deploy API returns `application/problem+json`. The CLI prints a sanitized object with `code`,
`traceId`, `status`, `title`, `detail`, `i18nKey`, and `locale` when present, then exits non-zero.
`traceId` is the correlation key for support; report it, not the raw response body.

## 7. Rules

- Use the scripts or the SDK; never call the Deploy or Drive HTTP endpoints directly.
- Do not log, echo, or commit tokens, presigned URLs, or session files.
- A missing or expired session is a stop-and-login condition, not a reason to retry with a public
  application token on a protected route.
- The public application access token is isolation material, not a user credential.
