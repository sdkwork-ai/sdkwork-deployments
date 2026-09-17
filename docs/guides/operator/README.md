# Operator Guide

## Deployment Manifest

- Canonical deployment declaration: [deployments/deploy.yaml](../../deployments/deploy.yaml)
- Source topology profiles: `etc/topology/*.env`
- Kubernetes workload: `deployments/kubernetes/deployment.yaml`
- Validate and plan: `pnpm deploy:validate`, `pnpm topology:validate`, `pnpm deploy:plan`
- Render Nginx-compatible ingress: `pnpm deploy:nginx:render`

## Production Provider Configuration

Production must fail closed into generated SDK adapters:

```text
SDKWORK_DEPLOY_USE_MEMORY_DRIVE=0
SDKWORK_DRIVE_FACADE_URL=<drive-app-api-url>
SDKWORK_DEPLOY_USE_MEMORY_CONTENT_PROVIDER=false
SDKWORK_DEPLOY_USE_MEMORY_CLOUD_ACCOUNTS=false
SDKWORK_DEPLOY_DRIVE_INTERNAL_API_URL=<drive-internal-api-url>
SDKWORK_DEPLOY_DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE=/run/secrets/sdkwork/deploy-drive-internal-ingress-token
SDKWORK_DEPLOY_KNOWLEDGEBASE_INTERNAL_API_URL=<knowledgebase-internal-api-url>
SDKWORK_DEPLOY_KNOWLEDGEBASE_INTERNAL_API_INGRESS_TOKEN_FILE=/run/secrets/sdkwork/deploy-knowledgebase-internal-ingress-token
SDKWORK_DEPLOY_WEB_INTERNAL_API_URL=<web-internal-api-url>
SDKWORK_DEPLOY_WEB_INTERNAL_API_INGRESS_TOKEN_FILE=/run/secrets/sdkwork/deploy-web-internal-ingress-token
```

The three ingress credentials are projected as read-only files under `/run/secrets/sdkwork/` with
pod `fsGroup: 10001`; secret values never appear in environment variables. Drive and Knowledgebase
tokens are read for each provider request, and the Web token is read for each runtime publication,
so an atomic secret-file rotation does not require a process restart. Missing, empty, or unreadable
files fail closed and provider transport errors are redacted.

## Composition And Rollout

`apps.composition.update` commits desired state only. A successful response proves the normalized
composition, immutable SiteRevision, desired pointer, assignments, idempotency result, and audit
record committed together. It does not prove Web Nodes activated the revision.

| Signal | Meaning |
| --- | --- |
| `deploy_app.desired_revision_id` | Latest committed configuration desired by the control plane |
| `deploy_runtime_assignment.publish_status` | Pending, claimed, or published assignment delivery state |
| `deploy_app.current_revision_id` | Revision confirmed active after Web observation/quorum |

Observation ingestion and strict all-frozen-target quorum are enabled: `current_revision_id`
advances only after every frozen target reports an authenticated matching `ACTIVE` observation.
Do not equate a published assignment, or even this node-local activation evidence, with public
DNS/TLS/CDN reachability. Keep the last-known-good runtime set and investigate repeated claim
expiry, generation mismatch, receipt mismatch, provider-event registration failure, provider
validation failure, or rejected observations before retrying. External public-domain probes remain
a production gate.

Ordinary Drive and Wiki file changes bypass this flow. They are served through provider reads and
cache revalidation and must not create Releases, Deployments, or SiteRevisions.
For Drive resources, the worker nevertheless owns the control-plane prerequisite: it registers and
renews each WebsiteRoot's exact Node callback before assignment publication. Event payloads travel
from Drive directly to Web Server.

## Database

PostgreSQL and SQLite materialize from the same prelaunch contract. Validate before planning or
applying lifecycle operations:

```powershell
pnpm db:validate
pnpm db:plan
pnpm db:migrate
pnpm db:drift:check
```

## Verification

```powershell
pnpm api:materialize
pnpm sdk:generate
pnpm api:check
pnpm verify
```

See [standards-alignment.md](../../standards-alignment.md), the
[technical architecture](../../architecture/tech/TECH-cloud-site-publishing-control-plane.md), and
`DOCUMENTATION_SPEC.md` section 2.
