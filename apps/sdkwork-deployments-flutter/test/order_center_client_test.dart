import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:sdkwork_deployments_flutter/marketplace_service.dart';
import 'package:sdkwork_deployments_flutter/order_center_client.dart';
import 'package:sdkwork_deployments_app_sdk/sdkwork_deployments_app_sdk.dart';

/// Transport seam fake: records the shaped request and answers canned JSON.
class _RecordingTransport implements OrderCenterTransport {
  _RecordingTransport(this.answer);

  final OrderCenterResponse Function(OrderCenterRequest request) answer;
  final List<OrderCenterRequest> requests = <OrderCenterRequest>[];
  bool closed = false;

  @override
  Future<OrderCenterResponse> send(OrderCenterRequest request) async {
    requests.add(request);
    return answer(request);
  }

  @override
  void close() => closed = true;
}

OrderCenterResponse _envelope(Object data) =>
    OrderCenterResponse(statusCode: 200, body: jsonEncode(<String, Object>{'code': 0, 'data': data}));

OrderCenterClient _client(_RecordingTransport transport, {String Function()? idempotencyKey}) => OrderCenterClient(
      baseUrl: 'http://127.0.0.1:3900/',
      transport: transport,
      authToken: 'tok-1',
      accessToken: 'acc-1',
      idempotencyKey: idempotencyKey,
    );

