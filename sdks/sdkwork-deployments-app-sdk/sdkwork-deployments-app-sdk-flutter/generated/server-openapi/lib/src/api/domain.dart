import 'dart:convert';
import '../http/client.dart';
import '../models.dart';

import 'paths.dart';
import 'response_helpers.dart';


class DomainApi {
  final HttpClient _client;

  DomainApi(this._client);

  /// List root domain zones
  Future<DomainZonesListResponse?> zonesList([int? page, int? pageSize, String? status, String? keyword, String? scope, String? providerAccountId]) async {
    final query = buildQueryString([
      QueryParameterSpec('page', page, 'form', true, false, null),
      QueryParameterSpec('page_size', pageSize, 'form', true, false, null),
      QueryParameterSpec('status', status, 'form', true, false, null),
      QueryParameterSpec('keyword', keyword, 'form', true, false, null),
      QueryParameterSpec('scope', scope, 'form', true, false, null),
      QueryParameterSpec('provider_account_id', providerAccountId, 'form', true, false, null)
    ]);
    final response = await _client.get(ApiPaths.appendQueryString(ApiPaths.appPath('/domain_zones'), query));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesListResponse.fromJson(map);
    })();
  }

  /// Define a root domain zone
  Future<DomainZonesCreateResponse201?> zonesCreate(CreateDomainZoneRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.post(ApiPaths.appPath('/domain_zones'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesCreateResponse201.fromJson(map);
    })();
  }

  /// Retrieve a root domain zone
  Future<DomainZonesRetrieveResponse?> zonesRetrieve(String zoneId) async {
    final response = await _client.get(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesRetrieveResponse.fromJson(map);
    })();
  }

  /// Update a root domain zone
  Future<DomainZonesUpdateResponse?> zonesUpdate(String zoneId, UpdateDomainZoneRequest body) async {
    final payload = body.toJson();
    final response = await _client.patch(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}'), body: payload, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesUpdateResponse.fromJson(map);
    })();
  }

  /// Delete an empty root domain zone
  Future<void> zonesDelete(String zoneId) async {
    await _client.delete(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}'));
  }

  /// List hostnames in a root domain zone
  Future<DomainZonesHostnamesListResponse?> zonesHostnamesList(String zoneId, [int? page, int? pageSize]) async {
    final query = buildQueryString([
      QueryParameterSpec('page', page, 'form', true, false, null),
      QueryParameterSpec('page_size', pageSize, 'form', true, false, null)
    ]);
    final response = await _client.get(ApiPaths.appendQueryString(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/hostnames'), query));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesHostnamesListResponse.fromJson(map);
    })();
  }

  /// Create a hostname in a root domain zone
  Future<DomainZonesHostnamesCreateResponse201?> zonesHostnamesCreate(String zoneId, CreateDomainHostnameRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.post(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/hostnames'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesHostnamesCreateResponse201.fromJson(map);
    })();
  }

  /// Retrieve a domain hostname
  Future<DomainZonesHostnamesRetrieveResponse?> zonesHostnamesRetrieve(String zoneId, String hostnameId) async {
    final response = await _client.get(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/hostnames/${serializePathParameter(hostnameId, const PathParameterSpec('hostnameId', 'simple', false))}'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesHostnamesRetrieveResponse.fromJson(map);
    })();
  }

  /// Rename a domain hostname
  Future<DomainZonesHostnamesUpdateResponse?> zonesHostnamesUpdate(String zoneId, String hostnameId, UpdateDomainHostnameRequest body) async {
    final payload = body.toJson();
    final response = await _client.patch(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/hostnames/${serializePathParameter(hostnameId, const PathParameterSpec('hostnameId', 'simple', false))}'), body: payload, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesHostnamesUpdateResponse.fromJson(map);
    })();
  }

  /// Delete an unbound domain hostname
  Future<void> zonesHostnamesDelete(String zoneId, String hostnameId) async {
    await _client.delete(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/hostnames/${serializePathParameter(hostnameId, const PathParameterSpec('hostnameId', 'simple', false))}'));
  }

  /// Verify domain hostname ownership
  Future<DomainZonesHostnamesVerifyResponse?> zonesHostnamesVerify(String zoneId, String hostnameId, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final response = await _client.post(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/hostnames/${serializePathParameter(hostnameId, const PathParameterSpec('hostnameId', 'simple', false))}/verify'), headers: requestHeaders);
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesHostnamesVerifyResponse.fromJson(map);
    })();
  }

  /// Ensure ownership of a set of hostnames
  Future<DomainZonesHostnameClaimsEnsureResponse?> zonesHostnameClaimsEnsure(String zoneId, EnsureDomainHostnameClaimsRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.post(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/hostname_claims'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesHostnameClaimsEnsureResponse.fromJson(map);
    })();
  }

  /// Create a resolution record through the Zone's cloud account
  Future<DomainZonesDnsRecordsCreateResponse201?> zonesDnsRecordsCreate(String zoneId, CreateDomainDnsRecordRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.post(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/dns_records'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesDnsRecordsCreateResponse201.fromJson(map);
    })();
  }

  /// List a Zone's synced DNS resolution records
  Future<DomainZonesDnsRecordsListResponse?> zonesDnsRecordsList(String zoneId, [int? page, int? pageSize, String? hostnameId, String? host, String? recordType]) async {
    final query = buildQueryString([
      QueryParameterSpec('page', page, 'form', true, false, null),
      QueryParameterSpec('page_size', pageSize, 'form', true, false, null),
      QueryParameterSpec('hostname_id', hostnameId, 'form', true, false, null),
      QueryParameterSpec('host', host, 'form', true, false, null),
      QueryParameterSpec('record_type', recordType, 'form', true, false, null)
    ]);
    final response = await _client.get(ApiPaths.appendQueryString(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/dns_records'), query));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesDnsRecordsListResponse.fromJson(map);
    })();
  }

  /// Sync a Zone's resolution records through its cloud account
  Future<DomainZonesDnsRecordsSyncResponse?> zonesDnsRecordsSync(String zoneId, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final response = await _client.post(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/dns_records/sync'), headers: requestHeaders);
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesDnsRecordsSyncResponse.fromJson(map);
    })();
  }

  /// Replace a resolution record in place through the Zone's cloud account
  Future<DomainZonesDnsRecordsUpdateResponse?> zonesDnsRecordsUpdate(String zoneId, String recordId, UpdateDomainDnsRecordRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.patch(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/dns_records/${serializePathParameter(recordId, const PathParameterSpec('recordId', 'simple', false))}'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesDnsRecordsUpdateResponse.fromJson(map);
    })();
  }

  /// Delete a resolution record through the Zone's cloud account
  Future<void> zonesDnsRecordsDelete(String zoneId, String recordId, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    await _client.delete(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/dns_records/${serializePathParameter(recordId, const PathParameterSpec('recordId', 'simple', false))}'), headers: requestHeaders);
  }

  /// Pause a resolution record or resume it
  Future<DomainZonesDnsRecordsStatusUpdateResponse?> zonesDnsRecordsStatusUpdate(String zoneId, String recordId, DomainDnsRecordStatusRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.patch(ApiPaths.appPath('/domain_zones/${serializePathParameter(zoneId, const PathParameterSpec('zoneId', 'simple', false))}/dns_records/${serializePathParameter(recordId, const PathParameterSpec('recordId', 'simple', false))}/status'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : DomainZonesDnsRecordsStatusUpdateResponse.fromJson(map);
    })();
  }

  /// List cloud accounts usable for DNS automation
  Future<CloudAccountsListResponse?> cloudAccountsList([int? page, int? pageSize, String? dnsProvider, String? scopeType, bool? mine, String? keyword]) async {
    final query = buildQueryString([
      QueryParameterSpec('page', page, 'form', true, false, null),
      QueryParameterSpec('page_size', pageSize, 'form', true, false, null),
      QueryParameterSpec('dnsProvider', dnsProvider, 'form', true, false, null),
      QueryParameterSpec('scopeType', scopeType, 'form', true, false, null),
      QueryParameterSpec('mine', mine, 'form', true, false, null),
      QueryParameterSpec('keyword', keyword, 'form', true, false, null)
    ]);
    final response = await _client.get(ApiPaths.appendQueryString(ApiPaths.appPath('/cloud_accounts'), query));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : CloudAccountsListResponse.fromJson(map);
    })();
  }

  /// Register a cloud account for DNS automation
  Future<CloudAccountsCreateResponse201?> cloudAccountsCreate(CreateCloudAccountRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.post(ApiPaths.appPath('/cloud_accounts'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : CloudAccountsCreateResponse201.fromJson(map);
    })();
  }
}

