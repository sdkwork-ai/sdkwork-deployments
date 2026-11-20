# sdkwork-deployments-pc-admin-scaffolds

Backend-admin module for the application scaffold marketplace (`docs/domains/APPLICATION_SCAFFOLD_MARKETPLACE.md`).

Contributes the `scaffoldCategories`, `scaffolds`, and `scaffoldVersions`
resource entries to the shared admin workspace. The pages are the generic
registry-driven `Page`; the data sources and the moderation actions live in
`@sdkwork/deployments-pc-admin-core` (`createDeploymentsAdminRegistry`), which
is the only place this module's surface touches the backend SDK.

The surface is catalog-only: buying a listing is an application-scaffold order
on the platform order center (`sdkwork-order`), which owns the payment and the
entitlement, so no admin resource here reads or revokes a purchase.

## Verification

```bash
pnpm --dir apps/sdkwork-deployments-pc typecheck
pnpm --dir apps/sdkwork-deployments-pc test
```
