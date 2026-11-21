# ADR-20261124: Deploy → IAM Channel Capability Credentials

Status: proposed
Date: 2026-11-24
Owners: SDKWork Deploy and SDKWork IAM maintainers
Related: `TECH-application-resource-bindings.md`, IAM channel authorization center,
Deploy channel publishing runbook (`docs/runbooks/wechat-channel-publishing.md`)

## Context

Deploy executes WeChat channel steps by calling one IAM capability operation at a
time (`POST /backend/v3/api/iam/oauth/resource_authorizations/{id}/capability_invocations`).
IAM owns the authorizer token; Deploy never holds a WeChat credential.

That hop needs a credential. Today Deploy reads a static bearer from
`SDKWORK_DEPLOY_IAM_CAPABILITY_BEARER` and the adapter fails closed when it is
absent. The static value works, but it has no expiry, no scope, and no rotation
protocol, and IAM cannot attribute the call to the Deploy service identity.

IAM does not implement an OAuth `client_credentials` grant; the backend API
accepts user/session `AuthToken` and application `AccessToken` through its dual
token extractor.

## Options

| Option | Expiry | Scope | Rotation | Notes |
| --- | --- | --- | --- | --- |
| A. Static bearer (current) | none | whole backend API | manual | simplest; weakest |
| B. IAM `client_credentials` grant for a service client | minutes | explicit permission scope | automatic refresh | standard OAuth; needs IAM work |
| C. Mutual TLS between Deploy and IAM | connection-bound | transport-level identity | certificate rotation | strongest identity; needs PKI |
| D. Forward the end user's bearer | minutes | user's permissions | user session | leaks user credential into a service hop; rejected |

## Decision

Target state is **B**: register a Deploy service client in IAM and allow it to
mint a short-lived access token with a narrow permission scope
(`iam.oauth.channel_capabilities.invoke`), then configure Deploy with the client
id and secret instead of a static bearer.

Until B ships:

- **A is interim only.** The static bearer must be rotated on a schedule and
  stored in the deployment secret store (never in a tracked profile).
- The capability invocation audit (`wechat_channel_capability.invoke`) remains
  the attribution record, because the interim bearer has no service identity.

C is a future hardening step once both services share a certificate authority.
D is rejected: forwarding a user bearer into a service hop makes the service
indistinguishable from the user and extends a user credential's reach.

## Consequences

**Positive**

- Service calls become attributable, expiring, and scope-limited.
- A leaked Deploy configuration no longer implies a permanent IAM credential.

**Negative / work**

- IAM must add a service-client registry, the grant, and token lifetime policy.
- Deploy must add a token client with cache and refresh-before-expiry.
- Operators must provision the service client and rotate its secret.

## Migration

1. IAM implements the grant and exposes `POST /iam/v3/api/oauth/token` support for
   the service client; audit records the `client_id`.
2. Deploy adds a `ChannelCapabilityAuth` implementation that mints and caches the
   token; the environment switches from a bearer to client id/secret.
3. Remove `SDKWORK_DEPLOY_IAM_CAPABILITY_BEARER` once every environment is
   migrated; keep the fail-closed default until then.

## Verification

- Deploy unit tests cover token caching, refresh-before-expiry, and refresh
  failure (fail closed, no fallback to an expired token).
- IAM tests cover scope enforcement and token expiry.
- The acceptance script (`tools/wechat_channel_acceptance.mjs`) runs against the
  service-client configuration; evidence records the client id.