class PathParameterSpec {
  final String name;
  final String style;
  final bool explode;

  const PathParameterSpec(this.name, this.style, this.explode);
}

String serializePathParameter(dynamic value, PathParameterSpec spec) {
  if (value == null) return '';
  final style = spec.style.trim().isEmpty ? 'simple' : spec.style;
  if (value is Iterable) {
    return serializePathArray(spec.name, value, style, spec.explode);
  }
  if (value is Map) {
    return serializePathObject(spec.name, value, style, spec.explode);
  }
  return pathPrimitivePrefix(spec.name, style) + Uri.encodeComponent(value.toString());
}

String serializePathArray(String name, Iterable values, String style, bool explode) {
  final serialized = values.where((item) => item != null).map((item) => Uri.encodeComponent(item.toString())).toList();
  if (serialized.isEmpty) return pathPrefix(name, style);
  if (style == 'matrix') {
    if (explode) {
      return serialized.map((item) => ';$name=$item').join();
    }
    return ';$name=${serialized.join(',')}';
  }
  final separator = explode ? '.' : ',';
  return pathPrefix(name, style) + serialized.join(separator);
}

String serializePathObject(String name, Map values, String style, bool explode) {
  final entries = <String>[];
  final exploded = <String>[];
  values.forEach((key, value) {
    if (value == null) return;
    final escapedKey = Uri.encodeComponent(key.toString());
    final escapedValue = Uri.encodeComponent(value.toString());
    if (explode) {
      if (style == 'matrix') {
        exploded.add(';$escapedKey=$escapedValue');
      } else {
        exploded.add('$escapedKey=$escapedValue');
      }
    } else {
      entries.add(escapedKey);
      entries.add(escapedValue);
    }
  });
  if (style == 'matrix') {
    if (explode) return exploded.join();
    return ';$name=${entries.join(',')}';
  }
  if (explode) {
    final separator = style == 'label' ? '.' : ',';
    return pathPrefix(name, style) + exploded.join(separator);
  }
  return pathPrefix(name, style) + entries.join(',');
}

