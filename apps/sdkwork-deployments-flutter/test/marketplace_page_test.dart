import 'package:flutter_test/flutter_test.dart';
import 'package:sdkwork_deployments_app_sdk/sdkwork_deployments_app_sdk.dart';
import 'package:sdkwork_deployments_flutter/marketplace_service.dart';
import 'package:sdkwork_deployments_flutter/marketplace_page.dart';
import 'package:sdkwork_deployments_flutter/my_templates_page.dart';
import 'package:sdkwork_deployments_flutter/order_center_client.dart';
import 'package:flutter/material.dart';

class _FakePort implements MarketplacePort {
  _FakePort({
    this.pricingModel = 'FREE',
    this.acquireStatus = TemplateOrder.statusPaid,
    this.cashierUrl,
    this.orders = const <TemplateOrderSummary>[],
  });

  /// Catalog pricing of the fixture listing.
  final String pricingModel;

  /// Order-center answer the fake trade returns.
  final String acquireStatus;

  /// Set together with a `pending_payment` answer.
  final String? cashierUrl;

  /// Buyer orders the fake list answers with.
  final List<TemplateOrderSummary> orders;

  int acquireCalls = 0;
  String? lastAcquiredUuid;
  int browseCalls = 0;
  String? lastPricingModel;
  String? lastSort;
  int withdrawCalls = 0;
  String? lastWithdrawnUuid;

  @override
  Future<List<TemplateCategoryResponse>> categories() async => <TemplateCategoryResponse>[];

  @override
  Future<MarketplaceBrowsePage> browse({
    int page = 1,
    int pageSize = 10,
    String? keyword,
    String? categoryUuid,
    String? templateType,
    String? pricingModel,
    String? sort,
  }) async {
    browseCalls += 1;
    lastPricingModel = pricingModel;
    lastSort = sort;
    return MarketplaceBrowsePage(
      items: <AppTemplateSummaryResponse>[
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
      ],
      hasMore: false,
      total: 1,
    );
  }

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
        pricingModel: pricingModel,
        priceMinor: pricingModel == 'PAID' ? '1990' : '0',
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
  Future<TemplateOrder> acquire(String templateUuid) async {
    acquireCalls += 1;
    lastAcquiredUuid = templateUuid;
    return TemplateOrder(
      orderId: 'ord-1',
      orderNo: 'T20261003000001',
      outTradeNo: 'ot-1',
      templateUuid: templateUuid,
      templateName: 'Blog starter',
      amount: pricingModel == 'PAID' ? '1990' : '0',
      currencyCode: 'CNY',
      paymentProduct: 'mobile_cashier_h5',
      qrCode: cashierUrl,
      qrCodeType: cashierUrl == null ? null : 'cashier_url',
      status: acquireStatus,
      reused: false,
      cashierUrl: cashierUrl,
    );
  }

  @override
  Future<List<TemplateOrderSummary>> myPurchases() async => orders;

  @override
  Future<List<AppTemplateResponse>> myTemplates() async => <AppTemplateResponse>[
        AppTemplateResponse(
          id: 'tmpl-1',
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
        ),
      ];

  @override
  Future<List<AppTemplateVersionResponse>> versions(String templateUuid) async => <AppTemplateVersionResponse>[];

  @override
  Future<AppTemplateResponse> submit(String templateUuid) async => throw UnimplementedError();

  @override
  Future<void> withdraw(String templateUuid) async {
    withdrawCalls += 1;
    lastWithdrawnUuid = templateUuid;
  }
}

