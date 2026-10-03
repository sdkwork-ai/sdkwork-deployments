import 'package:sdkwork_deployments_app_sdk/sdkwork_deployments_app_sdk.dart';

import 'runtime_config.dart';

/// The marketplace port the UI consumes. Keeping the port explicit lets the
/// widget tree and tests run against a fake while production delegates every
/// call to the generated SDK client (never raw HTTP).
abstract class MarketplacePort {
  Future<List<TemplateCategoryResponse>> categories();
  Future<List<AppTemplateSummaryResponse>> browse({
    int page,
    int pageSize,
    String? keyword,
    String? categoryUuid,
    String? templateType,
  });
  Future<AppTemplateResponse> retrieve(String templateUuid);
  Future<TemplatePurchaseResponse> acquire(String templateUuid);
  Future<List<TemplatePurchaseResponse>> myPurchases();
  Future<List<AppTemplateResponse>> myTemplates();
  Future<List<AppTemplateVersionResponse>> versions(String templateUuid);
  Future<AppTemplateResponse> submit(String templateUuid);
  Future<void> withdraw(String templateUuid);
}

/// Production port over the generated Flutter/Dart transport.
///
/// The generated envelope responses carry `data` as the decoded JSON object,
/// so this port is also where the `item` / `items` projection happens — one
/// place, typed at the boundary, instead of maps leaking into widgets.
class SdkworkMarketplacePort implements MarketplacePort {
  final SdkworkAppClient _client;

  SdkworkMarketplacePort(this._client);

  factory SdkworkMarketplacePort.fromConfig(DeploymentsRuntimeConfig config) {
    return SdkworkMarketplacePort(
      SdkworkAppClient.withBaseUrl(
        baseUrl: config.appApiBaseUrl,
        authToken: config.authToken.isEmpty ? null : config.authToken,
        accessToken: config.accessToken.isEmpty ? null : config.accessToken,
      ),
    );
  }

  static Map<String, dynamic> _dataMap(dynamic payload, String what) {
    if (payload is Map<String, dynamic>) return payload;
    throw MarketplaceException('$what returned an unexpected payload shape');
  }

  static T _item<T>(dynamic payload, String what, T Function(Map<String, dynamic>) fromJson) {
    final data = _dataMap(payload, what);
    final item = data['item'];
    if (item is! Map<String, dynamic>) {
      throw MarketplaceException('$what returned no item');
    }
    return fromJson(item);
  }

  static List<T> _items<T>(dynamic payload, String what, T Function(Map<String, dynamic>) fromJson) {
    final data = _dataMap(payload, what);
    final items = data['items'];
    if (items is! List) {
      throw MarketplaceException('$what returned no items');
    }
    return items
        .whereType<Map<String, dynamic>>()
        .map(fromJson)
        .toList(growable: false);
  }

  @override
  Future<List<TemplateCategoryResponse>> categories() async {
    final response = await _client.template.categoriesList(false);
    if (response == null) throw const MarketplaceException('categories unavailable');
    return _items(response.data, 'categories', TemplateCategoryResponse.fromJson);
  }

  @override
  Future<List<AppTemplateSummaryResponse>> browse({
    int page = 1,
    int pageSize = 10,
    String? keyword,
    String? categoryUuid,
    String? templateType,
  }) async {
    final response = await _client.template.marketplaceTemplatesList(
      page,
      pageSize,
      (keyword == null || keyword.trim().isEmpty) ? null : keyword.trim(),
      (categoryUuid == null || categoryUuid.isEmpty) ? null : categoryUuid,
      null,
      (templateType == null || templateType.isEmpty) ? null : templateType,
      null,
    );
    if (response == null) throw const MarketplaceException('marketplace unavailable');
    return _items(response.data, 'marketplace', AppTemplateSummaryResponse.fromJson);
  }

  @override
  Future<AppTemplateResponse> retrieve(String templateUuid) async {
    final response = await _client.template.marketplaceTemplatesRetrieve(templateUuid);
    if (response == null) throw const MarketplaceException('template unavailable');
    return _item(response.data, 'marketplace template', AppTemplateResponse.fromJson);
  }

  @override
  Future<TemplatePurchaseResponse> acquire(String templateUuid) async {
    final response = await _client.template.purchasesCreate(
      templateUuid,
      CreateTemplatePurchaseRequest(),
      DateTime.now().microsecondsSinceEpoch.toRadixString(36),
    );
    if (response == null) throw const MarketplaceException('acquire unavailable');
    return _item(response.data, 'acquire', TemplatePurchaseResponse.fromJson);
  }

  @override
  Future<List<TemplatePurchaseResponse>> myPurchases() async {
    final response = await _client.template.purchasesList(1, 100, null);
    if (response == null) throw const MarketplaceException('purchases unavailable');
    return _items(response.data, 'purchases', TemplatePurchaseResponse.fromJson);
  }

  @override
  Future<List<AppTemplateResponse>> myTemplates() async {
    final response = await _client.template.appTemplatesList(1, 50, null, null);
    if (response == null) throw const MarketplaceException('templates unavailable');
    return _items(response.data, 'templates', AppTemplateResponse.fromJson);
  }

  @override
  Future<List<AppTemplateVersionResponse>> versions(String templateUuid) async {
    final response = await _client.template.appTemplateVersionsList(templateUuid, 1, 50);
    if (response == null) throw const MarketplaceException('versions unavailable');
    return _items(response.data, 'versions', AppTemplateVersionResponse.fromJson);
  }

  @override
  Future<AppTemplateResponse> submit(String templateUuid) async {
    final response = await _client.template.appTemplatesSubmit(templateUuid);
    if (response == null) throw const MarketplaceException('submit unavailable');
    return _item(response.data, 'submit', AppTemplateResponse.fromJson);
  }

  @override
  Future<void> withdraw(String templateUuid) {
    return _client.template.appTemplatesDelete(templateUuid);
  }
}

class MarketplaceException implements Exception {
  final String message;

  const MarketplaceException(this.message);

  @override
  String toString() => 'MarketplaceException: $message';
}
