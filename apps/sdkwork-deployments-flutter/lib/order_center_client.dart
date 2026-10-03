/// Typed client for the order center's app-template trade.
///
/// The deployments module is catalog-only — its generated Dart client carries
/// no purchase family — and `sdkwork-order` publishes no Dart client, so this
/// app keeps the two calls it needs here:
/// `POST /app/v3/api/app_template_orders` (`appTemplateOrders.create`) and
/// `GET /app/v3/api/app_template_orders` (`appTemplateOrders.list`).
///
/// Transport is `dart:io` (no new pub dependency), and the origin plus the
/// credentials come from the same [DeploymentsRuntimeConfig] the generated
/// catalog client uses. [OrderCenterRequest]/[OrderCenterTransport] are the
/// seam that lets tests assert request shaping without a socket.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'runtime_config.dart';

/// One order-center HTTP exchange, as handed to an [OrderCenterTransport].
class OrderCenterRequest {
  final String method;
  final Uri url;
  final Map<String, String> headers;

  /// JSON-encodable request body; `null` for a body-less call.
  final Object? body;

  const OrderCenterRequest({
    required this.method,
    required this.url,
    required this.headers,
    this.body,
  });
}

/// The raw pieces of an order-center answer the client needs.
class OrderCenterResponse {
  final int statusCode;
  final String body;

  const OrderCenterResponse({required this.statusCode, required this.body});
}

/// Transport seam: production uses [HttpClientOrderCenterTransport].
abstract class OrderCenterTransport {
  Future<OrderCenterResponse> send(OrderCenterRequest request);

  void close();
}

/// `dart:io` transport — the app's only raw-HTTP surface, by design.
class HttpClientOrderCenterTransport implements OrderCenterTransport {
  final HttpClient _client;
  final Duration _timeout;

  HttpClientOrderCenterTransport({
    HttpClient? client,
    Duration timeout = const Duration(seconds: 30),
  })  : _client = client ?? HttpClient(),
        _timeout = timeout;

  @override
  Future<OrderCenterResponse> send(OrderCenterRequest request) async {
    final httpRequest = await _client.openUrl(request.method, request.url).timeout(_timeout);
    for (final entry in request.headers.entries) {
      httpRequest.headers.set(entry.key, entry.value);
    }
    final body = request.body;
    if (body != null) {
      // A fixed content length keeps the body off chunked transfer encoding,
      // which the platform gateway expects for JSON commands.
      final payload = utf8.encode(jsonEncode(body));
      httpRequest.headers.contentLength = payload.length;
      httpRequest.add(payload);
    }
    final response = await httpRequest.close().timeout(_timeout);
    final text = await response.transform(utf8.decoder).join();
    return OrderCenterResponse(statusCode: response.statusCode, body: text);
  }

  @override
  void close() => _client.close(force: true);
}

/// A started trade: an order the buyer owns, or one they still have to pay.
class TemplateOrder {
  final String orderId;
  final String orderNo;
  final String outTradeNo;
  final String templateUuid;
  final String templateName;
  final String? versionUuid;
  final String amount;
  final String currencyCode;
  final String? expiresAt;
  final String? paymentMethod;
  final String paymentProduct;
  final String? qrCode;
  final String? qrCodeType;
  final String? paymentId;
  final Map<String, dynamic> paymentParams;
  final String status;
  final bool reused;
  final String? cashierUrl;

  const TemplateOrder({
    required this.orderId,
    required this.orderNo,
    required this.outTradeNo,
    required this.templateUuid,
    required this.templateName,
    this.versionUuid,
    required this.amount,
    required this.currencyCode,
    this.expiresAt,
    this.paymentMethod,
    required this.paymentProduct,
    this.qrCode,
    this.qrCodeType,
    this.paymentId,
    this.paymentParams = const <String, dynamic>{},
    required this.status,
    required this.reused,
    this.cashierUrl,
  });

  /// `status == 'paid'` is the install entitlement.
  static const String statusPaid = 'paid';