void main() {
  test('MarketplacePort contract: acquire returns the order-center trade', () async {
    final port = _FakePort(acquireStatus: TemplateOrder.statusPendingPayment, cashierUrl: 'https://cashier.example/ord-1');
    final order = await port.acquire('tmpl-1');
    expect(port.acquireCalls, 1);
    expect(port.lastAcquiredUuid, 'tmpl-1');
    expect(order.status, TemplateOrder.statusPendingPayment);
    expect(order.requiresPayment, isTrue);
    expect(order.isPaid, isFalse);
    expect(order.paymentRequirement, 'https://cashier.example/ord-1');
  });

  testWidgets('MarketplacePage renders listings and acquires a FREE template in one tap', (tester) async {
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
    // A settled order carries no payment requirement to surface.
    expect(find.text('订单已创建，请完成支付'), findsNothing);
  });

  testWidgets('a PAID acquisition starts the trade and surfaces the cashier URL', (tester) async {
    final port = _FakePort(
      pricingModel: 'PAID',
      acquireStatus: TemplateOrder.statusPendingPayment,
      cashierUrl: 'https://cashier.example/ord-1',
    );
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: MarketplacePage(port: port))));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Blog starter'));
    await tester.pumpAndSettle();

    // The paid listing is purchasable here, not blocked client-side.
    await tester.tap(find.text('购买模板'));
    await tester.pumpAndSettle();

    expect(port.acquireCalls, 1);
    expect(port.lastAcquiredUuid, 'tmpl-1');
    expect(find.text('订单已创建，请完成支付'), findsOneWidget);
    expect(find.textContaining('cashier.example/ord-1'), findsWidgets);
    expect(find.text('复制链接'), findsOneWidget);
  });

  testWidgets('the 已获取 badge comes from settled orders only', (tester) async {
    final port = _FakePort(
      orders: const <TemplateOrderSummary>[
        TemplateOrderSummary(
          orderId: 'ord-1',
          orderNo: 'T20261003000001',
          templateUuid: 'tmpl-1',
          templateName: 'Blog starter',
          amount: '0',
          currencyCode: 'CNY',
          status: TemplateOrder.statusPaid,
          fulfillmentStatus: 'fulfilled',
          paidAt: '2026-10-03T00:10:00Z',
          createdAt: '2026-10-03T00:10:00Z',
        ),
        TemplateOrderSummary(
          orderId: 'ord-2',
          orderNo: 'T20261003000002',
          templateUuid: 'tmpl-2',
          templateName: 'Deck starter',
          amount: '1990',
          currencyCode: 'CNY',
          status: TemplateOrder.statusPendingPayment,
          fulfillmentStatus: 'unfulfilled',
          createdAt: '2026-10-03T00:20:00Z',
        ),
      ],
    );
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: MarketplacePage(port: port))));
    await tester.pumpAndSettle();

    // One badge for the settled order; the unpaid order row stays visible.
    expect(find.text('已获取'), findsOneWidget);
    expect(find.text('pending_payment'), findsOneWidget);
    expect(find.text('T20261003000001 · 0 CNY · fulfilled'), findsOneWidget);
  });

  testWidgets('MarketplacePage sends the pricing facet it selected', (tester) async {
    final port = _FakePort();
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: MarketplacePage(port: port))));
    await tester.pumpAndSettle();

    await tester.tap(find.text('付费'));
    await tester.pumpAndSettle();

    expect(port.lastPricingModel, 'PAID');
    expect(port.lastSort, 'NEWEST');
  });

  testWidgets('MyTemplatesPage withdraws only after the confirmation', (tester) async {
    final port = _FakePort();
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: MyTemplatesPage(port: port))));
    await tester.pumpAndSettle();

    await tester.tap(find.byType(PopupMenuButton<String>));
    await tester.pumpAndSettle();
    await tester.tap(find.text('下架模板'));
    await tester.pumpAndSettle();

    // Cancelling leaves the listing alone: the delete is the irreversible step.
    await tester.tap(find.text('取消'));
    await tester.pumpAndSettle();
    expect(port.withdrawCalls, 0);

    await tester.tap(find.byType(PopupMenuButton<String>));
    await tester.pumpAndSettle();
    await tester.tap(find.text('下架模板'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('确认下架'));
    await tester.pumpAndSettle();

    expect(port.withdrawCalls, 1);
    expect(port.lastWithdrawnUuid, 'tmpl-1');
  });
}
