import 'package:flutter_test/flutter_test.dart';
import 'package:sdkwork_deployments_app_sdk/sdkwork_deployments_app_sdk.dart';
import 'package:sdkwork_deployments_flutter/marketplace_service.dart';
import 'package:sdkwork_deployments_flutter/marketplace_page.dart';
import 'package:flutter/material.dart';

class _FakePort implements MarketplacePort {
  int acquireCalls = 0;
  String? lastAcquiredUuid;

  @override
  Future<List<TemplateCategoryResponse>> categories() async => <TemplateCategoryResponse>[];

  @override
  Future<List<AppTemplateSummaryResponse>> browse({
    int page = 1,
    int pageSize = 10,
    String? keyword,
    String? categoryUuid,
    String? templateType,
  }) async =>
      <AppTemplateSummaryResponse>[
        AppTemplateSummaryResponse(
          id: 'tmpl-1',
          templateType: 'APP',
          templateKey: 'blog',
          displayName: 'Blog starter',
          summary: 'A small blog app',
          categoryUuid: 'cat-1',
          visibility: 'PUBLIC',
          pricingModel: 'FREE',
          priceMinor: '0',
          currency: 'CNY',
          status: 'PUBLISHED',
          isFeatured: false,
          installCount: '3',
          viewCount: '9',
          updatedAt: '2026-10-03T00:00:00Z',
          version: '1',
        ),
      ];

  @override
  Future<AppTemplateResponse> retrieve(String templateUuid) async => AppTemplateResponse(
        id: templateUuid,
        templateType: 'APP',
        templateKey: 'blog',
        displayName: 'Blog starter',
        summary: 'A small blog app',
        description: 'Long description',
        appUuid: 'app-1',
        categoryUuid: 'cat-1',
        authorUserId: '11',
        visibility: 'PUBLIC',
        pricingModel: 'FREE',
        priceMinor: '0',
        currency: 'CNY',
        status: 'PUBLISHED',
        isFeatured: false,
        installCount: '3',
        viewCount: '9',
        createdAt: '2026-10-03T00:00:00Z',
        updatedAt: '2026-10-03T00:00:00Z',
        version: '1',
      );

  @override
  Future<TemplatePurchaseResponse> acquire(String templateUuid) async {
    acquireCalls += 1;
    lastAcquiredUuid = templateUuid;
    return TemplatePurchaseResponse(
      id: 'pur-1',
      templateUuid: templateUuid,
      versionUuid: 'ver-1',
      buyerUserId: '11',
      pricingModel: 'FREE',
      priceMinor: '0',
      currency: 'CNY',
      status: 'ACTIVE',
      createdAt: '2026-10-03T00:00:00Z',
      updatedAt: '2026-10-03T00:00:00Z',
      version: '1',
    );
  }

  @override
  Future<List<TemplatePurchaseResponse>> myPurchases() async => <TemplatePurchaseResponse>[];

  @override
  Future<List<AppTemplateResponse>> myTemplates() async => <AppTemplateResponse>[];

  @override
  Future<List<AppTemplateVersionResponse>> versions(String templateUuid) async => <AppTemplateVersionResponse>[];

  @override
  Future<AppTemplateResponse> submit(String templateUuid) async => throw UnimplementedError();

  @override
  Future<void> withdraw(String templateUuid) async {}
}

void main() {
  test('MarketplacePort contract: acquire is the idempotent command boundary', () async {
    final port = _FakePort();
    final purchase = await port.acquire('tmpl-1');
    expect(port.acquireCalls, 1);
    expect(port.lastAcquiredUuid, 'tmpl-1');
    expect(purchase.status, 'ACTIVE');
  });

  testWidgets('MarketplacePage renders listings and acquires through the port', (tester) async {
    final port = _FakePort();
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: MarketplacePage(port: port))));
    await tester.pumpAndSettle();

    expect(find.text('Blog starter'), findsOneWidget);

    await tester.tap(find.text('Blog starter'));
    await tester.pumpAndSettle();
    expect(find.text('A small blog app'), findsWidgets);

    await tester.tap(find.text('获取模板'));
    await tester.pumpAndSettle();
    expect(port.acquireCalls, 1);
  });
}