String pathPrefix(String name, String style) {
  if (style == 'label') return '.';
  if (style == 'matrix') return ';$name';
  return '';
}

String pathPrimitivePrefix(String name, String style) {
  return style == 'matrix' ? ';$name=' : pathPrefix(name, style);
}
class QueryParameterSpec {
  final String name;
  final dynamic value;
  final String style;
  final bool explode;
  final bool allowReserved;
  final String? contentType;

  const QueryParameterSpec(
    this.name,
    this.value,
    this.style,
    this.explode,
    this.allowReserved,
    this.contentType,
  );
}

String buildQueryString(List<QueryParameterSpec> parameters) {
  final pairs = <String>[];
  for (final parameter in parameters) {
    appendSerializedParameter(pairs, parameter);
  }
  return pairs.join('&');
}

void appendSerializedParameter(List<String> pairs, QueryParameterSpec parameter) {
  final value = parameter.value;
  if (value == null) return;

  final contentType = parameter.contentType;
  if (contentType != null && contentType.trim().isNotEmpty) {
    pairs.add('${urlEncode(parameter.name)}=${encodeQueryValue(jsonEncode(value), parameter.allowReserved)}');
    return;
  }

  final style = parameter.style.trim().isEmpty ? 'form' : parameter.style;
  if (style == 'deepObject' && value is Map) {
    appendDeepObjectParameter(pairs, parameter.name, value, parameter.allowReserved);
    return;
  }
  if (value is Iterable) {
    appendArrayParameter(pairs, parameter.name, value, style, parameter.explode, parameter.allowReserved);
    return;
  }
  if (value is Map) {
    appendObjectParameter(pairs, parameter.name, value, style, parameter.explode, parameter.allowReserved);
    return;
  }
  pairs.add('${urlEncode(parameter.name)}=${encodeQueryValue(value.toString(), parameter.allowReserved)}');
}

