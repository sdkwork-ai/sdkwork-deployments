# Integrator Guide

## API And SDK Surfaces

| Surface | Prefix | Composed package | OpenAPI authority |
| --- | --- | --- | --- |
| App API | `/app/v3/api` | `@sdkwork/deployments-app-sdk` | `apis/app-api/deploy/openapi.yaml` |
| Backend API | `/backend/v3/api` | `@sdkwork/deployments-backend-sdk` | `apis/backend-api/deploy/openapi.yaml` |

Materialized authority JSON, deterministic `*.sdkgen.json` input, family manifests, composed
facades, and generated transport live under `sdks/sdkwork-deploy-*-sdk/`. Generated SDKWork v3
clients unwrap `data` by default. Success uses `{ code: 0, data, traceId }`; HTTP errors use
`application/problem+json` with numeric `code` and `traceId`.

## Live Website Composition

Use `@sdkwork/deployments-app-sdk`; do not call Deploy, Drive, Knowledgebase, or Web Server with raw HTTP
or manually assembled credential headers. The active mutation is generated from
`PUT /app/v3/api/apps/{appId}/composition` with operation id `apps.composition.update`.

The request requires both `If-Match` and `Idempotency-Key`. The Site version is a decimal string.
Resources use one of these exact discriminated sources:

```json
{
  "type": "DRIVE_DIRECTORY",
  "websiteSpaceId": "space-id",
  "root": { "mode": "SPACE_ROOT" },
  "contentMode": "LIVE_TREE"
}
```

Use `{ "mode": "FOLDER", "folderNodeId": "node-id" }` for a selected Drive directory.

```json
{
  "type": "KNOWLEDGEBASE_WIKI",
  "publicationUuid": "publication-uuid"
}
```

Deploy validates every provider resource before database locking. A commit atomically replaces the
normalized composition, appends one immutable SiteRevision, advances `desired_revision_id`, and
enqueues complete runtime sets. `current_revision_id` advances only after Web observation/quorum.
Replaying the same idempotency key and request returns the committed result; changing the request
under the same key is a conflict.

Ordinary file upload, edit, move, rename, visibility change, publish, and delete operations do not
call this endpoint. Drive WebsiteRoot and Knowledgebase WikiPublication remain live provider
resources, so these changes do not create a Deploy Release, Deployment, or SiteRevision.

Backend composition mutation is intentionally absent. Operator automation must not submit tenant
provider identifiers until a trusted credential-delegation or resolved-resource admin contract is
approved.

## Package Upload Through Drive

Use `uploadSessions.create`, `retrieve`, `complete`, and `cancel`. Binary storage is delegated to
SDKWork Drive; Deploy stores session metadata only. Completing an artifact package upload with
`packageType` 1 through 5 creates an immutable artifact. This is the frozen artifact pipeline, not
the live WebsiteRoot/WikiPublication path.

Production artifact upload requires `SDKWORK_DRIVE_FACADE_URL` and
`SDKWORK_DEPLOY_USE_MEMORY_DRIVE=0`. Live provider attachment additionally requires
`SDKWORK_DEPLOY_USE_MEMORY_CONTENT_PROVIDER=false`, `SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS=false`
(the DNS credential account center is the default; the flag only opts out), and the
Drive/Knowledgebase Internal API URL and
ingress-token-file settings.

## Artifact And Release Pipeline

1. Complete a package upload to create `deploy_artifact`.
2. Call `sites.releases.create` with `artifactId` and `idempotencyKey` to create an immutable release.
3. Call `sites.deployments.create` with `releaseId` to deploy the frozen artifact path and checksum.

| Operation | Notes |
| --- | --- |
| `artifacts.list` / `retrieve` | Tenant-scoped immutable upload outputs |
| `artifacts.retain` | Marks the artifact retained; does not delete Drive nodes |
| `sites.releases.list` / `retrieve` / `create` | Site-scoped immutable releases |

Use this pipeline for Git, package, image, and frozen-bundle delivery. Do not use it for ordinary
Drive WebsiteRoot or Knowledgebase Wiki content changes.

## Managed TLS Prelaunch Gate

Managed certificate creation accepts one or more verified `domainIds` and creates an idempotent
certificate lifecycle intent. A hostname may be covered by multiple certificates, including
parallel RSA and ECDSA listener bindings. The current operation does not claim issuance,
distribution, Web Node activation, or served-SNI verification. There is no certificate/private-key
Drive upload API, and Drive node references are not a private-key custody mechanism.

The proposed
[managed domain and TLS decision](../../architecture/decisions/ADR-20260723-managed-domain-tls-control-plane.md)
defines the production contract for durable domain proof, ACME workflows, immutable certificate
versions, KMS/Secret Manager custody, one-time custom secret ingest, target-scoped distribution, and
loaded/served/public observations. Its
[implementation plan](../../engineering/plans/PLAN-2026-0002-managed-domain-tls-control-plane.md)
tracks the remaining provider and operational evidence. Do not treat a pending lifecycle intent as
an active certificate until the observation chain reaches the required quorum.

Until that release gate passes, production deployments must use the externally terminated TLS
profile declared by the deployment architecture. A pending row or planned renewal is control-plane
metadata only and must never be presented as ownership, issuance, renewal, activation, or served
certificate evidence.

## Regenerate And Verify

```powershell
pnpm api:materialize
pnpm sdk:generate
node ../sdkwork-specs/tools/check-sdk-standard.mjs --workspace .
pnpm --filter @sdkwork/deployments-app-sdk build
pnpm --filter @sdkwork/deployments-backend-sdk build
```

See `DOCUMENTATION_SPEC.md` section 2 and [standards-alignment.md](../../standards-alignment.md).
