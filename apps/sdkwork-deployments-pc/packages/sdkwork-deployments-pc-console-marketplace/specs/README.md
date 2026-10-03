# sdkwork-deployments-pc-console-marketplace

Console surface of the app template catalog
(`docs/domains/APP_TEMPLATE_MARKETPLACE.md`).

Contributes two custom pages to the shared console workspace:

- `marketplace` — the storefront: category/pricing facets over `PUBLIC` +
  `PUBLISHED` listings, a detail dialog, and the acquire command. Acquire asks
  the platform order center (`sdkwork-order`, `app_template_orders`) for an
  app-template order: the deployments module owns the catalog only, so a `FREE`
  listing comes back owned in one click and a `PAID` listing opens the cashier
  the order center returned (or surfaces the provider payload). Ownership is
  `status: "paid"` keyed by `templateUuid`.
- `myTemplates` — the author workbench: publish one of your apps as a listing,
  add versions, submit for review, withdraw.

Two clients arrive as props (no console-core context dependency): the deploy app
client for the catalog and the order app client for trade, both built together in
`createDeploymentsConsoleClients` on the shared app-API origin. SDK types come
from `@sdkwork/deployments-pc-console-core/sdk`, and copy is bilingual through
the package-local translator (`src/i18n.ts`).

## Verification

```bash
pnpm --dir apps/sdkwork-deployments-pc typecheck
pnpm --dir apps/sdkwork-deployments-pc test
```
