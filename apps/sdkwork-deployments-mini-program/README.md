# SDKWork Deployments Mini Program

WeChat mini-program client root of `sdkwork-deployments`, shipping the app
template marketplace mobile surface
(`docs/domains/APP_TEMPLATE_MARKETPLACE.md`):

- **模板市场 tab** — category chips + keyword search over `PUBLIC` + `PUBLISHED`
  listings and a modal detail/confirm dialog. Acquiring a template is an
  order-center trade (`appTemplateOrders.create`): a FREE listing comes back
  settled, a PAID listing comes back with the cashier URL / provider payment
  payload the buyer still has to settle, shown in a follow-up dialog. The
  `已获取` badge and the acquire button key off `status === "paid"` per
  `templateUuid` from `appTemplateOrders.list` — this app holds no purchase
  state of its own.
- **我的模板 tab** — the author's listings with review state, version history,
  and submit-for-review. Publishing a NEW listing stays on the PC console for
  v1 (Drive-packaged artifact flow).

## Architecture

- The WeChat runtime has no fetch/XHR, so
  `services/marketplace.ts` is the single hand-written `wx.request` transport,
  exposed as the typed `MarketplacePort`. It is the ONLY place raw platform
  HTTP happens; tests inject a fake requester
  (`tests/marketplace.service.test.ts`, plain `node --test`, no devtools) and
  assert the snake_case query shape, the `Idempotency-Key` header, the
  envelope decoding, and fail-closed error paths.
- The port talks to two app APIs on the same origin: the deployments catalog
  (`template_categories`, `marketplace/templates`, `app_templates`) and the
  order center's `app_template_orders`. The deployments module owns the
  catalog only — it records no purchase, order or entitlement.
- `app.ts` builds the port once into `globalData`; pages read it from there.
- `project.config.json` carries the placeholder `touristappid` — substitute
  the real appid when the mini-program is registered for a store release.

## Verification

```bash
pnpm --dir apps/sdkwork-deployments-mini-program install   # miniprogram-api-typings
pnpm --dir apps/sdkwork-deployments-mini-program typecheck
pnpm --dir apps/sdkwork-deployments-mini-program test
```

Runtime verification beyond these gates needs WeChat DevTools with the
deployments gateway reachable from the device.
