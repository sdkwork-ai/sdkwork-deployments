# SDKWork Deployments Flutter

Flutter client root of `sdkwork-deployments`, shipping the app template
marketplace mobile surface (`docs/domains/APP_TEMPLATE_MARKETPLACE.md`):

- **模板市场 tab** — category chips + keyword search over `PUBLIC` + `PUBLISHED`
  listings and a detail bottom sheet. Acquiring is an order-center trade
  (`appTemplateOrders.create`): a FREE listing comes back settled, a PAID
  listing comes back with the cashier URL / provider payment payload the buyer
  still has to settle, shown in a follow-up dialog. Ownership (`已获取`) is
  `status == 'paid'` from `appTemplateOrders.list`; the deployments module
  records no purchase of its own.
- **我的模板 tab** — the author's listings with review state and version
  history, submit-for-review. Publishing a NEW listing stays on the PC
  console for v1 (Drive-packaged artifact flow).

## Architecture

- Transport for the **catalog** is the **generated** Flutter/Dart SDK
  (`pnpm sdk:generate:app`, flutter target →
  `sdks/sdkwork-deployments-app-sdk/sdkwork-deployments-app-sdk-flutter/`);
  `lib/marketplace_service.dart` exposes the `MarketplacePort` and delegates
  those reads to the generated `SdkworkAppClient`.
- The **trade** (`POST`/`GET /app/v3/api/app_template_orders`) lives in the
  order center, which publishes no Dart client, so `lib/order_center_client.dart`
  is this app's small typed client for those two calls. It speaks `dart:io`
  (no added pub dependency) and takes its origin plus auth/access tokens from
  the same `DeploymentsRuntimeConfig` the catalog client uses.
- `MarketplacePort` is the seam: widgets and tests consume the port, tests run
  against a fake (`test/marketplace_page_test.dart`); the order-center client
  has its own transport seam and request-shaping tests
  (`test/order_center_client_test.dart`). Production injects
  `SdkworkMarketplacePort.fromConfig(...)`.
- Runtime values are compile-time (`--dart-define`):
  `SDKWORK_DEPLOY_APP_API_BASE_URL`, plus `SDKWORK_DEPLOY_AUTH_TOKEN` /
  `SDKWORK_DEPLOY_ACCESS_TOKEN` for local runs. The platform login flow is
  wired by the host shell; the marketplace UI itself is auth-agnostic.

## Commands

```bash
flutter pub get        # from apps/sdkwork-deployments-flutter
flutter analyze
flutter test
flutter run            # add --dart-define=SDKWORK_DEPLOY_APP_API_BASE_URL=...
```

The `android/` and `ios/` targets are checked in, so a store build runs from
this root directly (`flutter build apk` / `flutter build ipa`); this README's
`analyze` and `test` commands are the day-to-day ones.
