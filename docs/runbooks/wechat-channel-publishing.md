# WeChat Channel Publishing

Status: active
Owner: SDKWork Deploy maintainers
Related surfaces: IAM channel authorization center, Deploy channel release API, Cloud Router admin UI

## What runs where

| Capability | Owner | Transport |
| --- | --- | --- |
| Component tickets, component/authorizer tokens, authorization, callbacks | IAM | WeChat component API |
| Release records, state machine, quota, metering, publish commands | Deploy | `app/v3/api` |
| Operator console (service platforms, authorized accounts, release board) | Cloud Router admin | IAM + Deploy app SDK |

Deploy never holds a WeChat credential. Component-mode steps are one IAM
`capability_invocations` call each; direct-mode (`self_managed`) uploads run a
sandboxed `miniprogram-ci` process.

## Configuration

| Variable | Purpose |
| --- | --- |
| `SDKWORK_DEPLOY_IAM_CAPABILITY_BASE_URL` | IAM backend base URL for capability calls. |
| `SDKWORK_DEPLOY_IAM_CAPABILITY_BEARER` | Service bearer accepted by IAM; never logged. |
| `SDKWORK_DEPLOY_CI_PROJECT_ROOT` | Sandbox root for direct-mode project directories. |
| `SDKWORK_DEPLOY_CI_KEY_ROOT` | Sandbox root for direct-mode upload private keys. |

Each adapter fails closed when its variables are absent: publish commands return
`409` and startup logs one `info` line per unconfigured adapter.

## Onboard a service-provider platform

1. Create the operator platform row
   (`POST /backend/v3/api/iam/oauth/operator_platforms`).
2. Save component credentials
   (`POST .../operator_platforms/{id}/component_credentials`): Component AppID,
   AppSecret, EncodingAESKey, callback token, authorization callback URL. IAM
   encrypts the secrets and returns `callbackPublicId`.
3. Run the connectivity check
   (`POST .../operator_platforms/{id}/connectivity_checks`). Expect
   `componentTokenAcquired: true` and a ticket status. A `409` names the missing
   piece (AppID, secret, ticket, or customized platform type).
4. Register the callback URLs shown by IAM in the WeChat third-party platform
   console, then wait for the next `component_verify_ticket` push.
5. Generate an authorization link
   (`POST .../operator_platforms/{id}/pre_authorizations`) and send the operator
   to the returned URL.
6. After the operator authorizes, IAM exchanges the code, stores the encrypted
   refresh token, and fills the account snapshot (nickname, principal, original
   id, QR code). Confirm the resource account in the admin "Authorized Accounts"
   page.

## Publish a mini program or official account

1. Create a channel release with the target resource link
   (`POST /app/v3/api/apps/{appId}/channel_releases`).
2. Drive it with publish commands
   (`POST .../channel_releases/{id}/publish_commands`):

   | Action | Required fields |
   | --- | --- |
   | `upload` | `authorizationId`; `templateId`/`providerVersion` optional when the release carries them |
   | `submit_audit` | `authorizationId`, `items` (WeChat review categories) |
   | `release` | `authorizationId` |
   | `rollback` | `authorizationId`, `targetVersion` |
   | `gray` | `authorizationId`, `grayPercentage` |
   | `modify_domains` | `authorizationId`, `domainAction`, `domains` |
   | `set_privacy` | `authorizationId`, `privacy` |

3. Record the review verdict by re-running `submit_audit` observations through
   the review observer, or by `PATCH .../channel_releases/{id}` with the observed
   status and `expectedVersion`.
4. The release board (`/admin/iam/oauth/channel-releases`) shows status, version,
   commit, audit id, and gray percentage. Only `APPROVED` can be published.

## Direct mode (`self_managed`)

- Plan the invocation with
  `POST /app/v3/api/apps/{appId}/channel_direct_upload_plans`; the response is the
  exact `miniprogram-ci` program and argument vector.
- When the CI roots are configured, `upload` runs that invocation through the
  sandbox and records `ci:<version>` as the commit receipt.
- Only `upload` is executed. Submit review and release in the WeChat admin
  console, then record the observed status with a release transition. Every other
  publish action returns `422` with that guidance.

## Troubleshooting

| Symptom | Cause | Action |
| --- | --- | --- |
| `409 channel capability credential is not configured` | IAM adapter variables missing | Set both IAM variables and restart. |
| `409 channel release provider is not configured` | Provider build failed or skipped | Check startup logs; verify the IAM base URL. |
| `409 direct-mode channel upload is not configured` | CI roots missing | Set both CI root variables and restart. |
| `401` from IAM | Service bearer rejected | Rotate the bearer from the secret store. |
| `404 resource authorization not found or not active` | Wrong authorization id or re-authorization required | Re-read the authorized account and re-run the connectivity check. |
| `422` for a domain action | Domain rule violation (scheme, duplicates, count) | Fix the list; the provider quota was not spent. |
| `429 quota_exceeded` on create | `channel_release_count` plan limit reached | Upgrade the plan or archive old releases. |

## Acceptance run

`tools/wechat_channel_acceptance.mjs` drives the commercial acceptance path and
writes a JSON evidence file with HTTP status and trace id per step.

```bash
# 1. Review the plan without network calls.
node tools/wechat_channel_acceptance.mjs --dry-run --phase all

# 2. Run the IAM phase (connectivity, pre-authorization link, authorizations).
SDWORK_ACCEPTANCE_IAM_BASE_URL=https://iam.example.com \
SDWORK_ACCEPTANCE_IAM_BEARER=... \
SDWORK_ACCEPTANCE_OPERATOR_PLATFORM_ID=platform-1 \
node tools/wechat_channel_acceptance.mjs --phase iam --output target/acceptance/iam.json

# 3. Run the Deploy phase (create release, then publish commands).
SDWORK_ACCEPTANCE_DEPLOY_BASE_URL=https://deploy.example.com \
SDWORK_ACCEPTANCE_DEPLOY_BEARER=... \
SDWORK_ACCEPTANCE_APP_ID=app-1 \
SDWORK_ACCEPTANCE_RESOURCE_LINK_ID=link-1 \
SDWORK_ACCEPTANCE_AUTHORIZATION_ID=auth-1 \
SDWORK_ACCEPTANCE_ACTIONS=upload \
node tools/wechat_channel_acceptance.mjs --phase deploy --output target/acceptance/deploy.json
```

Attach the evidence file to the release review. A step that returns a non-2xx
status or an unexpected envelope stops the run with exit code `1`; missing
configuration stops it with exit code `2`.

## Verification

```bash
cargo test -p sdkwork-intelligence-deploy-service --lib
cargo test -p sdkwork-routes-iam-backend-api --test iam_backend_route_standard
node ../sdkwork-specs/tools/check-api-operation-patterns.mjs --workspace .
node ../sdkwork-specs/tools/check-api-response-envelope.mjs --workspace .
```

Every capability invocation writes an `iam_audit_event`
(`wechat_channel_capability.invoke`) with the authorization id and outcome, so a
support request can be traced from the release board to the provider call.
