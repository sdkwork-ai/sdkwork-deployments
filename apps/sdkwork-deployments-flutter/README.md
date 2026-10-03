# SDKWork Deployments Flutter

Flutter client root of `sdkwork-deployments`, shipping the app template
marketplace mobile surface (`docs/domains/APP_TEMPLATE_MARKETPLACE.md`):

- **模板市场 tab** — category chips + keyword search over `PUBLIC` + `PUBLISHED`
  listings, detail bottom sheet, idempotent acquire (FREE settles instantly;
  PAID reports the honest "待结算" state).
- **我的模板 tab** — the author's listings with review state and version
  history, submit-for-review. Publishing a NEW listing stays on the PC
  console for v1 (Drive-packaged artifact flow).

## Architecture

- Transport is the **generated** Flutter/Dart SDK
  (`pnpm sdk:generate:app`, flutter target →
  `sdks/sdkwork-deployments-app-sdk/sdkwork-deployments-app-sdk-flutter/`).
  This app never calls raw HTTP: `lib/marketplace_service.dart` exposes the
  `MarketplacePort` and delegates to the generated `SdkworkAppClient`.
- `MarketplacePort` is the seam: widgets and tests consume the port, tests run
  against a fake (`test/marketplace_page_test.dart`), production injects
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

Platform targets (android/, ios/) are added with `flutter create . --platforms
android,ios` when a store build is cut; this root ships source, analysis, and
widget tests first.