void appendArrayParameter(
  List<String> pairs,
  String name,
  Iterable values,
  String style,
  bool explode,
  bool allowReserved,
) {
  final serialized = values.where((item) => item != null).map((item) => item.toString()).toList();
  if (serialized.isEmpty) return;
  if (style == 'form' && explode) {
    for (final item in serialized) {
      pairs.add('${urlEncode(name)}=${encodeQueryValue(item, allowReserved)}');
    }
    return;
  }
  pairs.add('${urlEncode(name)}=${encodeQueryValue(serialized.join(','), allowReserved)}');
}

void appendObjectParameter(
  List<String> pairs,
  String name,
  Map values,
  String style,
  bool explode,
  bool allowReserved,
) {
  final serialized = <String>[];
  values.forEach((key, value) {
    if (value == null) return;
    if (style == 'form' && explode) {
      pairs.add('${urlEncode(key.toString())}=${encodeQueryValue(value.toString(), allowReserved)}');
      return;
    }
    serialized.add(key.toString());
    serialized.add(value.toString());
  });
  if (serialized.isNotEmpty) {
    pairs.add('${urlEncode(name)}=${encodeQueryValue(serialized.join(','), allowReserved)}');
  }
}

void appendDeepObjectParameter(List<String> pairs, String name, Map values, bool allowReserved) {
  values.forEach((key, value) {
    if (value != null) {
      pairs.add('${urlEncode('$name[$key]')}=${encodeQueryValue(value.toString(), allowReserved)}');
    }
  });
}

String encodeQueryValue(String value, bool allowReserved) {
  var encoded = urlEncode(value);
  if (!allowReserved) return encoded;
  const replacements = <String, String>{
    '%3A': ':',
    '%2F': '/',
    '%3F': '?',
    '%23': '#',
    '%5B': '[',
    '%5D': ']',
    '%40': '@',
    '%21': '!',
    '%24': r'$',
    '%26': '&',
    '%27': "'",
    '%28': '(',
    '%29': ')',
    '%2A': '*',
    '%2B': '+',
    '%2C': ',',
    '%3B': ';',
    '%3D': '=',
  };
  replacements.forEach((escaped, reserved) {
    encoded = encoded.replaceAll(escaped, reserved);
  });
  return encoded;
}

String urlEncode(String value) => Uri.encodeQueryComponent(value);
class HeaderParameterSpec {
  final dynamic value;
  final String style;
  final bool explode;
  final String? contentType;

  HeaderParameterSpec(this.value, this.style, this.explode, this.contentType);
}

Map<String, String>? buildRequestHeaders(
  Map<String, HeaderParameterSpec> headers, [
  Map<String, HeaderParameterSpec> cookies = const {},
]) {
  final requestHeaders = <String, String>{};

  headers.forEach((name, parameter) {
    final serialized = serializeParameterValue(parameter);
    if (serialized != null) {
      requestHeaders[name] = serialized;
    }
  });

  final cookieHeader = buildCookieHeader(cookies);
  if (cookieHeader != null && cookieHeader.isNotEmpty) {
    requestHeaders['Cookie'] = requestHeaders.containsKey('Cookie')
        ? '${requestHeaders['Cookie']}; $cookieHeader'
        : cookieHeader;
  }

  return requestHeaders.isEmpty ? null : requestHeaders;
}

String? buildCookieHeader(Map<String, HeaderParameterSpec> cookies) {
  final pairs = <String>[];
  cookies.forEach((name, parameter) {
    final serialized = serializeParameterValue(parameter);
    if (serialized != null) {
      pairs.add('${Uri.encodeComponent(name)}=${Uri.encodeComponent(serialized)}');
    }
  });
  return pairs.isEmpty ? null : pairs.join('; ');
}

String? serializeParameterValue(HeaderParameterSpec? parameter) {
  final value = parameter?.value;
  if (value == null) return null;
  if (parameter!.contentType != null && parameter.contentType!.trim().isNotEmpty) {
    return jsonEncode(value);
  }
  if (value is DateTime) return value.toIso8601String();
  if (value is Iterable) {
    return value
        .where((item) => item != null)
        .map((item) => item.toString())
        .whereType<String>()
        .join(',');
  }
  if (value is Map) {
    final serialized = <String>[];
    value.forEach((key, item) {
      if (item == null) return;
      if (parameter.explode) {
        serialized.add('$key=$item');
      } else {
        serialized.add(key.toString());
        serialized.add(item.toString());
      }
    });
    return serialized.join(',');
  }
  return value.toString();
}
