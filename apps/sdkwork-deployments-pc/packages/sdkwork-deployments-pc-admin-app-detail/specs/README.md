# app-detail

Owns the deployment-application detail surface on the `backend-admin` boundary:
the fact sheet of one `deploy_app`, its three metered planes (traffic,
service/API calls, storage), and its delivery state (readiness, hostnames,
releases, deployments).

Canonical SDKWork standards remain authoritative; this file only records what is
specific to this capability.

## Why it lives in this repository

A deployment application is the `deployment` domain's own resource. Cloud Router
publishes one and the App Store distributes one, but *what a deployment
application is and what it consumes* belongs to this domain, so the reusable
surface lives here rather than inside any one console's admin package.

Consumers outside this repository add the package through the workspace and pass
their own SDK client and commands; they do not copy the page.

## Boundary

| Owned here | Supplied by the consuming console |
| --- | --- |
| Read model for identity, traffic, service/API usage, and storage | The `deployments-app-sdk` client instance, through `DeploymentApplicationDetailDataSource` |
| The page, panels, trend chart, and formatting | Route path and navigation (`onBack`) |
| Availability classification and degradation rules | Permission scope (`DeploymentApplicationDetailPermissionPort`) |
| Locale fragments under `deployments.appDetail` | Global locale provider and theme |

The package constructs no SDK client, reads no base URL, attaches no credential,
and imports no router. `@sdkwork/deployments-app-sdk` is declared as a
dependency only so a consumer's port implementation can name its types.

## Extension points

Each is an interface from `src/services/ports/index.ts` with a deterministic
fallback:

- `DeploymentApplicationDetailDataSource` — `loadDetail` is required;
  `loadUsage` is optional, and omitting it renders the consumption plane as
  "meter not connected" rather than as zero.
- `DeploymentApplicationDetailPermissionPort` — absent members mean "not
  declared", and the control stays visible.
- `DeploymentApplicationDetailCapabilityPort` — a command the host did not
  supply is not rendered, so exactly one implementation of each write exists in
  the product.

## Verification

```bash
pnpm --dir apps/sdkwork-deployments-pc typecheck
pnpm --dir apps/sdkwork-deployments-pc test
node ../sdkwork-specs/tools/check-component-port-bindings.mjs --root .
node ../sdkwork-specs/tools/check-application-layering.mjs --root .
```
