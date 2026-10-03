# SDKWork Deployments H5

Mobile web (H5) client root of `sdkwork-deployments`. Ships the template
marketplace user surface (`docs/domains/APP_TEMPLATE_MARKETPLACE.md`):

- **Marketplace tab** — category chips and keyword search over `PUBLIC` +
  `PUBLISHED` listings, a bottom-sheet detail view, and the idempotent acquire
  command (FREE settles instantly; PAID reports the honest `PENDING`
  settlement state).
- **My templates tab** — the author's listings with review state, version
  history, submit-for-review, and withdraw. Publishing a NEW listing stays on
  the PC console for v1 (it is a Drive-packaged artifact flow).

## Architecture

- `src/bootstrap/runtime.ts` constructs the runtime once: public
  `/runtime-env.json` config, `createTokenManager` (`@sdkwork/sdk-common`),
  the generated app SDK client (`platform: "h5"`, dual-token), and the appbase
  IAM auth runtime with `platform: "h5"`. Views and services receive clients
  injected — they never build clients or call raw HTTP.
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
