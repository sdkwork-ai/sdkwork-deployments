# sdkwork-deployments-pc-admin-app-templates

Backend-admin module for the application center (应用中心) on the
`backend-admin` surface. It contributes four resource entries to the shared
admin workspace:

- `applications` — the applications ledger (`deploy_app`). It mounts the
  canonical `PublishingAppsPage` from `@sdkwork/deployments-pc-console-publishing`
  with `surface="admin"` (list/search/filter/create/edit/pause/activate/archive)
  rather than re-implementing the page.
- `appTemplates` — author-published listings: create/edit plus moderation
  (approve, reject, disable, feature, unfeature, delete).
- `templateCategories` — the marketplace taxonomy, plain registry CRUD.
- `appTemplateVersions` — a scoped read of one listing's version history
  (the shell's scope field carries the template UUID).

The data sources and actions live in
`@sdkwork/deployments-pc-admin-core` (`createDeploymentsAdminRegistry`).
Moderation and delete go through the generated backend-admin client; listing
create/edit and the applications ledger go through the generated app-plane
client, because the backend-admin contract has no create command and no
editable-copy update for a listing (and no apps ledger at all). That split is
declared in the admin-core component contract (`contracts.sdkDependencies`).

The surface is catalog-only for trade: buying a listing is an app-template
order on the platform order center (`sdkwork-order`), which owns the payment and
the entitlement, so no admin resource here reads or revokes a purchase.

## Verification

```bash
pnpm --dir apps/sdkwork-deployments-pc typecheck
pnpm --dir apps/sdkwork-deployments-pc test
```
