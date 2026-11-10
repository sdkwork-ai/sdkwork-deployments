# sdkwork-deployments-pc-admin-downloads

Backend-admin surface for the download catalog that used to be a free-form JSON
document in the Cloud Router site settings.

Contributes the `downloadCatalog` entry and one custom page
(`DownloadCatalogPage`) that edits catalogs, cards, installers and the
per-installer update configuration (provider, feed URL, channel, minimum
supported version, check interval, staged rollout).

All writes go through `@sdkwork/deployments-backend-sdk`; the client arrives
from the admin context (`useDeploymentsAdminClient`).

## Verification

```bash
pnpm --dir apps/sdkwork-deployments-pc typecheck
pnpm --dir apps/sdkwork-deployments-pc test
```