  /// The order exists but the buyer still has to settle it.
  static const String statusPendingPayment = 'pending_payment';

  /// `qrCode` carries a provider-native payment payload, not a URL.
  static const String qrCodeTypeProviderNative = 'provider_native';

  factory TemplateOrder.fromJson(Map<String, dynamic> json) => TemplateOrder(
        orderId: _text(json['orderId']),
        orderNo: _text(json['orderNo']),
        outTradeNo: _text(json['outTradeNo']),
        templateUuid: _text(json['templateUuid']),
        templateName: _text(json['templateName']),
        versionUuid: _optionalText(json['versionUuid']),
        amount: _text(json['amount']),
        currencyCode: _text(json['currencyCode']),
        expiresAt: _optionalText(json['expiresAt']),
        paymentMethod: _optionalText(json['paymentMethod']),
        paymentProduct: _text(json['paymentProduct']),
        qrCode: _optionalText(json['qrCode']),
        qrCodeType: _optionalText(json['qrCodeType']),
        paymentId: _optionalText(json['paymentId']),
        paymentParams: _map(json['paymentParams']),
        status: _text(json['status']),
        reused: json['reused'] == true,
        cashierUrl: _optionalText(json['cashierUrl']),
      );

  bool get isPaid => status == statusPaid;

  bool get requiresPayment => status == statusPendingPayment;

  /// What the buyer has to act on: the provider-native payload when the order
  /// center marks the QR as such, else the cashier URL. `null` for a settled
  /// order, which carries no payment requirement.
  String? get paymentRequirement {
    final payload = qrCode;
    if (qrCodeType == qrCodeTypeProviderNative && payload != null && payload.isNotEmpty) {
      return payload;
    }
    return cashierUrl ?? payload;
  }
}

/// One buyer order row from `appTemplateOrders.list`.
class TemplateOrderSummary {
  final String orderId;
  final String orderNo;
  final String templateUuid;
  final String templateName;
  final String? versionUuid;
  final String amount;
  final String currencyCode;
  final String status;
  final String fulfillmentStatus;
  final String? paidAt;
  final String createdAt;

  const TemplateOrderSummary({
    required this.orderId,
    required this.orderNo,
    required this.templateUuid,
    required this.templateName,
    this.versionUuid,
    required this.amount,
    required this.currencyCode,
    required this.status,
    required this.fulfillmentStatus,
    this.paidAt,
    required this.createdAt,
  });

  factory TemplateOrderSummary.fromJson(Map<String, dynamic> json) => TemplateOrderSummary(
        orderId: _text(json['orderId']),
        orderNo: _text(json['orderNo']),
        templateUuid: _text(json['templateUuid']),
        templateName: _text(json['templateName']),
        versionUuid: _optionalText(json['versionUuid']),
        amount: _text(json['amount']),
        currencyCode: _text(json['currencyCode']),
        status: _text(json['status']),
        fulfillmentStatus: _text(json['fulfillmentStatus']),
        paidAt: _optionalText(json['paidAt']),
        createdAt: _text(json['createdAt']),
      );

  /// Only a settled order entitles the buyer to install the template.
  bool get isEntitlement => status == TemplateOrder.statusPaid;
}

class OrderCenterException implements Exception {
  final String message;

  const OrderCenterException(this.message);

  @override
  String toString() => 'OrderCenterException: $message';
}

/// The two order-center calls the marketplace surfaces need.
class OrderCenterClient {
  final OrderCenterTransport _transport;
  final String _baseUrl;
  final String _authToken;
  final String _accessToken;
  final String Function() _idempotencyKey;

  OrderCenterClient({
    required String baseUrl,
    OrderCenterTransport? transport,
    String authToken = '',
    String accessToken = '',
    String Function()? idempotencyKey,
  })  : _baseUrl = baseUrl.replaceFirst(RegExp(r'/+$'), ''),
        _transport = transport ?? HttpClientOrderCenterTransport(),
        _authToken = authToken,
        _accessToken = accessToken,
        _idempotencyKey = idempotencyKey ?? _defaultIdempotencyKey;

