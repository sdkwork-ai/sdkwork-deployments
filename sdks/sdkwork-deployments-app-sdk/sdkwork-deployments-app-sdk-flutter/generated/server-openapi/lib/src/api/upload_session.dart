import 'dart:convert';
import '../http/client.dart';
import '../models.dart';

import 'paths.dart';
import 'response_helpers.dart';


class UploadSessionApi {
  final HttpClient _client;

  UploadSessionApi(this._client);

  /// 创建 Drive-backed 包上传会话
  Future<UploadSessionsCreateResponse201?> uploadSessionsCreate(CreateDeployUploadSessionRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.post(ApiPaths.appPath('/upload_sessions'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : UploadSessionsCreateResponse201.fromJson(map);
    })();
  }

  /// 获取上传会话
  Future<UploadSessionsRetrieveResponse?> uploadSessionsRetrieve(String uploadSessionId) async {
    final response = await _client.get(ApiPaths.appPath('/upload_sessions/${serializePathParameter(uploadSessionId, const PathParameterSpec('uploadSessionId', 'simple', false))}'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : UploadSessionsRetrieveResponse.fromJson(map);
    })();
  }

  /// 完成上传会话
  Future<UploadSessionsCompleteResponse?> uploadSessionsComplete(String uploadSessionId, CompleteDeployUploadSessionRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.post(ApiPaths.appPath('/upload_sessions/${serializePathParameter(uploadSessionId, const PathParameterSpec('uploadSessionId', 'simple', false))}/complete'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : UploadSessionsCompleteResponse.fromJson(map);
    })();
  }

  /// 取消上传会话
  Future<UploadSessionsCancelResponse?> uploadSessionsCancel(String uploadSessionId, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final response = await _client.post(ApiPaths.appPath('/upload_sessions/${serializePathParameter(uploadSessionId, const PathParameterSpec('uploadSessionId', 'simple', false))}/cancel'), headers: requestHeaders);
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : UploadSessionsCancelResponse.fromJson(map);
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
