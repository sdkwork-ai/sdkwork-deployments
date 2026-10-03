# SDKWork Deployments H5

Mobile web (H5) client root of `sdkwork-deployments`. Ships the template
catalog user surface (`docs/domains/APP_TEMPLATE_MARKETPLACE.md`):

- **Marketplace tab** — category chips and keyword search over `PUBLIC` +
  `PUBLISHED` listings, a bottom-sheet detail view, and the acquire command.
  Acquire asks the platform order center (`sdkwork-order`,
  `app_template_orders`) for an app-template order: the deployments module owns
  the catalog only, so a FREE listing is owned in one tap and a PAID listing
  opens the cashier the order center returned (or surfaces the provider
  payload). Ownership is `status: "paid"` keyed by `templateUuid`.
- **My templates tab** — the author's listings with review state, version
  history, submit-for-review, and withdraw. Publishing a NEW listing stays on
  the PC console for v1 (it is a Drive-packaged artifact flow).

## Architecture

- `src/bootstrap/runtime.ts` constructs the runtime once: public
  `/runtime-env.json` config, `createTokenManager` (`@sdkwork/sdk-common`), the
  generated deploy app SDK client (`platform: "h5"`, dual-token), the order app
  SDK client for app-template trade (same app-API origin, same token manager),
  and the appbase IAM auth runtime with `platform: "h5"`. Views and services
  receive clients injected — they never build clients or call raw HTTP.
- `src/auth/` gates the workspace on the auth controller state and mounts the
  shared `SdkworkAuthPage` under `/auth` (same stack as the PC app).
- `src/marketplace/` is the capability: `service.ts` (transport shaping),
  `MarketplaceView.tsx`, `MyTemplatesView.tsx`, `i18n.ts` (en/zh, typed).

## Commands

```bash
pnpm --dir apps/sdkwork-deployments-h5 dev        # vite dev server, port 5182
pnpm --dir apps/sdkwork-deployments-h5 typecheck
pnpm --dir apps/sdkwork-deployments-h5 test
pnpm --dir apps/sdkwork-deployments-h5 build      # dist/{standalone,cloud}/{env} via browser-dist-layout
```

`public/runtime-env.json` is the checked-in standalone development document;
other lanes are materialized by `build-browser-client.mjs` per profile.
