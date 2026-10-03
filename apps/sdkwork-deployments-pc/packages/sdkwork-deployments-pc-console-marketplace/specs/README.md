# sdkwork-deployments-pc-console-marketplace

Console surface of the app template marketplace
(`docs/domains/APP_TEMPLATE_MARKETPLACE.md`).

Contributes two custom pages to the shared console workspace:

- `marketplace` — the storefront: category/pricing facets over `PUBLIC` +
  `PUBLISHED` listings, a detail dialog, and the idempotent acquire command
  (`FREE` grants the entitlement, `PAID` routes to the commerce checkout).
- `myTemplates` — the author workbench: publish one of your apps as a listing,
  add versions, submit for review, withdraw.

Clients arrive as props (no console-core context dependency), SDK types come
from `@sdkwork/deployments-pc-console-core/sdk`, and copy is bilingual through
the package-local translator (`src/i18n.ts`).

## Verification

```bash
pnpm --dir apps/sdkwork-deployments-pc typecheck
pnpm --dir apps/sdkwork-deployments-pc test
```
