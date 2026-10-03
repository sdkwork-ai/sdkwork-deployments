# sdkwork-deployments-pc-admin-app-templates

Backend-admin module for the app template marketplace (`docs/domains/APP_TEMPLATE_MARKETPLACE.md`).

Contributes the `templateCategories`, `appTemplates`, and `templatePurchases`
resource entries to the shared admin workspace. The pages are the generic
registry-driven `Page`; the data sources and moderation/settlement actions live
in `@sdkwork/deployments-pc-admin-core` (`createDeploymentsAdminRegistry`), which
is the only place this module's surface touches the backend SDK.

## Verification

```bash
pnpm --dir apps/sdkwork-deployments-pc typecheck
pnpm --dir apps/sdkwork-deployments-pc test
```