void main() {
  test('createTemplateOrder posts the uuid to the order center with an idempotency key', () async {
    final transport = _RecordingTransport(
      (_) => _envelope(<String, Object>{
        'item': <String, Object>{
          'orderId': 'ord-1',
          'orderNo': 'T20261003000001',
          'outTradeNo': 'ot-1',
          'templateUuid': 'tmpl-1',
          'templateName': 'Blog starter',
          'amount': '1990',
          'currencyCode': 'CNY',
          'expiresAt': '2026-10-03T01:00:00Z',
          'paymentMethod': 'wechat',
          'paymentProduct': 'mobile_cashier_h5',
          'qrCode': 'https://cashier.example/ord-1',
          'qrCodeType': 'cashier_url',
          'status': 'pending_payment',
          'reused': false,
          'cashierUrl': 'https://cashier.example/ord-1',
        },
      }),
    );
    final order = await _client(transport).createTemplateOrder('tmpl-1');

    final request = transport.requests.single;
    expect(request.method, 'POST');
    expect(request.url.toString(), 'http://127.0.0.1:3900/app/v3/api/app_template_orders');
    expect(request.headers['Authorization'], 'Bearer tok-1');
    expect(request.headers['Access-Token'], 'acc-1');
    expect(request.headers['Content-Type'], 'application/json');
    expect(request.headers['Idempotency-Key'], isNotEmpty);
    expect(jsonDecode(jsonEncode(request.body)), <String, Object>{'templateUuid': 'tmpl-1'});

    expect(order.status, TemplateOrder.statusPendingPayment);
    expect(order.requiresPayment, isTrue);
    expect(order.isPaid, isFalse);
    expect(order.paymentRequirement, 'https://cashier.example/ord-1');
    expect(order.amount, '1990');
    expect(order.currencyCode, 'CNY');
  });

  test('a provider-native answer keeps its payload as the payment requirement', () async {
    final transport = _RecordingTransport(
      (_) => _envelope(<String, Object>{
        'item': <String, Object>{
          'orderId': 'ord-3',
          'orderNo': 'T20261003000003',
          'outTradeNo': 'ot-3',
          'templateUuid': 'tmpl-2',
          'templateName': 'Deck starter',
          'amount': '990',
          'currencyCode': 'CNY',
          'paymentProduct': 'wechat_native',
          'qrCode': 'weixin://wxpay/bizpayurl?pr=abc',
          'qrCodeType': 'provider_native',
          'cashierUrl': 'https://cashier.example/ord-3',
          'status': 'pending_payment',
          'reused': false,
        },
      }),
    );
    final order = await _client(transport).createTemplateOrder('tmpl-2');

    expect(order.qrCodeType, TemplateOrder.qrCodeTypeProviderNative);
    expect(order.paymentRequirement, 'weixin://wxpay/bizpayurl?pr=abc');
  });

  test('createTemplateOrder projects a settled FREE order and its reused flag', () async {
    final transport = _RecordingTransport(
      (_) => _envelope(<String, Object>{
        'item': <String, Object>{
          'orderId': 'ord-2',
          'orderNo': 'T20261003000002',
          'outTradeNo': 'ot-2',
          'templateUuid': 'tmpl-1',
          'templateName': 'Blog starter',
          'amount': '0',
          'currencyCode': 'CNY',
          'paymentProduct': 'mobile_cashier_h5',
          'status': 'paid',
          'reused': true,
        },
      }),
    );
    final order = await _client(transport, idempotencyKey: () => 'fixed-key').createTemplateOrder('tmpl-1');

    expect(transport.requests.single.headers['Idempotency-Key'], 'fixed-key');
    expect(order.isPaid, isTrue);
    expect(order.reused, isTrue);
    expect(order.paymentRequirement, isNull);
  });

  test('listTemplateOrders reads the page envelope and pages at 100', () async {
    final transport = _RecordingTransport(
      (_) => _envelope(<String, Object>{
        'items': <Object>[
          <String, Object>{
            'orderId': 'ord-1',
            'orderNo': 'T20261003000001',
            'templateUuid': 'tmpl-1',
            'templateName': 'Blog starter',
            'versionUuid': 'ver-1',
            'amount': '0',
            'currencyCode': 'CNY',
            'status': 'paid',
            'fulfillmentStatus': 'fulfilled',
            'paidAt': '2026-10-03T00:10:00Z',
            'createdAt': '2026-10-03T00:10:00Z',
          },
          <String, Object>{
            'orderId': 'ord-2',
            'orderNo': 'T20261003000002',
            'templateUuid': 'tmpl-2',
            'templateName': 'Deck starter',
            'amount': '1990',
            'currencyCode': 'CNY',
            'status': 'pending_payment',
            'fulfillmentStatus': 'unfulfilled',
            'createdAt': '2026-10-03T00:20:00Z',
          },
        ],
        'pageInfo': <String, Object>{'hasMore': false, 'totalItems': '2'},
      }),
    );
    final orders = await _client(transport).listTemplateOrders();

    final request = transport.requests.single;
    expect(request.method, 'GET');
    expect(request.url.path, '/app/v3/api/app_template_orders');
    expect(request.url.queryParameters, <String, String>{'page': '1', 'page_size': '100'});
    expect(request.body, isNull);
    expect(request.headers.containsKey('Idempotency-Key'), isFalse);

    expect(orders.length, 2);
    expect(orders.where((order) => order.isEntitlement).map((order) => order.templateUuid), <String>['tmpl-1']);
    expect(orders.last.fulfillmentStatus, 'unfulfilled');
  });

  test('the order center fails closed on transport and envelope errors', () async {
    final refused = _RecordingTransport(
      (_) => const OrderCenterResponse(statusCode: 503, body: 'upstream unavailable'),
    );
    await expectLater(
      _client(refused).createTemplateOrder('tmpl-1'),
      throwsA(isA<OrderCenterException>()),
    );

    final failedEnvelope = _RecordingTransport(
      (_) => OrderCenterResponse(statusCode: 200, body: jsonEncode(<String, Object>{'code': 7, 'data': <String, Object>{}})),
    );
    await expectLater(
      _client(failedEnvelope).listTemplateOrders(),
      throwsA(isA<OrderCenterException>()),
    );

    final notJson = _RecordingTransport((_) => const OrderCenterResponse(statusCode: 200, body: '<html/>'));
    await expectLater(
      _client(notJson).createTemplateOrder('tmpl-1'),
      throwsA(isA<OrderCenterException>()),
    );

    final noItem = _RecordingTransport((_) => _envelope(<String, Object>{}));
    await expectLater(
      _client(noItem).createTemplateOrder('tmpl-1'),
      throwsA(isA<OrderCenterException>()),
    );
  });

  test('HttpClientOrderCenterTransport performs that request over a real socket', () async {
    final seen = <String, Object?>{};
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    server.listen((request) async {
      seen['method'] = request.method;
      seen['path'] = request.uri.toString();
      seen['authorization'] = request.headers.value('authorization');
      seen['accessToken'] = request.headers.value('access-token');
      seen['idempotencyKey'] = request.headers.value('idempotency-key');
      seen['body'] = await utf8.decoder.bind(request).join();
      request.response
        ..statusCode = 200
        ..headers.contentType = ContentType.json
        ..write(jsonEncode(<String, Object>{
          'code': 0,
          'data': <String, Object>{
            'item': <String, Object>{
              'orderId': 'ord-1',
              'orderNo': 'T20261003000001',
              'outTradeNo': 'ot-1',
              'templateUuid': 'tmpl-1',
              'templateName': 'Blog starter',
              'amount': '0',
              'currencyCode': 'CNY',
              'paymentProduct': 'mobile_cashier_h5',
              'status': 'paid',
              'reused': false,
            },
          },
        }));
      await request.response.close();
    });

    final client = OrderCenterClient(
      baseUrl: 'http://127.0.0.1:${server.port}',
      authToken: 'tok-1',
      accessToken: 'acc-1',
      idempotencyKey: () => 'loopback-key',
    );
    addTearDown(client.close);

    final order = await client.createTemplateOrder('tmpl-1');

    expect(order.isPaid, isTrue);
    expect(seen['method'], 'POST');
    expect(seen['path'], '/app/v3/api/app_template_orders');
    expect(seen['authorization'], 'Bearer tok-1');
    expect(seen['accessToken'], 'acc-1');
    expect(seen['idempotencyKey'], 'loopback-key');
    expect(seen['body'], '{"templateUuid":"tmpl-1"}');
  });

  test('SdkworkMarketplacePort routes acquire and myPurchases through the order center', () async {
    final transport = _RecordingTransport(
      (request) => request.method == 'POST'
          ? _envelope(<String, Object>{
              'item': <String, Object>{
                'orderId': 'ord-1',
                'orderNo': 'T20261003000001',
                'outTradeNo': 'ot-1',
                'templateUuid': 'tmpl-1',
                'templateName': 'Blog starter',
                'amount': '0',
                'currencyCode': 'CNY',
                'paymentProduct': 'mobile_cashier_h5',
                'status': 'paid',
                'reused': false,
              },
            })
          : _envelope(<String, Object>{'items': <Object>[], 'pageInfo': <String, Object>{'hasMore': false}}),
    );
    final port = SdkworkMarketplacePort(
      SdkworkAppClient.withBaseUrl(baseUrl: 'http://127.0.0.1:3900'),
      _client(transport),
    );

    final order = await port.acquire('tmpl-1');
    final orders = await port.myPurchases();

    expect(order.isPaid, isTrue);
    expect(orders, isEmpty);
    expect(transport.requests.map((request) => request.method).toList(), <String>['POST', 'GET']);
    expect(
      transport.requests.map((request) => request.url.path).toSet(),
      <String>{'/app/v3/api/app_template_orders'},
    );
  });
}