  /// Same origin as the deployments app API; only the API family differs.
  factory OrderCenterClient.fromConfig(DeploymentsRuntimeConfig config) => OrderCenterClient(
        baseUrl: config.appApiBaseUrl,
        authToken: config.authToken,
        accessToken: config.accessToken,
      );

  static const String ordersPath = '/app/v3/api/app_template_orders';

  static String _defaultIdempotencyKey() {
    final random = Random();
    return '${DateTime.now().microsecondsSinceEpoch.toRadixString(36)}'
        '-${random.nextInt(1 << 32).toRadixString(36)}';
  }

  /// Starts (or reuses) the trade for one listing.
  ///
  /// A FREE listing answers `paid`; a PAID listing answers `pending_payment`
  /// with the cashier URL or provider QR code the buyer still has to settle.
  /// The command is idempotent per `Idempotency-Key`, so a retry after a
  /// transport failure cannot create a second order.
  Future<TemplateOrder> createTemplateOrder(String templateUuid) async {
    if (templateUuid.trim().isEmpty) {
      throw const OrderCenterException('a template uuid is required to start an order');
    }
    final data = await _send(
      'POST',
      Uri.parse('$_baseUrl$ordersPath'),
      body: <String, Object?>{'templateUuid': templateUuid},
      idempotencyKey: _idempotencyKey(),
    );
    final item = data['item'];
    if (item is! Map) {
      throw const OrderCenterException('the order center returned no order');
    }
    return TemplateOrder.fromJson(Map<String, dynamic>.from(item));
  }

  /// The buyer's template orders; `status == 'paid'` rows are entitlements.
  Future<List<TemplateOrderSummary>> listTemplateOrders({int page = 1, int pageSize = 100}) async {
    final data = await _send(
      'GET',
      Uri.parse('$_baseUrl$ordersPath').replace(
        queryParameters: <String, String>{'page': '$page', 'page_size': '$pageSize'},
      ),
    );
    final items = data['items'];
    if (items is! List) {
      throw const OrderCenterException('the order center returned no orders');
    }
    return items
        .whereType<Map>()
        .map((item) => TemplateOrderSummary.fromJson(Map<String, dynamic>.from(item)))
        .toList(growable: false);
  }

  void close() => _transport.close();

  Future<Map<String, dynamic>> _send(
    String method,
    Uri url, {
    Object? body,
    String? idempotencyKey,
  }) async {
    final headers = <String, String>{
      'Accept': 'application/json',
      if (body != null) 'Content-Type': 'application/json',
      if (idempotencyKey != null) 'Idempotency-Key': idempotencyKey,
      if (_authToken.isNotEmpty) 'Authorization': 'Bearer $_authToken',
      if (_accessToken.isNotEmpty) 'Access-Token': _accessToken,
    };
    final response = await _transport.send(
      OrderCenterRequest(method: method, url: url, headers: headers, body: body),
    );
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw OrderCenterException('the order center answered with ${response.statusCode}');
    }
    final decoded = _decode(response.body);
    if (decoded is! Map) {
      throw const OrderCenterException('the order center returned an unexpected envelope');
    }
    final envelope = Map<String, dynamic>.from(decoded);
    if (envelope['code'] != 0) {
      throw const OrderCenterException('the order center returned a failed envelope');
    }
    final data = envelope['data'];
    if (data is! Map) {
      throw const OrderCenterException('the order center returned no payload');
    }
    return Map<String, dynamic>.from(data);
  }

  static Object? _decode(String body) {
    try {
      return jsonDecode(body);
    } on FormatException {
      throw const OrderCenterException('the order center returned a non-JSON response');
    }
  }
}

String _text(Object? value) => value == null ? '' : value.toString();

String? _optionalText(Object? value) {
  final text = _text(value);
  return text.isEmpty ? null : text;
}

Map<String, dynamic> _map(Object? value) =>
    value is Map ? Map<String, dynamic>.from(value) : const <String, dynamic>{};
