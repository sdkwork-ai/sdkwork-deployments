Map<String, dynamic>? _sdkworkAsMap(dynamic value) {
  if (value is Map<String, dynamic>) {
    return value;
  }
  if (value is Map) {
    return value.map((key, item) => MapEntry(key.toString(), item));
  }
  return null;
}

List<dynamic>? _sdkworkAsList(dynamic value) {
  return value is List ? value : null;
}

class TemplateCategoryResponse {
  final String id;
  final String categoryKey;
  final String? parentId;
  final String displayName;
  final String? description;
  final int sortOrder;
  final String status;
  final String createdAt;
  final String updatedAt;
  final String version;

  TemplateCategoryResponse({
    required this.id,
    required this.categoryKey,
    this.parentId,
    required this.displayName,
    this.description,
    required this.sortOrder,
    required this.status,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory TemplateCategoryResponse.fromJson(Map<String, dynamic> json) {
    return TemplateCategoryResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('TemplateCategoryResponse.id is required');
        }
        return value;
      })(),
      categoryKey: (() {
        final value = json['categoryKey']?.toString();
        if (value == null) {
          throw FormatException('TemplateCategoryResponse.categoryKey is required');
        }
        return value;
      })(),
      parentId: json['parentId']?.toString(),
      displayName: (() {
        final value = json['displayName']?.toString();
        if (value == null) {
          throw FormatException('TemplateCategoryResponse.displayName is required');
        }
        return value;
      })(),
      description: json['description']?.toString(),
      sortOrder: (() {
        final value = json['sortOrder'];
        if (value is! int) {
          throw FormatException('TemplateCategoryResponse.sortOrder is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('TemplateCategoryResponse.status is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('TemplateCategoryResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('TemplateCategoryResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('TemplateCategoryResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'categoryKey': categoryKey,
      'parentId': parentId,
      'displayName': displayName,
      'description': description,
      'sortOrder': sortOrder,
      'status': status,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class AppTemplateSummaryResponse {
  final String id;
  final String templateType;
  final String templateKey;
  final String displayName;
  final String summary;
  final String categoryUuid;
  final String? iconMediaRef;
  final String? coverMediaRef;
  final String visibility;
  final String pricingModel;
  final String priceMinor;
  final String currency;
  final String status;
  final bool isFeatured;
  final String installCount;
  final String viewCount;
  final String? latestVersionUuid;
  final String updatedAt;
  final String version;

  AppTemplateSummaryResponse({
    required this.id,
    required this.templateType,
    required this.templateKey,
    required this.displayName,
    required this.summary,
    required this.categoryUuid,
    this.iconMediaRef,
    this.coverMediaRef,
    required this.visibility,
    required this.pricingModel,
    required this.priceMinor,
    required this.currency,
    required this.status,
    required this.isFeatured,
    required this.installCount,
    required this.viewCount,
    this.latestVersionUuid,
    required this.updatedAt,
    required this.version
  });

  factory AppTemplateSummaryResponse.fromJson(Map<String, dynamic> json) {
    return AppTemplateSummaryResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.id is required');
        }
        return value;
      })(),
      templateType: (() {
        final value = json['templateType']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.templateType is required');
        }
        return value;
      })(),
      templateKey: (() {
        final value = json['templateKey']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.templateKey is required');
        }
        return value;
      })(),
      displayName: (() {
        final value = json['displayName']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.displayName is required');
        }
        return value;
      })(),
      summary: (() {
        final value = json['summary']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.summary is required');
        }
        return value;
      })(),
      categoryUuid: (() {
        final value = json['categoryUuid']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.categoryUuid is required');
        }
        return value;
      })(),
      iconMediaRef: json['iconMediaRef']?.toString(),
      coverMediaRef: json['coverMediaRef']?.toString(),
      visibility: (() {
        final value = json['visibility']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.visibility is required');
        }
        return value;
      })(),
      pricingModel: (() {
        final value = json['pricingModel']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.pricingModel is required');
        }
        return value;
      })(),
      priceMinor: (() {
        final value = json['priceMinor']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.priceMinor is required');
        }
        return value;
      })(),
      currency: (() {
        final value = json['currency']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.currency is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.status is required');
        }
        return value;
      })(),
      isFeatured: (() {
        final value = json['isFeatured'];
        if (value is! bool) {
          throw FormatException('AppTemplateSummaryResponse.isFeatured is required');
        }
        return value;
      })(),
      installCount: (() {
        final value = json['installCount']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.installCount is required');
        }
        return value;
      })(),
      viewCount: (() {
        final value = json['viewCount']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.viewCount is required');
        }
        return value;
      })(),
      latestVersionUuid: json['latestVersionUuid']?.toString(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateSummaryResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'templateType': templateType,
      'templateKey': templateKey,
      'displayName': displayName,
      'summary': summary,
      'categoryUuid': categoryUuid,
      'iconMediaRef': iconMediaRef,
      'coverMediaRef': coverMediaRef,
      'visibility': visibility,
      'pricingModel': pricingModel,
      'priceMinor': priceMinor,
      'currency': currency,
      'status': status,
      'isFeatured': isFeatured,
      'installCount': installCount,
      'viewCount': viewCount,
      'latestVersionUuid': latestVersionUuid,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class AppTemplateResponse {
  final String id;
  final String templateKey;
  final String displayName;
  final String summary;
  final String description;
  final String appUuid;
  final String templateType;
  final String categoryUuid;
  final String authorUserId;
  final String? iconMediaRef;
  final String? coverMediaRef;
  final String visibility;
  final String pricingModel;
  final String priceMinor;
  final String currency;
  final String status;
  final String? reviewNote;
  final bool isFeatured;
  final String installCount;
  final String viewCount;
  final String? latestVersionUuid;
  final String createdAt;
  final String updatedAt;
  final String version;

  AppTemplateResponse({
    required this.id,
    required this.templateKey,
    required this.displayName,
    required this.summary,
    required this.description,
    required this.appUuid,
    required this.templateType,
    required this.categoryUuid,
    required this.authorUserId,
    this.iconMediaRef,
    this.coverMediaRef,
    required this.visibility,
    required this.pricingModel,
    required this.priceMinor,
    required this.currency,
    required this.status,
    this.reviewNote,
    required this.isFeatured,
    required this.installCount,
    required this.viewCount,
    this.latestVersionUuid,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory AppTemplateResponse.fromJson(Map<String, dynamic> json) {
    return AppTemplateResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.id is required');
        }
        return value;
      })(),
      templateKey: (() {
        final value = json['templateKey']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.templateKey is required');
        }
        return value;
      })(),
      displayName: (() {
        final value = json['displayName']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.displayName is required');
        }
        return value;
      })(),
      summary: (() {
        final value = json['summary']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.summary is required');
        }
        return value;
      })(),
      description: (() {
        final value = json['description']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.description is required');
        }
        return value;
      })(),
      appUuid: (() {
        final value = json['appUuid']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.appUuid is required');
        }
        return value;
      })(),
      templateType: (() {
        final value = json['templateType']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.templateType is required');
        }
        return value;
      })(),
      categoryUuid: (() {
        final value = json['categoryUuid']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.categoryUuid is required');
        }
        return value;
      })(),
      authorUserId: (() {
        final value = json['authorUserId']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.authorUserId is required');
        }
        return value;
      })(),
      iconMediaRef: json['iconMediaRef']?.toString(),
      coverMediaRef: json['coverMediaRef']?.toString(),
      visibility: (() {
        final value = json['visibility']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.visibility is required');
        }
        return value;
      })(),
      pricingModel: (() {
        final value = json['pricingModel']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.pricingModel is required');
        }
        return value;
      })(),
      priceMinor: (() {
        final value = json['priceMinor']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.priceMinor is required');
        }
        return value;
      })(),
      currency: (() {
        final value = json['currency']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.currency is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.status is required');
        }
        return value;
      })(),
      reviewNote: json['reviewNote']?.toString(),
      isFeatured: (() {
        final value = json['isFeatured'];
        if (value is! bool) {
          throw FormatException('AppTemplateResponse.isFeatured is required');
        }
        return value;
      })(),
      installCount: (() {
        final value = json['installCount']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.installCount is required');
        }
        return value;
      })(),
      viewCount: (() {
        final value = json['viewCount']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.viewCount is required');
        }
        return value;
      })(),
      latestVersionUuid: json['latestVersionUuid']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'templateKey': templateKey,
      'displayName': displayName,
      'summary': summary,
      'description': description,
      'appUuid': appUuid,
      'templateType': templateType,
      'categoryUuid': categoryUuid,
      'authorUserId': authorUserId,
      'iconMediaRef': iconMediaRef,
      'coverMediaRef': coverMediaRef,
      'visibility': visibility,
      'pricingModel': pricingModel,
      'priceMinor': priceMinor,
      'currency': currency,
      'status': status,
      'reviewNote': reviewNote,
      'isFeatured': isFeatured,
      'installCount': installCount,
      'viewCount': viewCount,
      'latestVersionUuid': latestVersionUuid,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class AppTemplateVersionResponse {
  final String id;
  final String templateUuid;
  final String templateVersion;
  final String changelog;
  final String? artifactUuid;
  final String? sourceAppVersion;
  final List<String> platformTargets;
  final String packageSizeBytes;
  final String? checksumSha256;
  final String status;
  final String? publishedAt;
  final String createdAt;
  final String updatedAt;
  final String version;

  AppTemplateVersionResponse({
    required this.id,
    required this.templateUuid,
    required this.templateVersion,
    required this.changelog,
    this.artifactUuid,
    this.sourceAppVersion,
    required this.platformTargets,
    required this.packageSizeBytes,
    this.checksumSha256,
    required this.status,
    this.publishedAt,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory AppTemplateVersionResponse.fromJson(Map<String, dynamic> json) {
    return AppTemplateVersionResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionResponse.id is required');
        }
        return value;
      })(),
      templateUuid: (() {
        final value = json['templateUuid']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionResponse.templateUuid is required');
        }
        return value;
      })(),
      templateVersion: (() {
        final value = json['templateVersion']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionResponse.templateVersion is required');
        }
        return value;
      })(),
      changelog: (() {
        final value = json['changelog']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionResponse.changelog is required');
        }
        return value;
      })(),
      artifactUuid: json['artifactUuid']?.toString(),
      sourceAppVersion: json['sourceAppVersion']?.toString(),
      platformTargets: (() {
        final list = _sdkworkAsList(json['platformTargets']);
        if (list == null) {
          throw FormatException('AppTemplateVersionResponse.platformTargets is required');
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      packageSizeBytes: (() {
        final value = json['packageSizeBytes']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionResponse.packageSizeBytes is required');
        }
        return value;
      })(),
      checksumSha256: json['checksumSha256']?.toString(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionResponse.status is required');
        }
        return value;
      })(),
      publishedAt: json['publishedAt']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'templateUuid': templateUuid,
      'templateVersion': templateVersion,
      'changelog': changelog,
      'artifactUuid': artifactUuid,
      'sourceAppVersion': sourceAppVersion,
      'platformTargets': platformTargets.map((item) => item).toList(),
      'packageSizeBytes': packageSizeBytes,
      'checksumSha256': checksumSha256,
      'status': status,
      'publishedAt': publishedAt,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class CreateAppTemplateRequest {
  final String appUuid;
  final String? templateType;
  final String categoryUuid;
  final String templateKey;
  final String displayName;
  final String summary;
  final String? description;
  final String? visibility;
  final String? pricingModel;
  final String? priceMinor;
  final String? currency;
  final String? iconMediaRef;
  final String? coverMediaRef;
  final CreateAppTemplateVersionRequest? initialVersion;

  CreateAppTemplateRequest({
    required this.appUuid,
    this.templateType,
    required this.categoryUuid,
    required this.templateKey,
    required this.displayName,
    required this.summary,
    this.description,
    this.visibility,
    this.pricingModel,
    this.priceMinor,
    this.currency,
    this.iconMediaRef,
    this.coverMediaRef,
    this.initialVersion
  });

  factory CreateAppTemplateRequest.fromJson(Map<String, dynamic> json) {
    return CreateAppTemplateRequest(
      appUuid: (() {
        final value = json['appUuid']?.toString();
        if (value == null) {
          throw FormatException('CreateAppTemplateRequest.appUuid is required');
        }
        return value;
      })(),
      templateType: json['templateType']?.toString(),
      categoryUuid: (() {
        final value = json['categoryUuid']?.toString();
        if (value == null) {
          throw FormatException('CreateAppTemplateRequest.categoryUuid is required');
        }
        return value;
      })(),
      templateKey: (() {
        final value = json['templateKey']?.toString();
        if (value == null) {
          throw FormatException('CreateAppTemplateRequest.templateKey is required');
        }
        return value;
      })(),
      displayName: (() {
        final value = json['displayName']?.toString();
        if (value == null) {
          throw FormatException('CreateAppTemplateRequest.displayName is required');
        }
        return value;
      })(),
      summary: (() {
        final value = json['summary']?.toString();
        if (value == null) {
          throw FormatException('CreateAppTemplateRequest.summary is required');
        }
        return value;
      })(),
      description: json['description']?.toString(),
      visibility: json['visibility']?.toString(),
      pricingModel: json['pricingModel']?.toString(),
      priceMinor: json['priceMinor']?.toString(),
      currency: json['currency']?.toString(),
      iconMediaRef: json['iconMediaRef']?.toString(),
      coverMediaRef: json['coverMediaRef']?.toString(),
      initialVersion: (() {
        final map = _sdkworkAsMap(json['initialVersion']);
        return map == null ? null : CreateAppTemplateVersionRequest.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'appUuid': appUuid,
      'templateType': templateType,
      'categoryUuid': categoryUuid,
      'templateKey': templateKey,
      'displayName': displayName,
      'summary': summary,
      'description': description,
      'visibility': visibility,
      'pricingModel': pricingModel,
      'priceMinor': priceMinor,
      'currency': currency,
      'iconMediaRef': iconMediaRef,
      'coverMediaRef': coverMediaRef,
      'initialVersion': initialVersion?.toJson(),
    };
  }
}

class UpdateAppTemplateRequest {
  final String? displayName;
  final String? summary;
  final String? description;
  final String? categoryUuid;
  final String? visibility;
  final String? pricingModel;
  final String? priceMinor;
  final String? currency;
  final String? iconMediaRef;
  final String? coverMediaRef;

  UpdateAppTemplateRequest({
    this.displayName,
    this.summary,
    this.description,
    this.categoryUuid,
    this.visibility,
    this.pricingModel,
    this.priceMinor,
    this.currency,
    this.iconMediaRef,
    this.coverMediaRef
  });

  factory UpdateAppTemplateRequest.fromJson(Map<String, dynamic> json) {
    return UpdateAppTemplateRequest(
      displayName: json['displayName']?.toString(),
      summary: json['summary']?.toString(),
      description: json['description']?.toString(),
      categoryUuid: json['categoryUuid']?.toString(),
      visibility: json['visibility']?.toString(),
      pricingModel: json['pricingModel']?.toString(),
      priceMinor: json['priceMinor']?.toString(),
      currency: json['currency']?.toString(),
      iconMediaRef: json['iconMediaRef']?.toString(),
      coverMediaRef: json['coverMediaRef']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'displayName': displayName,
      'summary': summary,
      'description': description,
      'categoryUuid': categoryUuid,
      'visibility': visibility,
      'pricingModel': pricingModel,
      'priceMinor': priceMinor,
      'currency': currency,
      'iconMediaRef': iconMediaRef,
      'coverMediaRef': coverMediaRef,
    };
  }
}

class CreateAppTemplateVersionRequest {
  final String version;
  final String? changelog;
  final String? artifactUuid;
  final String? sourceAppVersion;
  final List<String>? platformTargets;
  final String? packageSizeBytes;
  final String? checksumSha256;

  CreateAppTemplateVersionRequest({
    required this.version,
    this.changelog,
    this.artifactUuid,
    this.sourceAppVersion,
    this.platformTargets,
    this.packageSizeBytes,
    this.checksumSha256
  });

  factory CreateAppTemplateVersionRequest.fromJson(Map<String, dynamic> json) {
    return CreateAppTemplateVersionRequest(
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('CreateAppTemplateVersionRequest.version is required');
        }
        return value;
      })(),
      changelog: json['changelog']?.toString(),
      artifactUuid: json['artifactUuid']?.toString(),
      sourceAppVersion: json['sourceAppVersion']?.toString(),
      platformTargets: (() {
        final list = _sdkworkAsList(json['platformTargets']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      packageSizeBytes: json['packageSizeBytes']?.toString(),
      checksumSha256: json['checksumSha256']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'version': version,
      'changelog': changelog,
      'artifactUuid': artifactUuid,
      'sourceAppVersion': sourceAppVersion,
      'platformTargets': platformTargets?.map((item) => item).toList(),
      'packageSizeBytes': packageSizeBytes,
      'checksumSha256': checksumSha256,
    };
  }
}

class ProblemDetail {
  final String type;
  final String title;
  final int status;
  final String? detail;
  final String? instance;
  final int code;
  final String traceId;
  final String? i18nKey;
  final String? locale;
  final List<FieldError>? errors;

  ProblemDetail({
    required this.type,
    required this.title,
    required this.status,
    this.detail,
    this.instance,
    required this.code,
    required this.traceId,
    this.i18nKey,
    this.locale,
    this.errors
  });

  factory ProblemDetail.fromJson(Map<String, dynamic> json) {
    return ProblemDetail(
      type: (() {
        final value = json['type']?.toString();
        if (value == null) {
          throw FormatException('ProblemDetail.type is required');
        }
        return value;
      })(),
      title: (() {
        final value = json['title']?.toString();
        if (value == null) {
          throw FormatException('ProblemDetail.title is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('ProblemDetail.status is required');
        }
        return value;
      })(),
      detail: json['detail']?.toString(),
      instance: json['instance']?.toString(),
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ProblemDetail.code is required');
        }
        return value;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ProblemDetail.traceId is required');
        }
        return value;
      })(),
      i18nKey: json['i18nKey']?.toString(),
      locale: json['locale']?.toString(),
      errors: (() {
        final list = _sdkworkAsList(json['errors']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : FieldError.fromJson(map);
      })())
            .whereType<FieldError>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'type': type,
      'title': title,
      'status': status,
      'detail': detail,
      'instance': instance,
      'code': code,
      'traceId': traceId,
      'i18nKey': i18nKey,
      'locale': locale,
      'errors': errors?.map((item) => item.toJson()).toList(),
    };
  }
}

class UpdateAppCompositionRequest {
  final String environment;
  final String defaultVariantKey;
  final List<AppResourceDefinition> resources;
  final List<AppVariantDefinition> variants;
  final List<AppVariantRuleDefinition>? variantRules;
  final List<AppMountDefinition> mounts;
  final List<AppBindingDefinition> bindings;
  final List<AppSourceSpecDefinition>? sourceSpecs;
  final AppDeliveryPolicy? deliveryPolicy;
  final AppSecurityPolicy? securityPolicy;
  final AppRuntimeLimits? limits;
  final AppObservabilityPolicy? observabilityPolicy;

  UpdateAppCompositionRequest({
    required this.environment,
    required this.defaultVariantKey,
    required this.resources,
    required this.variants,
    this.variantRules,
    required this.mounts,
    required this.bindings,
    this.sourceSpecs,
    this.deliveryPolicy,
    this.securityPolicy,
    this.limits,
    this.observabilityPolicy
  });

  factory UpdateAppCompositionRequest.fromJson(Map<String, dynamic> json) {
    return UpdateAppCompositionRequest(
      environment: (() {
        final value = json['environment']?.toString();
        if (value == null) {
          throw FormatException('UpdateAppCompositionRequest.environment is required');
        }
        return value;
      })(),
      defaultVariantKey: (() {
        final value = json['defaultVariantKey']?.toString();
        if (value == null) {
          throw FormatException('UpdateAppCompositionRequest.defaultVariantKey is required');
        }
        return value;
      })(),
      resources: (() {
        final list = _sdkworkAsList(json['resources']);
        if (list == null) {
          throw FormatException('UpdateAppCompositionRequest.resources is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppResourceDefinition.fromJson(map);
      })())
            .whereType<AppResourceDefinition>()
            .toList();
      })(),
      variants: (() {
        final list = _sdkworkAsList(json['variants']);
        if (list == null) {
          throw FormatException('UpdateAppCompositionRequest.variants is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppVariantDefinition.fromJson(map);
      })())
            .whereType<AppVariantDefinition>()
            .toList();
      })(),
      variantRules: (() {
        final list = _sdkworkAsList(json['variantRules']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppVariantRuleDefinition.fromJson(map);
      })())
            .whereType<AppVariantRuleDefinition>()
            .toList();
      })(),
      mounts: (() {
        final list = _sdkworkAsList(json['mounts']);
        if (list == null) {
          throw FormatException('UpdateAppCompositionRequest.mounts is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppMountDefinition.fromJson(map);
      })())
            .whereType<AppMountDefinition>()
            .toList();
      })(),
      bindings: (() {
        final list = _sdkworkAsList(json['bindings']);
        if (list == null) {
          throw FormatException('UpdateAppCompositionRequest.bindings is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppBindingDefinition.fromJson(map);
      })())
            .whereType<AppBindingDefinition>()
            .toList();
      })(),
      sourceSpecs: (() {
        final list = _sdkworkAsList(json['sourceSpecs']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppSourceSpecDefinition.fromJson(map);
      })())
            .whereType<AppSourceSpecDefinition>()
            .toList();
      })(),
      deliveryPolicy: (() {
        final map = _sdkworkAsMap(json['deliveryPolicy']);
        return map == null ? null : AppDeliveryPolicy.fromJson(map);
      })(),
      securityPolicy: (() {
        final map = _sdkworkAsMap(json['securityPolicy']);
        return map == null ? null : AppSecurityPolicy.fromJson(map);
      })(),
      limits: (() {
        final map = _sdkworkAsMap(json['limits']);
        return map == null ? null : AppRuntimeLimits.fromJson(map);
      })(),
      observabilityPolicy: (() {
        final map = _sdkworkAsMap(json['observabilityPolicy']);
        return map == null ? null : AppObservabilityPolicy.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'environment': environment,
      'defaultVariantKey': defaultVariantKey,
      'resources': resources.map((item) => item.toJson()).toList(),
      'variants': variants.map((item) => item.toJson()).toList(),
      'variantRules': variantRules?.map((item) => item.toJson()).toList(),
      'mounts': mounts.map((item) => item.toJson()).toList(),
      'bindings': bindings.map((item) => item.toJson()).toList(),
      'sourceSpecs': sourceSpecs?.map((item) => item.toJson()).toList(),
      'deliveryPolicy': deliveryPolicy?.toJson(),
      'securityPolicy': securityPolicy?.toJson(),
      'limits': limits?.toJson(),
      'observabilityPolicy': observabilityPolicy?.toJson(),
    };
  }
}

class AppResourceDefinition {
  final String key;
  final dynamic source;

  AppResourceDefinition({
    required this.key,
    required this.source
  });

  factory AppResourceDefinition.fromJson(Map<String, dynamic> json) {
    return AppResourceDefinition(
      key: (() {
        final value = json['key']?.toString();
        if (value == null) {
          throw FormatException('AppResourceDefinition.key is required');
        }
        return value;
      })(),
      source: (() {
        final map = _sdkworkAsMap(json['source']);
        if (map == null) {
          throw FormatException('AppResourceDefinition.source is required');
        }
        return DriveDirectorySource.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'key': key,
      'source': source.toJson(),
    };
  }
}

class DriveDirectorySource {
  final String type;
  final String websiteSpaceId;
  final dynamic root;
  final String contentMode;

  DriveDirectorySource({
    required this.type,
    required this.websiteSpaceId,
    required this.root,
    required this.contentMode
  });

  factory DriveDirectorySource.fromJson(Map<String, dynamic> json) {
    return DriveDirectorySource(
      type: (() {
        final value = json['type']?.toString();
        if (value == null) {
          throw FormatException('DriveDirectorySource.type is required');
        }
        return value;
      })(),
      websiteSpaceId: (() {
        final value = json['websiteSpaceId']?.toString();
        if (value == null) {
          throw FormatException('DriveDirectorySource.websiteSpaceId is required');
        }
        return value;
      })(),
      root: (() {
        final map = _sdkworkAsMap(json['root']);
        if (map == null) {
          throw FormatException('DriveDirectorySource.root is required');
        }
        return DriveSpaceRootSelector.fromJson(map);
      })(),
      contentMode: (() {
        final value = json['contentMode']?.toString();
        if (value == null) {
          throw FormatException('DriveDirectorySource.contentMode is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'type': type,
      'websiteSpaceId': websiteSpaceId,
      'root': root.toJson(),
      'contentMode': contentMode,
    };
  }
}

class DriveSpaceRootSelector {
  final String mode;

  DriveSpaceRootSelector({
    required this.mode
  });

  factory DriveSpaceRootSelector.fromJson(Map<String, dynamic> json) {
    return DriveSpaceRootSelector(
      mode: (() {
        final value = json['mode']?.toString();
        if (value == null) {
          throw FormatException('DriveSpaceRootSelector.mode is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'mode': mode,
    };
  }
}

class DriveFolderSelector {
  final String mode;
  final String folderNodeId;

  DriveFolderSelector({
    required this.mode,
    required this.folderNodeId
  });

  factory DriveFolderSelector.fromJson(Map<String, dynamic> json) {
    return DriveFolderSelector(
      mode: (() {
        final value = json['mode']?.toString();
        if (value == null) {
          throw FormatException('DriveFolderSelector.mode is required');
        }
        return value;
      })(),
      folderNodeId: (() {
        final value = json['folderNodeId']?.toString();
        if (value == null) {
          throw FormatException('DriveFolderSelector.folderNodeId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'mode': mode,
      'folderNodeId': folderNodeId,
    };
  }
}

class KnowledgebaseWikiSource {
  final String type;
  final String publicationUuid;

  KnowledgebaseWikiSource({
    required this.type,
    required this.publicationUuid
  });

  factory KnowledgebaseWikiSource.fromJson(Map<String, dynamic> json) {
    return KnowledgebaseWikiSource(
      type: (() {
        final value = json['type']?.toString();
        if (value == null) {
          throw FormatException('KnowledgebaseWikiSource.type is required');
        }
        return value;
      })(),
      publicationUuid: (() {
        final value = json['publicationUuid']?.toString();
        if (value == null) {
          throw FormatException('KnowledgebaseWikiSource.publicationUuid is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'type': type,
      'publicationUuid': publicationUuid,
    };
  }
}

class AppVariantDefinition {
  final String key;
  final String label;
  final String? clientClass;
  final int? priority;

  AppVariantDefinition({
    required this.key,
    required this.label,
    this.clientClass,
    this.priority
  });

  factory AppVariantDefinition.fromJson(Map<String, dynamic> json) {
    return AppVariantDefinition(
      key: (() {
        final value = json['key']?.toString();
        if (value == null) {
          throw FormatException('AppVariantDefinition.key is required');
        }
        return value;
      })(),
      label: (() {
        final value = json['label']?.toString();
        if (value == null) {
          throw FormatException('AppVariantDefinition.label is required');
        }
        return value;
      })(),
      clientClass: json['clientClass']?.toString(),
      priority: json['priority'] is int ? json['priority'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'key': key,
      'label': label,
      'clientClass': clientClass,
      'priority': priority,
    };
  }
}

class AppVariantRuleDefinition {
  final String key;
  final String targetVariantKey;
  final int priority;
  final dynamic match;

  AppVariantRuleDefinition({
    required this.key,
    required this.targetVariantKey,
    required this.priority,
    required this.match
  });

  factory AppVariantRuleDefinition.fromJson(Map<String, dynamic> json) {
    return AppVariantRuleDefinition(
      key: (() {
        final value = json['key']?.toString();
        if (value == null) {
          throw FormatException('AppVariantRuleDefinition.key is required');
        }
        return value;
      })(),
      targetVariantKey: (() {
        final value = json['targetVariantKey']?.toString();
        if (value == null) {
          throw FormatException('AppVariantRuleDefinition.targetVariantKey is required');
        }
        return value;
      })(),
      priority: (() {
        final value = json['priority'];
        if (value is! int) {
          throw FormatException('AppVariantRuleDefinition.priority is required');
        }
        return value;
      })(),
      match: (() {
        final map = _sdkworkAsMap(json['match']);
        if (map == null) {
          throw FormatException('AppVariantRuleDefinition.match is required');
        }
        return PathPrefixVariantMatch.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'key': key,
      'targetVariantKey': targetVariantKey,
      'priority': priority,
      'match': match.toJson(),
    };
  }
}

class PathPrefixVariantMatch {
  final String type;
  final String pathPrefix;

  PathPrefixVariantMatch({
    required this.type,
    required this.pathPrefix
  });

  factory PathPrefixVariantMatch.fromJson(Map<String, dynamic> json) {
    return PathPrefixVariantMatch(
      type: (() {
        final value = json['type']?.toString();
        if (value == null) {
          throw FormatException('PathPrefixVariantMatch.type is required');
        }
        return value;
      })(),
      pathPrefix: (() {
        final value = json['pathPrefix']?.toString();
        if (value == null) {
          throw FormatException('PathPrefixVariantMatch.pathPrefix is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'type': type,
      'pathPrefix': pathPrefix,
    };
  }
}

class ClientClassVariantMatch {
  final String type;
  final String clientClass;

  ClientClassVariantMatch({
    required this.type,
    required this.clientClass
  });

  factory ClientClassVariantMatch.fromJson(Map<String, dynamic> json) {
    return ClientClassVariantMatch(
      type: (() {
        final value = json['type']?.toString();
        if (value == null) {
          throw FormatException('ClientClassVariantMatch.type is required');
        }
        return value;
      })(),
      clientClass: (() {
        final value = json['clientClass']?.toString();
        if (value == null) {
          throw FormatException('ClientClassVariantMatch.clientClass is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'type': type,
      'clientClass': clientClass,
    };
  }
}

class AppMountDefinition {
  final String key;
  final String variantKey;
  final String resourceKey;
  final String pathPrefix;
  final String resourceSubpath;
  final String mode;
  final String handler;
  final List<String>? indexFiles;
  final String? spaFallback;
  final int? priority;

  AppMountDefinition({
    required this.key,
    required this.variantKey,
    required this.resourceKey,
    required this.pathPrefix,
    required this.resourceSubpath,
    required this.mode,
    required this.handler,
    this.indexFiles,
    this.spaFallback,
    this.priority
  });

  factory AppMountDefinition.fromJson(Map<String, dynamic> json) {
    return AppMountDefinition(
      key: (() {
        final value = json['key']?.toString();
        if (value == null) {
          throw FormatException('AppMountDefinition.key is required');
        }
        return value;
      })(),
      variantKey: (() {
        final value = json['variantKey']?.toString();
        if (value == null) {
          throw FormatException('AppMountDefinition.variantKey is required');
        }
        return value;
      })(),
      resourceKey: (() {
        final value = json['resourceKey']?.toString();
        if (value == null) {
          throw FormatException('AppMountDefinition.resourceKey is required');
        }
        return value;
      })(),
      pathPrefix: (() {
        final value = json['pathPrefix']?.toString();
        if (value == null) {
          throw FormatException('AppMountDefinition.pathPrefix is required');
        }
        return value;
      })(),
      resourceSubpath: (() {
        final value = json['resourceSubpath']?.toString();
        if (value == null) {
          throw FormatException('AppMountDefinition.resourceSubpath is required');
        }
        return value;
      })(),
      mode: (() {
        final value = json['mode']?.toString();
        if (value == null) {
          throw FormatException('AppMountDefinition.mode is required');
        }
        return value;
      })(),
      handler: (() {
        final value = json['handler']?.toString();
        if (value == null) {
          throw FormatException('AppMountDefinition.handler is required');
        }
        return value;
      })(),
      indexFiles: (() {
        final list = _sdkworkAsList(json['indexFiles']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      spaFallback: json['spaFallback']?.toString(),
      priority: json['priority'] is int ? json['priority'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'key': key,
      'variantKey': variantKey,
      'resourceKey': resourceKey,
      'pathPrefix': pathPrefix,
      'resourceSubpath': resourceSubpath,
      'mode': mode,
      'handler': handler,
      'indexFiles': indexFiles?.map((item) => item).toList(),
      'spaFallback': spaFallback,
      'priority': priority,
    };
  }
}

class AppBindingDefinition {
  final String key;
  final String domainId;
  final String? pathPrefix;
  final dynamic action;

  AppBindingDefinition({
    required this.key,
    required this.domainId,
    this.pathPrefix,
    required this.action
  });

  factory AppBindingDefinition.fromJson(Map<String, dynamic> json) {
    return AppBindingDefinition(
      key: (() {
        final value = json['key']?.toString();
        if (value == null) {
          throw FormatException('AppBindingDefinition.key is required');
        }
        return value;
      })(),
      domainId: (() {
        final value = json['domainId']?.toString();
        if (value == null) {
          throw FormatException('AppBindingDefinition.domainId is required');
        }
        return value;
      })(),
      pathPrefix: json['pathPrefix']?.toString(),
      action: (() {
        final map = _sdkworkAsMap(json['action']);
        if (map == null) {
          throw FormatException('AppBindingDefinition.action is required');
        }
        return ServeBindingAction.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'key': key,
      'domainId': domainId,
      'pathPrefix': pathPrefix,
      'action': action.toJson(),
    };
  }
}

class ServeBindingAction {
  final String type;
  final String? defaultVariantKey;
  final String? forcedVariantKey;

  ServeBindingAction({
    required this.type,
    this.defaultVariantKey,
    this.forcedVariantKey
  });

  factory ServeBindingAction.fromJson(Map<String, dynamic> json) {
    return ServeBindingAction(
      type: (() {
        final value = json['type']?.toString();
        if (value == null) {
          throw FormatException('ServeBindingAction.type is required');
        }
        return value;
      })(),
      defaultVariantKey: json['defaultVariantKey']?.toString(),
      forcedVariantKey: json['forcedVariantKey']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'type': type,
      'defaultVariantKey': defaultVariantKey,
      'forcedVariantKey': forcedVariantKey,
    };
  }
}

class RedirectBindingAction {
  final String type;
  final int statusCode;
  final String scheme;
  final String hostname;
  final String pathPrefix;
  final bool? preservePath;
  final bool? preserveQuery;

  RedirectBindingAction({
    required this.type,
    required this.statusCode,
    required this.scheme,
    required this.hostname,
    required this.pathPrefix,
    this.preservePath,
    this.preserveQuery
  });

  factory RedirectBindingAction.fromJson(Map<String, dynamic> json) {
    return RedirectBindingAction(
      type: (() {
        final value = json['type']?.toString();
        if (value == null) {
          throw FormatException('RedirectBindingAction.type is required');
        }
        return value;
      })(),
      statusCode: (() {
        final value = json['statusCode'];
        if (value is! int) {
          throw FormatException('RedirectBindingAction.statusCode is required');
        }
        return value;
      })(),
      scheme: (() {
        final value = json['scheme']?.toString();
        if (value == null) {
          throw FormatException('RedirectBindingAction.scheme is required');
        }
        return value;
      })(),
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('RedirectBindingAction.hostname is required');
        }
        return value;
      })(),
      pathPrefix: (() {
        final value = json['pathPrefix']?.toString();
        if (value == null) {
          throw FormatException('RedirectBindingAction.pathPrefix is required');
        }
        return value;
      })(),
      preservePath: json['preservePath'] is bool ? json['preservePath'] : null,
      preserveQuery: json['preserveQuery'] is bool ? json['preserveQuery'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'type': type,
      'statusCode': statusCode,
      'scheme': scheme,
      'hostname': hostname,
      'pathPrefix': pathPrefix,
      'preservePath': preservePath,
      'preserveQuery': preserveQuery,
    };
  }
}

class AppDeliveryPolicy {
  final int? providerTimeoutMs;
  final int? metadataCacheTtlSeconds;
  final int? negativeCacheTtlSeconds;
  final int? staleWhileRevalidateSeconds;
  final int? maximumObjectBytes;

  AppDeliveryPolicy({
    this.providerTimeoutMs,
    this.metadataCacheTtlSeconds,
    this.negativeCacheTtlSeconds,
    this.staleWhileRevalidateSeconds,
    this.maximumObjectBytes
  });

  factory AppDeliveryPolicy.fromJson(Map<String, dynamic> json) {
    return AppDeliveryPolicy(
      providerTimeoutMs: json['providerTimeoutMs'] is int ? json['providerTimeoutMs'] : null,
      metadataCacheTtlSeconds: json['metadataCacheTtlSeconds'] is int ? json['metadataCacheTtlSeconds'] : null,
      negativeCacheTtlSeconds: json['negativeCacheTtlSeconds'] is int ? json['negativeCacheTtlSeconds'] : null,
      staleWhileRevalidateSeconds: json['staleWhileRevalidateSeconds'] is int ? json['staleWhileRevalidateSeconds'] : null,
      maximumObjectBytes: json['maximumObjectBytes'] is int ? json['maximumObjectBytes'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'providerTimeoutMs': providerTimeoutMs,
      'metadataCacheTtlSeconds': metadataCacheTtlSeconds,
      'negativeCacheTtlSeconds': negativeCacheTtlSeconds,
      'staleWhileRevalidateSeconds': staleWhileRevalidateSeconds,
      'maximumObjectBytes': maximumObjectBytes,
    };
  }
}

class AppSecurityPolicy {
  final bool? forceHttps;
  final bool? denyDotFiles;
  final List<String>? deniedPathPrefixes;

  AppSecurityPolicy({
    this.forceHttps,
    this.denyDotFiles,
    this.deniedPathPrefixes
  });

  factory AppSecurityPolicy.fromJson(Map<String, dynamic> json) {
    return AppSecurityPolicy(
      forceHttps: json['forceHttps'] is bool ? json['forceHttps'] : null,
      denyDotFiles: json['denyDotFiles'] is bool ? json['denyDotFiles'] : null,
      deniedPathPrefixes: (() {
        final list = _sdkworkAsList(json['deniedPathPrefixes']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'forceHttps': forceHttps,
      'denyDotFiles': denyDotFiles,
      'deniedPathPrefixes': deniedPathPrefixes?.map((item) => item).toList(),
    };
  }
}

class AppRuntimeLimits {
  final int? maximumBindings;
  final int? maximumVariants;
  final int? maximumVariantRules;
  final int? maximumResources;
  final int? maximumMounts;
  final int? maximumIndexFilesPerMount;
  final int? maximumPathBytes;
  final int? maximumPathSegments;

  AppRuntimeLimits({
    this.maximumBindings,
    this.maximumVariants,
    this.maximumVariantRules,
    this.maximumResources,
    this.maximumMounts,
    this.maximumIndexFilesPerMount,
    this.maximumPathBytes,
    this.maximumPathSegments
  });

  factory AppRuntimeLimits.fromJson(Map<String, dynamic> json) {
    return AppRuntimeLimits(
      maximumBindings: json['maximumBindings'] is int ? json['maximumBindings'] : null,
      maximumVariants: json['maximumVariants'] is int ? json['maximumVariants'] : null,
      maximumVariantRules: json['maximumVariantRules'] is int ? json['maximumVariantRules'] : null,
      maximumResources: json['maximumResources'] is int ? json['maximumResources'] : null,
      maximumMounts: json['maximumMounts'] is int ? json['maximumMounts'] : null,
      maximumIndexFilesPerMount: json['maximumIndexFilesPerMount'] is int ? json['maximumIndexFilesPerMount'] : null,
      maximumPathBytes: json['maximumPathBytes'] is int ? json['maximumPathBytes'] : null,
      maximumPathSegments: json['maximumPathSegments'] is int ? json['maximumPathSegments'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'maximumBindings': maximumBindings,
      'maximumVariants': maximumVariants,
      'maximumVariantRules': maximumVariantRules,
      'maximumResources': maximumResources,
      'maximumMounts': maximumMounts,
      'maximumIndexFilesPerMount': maximumIndexFilesPerMount,
      'maximumPathBytes': maximumPathBytes,
      'maximumPathSegments': maximumPathSegments,
    };
  }
}

class AppObservabilityPolicy {
  final bool? accessLogEnabled;
  final bool? usageMeteringEnabled;
  final int? traceSampleRatePerMille;

  AppObservabilityPolicy({
    this.accessLogEnabled,
    this.usageMeteringEnabled,
    this.traceSampleRatePerMille
  });

  factory AppObservabilityPolicy.fromJson(Map<String, dynamic> json) {
    return AppObservabilityPolicy(
      accessLogEnabled: json['accessLogEnabled'] is bool ? json['accessLogEnabled'] : null,
      usageMeteringEnabled: json['usageMeteringEnabled'] is bool ? json['usageMeteringEnabled'] : null,
      traceSampleRatePerMille: json['traceSampleRatePerMille'] is int ? json['traceSampleRatePerMille'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'accessLogEnabled': accessLogEnabled,
      'usageMeteringEnabled': usageMeteringEnabled,
      'traceSampleRatePerMille': traceSampleRatePerMille,
    };
  }
}

class AppSourceSpecRoute {
  final String clientClass;
  final int preference;

  AppSourceSpecRoute({
    required this.clientClass,
    required this.preference
  });

  factory AppSourceSpecRoute.fromJson(Map<String, dynamic> json) {
    return AppSourceSpecRoute(
      clientClass: (() {
        final value = json['clientClass']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecRoute.clientClass is required');
        }
        return value;
      })(),
      preference: (() {
        final value = json['preference'];
        if (value is! int) {
          throw FormatException('AppSourceSpecRoute.preference is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'clientClass': clientClass,
      'preference': preference,
    };
  }
}

class AppSourceBindingResponse {
  final String providerType;
  final String providerResourceUuid;
  final String contractVersion;
  final String status;
  final String? updatedAt;

  AppSourceBindingResponse({
    required this.providerType,
    required this.providerResourceUuid,
    required this.contractVersion,
    required this.status,
    this.updatedAt
  });

  factory AppSourceBindingResponse.fromJson(Map<String, dynamic> json) {
    return AppSourceBindingResponse(
      providerType: (() {
        final value = json['providerType']?.toString();
        if (value == null) {
          throw FormatException('AppSourceBindingResponse.providerType is required');
        }
        return value;
      })(),
      providerResourceUuid: (() {
        final value = json['providerResourceUuid']?.toString();
        if (value == null) {
          throw FormatException('AppSourceBindingResponse.providerResourceUuid is required');
        }
        return value;
      })(),
      contractVersion: (() {
        final value = json['contractVersion']?.toString();
        if (value == null) {
          throw FormatException('AppSourceBindingResponse.contractVersion is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('AppSourceBindingResponse.status is required');
        }
        return value;
      })(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'providerType': providerType,
      'providerResourceUuid': providerResourceUuid,
      'contractVersion': contractVersion,
      'status': status,
      'updatedAt': updatedAt,
    };
  }
}

class AppSourceSpecResponse {
  final String id;
  final String specKey;
  final String label;
  final String runtimeTarget;
  final String clientArchitecture;
  final List<AppSourceSpecRoute> clientClassRoutes;
  final String pathPrefix;
  final String handler;
  final List<String> indexFiles;
  final String? spaFallback;
  final bool isDefault;
  final int priority;
  final String status;
  final String sourceStatus;
  final AppSourceBindingResponse? source;
  final String createdAt;
  final String updatedAt;
  final String version;

  AppSourceSpecResponse({
    required this.id,
    required this.specKey,
    required this.label,
    required this.runtimeTarget,
    required this.clientArchitecture,
    required this.clientClassRoutes,
    required this.pathPrefix,
    required this.handler,
    required this.indexFiles,
    this.spaFallback,
    required this.isDefault,
    required this.priority,
    required this.status,
    required this.sourceStatus,
    this.source,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory AppSourceSpecResponse.fromJson(Map<String, dynamic> json) {
    return AppSourceSpecResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.id is required');
        }
        return value;
      })(),
      specKey: (() {
        final value = json['specKey']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.specKey is required');
        }
        return value;
      })(),
      label: (() {
        final value = json['label']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.label is required');
        }
        return value;
      })(),
      runtimeTarget: (() {
        final value = json['runtimeTarget']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.runtimeTarget is required');
        }
        return value;
      })(),
      clientArchitecture: (() {
        final value = json['clientArchitecture']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.clientArchitecture is required');
        }
        return value;
      })(),
      clientClassRoutes: (() {
        final list = _sdkworkAsList(json['clientClassRoutes']);
        if (list == null) {
          throw FormatException('AppSourceSpecResponse.clientClassRoutes is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppSourceSpecRoute.fromJson(map);
      })())
            .whereType<AppSourceSpecRoute>()
            .toList();
      })(),
      pathPrefix: (() {
        final value = json['pathPrefix']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.pathPrefix is required');
        }
        return value;
      })(),
      handler: (() {
        final value = json['handler']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.handler is required');
        }
        return value;
      })(),
      indexFiles: (() {
        final list = _sdkworkAsList(json['indexFiles']);
        if (list == null) {
          throw FormatException('AppSourceSpecResponse.indexFiles is required');
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      spaFallback: json['spaFallback']?.toString(),
      isDefault: (() {
        final value = json['isDefault'];
        if (value is! bool) {
          throw FormatException('AppSourceSpecResponse.isDefault is required');
        }
        return value;
      })(),
      priority: (() {
        final value = json['priority'];
        if (value is! int) {
          throw FormatException('AppSourceSpecResponse.priority is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.status is required');
        }
        return value;
      })(),
      sourceStatus: (() {
        final value = json['sourceStatus']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.sourceStatus is required');
        }
        return value;
      })(),
      source: (() {
        final map = _sdkworkAsMap(json['source']);
        return map == null ? null : AppSourceBindingResponse.fromJson(map);
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'specKey': specKey,
      'label': label,
      'runtimeTarget': runtimeTarget,
      'clientArchitecture': clientArchitecture,
      'clientClassRoutes': clientClassRoutes.map((item) => item.toJson()).toList(),
      'pathPrefix': pathPrefix,
      'handler': handler,
      'indexFiles': indexFiles.map((item) => item).toList(),
      'spaFallback': spaFallback,
      'isDefault': isDefault,
      'priority': priority,
      'status': status,
      'sourceStatus': sourceStatus,
      'source': source?.toJson(),
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class AppSourceSpecDefinition {
  final String specKey;
  final String label;
  final String? runtimeTarget;
  final String? clientArchitecture;
  final List<AppSourceSpecRoute>? clientClassRoutes;
  final String? pathPrefix;
  final String? handler;
  final List<String>? indexFiles;
  final String? spaFallback;
  final bool? isDefault;
  final int? priority;

  AppSourceSpecDefinition({
    required this.specKey,
    required this.label,
    this.runtimeTarget,
    this.clientArchitecture,
    this.clientClassRoutes,
    this.pathPrefix,
    this.handler,
    this.indexFiles,
    this.spaFallback,
    this.isDefault,
    this.priority
  });

  factory AppSourceSpecDefinition.fromJson(Map<String, dynamic> json) {
    return AppSourceSpecDefinition(
      specKey: (() {
        final value = json['specKey']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecDefinition.specKey is required');
        }
        return value;
      })(),
      label: (() {
        final value = json['label']?.toString();
        if (value == null) {
          throw FormatException('AppSourceSpecDefinition.label is required');
        }
        return value;
      })(),
      runtimeTarget: json['runtimeTarget']?.toString(),
      clientArchitecture: json['clientArchitecture']?.toString(),
      clientClassRoutes: (() {
        final list = _sdkworkAsList(json['clientClassRoutes']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppSourceSpecRoute.fromJson(map);
      })())
            .whereType<AppSourceSpecRoute>()
            .toList();
      })(),
      pathPrefix: json['pathPrefix']?.toString(),
      handler: json['handler']?.toString(),
      indexFiles: (() {
        final list = _sdkworkAsList(json['indexFiles']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      spaFallback: json['spaFallback']?.toString(),
      isDefault: json['isDefault'] is bool ? json['isDefault'] : null,
      priority: json['priority'] is int ? json['priority'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'specKey': specKey,
      'label': label,
      'runtimeTarget': runtimeTarget,
      'clientArchitecture': clientArchitecture,
      'clientClassRoutes': clientClassRoutes?.map((item) => item.toJson()).toList(),
      'pathPrefix': pathPrefix,
      'handler': handler,
      'indexFiles': indexFiles?.map((item) => item).toList(),
      'spaFallback': spaFallback,
      'isDefault': isDefault,
      'priority': priority,
    };
  }
}

class CreateAppSourceSpecRequest {
  final String environment;
  final String specKey;
  final String label;
  final String? runtimeTarget;
  final String? clientArchitecture;
  final List<AppSourceSpecRoute>? clientClassRoutes;
  final String? pathPrefix;
  final String? handler;
  final List<String>? indexFiles;
  final String? spaFallback;
  final bool? isDefault;
  final int? priority;

  CreateAppSourceSpecRequest({
    required this.environment,
    required this.specKey,
    required this.label,
    this.runtimeTarget,
    this.clientArchitecture,
    this.clientClassRoutes,
    this.pathPrefix,
    this.handler,
    this.indexFiles,
    this.spaFallback,
    this.isDefault,
    this.priority
  });

  factory CreateAppSourceSpecRequest.fromJson(Map<String, dynamic> json) {
    return CreateAppSourceSpecRequest(
      environment: (() {
        final value = json['environment']?.toString();
        if (value == null) {
          throw FormatException('CreateAppSourceSpecRequest.environment is required');
        }
        return value;
      })(),
      specKey: (() {
        final value = json['specKey']?.toString();
        if (value == null) {
          throw FormatException('CreateAppSourceSpecRequest.specKey is required');
        }
        return value;
      })(),
      label: (() {
        final value = json['label']?.toString();
        if (value == null) {
          throw FormatException('CreateAppSourceSpecRequest.label is required');
        }
        return value;
      })(),
      runtimeTarget: json['runtimeTarget']?.toString(),
      clientArchitecture: json['clientArchitecture']?.toString(),
      clientClassRoutes: (() {
        final list = _sdkworkAsList(json['clientClassRoutes']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppSourceSpecRoute.fromJson(map);
      })())
            .whereType<AppSourceSpecRoute>()
            .toList();
      })(),
      pathPrefix: json['pathPrefix']?.toString(),
      handler: json['handler']?.toString(),
      indexFiles: (() {
        final list = _sdkworkAsList(json['indexFiles']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      spaFallback: json['spaFallback']?.toString(),
      isDefault: json['isDefault'] is bool ? json['isDefault'] : null,
      priority: json['priority'] is int ? json['priority'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'environment': environment,
      'specKey': specKey,
      'label': label,
      'runtimeTarget': runtimeTarget,
      'clientArchitecture': clientArchitecture,
      'clientClassRoutes': clientClassRoutes?.map((item) => item.toJson()).toList(),
      'pathPrefix': pathPrefix,
      'handler': handler,
      'indexFiles': indexFiles?.map((item) => item).toList(),
      'spaFallback': spaFallback,
      'isDefault': isDefault,
      'priority': priority,
    };
  }
}

class UpdateAppSourceSpecRequest {
  final String? label;
  final String? runtimeTarget;
  final String? clientArchitecture;
  final List<AppSourceSpecRoute>? clientClassRoutes;
  final String? pathPrefix;
  final String? handler;
  final List<String>? indexFiles;
  final String? spaFallback;
  final bool? isDefault;
  final int? priority;
  final String? status;

  UpdateAppSourceSpecRequest({
    this.label,
    this.runtimeTarget,
    this.clientArchitecture,
    this.clientClassRoutes,
    this.pathPrefix,
    this.handler,
    this.indexFiles,
    this.spaFallback,
    this.isDefault,
    this.priority,
    this.status
  });

  factory UpdateAppSourceSpecRequest.fromJson(Map<String, dynamic> json) {
    return UpdateAppSourceSpecRequest(
      label: json['label']?.toString(),
      runtimeTarget: json['runtimeTarget']?.toString(),
      clientArchitecture: json['clientArchitecture']?.toString(),
      clientClassRoutes: (() {
        final list = _sdkworkAsList(json['clientClassRoutes']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppSourceSpecRoute.fromJson(map);
      })())
            .whereType<AppSourceSpecRoute>()
            .toList();
      })(),
      pathPrefix: json['pathPrefix']?.toString(),
      handler: json['handler']?.toString(),
      indexFiles: (() {
        final list = _sdkworkAsList(json['indexFiles']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      spaFallback: json['spaFallback']?.toString(),
      isDefault: json['isDefault'] is bool ? json['isDefault'] : null,
      priority: json['priority'] is int ? json['priority'] : null,
      status: json['status']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'label': label,
      'runtimeTarget': runtimeTarget,
      'clientArchitecture': clientArchitecture,
      'clientClassRoutes': clientClassRoutes?.map((item) => item.toJson()).toList(),
      'pathPrefix': pathPrefix,
      'handler': handler,
      'indexFiles': indexFiles?.map((item) => item).toList(),
      'spaFallback': spaFallback,
      'isDefault': isDefault,
      'priority': priority,
      'status': status,
    };
  }
}

class BindAppSourceSpecSourceRequest {
  final dynamic source;

  BindAppSourceSpecSourceRequest({
    required this.source
  });

  factory BindAppSourceSpecSourceRequest.fromJson(Map<String, dynamic> json) {
    return BindAppSourceSpecSourceRequest(
      source: (() {
        final map = _sdkworkAsMap(json['source']);
        if (map == null) {
          throw FormatException('BindAppSourceSpecSourceRequest.source is required');
        }
        return DriveDirectorySource.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'source': source.toJson(),
    };
  }
}

class AppCompositionResponse {
  final String siteId;
  final String siteVersion;
  final AppRevisionResponse revision;
  final List<AppRuntimeAssignmentResponse> runtimeAssignments;

  AppCompositionResponse({
    required this.siteId,
    required this.siteVersion,
    required this.revision,
    required this.runtimeAssignments
  });

  factory AppCompositionResponse.fromJson(Map<String, dynamic> json) {
    return AppCompositionResponse(
      siteId: (() {
        final value = json['siteId']?.toString();
        if (value == null) {
          throw FormatException('AppCompositionResponse.siteId is required');
        }
        return value;
      })(),
      siteVersion: (() {
        final value = json['siteVersion']?.toString();
        if (value == null) {
          throw FormatException('AppCompositionResponse.siteVersion is required');
        }
        return value;
      })(),
      revision: (() {
        final map = _sdkworkAsMap(json['revision']);
        if (map == null) {
          throw FormatException('AppCompositionResponse.revision is required');
        }
        return AppRevisionResponse.fromJson(map);
      })(),
      runtimeAssignments: (() {
        final list = _sdkworkAsList(json['runtimeAssignments']);
        if (list == null) {
          throw FormatException('AppCompositionResponse.runtimeAssignments is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppRuntimeAssignmentResponse.fromJson(map);
      })())
            .whereType<AppRuntimeAssignmentResponse>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'siteId': siteId,
      'siteVersion': siteVersion,
      'revision': revision.toJson(),
      'runtimeAssignments': runtimeAssignments.map((item) => item.toJson()).toList(),
    };
  }
}

class AppRevisionResponse {
  final String id;
  final String number;
  final String descriptorSha256;
  final String validationStatus;

  AppRevisionResponse({
    required this.id,
    required this.number,
    required this.descriptorSha256,
    required this.validationStatus
  });

  factory AppRevisionResponse.fromJson(Map<String, dynamic> json) {
    return AppRevisionResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppRevisionResponse.id is required');
        }
        return value;
      })(),
      number: (() {
        final value = json['number']?.toString();
        if (value == null) {
          throw FormatException('AppRevisionResponse.number is required');
        }
        return value;
      })(),
      descriptorSha256: (() {
        final value = json['descriptorSha256']?.toString();
        if (value == null) {
          throw FormatException('AppRevisionResponse.descriptorSha256 is required');
        }
        return value;
      })(),
      validationStatus: (() {
        final value = json['validationStatus']?.toString();
        if (value == null) {
          throw FormatException('AppRevisionResponse.validationStatus is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'number': number,
      'descriptorSha256': descriptorSha256,
      'validationStatus': validationStatus,
    };
  }
}

class AppRuntimeAssignmentResponse {
  final String targetId;
  final String assignmentId;
  final String generation;
  final String status;

  AppRuntimeAssignmentResponse({
    required this.targetId,
    required this.assignmentId,
    required this.generation,
    required this.status
  });

  factory AppRuntimeAssignmentResponse.fromJson(Map<String, dynamic> json) {
    return AppRuntimeAssignmentResponse(
      targetId: (() {
        final value = json['targetId']?.toString();
        if (value == null) {
          throw FormatException('AppRuntimeAssignmentResponse.targetId is required');
        }
        return value;
      })(),
      assignmentId: (() {
        final value = json['assignmentId']?.toString();
        if (value == null) {
          throw FormatException('AppRuntimeAssignmentResponse.assignmentId is required');
        }
        return value;
      })(),
      generation: (() {
        final value = json['generation']?.toString();
        if (value == null) {
          throw FormatException('AppRuntimeAssignmentResponse.generation is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('AppRuntimeAssignmentResponse.status is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'targetId': targetId,
      'assignmentId': assignmentId,
      'generation': generation,
      'status': status,
    };
  }
}

class CreateDomainZoneRequest {
  final String apexHostname;
  final String? displayName;
  final String? dnsProvider;
  final String? providerZoneRef;
  final String? providerAccountId;

  CreateDomainZoneRequest({
    required this.apexHostname,
    this.displayName,
    this.dnsProvider,
    this.providerZoneRef,
    this.providerAccountId
  });

  factory CreateDomainZoneRequest.fromJson(Map<String, dynamic> json) {
    return CreateDomainZoneRequest(
      apexHostname: (() {
        final value = json['apexHostname']?.toString();
        if (value == null) {
          throw FormatException('CreateDomainZoneRequest.apexHostname is required');
        }
        return value;
      })(),
      displayName: json['displayName']?.toString(),
      dnsProvider: json['dnsProvider']?.toString(),
      providerZoneRef: json['providerZoneRef']?.toString(),
      providerAccountId: json['providerAccountId']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'apexHostname': apexHostname,
      'displayName': displayName,
      'dnsProvider': dnsProvider,
      'providerZoneRef': providerZoneRef,
      'providerAccountId': providerAccountId,
    };
  }
}

class UpdateDomainZoneRequest {
  final String? displayName;
  final String? dnsProvider;
  final String? providerZoneRef;
  final String? providerAccountId;
  final String? status;

  UpdateDomainZoneRequest({
    this.displayName,
    this.dnsProvider,
    this.providerZoneRef,
    this.providerAccountId,
    this.status
  });

  factory UpdateDomainZoneRequest.fromJson(Map<String, dynamic> json) {
    return UpdateDomainZoneRequest(
      displayName: json['displayName']?.toString(),
      dnsProvider: json['dnsProvider']?.toString(),
      providerZoneRef: json['providerZoneRef']?.toString(),
      providerAccountId: json['providerAccountId']?.toString(),
      status: json['status']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'displayName': displayName,
      'dnsProvider': dnsProvider,
      'providerZoneRef': providerZoneRef,
      'providerAccountId': providerAccountId,
      'status': status,
    };
  }
}

class DomainZoneResponse {
  final String id;
  final String apexHostname;
  final String scope;
  final String? displayName;
  final String? dnsProvider;
  final String? providerAccountId;
  final String status;
  final String hostnameCount;
  final String verifiedHostnameCount;
  final String certificateCount;
  final String bindingCount;
  final String updatedAt;
  final String version;

  DomainZoneResponse({
    required this.id,
    required this.apexHostname,
    required this.scope,
    this.displayName,
    this.dnsProvider,
    this.providerAccountId,
    required this.status,
    required this.hostnameCount,
    required this.verifiedHostnameCount,
    required this.certificateCount,
    required this.bindingCount,
    required this.updatedAt,
    required this.version
  });

  factory DomainZoneResponse.fromJson(Map<String, dynamic> json) {
    return DomainZoneResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('DomainZoneResponse.id is required');
        }
        return value;
      })(),
      apexHostname: (() {
        final value = json['apexHostname']?.toString();
        if (value == null) {
          throw FormatException('DomainZoneResponse.apexHostname is required');
        }
        return value;
      })(),
      scope: (() {
        final value = json['scope']?.toString();
        if (value == null) {
          throw FormatException('DomainZoneResponse.scope is required');
        }
        return value;
      })(),
      displayName: json['displayName']?.toString(),
      dnsProvider: json['dnsProvider']?.toString(),
      providerAccountId: json['providerAccountId']?.toString(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('DomainZoneResponse.status is required');
        }
        return value;
      })(),
      hostnameCount: (() {
        final value = json['hostnameCount']?.toString();
        if (value == null) {
          throw FormatException('DomainZoneResponse.hostnameCount is required');
        }
        return value;
      })(),
      verifiedHostnameCount: (() {
        final value = json['verifiedHostnameCount']?.toString();
        if (value == null) {
          throw FormatException('DomainZoneResponse.verifiedHostnameCount is required');
        }
        return value;
      })(),
      certificateCount: (() {
        final value = json['certificateCount']?.toString();
        if (value == null) {
          throw FormatException('DomainZoneResponse.certificateCount is required');
        }
        return value;
      })(),
      bindingCount: (() {
        final value = json['bindingCount']?.toString();
        if (value == null) {
          throw FormatException('DomainZoneResponse.bindingCount is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('DomainZoneResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('DomainZoneResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'apexHostname': apexHostname,
      'scope': scope,
      'displayName': displayName,
      'dnsProvider': dnsProvider,
      'providerAccountId': providerAccountId,
      'status': status,
      'hostnameCount': hostnameCount,
      'verifiedHostnameCount': verifiedHostnameCount,
      'certificateCount': certificateCount,
      'bindingCount': bindingCount,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class CloudAccountResponse {
  final String id;
  final String accountCode;
  final String displayName;
  final String vendorCode;
  final String? dnsProvider;
  final String scopeType;
  final bool tenantGlobal;
  final String? ownerUserId;
  final bool isDefault;
  final String status;
  final bool credentialConfigured;
  final List<String>? capabilityCodes;
  final String? environment;

  CloudAccountResponse({
    required this.id,
    required this.accountCode,
    required this.displayName,
    required this.vendorCode,
    this.dnsProvider,
    required this.scopeType,
    required this.tenantGlobal,
    this.ownerUserId,
    required this.isDefault,
    required this.status,
    required this.credentialConfigured,
    this.capabilityCodes,
    this.environment
  });

  factory CloudAccountResponse.fromJson(Map<String, dynamic> json) {
    return CloudAccountResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('CloudAccountResponse.id is required');
        }
        return value;
      })(),
      accountCode: (() {
        final value = json['accountCode']?.toString();
        if (value == null) {
          throw FormatException('CloudAccountResponse.accountCode is required');
        }
        return value;
      })(),
      displayName: (() {
        final value = json['displayName']?.toString();
        if (value == null) {
          throw FormatException('CloudAccountResponse.displayName is required');
        }
        return value;
      })(),
      vendorCode: (() {
        final value = json['vendorCode']?.toString();
        if (value == null) {
          throw FormatException('CloudAccountResponse.vendorCode is required');
        }
        return value;
      })(),
      dnsProvider: json['dnsProvider']?.toString(),
      scopeType: (() {
        final value = json['scopeType']?.toString();
        if (value == null) {
          throw FormatException('CloudAccountResponse.scopeType is required');
        }
        return value;
      })(),
      tenantGlobal: (() {
        final value = json['tenantGlobal'];
        if (value is! bool) {
          throw FormatException('CloudAccountResponse.tenantGlobal is required');
        }
        return value;
      })(),
      ownerUserId: json['ownerUserId']?.toString(),
      isDefault: (() {
        final value = json['isDefault'];
        if (value is! bool) {
          throw FormatException('CloudAccountResponse.isDefault is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('CloudAccountResponse.status is required');
        }
        return value;
      })(),
      credentialConfigured: (() {
        final value = json['credentialConfigured'];
        if (value is! bool) {
          throw FormatException('CloudAccountResponse.credentialConfigured is required');
        }
        return value;
      })(),
      capabilityCodes: (() {
        final list = _sdkworkAsList(json['capabilityCodes']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      environment: json['environment']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'accountCode': accountCode,
      'displayName': displayName,
      'vendorCode': vendorCode,
      'dnsProvider': dnsProvider,
      'scopeType': scopeType,
      'tenantGlobal': tenantGlobal,
      'ownerUserId': ownerUserId,
      'isDefault': isDefault,
      'status': status,
      'credentialConfigured': credentialConfigured,
      'capabilityCodes': capabilityCodes?.map((item) => item).toList(),
      'environment': environment,
    };
  }
}

class CreateCloudAccountRequest {
  final String displayName;
  final String? accountCode;
  final String dnsProvider;
  final String? scopeType;
  final String? environment;
  final bool? isDefault;
  final String? accessKeyId;
  final String secretAccessKey;
  final String? sessionToken;
  final bool? confirmsCredential;

  CreateCloudAccountRequest({
    required this.displayName,
    this.accountCode,
    required this.dnsProvider,
    this.scopeType,
    this.environment,
    this.isDefault,
    this.accessKeyId,
    required this.secretAccessKey,
    this.sessionToken,
    this.confirmsCredential
  });

  factory CreateCloudAccountRequest.fromJson(Map<String, dynamic> json) {
    return CreateCloudAccountRequest(
      displayName: (() {
        final value = json['displayName']?.toString();
        if (value == null) {
          throw FormatException('CreateCloudAccountRequest.displayName is required');
        }
        return value;
      })(),
      accountCode: json['accountCode']?.toString(),
      dnsProvider: (() {
        final value = json['dnsProvider']?.toString();
        if (value == null) {
          throw FormatException('CreateCloudAccountRequest.dnsProvider is required');
        }
        return value;
      })(),
      scopeType: json['scopeType']?.toString(),
      environment: json['environment']?.toString(),
      isDefault: json['isDefault'] is bool ? json['isDefault'] : null,
      accessKeyId: json['accessKeyId']?.toString(),
      secretAccessKey: (() {
        final value = json['secretAccessKey']?.toString();
        if (value == null) {
          throw FormatException('CreateCloudAccountRequest.secretAccessKey is required');
        }
        return value;
      })(),
      sessionToken: json['sessionToken']?.toString(),
      confirmsCredential: json['confirmsCredential'] is bool ? json['confirmsCredential'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'displayName': displayName,
      'accountCode': accountCode,
      'dnsProvider': dnsProvider,
      'scopeType': scopeType,
      'environment': environment,
      'isDefault': isDefault,
      'accessKeyId': accessKeyId,
      'secretAccessKey': secretAccessKey,
      'sessionToken': sessionToken,
      'confirmsCredential': confirmsCredential,
    };
  }
}

class CloudAccountRegistrationResponse {
  final CloudAccountResponse account;
  final bool reused;
  final bool credentialApplied;

  CloudAccountRegistrationResponse({
    required this.account,
    required this.reused,
    required this.credentialApplied
  });

  factory CloudAccountRegistrationResponse.fromJson(Map<String, dynamic> json) {
    return CloudAccountRegistrationResponse(
      account: (() {
        final map = _sdkworkAsMap(json['account']);
        if (map == null) {
          throw FormatException('CloudAccountRegistrationResponse.account is required');
        }
        return CloudAccountResponse.fromJson(map);
      })(),
      reused: (() {
        final value = json['reused'];
        if (value is! bool) {
          throw FormatException('CloudAccountRegistrationResponse.reused is required');
        }
        return value;
      })(),
      credentialApplied: (() {
        final value = json['credentialApplied'];
        if (value is! bool) {
          throw FormatException('CloudAccountRegistrationResponse.credentialApplied is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'account': account.toJson(),
      'reused': reused,
      'credentialApplied': credentialApplied,
    };
  }
}

class CreateDomainHostnameRequest {
  final String relativeName;

  CreateDomainHostnameRequest({
    required this.relativeName
  });

  factory CreateDomainHostnameRequest.fromJson(Map<String, dynamic> json) {
    return CreateDomainHostnameRequest(
      relativeName: (() {
        final value = json['relativeName']?.toString();
        if (value == null) {
          throw FormatException('CreateDomainHostnameRequest.relativeName is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'relativeName': relativeName,
    };
  }
}

class UpdateDomainHostnameRequest {
  final String relativeName;

  UpdateDomainHostnameRequest({
    required this.relativeName
  });

  factory UpdateDomainHostnameRequest.fromJson(Map<String, dynamic> json) {
    return UpdateDomainHostnameRequest(
      relativeName: (() {
        final value = json['relativeName']?.toString();
        if (value == null) {
          throw FormatException('UpdateDomainHostnameRequest.relativeName is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'relativeName': relativeName,
    };
  }
}

class DomainHostnameResponse {
  final String id;
  final String zoneId;
  final String hostname;
  final String relativeName;
  final String hostnameType;
  final String verificationStatus;
  final String? verifiedAt;
  final String status;
  final String certificateCount;
  final String bindingCount;
  final String createdAt;
  final String updatedAt;
  final String version;

  DomainHostnameResponse({
    required this.id,
    required this.zoneId,
    required this.hostname,
    required this.relativeName,
    required this.hostnameType,
    required this.verificationStatus,
    this.verifiedAt,
    required this.status,
    required this.certificateCount,
    required this.bindingCount,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory DomainHostnameResponse.fromJson(Map<String, dynamic> json) {
    return DomainHostnameResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.id is required');
        }
        return value;
      })(),
      zoneId: (() {
        final value = json['zoneId']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.zoneId is required');
        }
        return value;
      })(),
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.hostname is required');
        }
        return value;
      })(),
      relativeName: (() {
        final value = json['relativeName']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.relativeName is required');
        }
        return value;
      })(),
      hostnameType: (() {
        final value = json['hostnameType']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.hostnameType is required');
        }
        return value;
      })(),
      verificationStatus: (() {
        final value = json['verificationStatus']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.verificationStatus is required');
        }
        return value;
      })(),
      verifiedAt: json['verifiedAt']?.toString(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.status is required');
        }
        return value;
      })(),
      certificateCount: (() {
        final value = json['certificateCount']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.certificateCount is required');
        }
        return value;
      })(),
      bindingCount: (() {
        final value = json['bindingCount']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.bindingCount is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('DomainHostnameResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'zoneId': zoneId,
      'hostname': hostname,
      'relativeName': relativeName,
      'hostnameType': hostnameType,
      'verificationStatus': verificationStatus,
      'verifiedAt': verifiedAt,
      'status': status,
      'certificateCount': certificateCount,
      'bindingCount': bindingCount,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class DomainVerifyResponse {
  final bool verified;
  final String method;
  final String? verificationId;
  final String? recordName;
  final String? recordRelativeName;
  final String? token;
  final String? expiresAt;

  DomainVerifyResponse({
    required this.verified,
    required this.method,
    this.verificationId,
    this.recordName,
    this.recordRelativeName,
    this.token,
    this.expiresAt
  });

  factory DomainVerifyResponse.fromJson(Map<String, dynamic> json) {
    return DomainVerifyResponse(
      verified: (() {
        final value = json['verified'];
        if (value is! bool) {
          throw FormatException('DomainVerifyResponse.verified is required');
        }
        return value;
      })(),
      method: (() {
        final value = json['method']?.toString();
        if (value == null) {
          throw FormatException('DomainVerifyResponse.method is required');
        }
        return value;
      })(),
      verificationId: json['verificationId']?.toString(),
      recordName: json['recordName']?.toString(),
      recordRelativeName: json['recordRelativeName']?.toString(),
      token: json['token']?.toString(),
      expiresAt: json['expiresAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'verified': verified,
      'method': method,
      'verificationId': verificationId,
      'recordName': recordName,
      'recordRelativeName': recordRelativeName,
      'token': token,
      'expiresAt': expiresAt,
    };
  }
}

class DomainDnsRecordResponse {
  final String id;
  final String recordName;
  final String recordType;
  final String recordValue;
  final int? ttlSeconds;
  final int? priority;
  final String? recordLine;
  final String? hostnameId;
  final String dnsProvider;
  final String providerAccountId;
  final String? providerRecordRef;
  final String syncedAt;

  DomainDnsRecordResponse({
    required this.id,
    required this.recordName,
    required this.recordType,
    required this.recordValue,
    this.ttlSeconds,
    this.priority,
    this.recordLine,
    this.hostnameId,
    required this.dnsProvider,
    required this.providerAccountId,
    this.providerRecordRef,
    required this.syncedAt
  });

  factory DomainDnsRecordResponse.fromJson(Map<String, dynamic> json) {
    return DomainDnsRecordResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsRecordResponse.id is required');
        }
        return value;
      })(),
      recordName: (() {
        final value = json['recordName']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsRecordResponse.recordName is required');
        }
        return value;
      })(),
      recordType: (() {
        final value = json['recordType']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsRecordResponse.recordType is required');
        }
        return value;
      })(),
      recordValue: (() {
        final value = json['recordValue']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsRecordResponse.recordValue is required');
        }
        return value;
      })(),
      ttlSeconds: json['ttlSeconds'] is int ? json['ttlSeconds'] : null,
      priority: json['priority'] is int ? json['priority'] : null,
      recordLine: json['recordLine']?.toString(),
      hostnameId: json['hostnameId']?.toString(),
      dnsProvider: (() {
        final value = json['dnsProvider']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsRecordResponse.dnsProvider is required');
        }
        return value;
      })(),
      providerAccountId: (() {
        final value = json['providerAccountId']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsRecordResponse.providerAccountId is required');
        }
        return value;
      })(),
      providerRecordRef: json['providerRecordRef']?.toString(),
      syncedAt: (() {
        final value = json['syncedAt']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsRecordResponse.syncedAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'recordName': recordName,
      'recordType': recordType,
      'recordValue': recordValue,
      'ttlSeconds': ttlSeconds,
      'priority': priority,
      'recordLine': recordLine,
      'hostnameId': hostnameId,
      'dnsProvider': dnsProvider,
      'providerAccountId': providerAccountId,
      'providerRecordRef': providerRecordRef,
      'syncedAt': syncedAt,
    };
  }
}

class DomainDnsSyncResponse {
  final String recordCount;
  final String syncedAt;
  final String zoneApex;
  final String dnsProvider;
  final String providerAccountId;

  DomainDnsSyncResponse({
    required this.recordCount,
    required this.syncedAt,
    required this.zoneApex,
    required this.dnsProvider,
    required this.providerAccountId
  });

  factory DomainDnsSyncResponse.fromJson(Map<String, dynamic> json) {
    return DomainDnsSyncResponse(
      recordCount: (() {
        final value = json['recordCount']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsSyncResponse.recordCount is required');
        }
        return value;
      })(),
      syncedAt: (() {
        final value = json['syncedAt']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsSyncResponse.syncedAt is required');
        }
        return value;
      })(),
      zoneApex: (() {
        final value = json['zoneApex']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsSyncResponse.zoneApex is required');
        }
        return value;
      })(),
      dnsProvider: (() {
        final value = json['dnsProvider']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsSyncResponse.dnsProvider is required');
        }
        return value;
      })(),
      providerAccountId: (() {
        final value = json['providerAccountId']?.toString();
        if (value == null) {
          throw FormatException('DomainDnsSyncResponse.providerAccountId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'recordCount': recordCount,
      'syncedAt': syncedAt,
      'zoneApex': zoneApex,
      'dnsProvider': dnsProvider,
      'providerAccountId': providerAccountId,
    };
  }
}

class EnsureDomainHostnameClaimsRequest {
  final List<String> hostnames;

  EnsureDomainHostnameClaimsRequest({
    required this.hostnames
  });

  factory EnsureDomainHostnameClaimsRequest.fromJson(Map<String, dynamic> json) {
    return EnsureDomainHostnameClaimsRequest(
      hostnames: (() {
        final list = _sdkworkAsList(json['hostnames']);
        if (list == null) {
          throw FormatException('EnsureDomainHostnameClaimsRequest.hostnames is required');
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'hostnames': hostnames.map((item) => item).toList(),
    };
  }
}

class DomainHostnameClaimResponse {
  final DomainHostnameResponse hostname;
  final bool verified;
  final String? dnsRecordName;
  final String? dnsRecordRelativeName;
  final String? dnsRecordType;
  final String? dnsRecordValue;
  final String? expiresAt;

  DomainHostnameClaimResponse({
    required this.hostname,
    required this.verified,
    this.dnsRecordName,
    this.dnsRecordRelativeName,
    this.dnsRecordType,
    this.dnsRecordValue,
    this.expiresAt
  });

  factory DomainHostnameClaimResponse.fromJson(Map<String, dynamic> json) {
    return DomainHostnameClaimResponse(
      hostname: (() {
        final map = _sdkworkAsMap(json['hostname']);
        if (map == null) {
          throw FormatException('DomainHostnameClaimResponse.hostname is required');
        }
        return DomainHostnameResponse.fromJson(map);
      })(),
      verified: (() {
        final value = json['verified'];
        if (value is! bool) {
          throw FormatException('DomainHostnameClaimResponse.verified is required');
        }
        return value;
      })(),
      dnsRecordName: json['dnsRecordName']?.toString(),
      dnsRecordRelativeName: json['dnsRecordRelativeName']?.toString(),
      dnsRecordType: json['dnsRecordType']?.toString(),
      dnsRecordValue: json['dnsRecordValue']?.toString(),
      expiresAt: json['expiresAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'hostname': hostname.toJson(),
      'verified': verified,
      'dnsRecordName': dnsRecordName,
      'dnsRecordRelativeName': dnsRecordRelativeName,
      'dnsRecordType': dnsRecordType,
      'dnsRecordValue': dnsRecordValue,
      'expiresAt': expiresAt,
    };
  }
}

class CreateDeploymentRequest {
  final int deployType;
  final String? versionTag;
  final String? commitHash;
  final String? sourceRef;
  final String? environment;
  final String? releaseId;
  final String? idempotencyKey;

  CreateDeploymentRequest({
    required this.deployType,
    this.versionTag,
    this.commitHash,
    this.sourceRef,
    this.environment,
    this.releaseId,
    this.idempotencyKey
  });

  factory CreateDeploymentRequest.fromJson(Map<String, dynamic> json) {
    return CreateDeploymentRequest(
      deployType: (() {
        final value = json['deployType'];
        if (value is! int) {
          throw FormatException('CreateDeploymentRequest.deployType is required');
        }
        return value;
      })(),
      versionTag: json['versionTag']?.toString(),
      commitHash: json['commitHash']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      environment: json['environment']?.toString(),
      releaseId: json['releaseId']?.toString(),
      idempotencyKey: json['idempotencyKey']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'deployType': deployType,
      'versionTag': versionTag,
      'commitHash': commitHash,
      'sourceRef': sourceRef,
      'environment': environment,
      'releaseId': releaseId,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class DeploymentResponse {
  final String? id;
  final String? siteId;
  final int? deployType;
  final String? releaseId;
  final String? versionTag;
  final int? status;
  final String? startedAt;
  final String? completedAt;
  final String? durationMs;
  final String? createdAt;

  DeploymentResponse({
    this.id,
    this.siteId,
    this.deployType,
    this.releaseId,
    this.versionTag,
    this.status,
    this.startedAt,
    this.completedAt,
    this.durationMs,
    this.createdAt
  });

  factory DeploymentResponse.fromJson(Map<String, dynamic> json) {
    return DeploymentResponse(
      id: json['id']?.toString(),
      siteId: json['siteId']?.toString(),
      deployType: json['deployType'] is int ? json['deployType'] : null,
      releaseId: json['releaseId']?.toString(),
      versionTag: json['versionTag']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      startedAt: json['startedAt']?.toString(),
      completedAt: json['completedAt']?.toString(),
      durationMs: json['durationMs']?.toString(),
      createdAt: json['createdAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'siteId': siteId,
      'deployType': deployType,
      'releaseId': releaseId,
      'versionTag': versionTag,
      'status': status,
      'startedAt': startedAt,
      'completedAt': completedAt,
      'durationMs': durationMs,
      'createdAt': createdAt,
    };
  }
}

class DeploymentPage {
  final List<DeploymentResponse>? items;
  final String? total;

  DeploymentPage({
    this.items,
    this.total
  });

  factory DeploymentPage.fromJson(Map<String, dynamic> json) {
    return DeploymentPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : DeploymentResponse.fromJson(map);
      })())
            .whereType<DeploymentResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class CreateEnvVariableRequest {
  final String key;
  final String value;
  final String? environment;
  final bool? isSecret;

  CreateEnvVariableRequest({
    required this.key,
    required this.value,
    this.environment,
    this.isSecret
  });

  factory CreateEnvVariableRequest.fromJson(Map<String, dynamic> json) {
    return CreateEnvVariableRequest(
      key: (() {
        final value = json['key']?.toString();
        if (value == null) {
          throw FormatException('CreateEnvVariableRequest.key is required');
        }
        return value;
      })(),
      value: (() {
        final value = json['value']?.toString();
        if (value == null) {
          throw FormatException('CreateEnvVariableRequest.value is required');
        }
        return value;
      })(),
      environment: json['environment']?.toString(),
      isSecret: json['isSecret'] is bool ? json['isSecret'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'key': key,
      'value': value,
      'environment': environment,
      'isSecret': isSecret,
    };
  }
}

class EnvVariableResponse {
  final String? id;
  final String? key;
  final String? environment;
  final bool? isSecret;
  final String? createdAt;

  EnvVariableResponse({
    this.id,
    this.key,
    this.environment,
    this.isSecret,
    this.createdAt
  });

  factory EnvVariableResponse.fromJson(Map<String, dynamic> json) {
    return EnvVariableResponse(
      id: json['id']?.toString(),
      key: json['key']?.toString(),
      environment: json['environment']?.toString(),
      isSecret: json['isSecret'] is bool ? json['isSecret'] : null,
      createdAt: json['createdAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'key': key,
      'environment': environment,
      'isSecret': isSecret,
      'createdAt': createdAt,
    };
  }
}

class EnvVariablePage {
  final List<EnvVariableResponse>? items;
  final String? total;

  EnvVariablePage({
    this.items,
    this.total
  });

  factory EnvVariablePage.fromJson(Map<String, dynamic> json) {
    return EnvVariablePage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : EnvVariableResponse.fromJson(map);
      })())
            .whereType<EnvVariableResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class CreateCertificateRequest {
  final String certName;
  final List<String> domainIds;
  final String? certificateScope;
  final String? validationMethod;
  final String? caProfile;
  final String? preferredKeyAlgorithm;
  final bool? autoRenew;
  final int? renewBeforeDays;
  final String? providerAccountId;

  CreateCertificateRequest({
    required this.certName,
    required this.domainIds,
    this.certificateScope,
    this.validationMethod,
    this.caProfile,
    this.preferredKeyAlgorithm,
    this.autoRenew,
    this.renewBeforeDays,
    this.providerAccountId
  });

  factory CreateCertificateRequest.fromJson(Map<String, dynamic> json) {
    return CreateCertificateRequest(
      certName: (() {
        final value = json['certName']?.toString();
        if (value == null) {
          throw FormatException('CreateCertificateRequest.certName is required');
        }
        return value;
      })(),
      domainIds: (() {
        final list = _sdkworkAsList(json['domainIds']);
        if (list == null) {
          throw FormatException('CreateCertificateRequest.domainIds is required');
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      certificateScope: json['certificateScope']?.toString(),
      validationMethod: json['validationMethod']?.toString(),
      caProfile: json['caProfile']?.toString(),
      preferredKeyAlgorithm: json['preferredKeyAlgorithm']?.toString(),
      autoRenew: json['autoRenew'] is bool ? json['autoRenew'] : null,
      renewBeforeDays: json['renewBeforeDays'] is int ? json['renewBeforeDays'] : null,
      providerAccountId: json['providerAccountId']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'certName': certName,
      'domainIds': domainIds.map((item) => item).toList(),
      'certificateScope': certificateScope,
      'validationMethod': validationMethod,
      'caProfile': caProfile,
      'preferredKeyAlgorithm': preferredKeyAlgorithm,
      'autoRenew': autoRenew,
      'renewBeforeDays': renewBeforeDays,
      'providerAccountId': providerAccountId,
    };
  }
}

class CertificateRenewalResponse {
  final String id;
  final String certificateId;
  final String triggerKind;
  final String status;
  final int attemptNo;
  final String? previousVersionId;
  final String? resultingVersionId;
  final String? previousNotBefore;
  final String? previousNotAfter;
  final String? newNotBefore;
  final String? newNotAfter;
  final String scheduledAt;
  final String? startedAt;
  final String? finishedAt;
  final String? lastErrorCode;
  final String createdAt;

  CertificateRenewalResponse({
    required this.id,
    required this.certificateId,
    required this.triggerKind,
    required this.status,
    required this.attemptNo,
    this.previousVersionId,
    this.resultingVersionId,
    this.previousNotBefore,
    this.previousNotAfter,
    this.newNotBefore,
    this.newNotAfter,
    required this.scheduledAt,
    this.startedAt,
    this.finishedAt,
    this.lastErrorCode,
    required this.createdAt
  });

  factory CertificateRenewalResponse.fromJson(Map<String, dynamic> json) {
    return CertificateRenewalResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('CertificateRenewalResponse.id is required');
        }
        return value;
      })(),
      certificateId: (() {
        final value = json['certificateId']?.toString();
        if (value == null) {
          throw FormatException('CertificateRenewalResponse.certificateId is required');
        }
        return value;
      })(),
      triggerKind: (() {
        final value = json['triggerKind']?.toString();
        if (value == null) {
          throw FormatException('CertificateRenewalResponse.triggerKind is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('CertificateRenewalResponse.status is required');
        }
        return value;
      })(),
      attemptNo: (() {
        final value = json['attemptNo'];
        if (value is! int) {
          throw FormatException('CertificateRenewalResponse.attemptNo is required');
        }
        return value;
      })(),
      previousVersionId: json['previousVersionId']?.toString(),
      resultingVersionId: json['resultingVersionId']?.toString(),
      previousNotBefore: json['previousNotBefore']?.toString(),
      previousNotAfter: json['previousNotAfter']?.toString(),
      newNotBefore: json['newNotBefore']?.toString(),
      newNotAfter: json['newNotAfter']?.toString(),
      scheduledAt: (() {
        final value = json['scheduledAt']?.toString();
        if (value == null) {
          throw FormatException('CertificateRenewalResponse.scheduledAt is required');
        }
        return value;
      })(),
      startedAt: json['startedAt']?.toString(),
      finishedAt: json['finishedAt']?.toString(),
      lastErrorCode: json['lastErrorCode']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('CertificateRenewalResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'certificateId': certificateId,
      'triggerKind': triggerKind,
      'status': status,
      'attemptNo': attemptNo,
      'previousVersionId': previousVersionId,
      'resultingVersionId': resultingVersionId,
      'previousNotBefore': previousNotBefore,
      'previousNotAfter': previousNotAfter,
      'newNotBefore': newNotBefore,
      'newNotAfter': newNotAfter,
      'scheduledAt': scheduledAt,
      'startedAt': startedAt,
      'finishedAt': finishedAt,
      'lastErrorCode': lastErrorCode,
      'createdAt': createdAt,
    };
  }
}

class CertificateResponse {
  final String id;
  final String certName;
  final String certificateSource;
  final String caProfile;
  final String certificateScope;
  final String validationMethod;
  final String? providerAccountId;
  final String preferredKeyAlgorithm;
  final List<String> identifiers;
  final String? currentVersionId;
  final String? issuer;
  final String? notBefore;
  final String? notAfter;
  final bool autoRenew;
  final String renewalStatus;
  final String status;
  final int renewBeforeDays;
  final String? renewalDueAt;
  final int? daysUntilExpiry;
  final String? validityPhase;
  final String? lastRenewalAt;
  final int renewalFailureCount;
  final String createdAt;
  final String updatedAt;
  final String version;

  CertificateResponse({
    required this.id,
    required this.certName,
    required this.certificateSource,
    required this.caProfile,
    required this.certificateScope,
    required this.validationMethod,
    this.providerAccountId,
    required this.preferredKeyAlgorithm,
    required this.identifiers,
    this.currentVersionId,
    this.issuer,
    this.notBefore,
    this.notAfter,
    required this.autoRenew,
    required this.renewalStatus,
    required this.status,
    required this.renewBeforeDays,
    this.renewalDueAt,
    this.daysUntilExpiry,
    this.validityPhase,
    this.lastRenewalAt,
    required this.renewalFailureCount,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory CertificateResponse.fromJson(Map<String, dynamic> json) {
    return CertificateResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.id is required');
        }
        return value;
      })(),
      certName: (() {
        final value = json['certName']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.certName is required');
        }
        return value;
      })(),
      certificateSource: (() {
        final value = json['certificateSource']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.certificateSource is required');
        }
        return value;
      })(),
      caProfile: (() {
        final value = json['caProfile']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.caProfile is required');
        }
        return value;
      })(),
      certificateScope: (() {
        final value = json['certificateScope']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.certificateScope is required');
        }
        return value;
      })(),
      validationMethod: (() {
        final value = json['validationMethod']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.validationMethod is required');
        }
        return value;
      })(),
      providerAccountId: json['providerAccountId']?.toString(),
      preferredKeyAlgorithm: (() {
        final value = json['preferredKeyAlgorithm']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.preferredKeyAlgorithm is required');
        }
        return value;
      })(),
      identifiers: (() {
        final list = _sdkworkAsList(json['identifiers']);
        if (list == null) {
          throw FormatException('CertificateResponse.identifiers is required');
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      currentVersionId: json['currentVersionId']?.toString(),
      issuer: json['issuer']?.toString(),
      notBefore: json['notBefore']?.toString(),
      notAfter: json['notAfter']?.toString(),
      autoRenew: (() {
        final value = json['autoRenew'];
        if (value is! bool) {
          throw FormatException('CertificateResponse.autoRenew is required');
        }
        return value;
      })(),
      renewalStatus: (() {
        final value = json['renewalStatus']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.renewalStatus is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.status is required');
        }
        return value;
      })(),
      renewBeforeDays: (() {
        final value = json['renewBeforeDays'];
        if (value is! int) {
          throw FormatException('CertificateResponse.renewBeforeDays is required');
        }
        return value;
      })(),
      renewalDueAt: json['renewalDueAt']?.toString(),
      daysUntilExpiry: json['daysUntilExpiry'] is int ? json['daysUntilExpiry'] : null,
      validityPhase: json['validityPhase']?.toString(),
      lastRenewalAt: json['lastRenewalAt']?.toString(),
      renewalFailureCount: (() {
        final value = json['renewalFailureCount'];
        if (value is! int) {
          throw FormatException('CertificateResponse.renewalFailureCount is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'certName': certName,
      'certificateSource': certificateSource,
      'caProfile': caProfile,
      'certificateScope': certificateScope,
      'validationMethod': validationMethod,
      'providerAccountId': providerAccountId,
      'preferredKeyAlgorithm': preferredKeyAlgorithm,
      'identifiers': identifiers.map((item) => item).toList(),
      'currentVersionId': currentVersionId,
      'issuer': issuer,
      'notBefore': notBefore,
      'notAfter': notAfter,
      'autoRenew': autoRenew,
      'renewalStatus': renewalStatus,
      'status': status,
      'renewBeforeDays': renewBeforeDays,
      'renewalDueAt': renewalDueAt,
      'daysUntilExpiry': daysUntilExpiry,
      'validityPhase': validityPhase,
      'lastRenewalAt': lastRenewalAt,
      'renewalFailureCount': renewalFailureCount,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class CertificatePage {
  final List<CertificateResponse>? items;
  final String? total;

  CertificatePage({
    this.items,
    this.total
  });

  factory CertificatePage.fromJson(Map<String, dynamic> json) {
    return CertificatePage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : CertificateResponse.fromJson(map);
      })())
            .whereType<CertificateResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class CreateHealthCheckRequest {
  final int checkType;
  final String? checkUrl;
  final int? checkInterval;
  final int? timeoutMs;
  final int? retryCount;

  CreateHealthCheckRequest({
    required this.checkType,
    this.checkUrl,
    this.checkInterval,
    this.timeoutMs,
    this.retryCount
  });

  factory CreateHealthCheckRequest.fromJson(Map<String, dynamic> json) {
    return CreateHealthCheckRequest(
      checkType: (() {
        final value = json['checkType'];
        if (value is! int) {
          throw FormatException('CreateHealthCheckRequest.checkType is required');
        }
        return value;
      })(),
      checkUrl: json['checkUrl']?.toString(),
      checkInterval: json['checkInterval'] is int ? json['checkInterval'] : null,
      timeoutMs: json['timeoutMs'] is int ? json['timeoutMs'] : null,
      retryCount: json['retryCount'] is int ? json['retryCount'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'checkType': checkType,
      'checkUrl': checkUrl,
      'checkInterval': checkInterval,
      'timeoutMs': timeoutMs,
      'retryCount': retryCount,
    };
  }
}

class HealthCheckResponse {
  final String? id;
  final int? checkType;
  final String? checkUrl;
  final int? checkInterval;
  final int? status;
  final String? createdAt;

  HealthCheckResponse({
    this.id,
    this.checkType,
    this.checkUrl,
    this.checkInterval,
    this.status,
    this.createdAt
  });

  factory HealthCheckResponse.fromJson(Map<String, dynamic> json) {
    return HealthCheckResponse(
      id: json['id']?.toString(),
      checkType: json['checkType'] is int ? json['checkType'] : null,
      checkUrl: json['checkUrl']?.toString(),
      checkInterval: json['checkInterval'] is int ? json['checkInterval'] : null,
      status: json['status'] is int ? json['status'] : null,
      createdAt: json['createdAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'checkType': checkType,
      'checkUrl': checkUrl,
      'checkInterval': checkInterval,
      'status': status,
      'createdAt': createdAt,
    };
  }
}

class HealthCheckPage {
  final List<HealthCheckResponse>? items;
  final String? total;

  HealthCheckPage({
    this.items,
    this.total
  });

  factory HealthCheckPage.fromJson(Map<String, dynamic> json) {
    return HealthCheckPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : HealthCheckResponse.fromJson(map);
      })())
            .whereType<HealthCheckResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class ArtifactResponse {
  final String? id;
  final String? siteId;
  final int? packageType;
  final String? fileName;
  final String? contentType;
  final String? contentLength;
  final String? checksumSha256;
  final String? driveNodeId;
  final String? uploadSessionId;
  final int? status;
  final String? createdAt;

  ArtifactResponse({
    this.id,
    this.siteId,
    this.packageType,
    this.fileName,
    this.contentType,
    this.contentLength,
    this.checksumSha256,
    this.driveNodeId,
    this.uploadSessionId,
    this.status,
    this.createdAt
  });

  factory ArtifactResponse.fromJson(Map<String, dynamic> json) {
    return ArtifactResponse(
      id: json['id']?.toString(),
      siteId: json['siteId']?.toString(),
      packageType: json['packageType'] is int ? json['packageType'] : null,
      fileName: json['fileName']?.toString(),
      contentType: json['contentType']?.toString(),
      contentLength: json['contentLength']?.toString(),
      checksumSha256: json['checksumSha256']?.toString(),
      driveNodeId: json['driveNodeId']?.toString(),
      uploadSessionId: json['uploadSessionId']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      createdAt: json['createdAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'siteId': siteId,
      'packageType': packageType,
      'fileName': fileName,
      'contentType': contentType,
      'contentLength': contentLength,
      'checksumSha256': checksumSha256,
      'driveNodeId': driveNodeId,
      'uploadSessionId': uploadSessionId,
      'status': status,
      'createdAt': createdAt,
    };
  }
}

class CreateArtifactRequest {
  final String? siteId;
  final int packageType;
  final String fileName;
  final String contentType;
  final String contentLength;
  final String? checksumSha256;
  final String driveUploadSessionId;
  final String? driveUploadItemId;
  final String driveSpaceId;
  final String driveNodeId;
  final String idempotencyKey;

  CreateArtifactRequest({
    this.siteId,
    required this.packageType,
    required this.fileName,
    required this.contentType,
    required this.contentLength,
    this.checksumSha256,
    required this.driveUploadSessionId,
    this.driveUploadItemId,
    required this.driveSpaceId,
    required this.driveNodeId,
    required this.idempotencyKey
  });

  factory CreateArtifactRequest.fromJson(Map<String, dynamic> json) {
    return CreateArtifactRequest(
      siteId: json['siteId']?.toString(),
      packageType: (() {
        final value = json['packageType'];
        if (value is! int) {
          throw FormatException('CreateArtifactRequest.packageType is required');
        }
        return value;
      })(),
      fileName: (() {
        final value = json['fileName']?.toString();
        if (value == null) {
          throw FormatException('CreateArtifactRequest.fileName is required');
        }
        return value;
      })(),
      contentType: (() {
        final value = json['contentType']?.toString();
        if (value == null) {
          throw FormatException('CreateArtifactRequest.contentType is required');
        }
        return value;
      })(),
      contentLength: (() {
        final value = json['contentLength']?.toString();
        if (value == null) {
          throw FormatException('CreateArtifactRequest.contentLength is required');
        }
        return value;
      })(),
      checksumSha256: json['checksumSha256']?.toString(),
      driveUploadSessionId: (() {
        final value = json['driveUploadSessionId']?.toString();
        if (value == null) {
          throw FormatException('CreateArtifactRequest.driveUploadSessionId is required');
        }
        return value;
      })(),
      driveUploadItemId: json['driveUploadItemId']?.toString(),
      driveSpaceId: (() {
        final value = json['driveSpaceId']?.toString();
        if (value == null) {
          throw FormatException('CreateArtifactRequest.driveSpaceId is required');
        }
        return value;
      })(),
      driveNodeId: (() {
        final value = json['driveNodeId']?.toString();
        if (value == null) {
          throw FormatException('CreateArtifactRequest.driveNodeId is required');
        }
        return value;
      })(),
      idempotencyKey: (() {
        final value = json['idempotencyKey']?.toString();
        if (value == null) {
          throw FormatException('CreateArtifactRequest.idempotencyKey is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'siteId': siteId,
      'packageType': packageType,
      'fileName': fileName,
      'contentType': contentType,
      'contentLength': contentLength,
      'checksumSha256': checksumSha256,
      'driveUploadSessionId': driveUploadSessionId,
      'driveUploadItemId': driveUploadItemId,
      'driveSpaceId': driveSpaceId,
      'driveNodeId': driveNodeId,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class ArtifactPage {
  final List<ArtifactResponse>? items;
  final String? total;

  ArtifactPage({
    this.items,
    this.total
  });

  factory ArtifactPage.fromJson(Map<String, dynamic> json) {
    return ArtifactPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : ArtifactResponse.fromJson(map);
      })())
            .whereType<ArtifactResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class CreateReleaseRequest {
  final String artifactId;
  final String? versionTag;
  final String idempotencyKey;

  CreateReleaseRequest({
    required this.artifactId,
    this.versionTag,
    required this.idempotencyKey
  });

  factory CreateReleaseRequest.fromJson(Map<String, dynamic> json) {
    return CreateReleaseRequest(
      artifactId: (() {
        final value = json['artifactId']?.toString();
        if (value == null) {
          throw FormatException('CreateReleaseRequest.artifactId is required');
        }
        return value;
      })(),
      versionTag: json['versionTag']?.toString(),
      idempotencyKey: (() {
        final value = json['idempotencyKey']?.toString();
        if (value == null) {
          throw FormatException('CreateReleaseRequest.idempotencyKey is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'artifactId': artifactId,
      'versionTag': versionTag,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class ReleaseResponse {
  final String? id;
  final String? siteId;
  final String? artifactId;
  final String? versionTag;
  final int? status;
  final String? createdAt;

  ReleaseResponse({
    this.id,
    this.siteId,
    this.artifactId,
    this.versionTag,
    this.status,
    this.createdAt
  });

  factory ReleaseResponse.fromJson(Map<String, dynamic> json) {
    return ReleaseResponse(
      id: json['id']?.toString(),
      siteId: json['siteId']?.toString(),
      artifactId: json['artifactId']?.toString(),
      versionTag: json['versionTag']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      createdAt: json['createdAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'siteId': siteId,
      'artifactId': artifactId,
      'versionTag': versionTag,
      'status': status,
      'createdAt': createdAt,
    };
  }
}

class ReleasePage {
  final List<ReleaseResponse>? items;
  final String? total;

  ReleasePage({
    this.items,
    this.total
  });

  factory ReleasePage.fromJson(Map<String, dynamic> json) {
    return ReleasePage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : ReleaseResponse.fromJson(map);
      })())
            .whereType<ReleaseResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class CreateDeployUploadSessionRequest {
  final String? siteId;
  final int packageType;
  final String fileName;
  final String contentType;
  final String contentLength;
  final String? checksum;
  final String idempotencyKey;

  CreateDeployUploadSessionRequest({
    this.siteId,
    required this.packageType,
    required this.fileName,
    required this.contentType,
    required this.contentLength,
    this.checksum,
    required this.idempotencyKey
  });

  factory CreateDeployUploadSessionRequest.fromJson(Map<String, dynamic> json) {
    return CreateDeployUploadSessionRequest(
      siteId: json['siteId']?.toString(),
      packageType: (() {
        final value = json['packageType'];
        if (value is! int) {
          throw FormatException('CreateDeployUploadSessionRequest.packageType is required');
        }
        return value;
      })(),
      fileName: (() {
        final value = json['fileName']?.toString();
        if (value == null) {
          throw FormatException('CreateDeployUploadSessionRequest.fileName is required');
        }
        return value;
      })(),
      contentType: (() {
        final value = json['contentType']?.toString();
        if (value == null) {
          throw FormatException('CreateDeployUploadSessionRequest.contentType is required');
        }
        return value;
      })(),
      contentLength: (() {
        final value = json['contentLength']?.toString();
        if (value == null) {
          throw FormatException('CreateDeployUploadSessionRequest.contentLength is required');
        }
        return value;
      })(),
      checksum: json['checksum']?.toString(),
      idempotencyKey: (() {
        final value = json['idempotencyKey']?.toString();
        if (value == null) {
          throw FormatException('CreateDeployUploadSessionRequest.idempotencyKey is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'siteId': siteId,
      'packageType': packageType,
      'fileName': fileName,
      'contentType': contentType,
      'contentLength': contentLength,
      'checksum': checksum,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class DeployUploadSessionResponse {
  final String? id;
  final String? siteId;
  final int? packageType;
  final String? fileName;
  final String? contentType;
  final String? contentLength;
  final String? checksum;
  final int? status;
  final String? driveUploadSessionId;
  final String? driveUploadItemId;
  final String? driveSpaceId;
  final String? driveNodeId;
  final String? createdAt;
  final String? updatedAt;

  DeployUploadSessionResponse({
    this.id,
    this.siteId,
    this.packageType,
    this.fileName,
    this.contentType,
    this.contentLength,
    this.checksum,
    this.status,
    this.driveUploadSessionId,
    this.driveUploadItemId,
    this.driveSpaceId,
    this.driveNodeId,
    this.createdAt,
    this.updatedAt
  });

  factory DeployUploadSessionResponse.fromJson(Map<String, dynamic> json) {
    return DeployUploadSessionResponse(
      id: json['id']?.toString(),
      siteId: json['siteId']?.toString(),
      packageType: json['packageType'] is int ? json['packageType'] : null,
      fileName: json['fileName']?.toString(),
      contentType: json['contentType']?.toString(),
      contentLength: json['contentLength']?.toString(),
      checksum: json['checksum']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      driveUploadSessionId: json['driveUploadSessionId']?.toString(),
      driveUploadItemId: json['driveUploadItemId']?.toString(),
      driveSpaceId: json['driveSpaceId']?.toString(),
      driveNodeId: json['driveNodeId']?.toString(),
      createdAt: json['createdAt']?.toString(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'siteId': siteId,
      'packageType': packageType,
      'fileName': fileName,
      'contentType': contentType,
      'contentLength': contentLength,
      'checksum': checksum,
      'status': status,
      'driveUploadSessionId': driveUploadSessionId,
      'driveUploadItemId': driveUploadItemId,
      'driveSpaceId': driveSpaceId,
      'driveNodeId': driveNodeId,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
    };
  }
}

class CompletedUploadPartInput {
  final String partNo;
  final String etag;

  CompletedUploadPartInput({
    required this.partNo,
    required this.etag
  });

  factory CompletedUploadPartInput.fromJson(Map<String, dynamic> json) {
    return CompletedUploadPartInput(
      partNo: (() {
        final value = json['partNo']?.toString();
        if (value == null) {
          throw FormatException('CompletedUploadPartInput.partNo is required');
        }
        return value;
      })(),
      etag: (() {
        final value = json['etag']?.toString();
        if (value == null) {
          throw FormatException('CompletedUploadPartInput.etag is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'partNo': partNo,
      'etag': etag,
    };
  }
}

class CompleteDeployUploadSessionRequest {
  final String checksumSha256Hex;
  final String? contentLength;
  final String? contentType;
  final List<CompletedUploadPartInput>? parts;

  CompleteDeployUploadSessionRequest({
    required this.checksumSha256Hex,
    this.contentLength,
    this.contentType,
    this.parts
  });

  factory CompleteDeployUploadSessionRequest.fromJson(Map<String, dynamic> json) {
    return CompleteDeployUploadSessionRequest(
      checksumSha256Hex: (() {
        final value = json['checksumSha256Hex']?.toString();
        if (value == null) {
          throw FormatException('CompleteDeployUploadSessionRequest.checksumSha256Hex is required');
        }
        return value;
      })(),
      contentLength: json['contentLength']?.toString(),
      contentType: json['contentType']?.toString(),
      parts: (() {
        final list = _sdkworkAsList(json['parts']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : CompletedUploadPartInput.fromJson(map);
      })())
            .whereType<CompletedUploadPartInput>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'checksumSha256Hex': checksumSha256Hex,
      'contentLength': contentLength,
      'contentType': contentType,
      'parts': parts?.map((item) => item.toJson()).toList(),
    };
  }
}

class SdkWorkApiResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SdkWorkApiResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SdkWorkApiResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkApiResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SdkWorkApiResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SdkWorkApiResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SdkWorkResourceData {
  final Map<String, dynamic> item;

  SdkWorkResourceData({
    required this.item
  });

  factory SdkWorkResourceData.fromJson(Map<String, dynamic> json) {
    return SdkWorkResourceData(
      item: (() {
        final map = _sdkworkAsMap(json['item']);
        if (map == null) {
          throw FormatException('SdkWorkResourceData.item is required');
        }
        return map;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'item': item,
    };
  }
}

class SdkWorkPageData {
  final List<Map<String, dynamic>> items;
  final PageInfo pageInfo;

  SdkWorkPageData({
    required this.items,
    required this.pageInfo
  });

  factory SdkWorkPageData.fromJson(Map<String, dynamic> json) {
    return SdkWorkPageData(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          throw FormatException('SdkWorkPageData.items is required');
        }
        return list
            .map((item) => _sdkworkAsMap(item))
            .whereType<Map<String, dynamic>>()
            .toList();
      })(),
      pageInfo: (() {
        final map = _sdkworkAsMap(json['pageInfo']);
        if (map == null) {
          throw FormatException('SdkWorkPageData.pageInfo is required');
        }
        return PageInfo.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items.map((item) => item).toList(),
      'pageInfo': pageInfo.toJson(),
    };
  }
}

class SdkWorkCommandData {
  final bool accepted;
  final String? resourceId;
  final String? status;

  SdkWorkCommandData({
    required this.accepted,
    this.resourceId,
    this.status
  });

  factory SdkWorkCommandData.fromJson(Map<String, dynamic> json) {
    return SdkWorkCommandData(
      accepted: (() {
        final value = json['accepted'];
        if (value is! bool) {
          throw FormatException('SdkWorkCommandData.accepted is required');
        }
        return value;
      })(),
      resourceId: json['resourceId']?.toString(),
      status: json['status']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'accepted': accepted,
      'resourceId': resourceId,
      'status': status,
    };
  }
}

class PageInfo {
  final String mode;
  final int? page;
  final int? pageSize;
  final String? totalItems;
  final int? totalPages;
  final String? nextCursor;
  final bool? hasMore;

  PageInfo({
    required this.mode,
    this.page,
    this.pageSize,
    this.totalItems,
    this.totalPages,
    this.nextCursor,
    this.hasMore
  });

  factory PageInfo.fromJson(Map<String, dynamic> json) {
    return PageInfo(
      mode: (() {
        final value = json['mode']?.toString();
        if (value == null) {
          throw FormatException('PageInfo.mode is required');
        }
        return value;
      })(),
      page: json['page'] is int ? json['page'] : null,
      pageSize: json['pageSize'] is int ? json['pageSize'] : null,
      totalItems: json['totalItems']?.toString(),
      totalPages: json['totalPages'] is int ? json['totalPages'] : null,
      nextCursor: json['nextCursor']?.toString(),
      hasMore: json['hasMore'] is bool ? json['hasMore'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'mode': mode,
      'page': page,
      'pageSize': pageSize,
      'totalItems': totalItems,
      'totalPages': totalPages,
      'nextCursor': nextCursor,
      'hasMore': hasMore,
    };
  }
}

class FieldError {
  final String field;
  final String message;
  final int? code;
  final String? i18nKey;
  final Map<String, dynamic>? params;

  FieldError({
    required this.field,
    required this.message,
    this.code,
    this.i18nKey,
    this.params
  });

  factory FieldError.fromJson(Map<String, dynamic> json) {
    return FieldError(
      field: (() {
        final value = json['field']?.toString();
        if (value == null) {
          throw FormatException('FieldError.field is required');
        }
        return value;
      })(),
      message: (() {
        final value = json['message']?.toString();
        if (value == null) {
          throw FormatException('FieldError.message is required');
        }
        return value;
      })(),
      code: json['code'] is int ? json['code'] : null,
      i18nKey: json['i18nKey']?.toString(),
      params: (() {
        final map = _sdkworkAsMap(json['params']);
        if (map == null) {
          return null;
        }
        final result = <String, String>{};
        map.forEach((key, item) {
          final deserialized = item?.toString();
          if (deserialized is String) {
            result[key] = deserialized;
          }
        });
        return result;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'field': field,
      'message': message,
      'code': code,
      'i18nKey': i18nKey,
      'params': params?.map((key, item) => MapEntry(key, item)),
    };
  }
}

class SdkWorkResourceResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SdkWorkResourceResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SdkWorkResourceResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkResourceResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SdkWorkResourceResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SdkWorkResourceResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SdkWorkListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SdkWorkListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SdkWorkListResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SdkWorkListResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SdkWorkListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SdkWorkCommandResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SdkWorkCommandResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SdkWorkCommandResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkCommandResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SdkWorkCommandResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SdkWorkCommandResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class CreateAppRequest {
  final String name;
  final String? slug;
  final String appKind;
  final String? ownerType;
  final String? description;
  final String? defaultEnvironment;
  final Map<String, dynamic>? metadata;
  final String? appDomainLabel;
  final List<String>? appDomainSuffixes;
  final List<AppSourceSpecDefinition>? sourceSpecs;
  final String? idempotencyKey;

  CreateAppRequest({
    required this.name,
    this.slug,
    required this.appKind,
    this.ownerType,
    this.description,
    this.defaultEnvironment,
    this.metadata,
    this.appDomainLabel,
    this.appDomainSuffixes,
    this.sourceSpecs,
    this.idempotencyKey
  });

  factory CreateAppRequest.fromJson(Map<String, dynamic> json) {
    return CreateAppRequest(
      name: (() {
        final value = json['name']?.toString();
        if (value == null) {
          throw FormatException('CreateAppRequest.name is required');
        }
        return value;
      })(),
      slug: json['slug']?.toString(),
      appKind: (() {
        final value = json['appKind']?.toString();
        if (value == null) {
          throw FormatException('CreateAppRequest.appKind is required');
        }
        return value;
      })(),
      ownerType: json['ownerType']?.toString(),
      description: json['description']?.toString(),
      defaultEnvironment: json['defaultEnvironment']?.toString(),
      metadata: _sdkworkAsMap(json['metadata']),
      appDomainLabel: json['appDomainLabel']?.toString(),
      appDomainSuffixes: (() {
        final list = _sdkworkAsList(json['appDomainSuffixes']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      sourceSpecs: (() {
        final list = _sdkworkAsList(json['sourceSpecs']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AppSourceSpecDefinition.fromJson(map);
      })())
            .whereType<AppSourceSpecDefinition>()
            .toList();
      })(),
      idempotencyKey: json['idempotencyKey']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'name': name,
      'slug': slug,
      'appKind': appKind,
      'ownerType': ownerType,
      'description': description,
      'defaultEnvironment': defaultEnvironment,
      'metadata': metadata,
      'appDomainLabel': appDomainLabel,
      'appDomainSuffixes': appDomainSuffixes?.map((item) => item).toList(),
      'sourceSpecs': sourceSpecs?.map((item) => item.toJson()).toList(),
      'idempotencyKey': idempotencyKey,
    };
  }
}

class AppResponse {
  final String id;
  final String name;
  final String slug;
  final String appKind;
  final String appStatus;
  final String? description;
  final int? type;
  final Map<String, dynamic>? runtimeConfig;
  final String? currentRevisionId;
  final String? desiredRevisionId;
  final String defaultEnvironment;
  final Map<String, dynamic>? metadata;
  final String? platformTargetCount;
  final String? appDomainLabel;
  final List<String>? appDomainSuffixes;
  final String? latestReleaseTag;
  final String ownerType;
  final String? ownerUserId;
  final String? ownerId;
  final String tenantId;
  final String? organizationId;
  final String? createdBy;
  final String? updatedBy;
  final String? activatedAt;
  final String? pausedAt;
  final String? archivedAt;
  final bool nginxConfigOverridden;
  final String createdAt;
  final String updatedAt;
  final String version;

  AppResponse({
    required this.id,
    required this.name,
    required this.slug,
    required this.appKind,
    required this.appStatus,
    this.description,
    this.type,
    this.runtimeConfig,
    this.currentRevisionId,
    this.desiredRevisionId,
    required this.defaultEnvironment,
    this.metadata,
    this.platformTargetCount,
    this.appDomainLabel,
    this.appDomainSuffixes,
    this.latestReleaseTag,
    required this.ownerType,
    this.ownerUserId,
    this.ownerId,
    required this.tenantId,
    this.organizationId,
    this.createdBy,
    this.updatedBy,
    this.activatedAt,
    this.pausedAt,
    this.archivedAt,
    required this.nginxConfigOverridden,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory AppResponse.fromJson(Map<String, dynamic> json) {
    return AppResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.id is required');
        }
        return value;
      })(),
      name: (() {
        final value = json['name']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.name is required');
        }
        return value;
      })(),
      slug: (() {
        final value = json['slug']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.slug is required');
        }
        return value;
      })(),
      appKind: (() {
        final value = json['appKind']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.appKind is required');
        }
        return value;
      })(),
      appStatus: (() {
        final value = json['appStatus']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.appStatus is required');
        }
        return value;
      })(),
      description: json['description']?.toString(),
      type: json['type'] is int ? json['type'] : null,
      runtimeConfig: _sdkworkAsMap(json['runtimeConfig']),
      currentRevisionId: json['currentRevisionId']?.toString(),
      desiredRevisionId: json['desiredRevisionId']?.toString(),
      defaultEnvironment: (() {
        final value = json['defaultEnvironment']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.defaultEnvironment is required');
        }
        return value;
      })(),
      metadata: _sdkworkAsMap(json['metadata']),
      platformTargetCount: json['platformTargetCount']?.toString(),
      appDomainLabel: json['appDomainLabel']?.toString(),
      appDomainSuffixes: (() {
        final list = _sdkworkAsList(json['appDomainSuffixes']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      latestReleaseTag: json['latestReleaseTag']?.toString(),
      ownerType: (() {
        final value = json['ownerType']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.ownerType is required');
        }
        return value;
      })(),
      ownerUserId: json['ownerUserId']?.toString(),
      ownerId: json['ownerId']?.toString(),
      tenantId: (() {
        final value = json['tenantId']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.tenantId is required');
        }
        return value;
      })(),
      organizationId: json['organizationId']?.toString(),
      createdBy: json['createdBy']?.toString(),
      updatedBy: json['updatedBy']?.toString(),
      activatedAt: json['activatedAt']?.toString(),
      pausedAt: json['pausedAt']?.toString(),
      archivedAt: json['archivedAt']?.toString(),
      nginxConfigOverridden: (() {
        final value = json['nginxConfigOverridden'];
        if (value is! bool) {
          throw FormatException('AppResponse.nginxConfigOverridden is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AppResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'name': name,
      'slug': slug,
      'appKind': appKind,
      'appStatus': appStatus,
      'description': description,
      'type': type,
      'runtimeConfig': runtimeConfig,
      'currentRevisionId': currentRevisionId,
      'desiredRevisionId': desiredRevisionId,
      'defaultEnvironment': defaultEnvironment,
      'metadata': metadata,
      'platformTargetCount': platformTargetCount,
      'appDomainLabel': appDomainLabel,
      'appDomainSuffixes': appDomainSuffixes?.map((item) => item).toList(),
      'latestReleaseTag': latestReleaseTag,
      'ownerType': ownerType,
      'ownerUserId': ownerUserId,
      'ownerId': ownerId,
      'tenantId': tenantId,
      'organizationId': organizationId,
      'createdBy': createdBy,
      'updatedBy': updatedBy,
      'activatedAt': activatedAt,
      'pausedAt': pausedAt,
      'archivedAt': archivedAt,
      'nginxConfigOverridden': nginxConfigOverridden,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class AppDomainResponse {
  final String hostname;
  final String kind;
  final String environment;
  final String bindingStatus;
  final String verificationStatus;
  final bool? isCanonical;
  final String? pathPrefix;
  final String? domainId;
  final String? dnsRecordName;
  final String? dnsRecordValue;
  final String? cnameTarget;

  AppDomainResponse({
    required this.hostname,
    required this.kind,
    required this.environment,
    required this.bindingStatus,
    required this.verificationStatus,
    this.isCanonical,
    this.pathPrefix,
    this.domainId,
    this.dnsRecordName,
    this.dnsRecordValue,
    this.cnameTarget
  });

  factory AppDomainResponse.fromJson(Map<String, dynamic> json) {
    return AppDomainResponse(
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('AppDomainResponse.hostname is required');
        }
        return value;
      })(),
      kind: (() {
        final value = json['kind']?.toString();
        if (value == null) {
          throw FormatException('AppDomainResponse.kind is required');
        }
        return value;
      })(),
      environment: (() {
        final value = json['environment']?.toString();
        if (value == null) {
          throw FormatException('AppDomainResponse.environment is required');
        }
        return value;
      })(),
      bindingStatus: (() {
        final value = json['bindingStatus']?.toString();
        if (value == null) {
          throw FormatException('AppDomainResponse.bindingStatus is required');
        }
        return value;
      })(),
      verificationStatus: (() {
        final value = json['verificationStatus']?.toString();
        if (value == null) {
          throw FormatException('AppDomainResponse.verificationStatus is required');
        }
        return value;
      })(),
      isCanonical: json['isCanonical'] is bool ? json['isCanonical'] : null,
      pathPrefix: json['pathPrefix']?.toString(),
      domainId: json['domainId']?.toString(),
      dnsRecordName: json['dnsRecordName']?.toString(),
      dnsRecordValue: json['dnsRecordValue']?.toString(),
      cnameTarget: json['cnameTarget']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'hostname': hostname,
      'kind': kind,
      'environment': environment,
      'bindingStatus': bindingStatus,
      'verificationStatus': verificationStatus,
      'isCanonical': isCanonical,
      'pathPrefix': pathPrefix,
      'domainId': domainId,
      'dnsRecordName': dnsRecordName,
      'dnsRecordValue': dnsRecordValue,
      'cnameTarget': cnameTarget,
    };
  }
}

class UpdateAppRequest {
  final String? name;
  final String? description;
  final String? ownerType;
  final String? ownerUserId;
  final String? appStatus;
  final String? defaultEnvironment;
  final Map<String, dynamic>? metadata;
  final String? appDomainLabel;
  final List<String>? appDomainSuffixes;

  UpdateAppRequest({
    this.name,
    this.description,
    this.ownerType,
    this.ownerUserId,
    this.appStatus,
    this.defaultEnvironment,
    this.metadata,
    this.appDomainLabel,
    this.appDomainSuffixes
  });

  factory UpdateAppRequest.fromJson(Map<String, dynamic> json) {
    return UpdateAppRequest(
      name: json['name']?.toString(),
      description: json['description']?.toString(),
      ownerType: json['ownerType']?.toString(),
      ownerUserId: json['ownerUserId']?.toString(),
      appStatus: json['appStatus']?.toString(),
      defaultEnvironment: json['defaultEnvironment']?.toString(),
      metadata: _sdkworkAsMap(json['metadata']),
      appDomainLabel: json['appDomainLabel']?.toString(),
      appDomainSuffixes: (() {
        final list = _sdkworkAsList(json['appDomainSuffixes']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'name': name,
      'description': description,
      'ownerType': ownerType,
      'ownerUserId': ownerUserId,
      'appStatus': appStatus,
      'defaultEnvironment': defaultEnvironment,
      'metadata': metadata,
      'appDomainLabel': appDomainLabel,
      'appDomainSuffixes': appDomainSuffixes?.map((item) => item).toList(),
    };
  }
}

class CreatePlatformTargetRequest {
  final String targetKey;
  final String platform;
  final String? techStack;
  final String? bundleId;
  final String? packageName;
  final String? appId;
  final String? bundleName;
  final String? buildTemplateId;
  final List<String>? allowedChannels;
  final String? idempotencyKey;

  CreatePlatformTargetRequest({
    required this.targetKey,
    required this.platform,
    this.techStack,
    this.bundleId,
    this.packageName,
    this.appId,
    this.bundleName,
    this.buildTemplateId,
    this.allowedChannels,
    this.idempotencyKey
  });

  factory CreatePlatformTargetRequest.fromJson(Map<String, dynamic> json) {
    return CreatePlatformTargetRequest(
      targetKey: (() {
        final value = json['targetKey']?.toString();
        if (value == null) {
          throw FormatException('CreatePlatformTargetRequest.targetKey is required');
        }
        return value;
      })(),
      platform: (() {
        final value = json['platform']?.toString();
        if (value == null) {
          throw FormatException('CreatePlatformTargetRequest.platform is required');
        }
        return value;
      })(),
      techStack: json['techStack']?.toString(),
      bundleId: json['bundleId']?.toString(),
      packageName: json['packageName']?.toString(),
      appId: json['appId']?.toString(),
      bundleName: json['bundleName']?.toString(),
      buildTemplateId: json['buildTemplateId']?.toString(),
      allowedChannels: (() {
        final list = _sdkworkAsList(json['allowedChannels']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      idempotencyKey: json['idempotencyKey']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'targetKey': targetKey,
      'platform': platform,
      'techStack': techStack,
      'bundleId': bundleId,
      'packageName': packageName,
      'appId': appId,
      'bundleName': bundleName,
      'buildTemplateId': buildTemplateId,
      'allowedChannels': allowedChannels?.map((item) => item).toList(),
      'idempotencyKey': idempotencyKey,
    };
  }
}

class PlatformTargetResponse {
  final String id;
  final String appId;
  final String targetKey;
  final String platform;
  final String techStack;
  final String? bundleId;
  final String? packageName;
  final String? appIdValue;
  final String? bundleName;
  final String? buildTemplateId;
  final List<String> allowedChannels;
  final String targetStatus;
  final String createdAt;
  final String updatedAt;
  final String version;

  PlatformTargetResponse({
    required this.id,
    required this.appId,
    required this.targetKey,
    required this.platform,
    required this.techStack,
    this.bundleId,
    this.packageName,
    this.appIdValue,
    this.bundleName,
    this.buildTemplateId,
    required this.allowedChannels,
    required this.targetStatus,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory PlatformTargetResponse.fromJson(Map<String, dynamic> json) {
    return PlatformTargetResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetResponse.id is required');
        }
        return value;
      })(),
      appId: (() {
        final value = json['appId']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetResponse.appId is required');
        }
        return value;
      })(),
      targetKey: (() {
        final value = json['targetKey']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetResponse.targetKey is required');
        }
        return value;
      })(),
      platform: (() {
        final value = json['platform']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetResponse.platform is required');
        }
        return value;
      })(),
      techStack: (() {
        final value = json['techStack']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetResponse.techStack is required');
        }
        return value;
      })(),
      bundleId: json['bundleId']?.toString(),
      packageName: json['packageName']?.toString(),
      appIdValue: json['appIdValue']?.toString(),
      bundleName: json['bundleName']?.toString(),
      buildTemplateId: json['buildTemplateId']?.toString(),
      allowedChannels: (() {
        final list = _sdkworkAsList(json['allowedChannels']);
        if (list == null) {
          throw FormatException('PlatformTargetResponse.allowedChannels is required');
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      targetStatus: (() {
        final value = json['targetStatus']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetResponse.targetStatus is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'targetKey': targetKey,
      'platform': platform,
      'techStack': techStack,
      'bundleId': bundleId,
      'packageName': packageName,
      'appIdValue': appIdValue,
      'bundleName': bundleName,
      'buildTemplateId': buildTemplateId,
      'allowedChannels': allowedChannels.map((item) => item).toList(),
      'targetStatus': targetStatus,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class CreateSourceRepositoryRequest {
  final String repoKey;
  final String repoProvider;
  final String repoUrl;
  final String? defaultBranch;
  final String? cloneMode;
  final String? credentialSecretRef;
  final String? idempotencyKey;

  CreateSourceRepositoryRequest({
    required this.repoKey,
    required this.repoProvider,
    required this.repoUrl,
    this.defaultBranch,
    this.cloneMode,
    this.credentialSecretRef,
    this.idempotencyKey
  });

  factory CreateSourceRepositoryRequest.fromJson(Map<String, dynamic> json) {
    return CreateSourceRepositoryRequest(
      repoKey: (() {
        final value = json['repoKey']?.toString();
        if (value == null) {
          throw FormatException('CreateSourceRepositoryRequest.repoKey is required');
        }
        return value;
      })(),
      repoProvider: (() {
        final value = json['repoProvider']?.toString();
        if (value == null) {
          throw FormatException('CreateSourceRepositoryRequest.repoProvider is required');
        }
        return value;
      })(),
      repoUrl: (() {
        final value = json['repoUrl']?.toString();
        if (value == null) {
          throw FormatException('CreateSourceRepositoryRequest.repoUrl is required');
        }
        return value;
      })(),
      defaultBranch: json['defaultBranch']?.toString(),
      cloneMode: json['cloneMode']?.toString(),
      credentialSecretRef: json['credentialSecretRef']?.toString(),
      idempotencyKey: json['idempotencyKey']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'repoKey': repoKey,
      'repoProvider': repoProvider,
      'repoUrl': repoUrl,
      'defaultBranch': defaultBranch,
      'cloneMode': cloneMode,
      'credentialSecretRef': credentialSecretRef,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class SourceRepositoryResponse {
  final String id;
  final String appId;
  final String repoKey;
  final String repoProvider;
  final String repoUrl;
  final String defaultBranch;
  final String cloneMode;
  final String? credentialSecretRef;
  final String repoStatus;
  final String? lastErrorCode;
  final String createdAt;
  final String updatedAt;
  final String version;

  SourceRepositoryResponse({
    required this.id,
    required this.appId,
    required this.repoKey,
    required this.repoProvider,
    required this.repoUrl,
    required this.defaultBranch,
    required this.cloneMode,
    this.credentialSecretRef,
    required this.repoStatus,
    this.lastErrorCode,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory SourceRepositoryResponse.fromJson(Map<String, dynamic> json) {
    return SourceRepositoryResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.id is required');
        }
        return value;
      })(),
      appId: (() {
        final value = json['appId']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.appId is required');
        }
        return value;
      })(),
      repoKey: (() {
        final value = json['repoKey']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.repoKey is required');
        }
        return value;
      })(),
      repoProvider: (() {
        final value = json['repoProvider']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.repoProvider is required');
        }
        return value;
      })(),
      repoUrl: (() {
        final value = json['repoUrl']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.repoUrl is required');
        }
        return value;
      })(),
      defaultBranch: (() {
        final value = json['defaultBranch']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.defaultBranch is required');
        }
        return value;
      })(),
      cloneMode: (() {
        final value = json['cloneMode']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.cloneMode is required');
        }
        return value;
      })(),
      credentialSecretRef: json['credentialSecretRef']?.toString(),
      repoStatus: (() {
        final value = json['repoStatus']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.repoStatus is required');
        }
        return value;
      })(),
      lastErrorCode: json['lastErrorCode']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoryResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'repoKey': repoKey,
      'repoProvider': repoProvider,
      'repoUrl': repoUrl,
      'defaultBranch': defaultBranch,
      'cloneMode': cloneMode,
      'credentialSecretRef': credentialSecretRef,
      'repoStatus': repoStatus,
      'lastErrorCode': lastErrorCode,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class CreateBuildTemplateRequest {
  final String templateName;
  final String templateVersion;
  final String platform;
  final String? techStack;
  final Map<String, dynamic>? toolchain;
  final List<String>? commands;
  final List<String>? artifactOutputs;
  final Map<String, dynamic>? qualityGates;
  final String? idempotencyKey;

  CreateBuildTemplateRequest({
    required this.templateName,
    required this.templateVersion,
    required this.platform,
    this.techStack,
    this.toolchain,
    this.commands,
    this.artifactOutputs,
    this.qualityGates,
    this.idempotencyKey
  });

  factory CreateBuildTemplateRequest.fromJson(Map<String, dynamic> json) {
    return CreateBuildTemplateRequest(
      templateName: (() {
        final value = json['templateName']?.toString();
        if (value == null) {
          throw FormatException('CreateBuildTemplateRequest.templateName is required');
        }
        return value;
      })(),
      templateVersion: (() {
        final value = json['templateVersion']?.toString();
        if (value == null) {
          throw FormatException('CreateBuildTemplateRequest.templateVersion is required');
        }
        return value;
      })(),
      platform: (() {
        final value = json['platform']?.toString();
        if (value == null) {
          throw FormatException('CreateBuildTemplateRequest.platform is required');
        }
        return value;
      })(),
      techStack: json['techStack']?.toString(),
      toolchain: _sdkworkAsMap(json['toolchain']),
      commands: (() {
        final list = _sdkworkAsList(json['commands']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      artifactOutputs: (() {
        final list = _sdkworkAsList(json['artifactOutputs']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      qualityGates: _sdkworkAsMap(json['qualityGates']),
      idempotencyKey: json['idempotencyKey']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'templateName': templateName,
      'templateVersion': templateVersion,
      'platform': platform,
      'techStack': techStack,
      'toolchain': toolchain,
      'commands': commands?.map((item) => item).toList(),
      'artifactOutputs': artifactOutputs?.map((item) => item).toList(),
      'qualityGates': qualityGates,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class BuildTemplateResponse {
  final String id;
  final String templateName;
  final String templateVersion;
  final String platform;
  final String techStack;
  final Map<String, dynamic>? toolchain;
  final List<String>? commands;
  final List<String>? artifactOutputs;
  final Map<String, dynamic>? qualityGates;
  final String templateStatus;
  final String createdAt;
  final String updatedAt;
  final String version;

  BuildTemplateResponse({
    required this.id,
    required this.templateName,
    required this.templateVersion,
    required this.platform,
    required this.techStack,
    this.toolchain,
    this.commands,
    this.artifactOutputs,
    this.qualityGates,
    required this.templateStatus,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory BuildTemplateResponse.fromJson(Map<String, dynamic> json) {
    return BuildTemplateResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplateResponse.id is required');
        }
        return value;
      })(),
      templateName: (() {
        final value = json['templateName']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplateResponse.templateName is required');
        }
        return value;
      })(),
      templateVersion: (() {
        final value = json['templateVersion']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplateResponse.templateVersion is required');
        }
        return value;
      })(),
      platform: (() {
        final value = json['platform']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplateResponse.platform is required');
        }
        return value;
      })(),
      techStack: (() {
        final value = json['techStack']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplateResponse.techStack is required');
        }
        return value;
      })(),
      toolchain: _sdkworkAsMap(json['toolchain']),
      commands: (() {
        final list = _sdkworkAsList(json['commands']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      artifactOutputs: (() {
        final list = _sdkworkAsList(json['artifactOutputs']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      qualityGates: _sdkworkAsMap(json['qualityGates']),
      templateStatus: (() {
        final value = json['templateStatus']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplateResponse.templateStatus is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplateResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplateResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplateResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'templateName': templateName,
      'templateVersion': templateVersion,
      'platform': platform,
      'techStack': techStack,
      'toolchain': toolchain,
      'commands': commands?.map((item) => item).toList(),
      'artifactOutputs': artifactOutputs?.map((item) => item).toList(),
      'qualityGates': qualityGates,
      'templateStatus': templateStatus,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class CreateBuildRequest {
  final String platformTargetId;
  final String? sourceRepositoryId;
  final String? sourceRef;
  final String? templateId;
  final String? semanticVersion;
  final String idempotencyKey;

  CreateBuildRequest({
    required this.platformTargetId,
    this.sourceRepositoryId,
    this.sourceRef,
    this.templateId,
    this.semanticVersion,
    required this.idempotencyKey
  });

  factory CreateBuildRequest.fromJson(Map<String, dynamic> json) {
    return CreateBuildRequest(
      platformTargetId: (() {
        final value = json['platformTargetId']?.toString();
        if (value == null) {
          throw FormatException('CreateBuildRequest.platformTargetId is required');
        }
        return value;
      })(),
      sourceRepositoryId: json['sourceRepositoryId']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      templateId: json['templateId']?.toString(),
      semanticVersion: json['semanticVersion']?.toString(),
      idempotencyKey: (() {
        final value = json['idempotencyKey']?.toString();
        if (value == null) {
          throw FormatException('CreateBuildRequest.idempotencyKey is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'platformTargetId': platformTargetId,
      'sourceRepositoryId': sourceRepositoryId,
      'sourceRef': sourceRef,
      'templateId': templateId,
      'semanticVersion': semanticVersion,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class BuildResponse {
  final String id;
  final String appId;
  final String platformTargetId;
  final String? templateId;
  final String buildNumber;
  final String? sourceRepositoryId;
  final String? sourceRef;
  final Map<String, dynamic>? sourceSnapshot;
  final String buildStatus;
  final String? logRef;
  final String? producedPackageId;
  final Map<String, dynamic>? qualityGate;
  final String? runnerNodeUuid;
  final String? errorCode;
  final String? startedAt;
  final String? finishedAt;
  final String? durationMs;
  final String createdAt;
  final String updatedAt;
  final String version;

  BuildResponse({
    required this.id,
    required this.appId,
    required this.platformTargetId,
    this.templateId,
    required this.buildNumber,
    this.sourceRepositoryId,
    this.sourceRef,
    this.sourceSnapshot,
    required this.buildStatus,
    this.logRef,
    this.producedPackageId,
    this.qualityGate,
    this.runnerNodeUuid,
    this.errorCode,
    this.startedAt,
    this.finishedAt,
    this.durationMs,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory BuildResponse.fromJson(Map<String, dynamic> json) {
    return BuildResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('BuildResponse.id is required');
        }
        return value;
      })(),
      appId: (() {
        final value = json['appId']?.toString();
        if (value == null) {
          throw FormatException('BuildResponse.appId is required');
        }
        return value;
      })(),
      platformTargetId: (() {
        final value = json['platformTargetId']?.toString();
        if (value == null) {
          throw FormatException('BuildResponse.platformTargetId is required');
        }
        return value;
      })(),
      templateId: json['templateId']?.toString(),
      buildNumber: (() {
        final value = json['buildNumber']?.toString();
        if (value == null) {
          throw FormatException('BuildResponse.buildNumber is required');
        }
        return value;
      })(),
      sourceRepositoryId: json['sourceRepositoryId']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      sourceSnapshot: _sdkworkAsMap(json['sourceSnapshot']),
      buildStatus: (() {
        final value = json['buildStatus']?.toString();
        if (value == null) {
          throw FormatException('BuildResponse.buildStatus is required');
        }
        return value;
      })(),
      logRef: json['logRef']?.toString(),
      producedPackageId: json['producedPackageId']?.toString(),
      qualityGate: _sdkworkAsMap(json['qualityGate']),
      runnerNodeUuid: json['runnerNodeUuid']?.toString(),
      errorCode: json['errorCode']?.toString(),
      startedAt: json['startedAt']?.toString(),
      finishedAt: json['finishedAt']?.toString(),
      durationMs: json['durationMs']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('BuildResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('BuildResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('BuildResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'platformTargetId': platformTargetId,
      'templateId': templateId,
      'buildNumber': buildNumber,
      'sourceRepositoryId': sourceRepositoryId,
      'sourceRef': sourceRef,
      'sourceSnapshot': sourceSnapshot,
      'buildStatus': buildStatus,
      'logRef': logRef,
      'producedPackageId': producedPackageId,
      'qualityGate': qualityGate,
      'runnerNodeUuid': runnerNodeUuid,
      'errorCode': errorCode,
      'startedAt': startedAt,
      'finishedAt': finishedAt,
      'durationMs': durationMs,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class UpdateBuildStateRequest {
  final String buildStatus;
  final String runnerNodeUuid;
  final String? runnerVersion;
  final String? logRef;
  final Map<String, dynamic>? sourceSnapshot;
  final Map<String, dynamic>? qualityGate;
  final String? errorCode;
  final String? startedAt;
  final String? finishedAt;

  UpdateBuildStateRequest({
    required this.buildStatus,
    required this.runnerNodeUuid,
    this.runnerVersion,
    this.logRef,
    this.sourceSnapshot,
    this.qualityGate,
    this.errorCode,
    this.startedAt,
    this.finishedAt
  });

  factory UpdateBuildStateRequest.fromJson(Map<String, dynamic> json) {
    return UpdateBuildStateRequest(
      buildStatus: (() {
        final value = json['buildStatus']?.toString();
        if (value == null) {
          throw FormatException('UpdateBuildStateRequest.buildStatus is required');
        }
        return value;
      })(),
      runnerNodeUuid: (() {
        final value = json['runnerNodeUuid']?.toString();
        if (value == null) {
          throw FormatException('UpdateBuildStateRequest.runnerNodeUuid is required');
        }
        return value;
      })(),
      runnerVersion: json['runnerVersion']?.toString(),
      logRef: json['logRef']?.toString(),
      sourceSnapshot: _sdkworkAsMap(json['sourceSnapshot']),
      qualityGate: _sdkworkAsMap(json['qualityGate']),
      errorCode: json['errorCode']?.toString(),
      startedAt: json['startedAt']?.toString(),
      finishedAt: json['finishedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'buildStatus': buildStatus,
      'runnerNodeUuid': runnerNodeUuid,
      'runnerVersion': runnerVersion,
      'logRef': logRef,
      'sourceSnapshot': sourceSnapshot,
      'qualityGate': qualityGate,
      'errorCode': errorCode,
      'startedAt': startedAt,
      'finishedAt': finishedAt,
    };
  }
}

class RegisterPackageRequest {
  final String platformTargetId;
  final String buildId;
  final String packageFormat;
  final String semanticVersion;
  final String packageSizeBytes;
  final String checksumSha256;
  final String manifestSha256;
  final String driveNodeId;
  final String? driveSpaceId;
  final String? signingIdentityId;
  final String? minPlatformVersion;
  final List<String>? architectures;
  final Map<String, dynamic>? bundleIdentity;
  final Map<String, dynamic>? validationReport;
  final String? idempotencyKey;

  RegisterPackageRequest({
    required this.platformTargetId,
    required this.buildId,
    required this.packageFormat,
    required this.semanticVersion,
    required this.packageSizeBytes,
    required this.checksumSha256,
    required this.manifestSha256,
    required this.driveNodeId,
    this.driveSpaceId,
    this.signingIdentityId,
    this.minPlatformVersion,
    this.architectures,
    this.bundleIdentity,
    this.validationReport,
    this.idempotencyKey
  });

  factory RegisterPackageRequest.fromJson(Map<String, dynamic> json) {
    return RegisterPackageRequest(
      platformTargetId: (() {
        final value = json['platformTargetId']?.toString();
        if (value == null) {
          throw FormatException('RegisterPackageRequest.platformTargetId is required');
        }
        return value;
      })(),
      buildId: (() {
        final value = json['buildId']?.toString();
        if (value == null) {
          throw FormatException('RegisterPackageRequest.buildId is required');
        }
        return value;
      })(),
      packageFormat: (() {
        final value = json['packageFormat']?.toString();
        if (value == null) {
          throw FormatException('RegisterPackageRequest.packageFormat is required');
        }
        return value;
      })(),
      semanticVersion: (() {
        final value = json['semanticVersion']?.toString();
        if (value == null) {
          throw FormatException('RegisterPackageRequest.semanticVersion is required');
        }
        return value;
      })(),
      packageSizeBytes: (() {
        final value = json['packageSizeBytes']?.toString();
        if (value == null) {
          throw FormatException('RegisterPackageRequest.packageSizeBytes is required');
        }
        return value;
      })(),
      checksumSha256: (() {
        final value = json['checksumSha256']?.toString();
        if (value == null) {
          throw FormatException('RegisterPackageRequest.checksumSha256 is required');
        }
        return value;
      })(),
      manifestSha256: (() {
        final value = json['manifestSha256']?.toString();
        if (value == null) {
          throw FormatException('RegisterPackageRequest.manifestSha256 is required');
        }
        return value;
      })(),
      driveNodeId: (() {
        final value = json['driveNodeId']?.toString();
        if (value == null) {
          throw FormatException('RegisterPackageRequest.driveNodeId is required');
        }
        return value;
      })(),
      driveSpaceId: json['driveSpaceId']?.toString(),
      signingIdentityId: json['signingIdentityId']?.toString(),
      minPlatformVersion: json['minPlatformVersion']?.toString(),
      architectures: (() {
        final list = _sdkworkAsList(json['architectures']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      bundleIdentity: _sdkworkAsMap(json['bundleIdentity']),
      validationReport: _sdkworkAsMap(json['validationReport']),
      idempotencyKey: json['idempotencyKey']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'platformTargetId': platformTargetId,
      'buildId': buildId,
      'packageFormat': packageFormat,
      'semanticVersion': semanticVersion,
      'packageSizeBytes': packageSizeBytes,
      'checksumSha256': checksumSha256,
      'manifestSha256': manifestSha256,
      'driveNodeId': driveNodeId,
      'driveSpaceId': driveSpaceId,
      'signingIdentityId': signingIdentityId,
      'minPlatformVersion': minPlatformVersion,
      'architectures': architectures?.map((item) => item).toList(),
      'bundleIdentity': bundleIdentity,
      'validationReport': validationReport,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class PackageResponse {
  final String id;
  final String appId;
  final String platformTargetId;
  final String buildId;
  final String packageFormat;
  final String semanticVersion;
  final String packageSizeBytes;
  final String checksumSha256;
  final String manifestSha256;
  final String? driveNodeId;
  final String? signingIdentityId;
  final String? minPlatformVersion;
  final List<String>? architectures;
  final String packageStatus;
  final String createdAt;
  final String updatedAt;
  final String version;

  PackageResponse({
    required this.id,
    required this.appId,
    required this.platformTargetId,
    required this.buildId,
    required this.packageFormat,
    required this.semanticVersion,
    required this.packageSizeBytes,
    required this.checksumSha256,
    required this.manifestSha256,
    this.driveNodeId,
    this.signingIdentityId,
    this.minPlatformVersion,
    this.architectures,
    required this.packageStatus,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory PackageResponse.fromJson(Map<String, dynamic> json) {
    return PackageResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.id is required');
        }
        return value;
      })(),
      appId: (() {
        final value = json['appId']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.appId is required');
        }
        return value;
      })(),
      platformTargetId: (() {
        final value = json['platformTargetId']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.platformTargetId is required');
        }
        return value;
      })(),
      buildId: (() {
        final value = json['buildId']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.buildId is required');
        }
        return value;
      })(),
      packageFormat: (() {
        final value = json['packageFormat']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.packageFormat is required');
        }
        return value;
      })(),
      semanticVersion: (() {
        final value = json['semanticVersion']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.semanticVersion is required');
        }
        return value;
      })(),
      packageSizeBytes: (() {
        final value = json['packageSizeBytes']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.packageSizeBytes is required');
        }
        return value;
      })(),
      checksumSha256: (() {
        final value = json['checksumSha256']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.checksumSha256 is required');
        }
        return value;
      })(),
      manifestSha256: (() {
        final value = json['manifestSha256']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.manifestSha256 is required');
        }
        return value;
      })(),
      driveNodeId: json['driveNodeId']?.toString(),
      signingIdentityId: json['signingIdentityId']?.toString(),
      minPlatformVersion: json['minPlatformVersion']?.toString(),
      architectures: (() {
        final list = _sdkworkAsList(json['architectures']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      packageStatus: (() {
        final value = json['packageStatus']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.packageStatus is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('PackageResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'platformTargetId': platformTargetId,
      'buildId': buildId,
      'packageFormat': packageFormat,
      'semanticVersion': semanticVersion,
      'packageSizeBytes': packageSizeBytes,
      'checksumSha256': checksumSha256,
      'manifestSha256': manifestSha256,
      'driveNodeId': driveNodeId,
      'signingIdentityId': signingIdentityId,
      'minPlatformVersion': minPlatformVersion,
      'architectures': architectures?.map((item) => item).toList(),
      'packageStatus': packageStatus,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class CreateAppReleaseRequest {
  final String platformTargetId;
  final String packageId;
  final String semanticVersion;
  final Map<String, dynamic>? releaseNotes;
  final String? releaseStatus;
  final String idempotencyKey;

  CreateAppReleaseRequest({
    required this.platformTargetId,
    required this.packageId,
    required this.semanticVersion,
    this.releaseNotes,
    this.releaseStatus,
    required this.idempotencyKey
  });

  factory CreateAppReleaseRequest.fromJson(Map<String, dynamic> json) {
    return CreateAppReleaseRequest(
      platformTargetId: (() {
        final value = json['platformTargetId']?.toString();
        if (value == null) {
          throw FormatException('CreateAppReleaseRequest.platformTargetId is required');
        }
        return value;
      })(),
      packageId: (() {
        final value = json['packageId']?.toString();
        if (value == null) {
          throw FormatException('CreateAppReleaseRequest.packageId is required');
        }
        return value;
      })(),
      semanticVersion: (() {
        final value = json['semanticVersion']?.toString();
        if (value == null) {
          throw FormatException('CreateAppReleaseRequest.semanticVersion is required');
        }
        return value;
      })(),
      releaseNotes: _sdkworkAsMap(json['releaseNotes']),
      releaseStatus: json['releaseStatus']?.toString(),
      idempotencyKey: (() {
        final value = json['idempotencyKey']?.toString();
        if (value == null) {
          throw FormatException('CreateAppReleaseRequest.idempotencyKey is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'platformTargetId': platformTargetId,
      'packageId': packageId,
      'semanticVersion': semanticVersion,
      'releaseNotes': releaseNotes,
      'releaseStatus': releaseStatus,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class AppReleaseResponse {
  final String id;
  final String appId;
  final String platformTargetId;
  final String packageId;
  final String semanticVersion;
  final String buildNumber;
  final String releaseStatus;
  final Map<String, dynamic>? releaseNotes;
  final String createdAt;
  final String updatedAt;
  final String version;

  AppReleaseResponse({
    required this.id,
    required this.appId,
    required this.platformTargetId,
    required this.packageId,
    required this.semanticVersion,
    required this.buildNumber,
    required this.releaseStatus,
    this.releaseNotes,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory AppReleaseResponse.fromJson(Map<String, dynamic> json) {
    return AppReleaseResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppReleaseResponse.id is required');
        }
        return value;
      })(),
      appId: (() {
        final value = json['appId']?.toString();
        if (value == null) {
          throw FormatException('AppReleaseResponse.appId is required');
        }
        return value;
      })(),
      platformTargetId: (() {
        final value = json['platformTargetId']?.toString();
        if (value == null) {
          throw FormatException('AppReleaseResponse.platformTargetId is required');
        }
        return value;
      })(),
      packageId: (() {
        final value = json['packageId']?.toString();
        if (value == null) {
          throw FormatException('AppReleaseResponse.packageId is required');
        }
        return value;
      })(),
      semanticVersion: (() {
        final value = json['semanticVersion']?.toString();
        if (value == null) {
          throw FormatException('AppReleaseResponse.semanticVersion is required');
        }
        return value;
      })(),
      buildNumber: (() {
        final value = json['buildNumber']?.toString();
        if (value == null) {
          throw FormatException('AppReleaseResponse.buildNumber is required');
        }
        return value;
      })(),
      releaseStatus: (() {
        final value = json['releaseStatus']?.toString();
        if (value == null) {
          throw FormatException('AppReleaseResponse.releaseStatus is required');
        }
        return value;
      })(),
      releaseNotes: _sdkworkAsMap(json['releaseNotes']),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('AppReleaseResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('AppReleaseResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AppReleaseResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'platformTargetId': platformTargetId,
      'packageId': packageId,
      'semanticVersion': semanticVersion,
      'buildNumber': buildNumber,
      'releaseStatus': releaseStatus,
      'releaseNotes': releaseNotes,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class PromoteChannelRequest {
  final String releaseId;
  final String? strategy;
  final int? percentage;
  final String? idempotencyKey;

  PromoteChannelRequest({
    required this.releaseId,
    this.strategy,
    this.percentage,
    this.idempotencyKey
  });

  factory PromoteChannelRequest.fromJson(Map<String, dynamic> json) {
    return PromoteChannelRequest(
      releaseId: (() {
        final value = json['releaseId']?.toString();
        if (value == null) {
          throw FormatException('PromoteChannelRequest.releaseId is required');
        }
        return value;
      })(),
      strategy: json['strategy']?.toString(),
      percentage: json['percentage'] is int ? json['percentage'] : null,
      idempotencyKey: json['idempotencyKey']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'releaseId': releaseId,
      'strategy': strategy,
      'percentage': percentage,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class ChannelResponse {
  final String id;
  final String appId;
  final String platformTargetId;
  final String channelKey;
  final String? currentReleaseId;
  final String? currentReleaseVersion;
  final String channelStatus;
  final String updatedAt;
  final String version;

  ChannelResponse({
    required this.id,
    required this.appId,
    required this.platformTargetId,
    required this.channelKey,
    this.currentReleaseId,
    this.currentReleaseVersion,
    required this.channelStatus,
    required this.updatedAt,
    required this.version
  });

  factory ChannelResponse.fromJson(Map<String, dynamic> json) {
    return ChannelResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('ChannelResponse.id is required');
        }
        return value;
      })(),
      appId: (() {
        final value = json['appId']?.toString();
        if (value == null) {
          throw FormatException('ChannelResponse.appId is required');
        }
        return value;
      })(),
      platformTargetId: (() {
        final value = json['platformTargetId']?.toString();
        if (value == null) {
          throw FormatException('ChannelResponse.platformTargetId is required');
        }
        return value;
      })(),
      channelKey: (() {
        final value = json['channelKey']?.toString();
        if (value == null) {
          throw FormatException('ChannelResponse.channelKey is required');
        }
        return value;
      })(),
      currentReleaseId: json['currentReleaseId']?.toString(),
      currentReleaseVersion: json['currentReleaseVersion']?.toString(),
      channelStatus: (() {
        final value = json['channelStatus']?.toString();
        if (value == null) {
          throw FormatException('ChannelResponse.channelStatus is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('ChannelResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('ChannelResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'platformTargetId': platformTargetId,
      'channelKey': channelKey,
      'currentReleaseId': currentReleaseId,
      'currentReleaseVersion': currentReleaseVersion,
      'channelStatus': channelStatus,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class ChannelRolloutResponse {
  final String id;
  final String channelId;
  final String releaseId;
  final String releaseVersion;
  final String strategy;
  final int? percentage;
  final String rolloutStatus;
  final String? supersedesRolloutId;
  final String requestedAt;
  final String? completedAt;

  ChannelRolloutResponse({
    required this.id,
    required this.channelId,
    required this.releaseId,
    required this.releaseVersion,
    required this.strategy,
    this.percentage,
    required this.rolloutStatus,
    this.supersedesRolloutId,
    required this.requestedAt,
    this.completedAt
  });

  factory ChannelRolloutResponse.fromJson(Map<String, dynamic> json) {
    return ChannelRolloutResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('ChannelRolloutResponse.id is required');
        }
        return value;
      })(),
      channelId: (() {
        final value = json['channelId']?.toString();
        if (value == null) {
          throw FormatException('ChannelRolloutResponse.channelId is required');
        }
        return value;
      })(),
      releaseId: (() {
        final value = json['releaseId']?.toString();
        if (value == null) {
          throw FormatException('ChannelRolloutResponse.releaseId is required');
        }
        return value;
      })(),
      releaseVersion: (() {
        final value = json['releaseVersion']?.toString();
        if (value == null) {
          throw FormatException('ChannelRolloutResponse.releaseVersion is required');
        }
        return value;
      })(),
      strategy: (() {
        final value = json['strategy']?.toString();
        if (value == null) {
          throw FormatException('ChannelRolloutResponse.strategy is required');
        }
        return value;
      })(),
      percentage: json['percentage'] is int ? json['percentage'] : null,
      rolloutStatus: (() {
        final value = json['rolloutStatus']?.toString();
        if (value == null) {
          throw FormatException('ChannelRolloutResponse.rolloutStatus is required');
        }
        return value;
      })(),
      supersedesRolloutId: json['supersedesRolloutId']?.toString(),
      requestedAt: (() {
        final value = json['requestedAt']?.toString();
        if (value == null) {
          throw FormatException('ChannelRolloutResponse.requestedAt is required');
        }
        return value;
      })(),
      completedAt: json['completedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'channelId': channelId,
      'releaseId': releaseId,
      'releaseVersion': releaseVersion,
      'strategy': strategy,
      'percentage': percentage,
      'rolloutStatus': rolloutStatus,
      'supersedesRolloutId': supersedesRolloutId,
      'requestedAt': requestedAt,
      'completedAt': completedAt,
    };
  }
}

class CreateAppDeploymentRequest {
  final String platformTargetId;
  final String releaseId;
  final String deploymentKind;
  final String deploymentTarget;
  final String? environment;
  final String? strategy;
  final int? percentage;
  final String idempotencyKey;

  CreateAppDeploymentRequest({
    required this.platformTargetId,
    required this.releaseId,
    required this.deploymentKind,
    required this.deploymentTarget,
    this.environment,
    this.strategy,
    this.percentage,
    required this.idempotencyKey
  });

  factory CreateAppDeploymentRequest.fromJson(Map<String, dynamic> json) {
    return CreateAppDeploymentRequest(
      platformTargetId: (() {
        final value = json['platformTargetId']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDeploymentRequest.platformTargetId is required');
        }
        return value;
      })(),
      releaseId: (() {
        final value = json['releaseId']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDeploymentRequest.releaseId is required');
        }
        return value;
      })(),
      deploymentKind: (() {
        final value = json['deploymentKind']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDeploymentRequest.deploymentKind is required');
        }
        return value;
      })(),
      deploymentTarget: (() {
        final value = json['deploymentTarget']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDeploymentRequest.deploymentTarget is required');
        }
        return value;
      })(),
      environment: json['environment']?.toString(),
      strategy: json['strategy']?.toString(),
      percentage: json['percentage'] is int ? json['percentage'] : null,
      idempotencyKey: (() {
        final value = json['idempotencyKey']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDeploymentRequest.idempotencyKey is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'platformTargetId': platformTargetId,
      'releaseId': releaseId,
      'deploymentKind': deploymentKind,
      'deploymentTarget': deploymentTarget,
      'environment': environment,
      'strategy': strategy,
      'percentage': percentage,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class AppDeploymentResponse {
  final String id;
  final String appId;
  final String? platformTargetId;
  final String? siteId;
  final String? releaseId;
  final String? deploymentKind;
  final String? deploymentTarget;
  final String environment;
  final String? strategy;
  final int? percentage;
  final String? platformReviewRef;
  final String deploymentStatus;
  final String? rollbackFromDeploymentId;
  final String? startedAt;
  final String? completedAt;
  final String createdAt;
  final String updatedAt;
  final String version;

  AppDeploymentResponse({
    required this.id,
    required this.appId,
    this.platformTargetId,
    this.siteId,
    this.releaseId,
    this.deploymentKind,
    this.deploymentTarget,
    required this.environment,
    this.strategy,
    this.percentage,
    this.platformReviewRef,
    required this.deploymentStatus,
    this.rollbackFromDeploymentId,
    this.startedAt,
    this.completedAt,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory AppDeploymentResponse.fromJson(Map<String, dynamic> json) {
    return AppDeploymentResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppDeploymentResponse.id is required');
        }
        return value;
      })(),
      appId: (() {
        final value = json['appId']?.toString();
        if (value == null) {
          throw FormatException('AppDeploymentResponse.appId is required');
        }
        return value;
      })(),
      platformTargetId: json['platformTargetId']?.toString(),
      siteId: json['siteId']?.toString(),
      releaseId: json['releaseId']?.toString(),
      deploymentKind: json['deploymentKind']?.toString(),
      deploymentTarget: json['deploymentTarget']?.toString(),
      environment: (() {
        final value = json['environment']?.toString();
        if (value == null) {
          throw FormatException('AppDeploymentResponse.environment is required');
        }
        return value;
      })(),
      strategy: json['strategy']?.toString(),
      percentage: json['percentage'] is int ? json['percentage'] : null,
      platformReviewRef: json['platformReviewRef']?.toString(),
      deploymentStatus: (() {
        final value = json['deploymentStatus']?.toString();
        if (value == null) {
          throw FormatException('AppDeploymentResponse.deploymentStatus is required');
        }
        return value;
      })(),
      rollbackFromDeploymentId: json['rollbackFromDeploymentId']?.toString(),
      startedAt: json['startedAt']?.toString(),
      completedAt: json['completedAt']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('AppDeploymentResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('AppDeploymentResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AppDeploymentResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'platformTargetId': platformTargetId,
      'siteId': siteId,
      'releaseId': releaseId,
      'deploymentKind': deploymentKind,
      'deploymentTarget': deploymentTarget,
      'environment': environment,
      'strategy': strategy,
      'percentage': percentage,
      'platformReviewRef': platformReviewRef,
      'deploymentStatus': deploymentStatus,
      'rollbackFromDeploymentId': rollbackFromDeploymentId,
      'startedAt': startedAt,
      'completedAt': completedAt,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class CreateSigningIdentityRequest {
  final String identityName;
  final String signingKind;
  final String? platformTargetId;
  final String? fingerprintSha256;
  final String? expiresAt;
  final String? secretRef;
  final String? idempotencyKey;

  CreateSigningIdentityRequest({
    required this.identityName,
    required this.signingKind,
    this.platformTargetId,
    this.fingerprintSha256,
    this.expiresAt,
    this.secretRef,
    this.idempotencyKey
  });

  factory CreateSigningIdentityRequest.fromJson(Map<String, dynamic> json) {
    return CreateSigningIdentityRequest(
      identityName: (() {
        final value = json['identityName']?.toString();
        if (value == null) {
          throw FormatException('CreateSigningIdentityRequest.identityName is required');
        }
        return value;
      })(),
      signingKind: (() {
        final value = json['signingKind']?.toString();
        if (value == null) {
          throw FormatException('CreateSigningIdentityRequest.signingKind is required');
        }
        return value;
      })(),
      platformTargetId: json['platformTargetId']?.toString(),
      fingerprintSha256: json['fingerprintSha256']?.toString(),
      expiresAt: json['expiresAt']?.toString(),
      secretRef: json['secretRef']?.toString(),
      idempotencyKey: json['idempotencyKey']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'identityName': identityName,
      'signingKind': signingKind,
      'platformTargetId': platformTargetId,
      'fingerprintSha256': fingerprintSha256,
      'expiresAt': expiresAt,
      'secretRef': secretRef,
      'idempotencyKey': idempotencyKey,
    };
  }
}

class SigningIdentityResponse {
  final String id;
  final String identityName;
  final String signingKind;
  final String? platformTargetId;
  final String? fingerprintSha256;
  final String? expiresAt;
  final String? secretRef;
  final String identityStatus;
  final String createdAt;
  final String updatedAt;
  final String version;

  SigningIdentityResponse({
    required this.id,
    required this.identityName,
    required this.signingKind,
    this.platformTargetId,
    this.fingerprintSha256,
    this.expiresAt,
    this.secretRef,
    required this.identityStatus,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory SigningIdentityResponse.fromJson(Map<String, dynamic> json) {
    return SigningIdentityResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('SigningIdentityResponse.id is required');
        }
        return value;
      })(),
      identityName: (() {
        final value = json['identityName']?.toString();
        if (value == null) {
          throw FormatException('SigningIdentityResponse.identityName is required');
        }
        return value;
      })(),
      signingKind: (() {
        final value = json['signingKind']?.toString();
        if (value == null) {
          throw FormatException('SigningIdentityResponse.signingKind is required');
        }
        return value;
      })(),
      platformTargetId: json['platformTargetId']?.toString(),
      fingerprintSha256: json['fingerprintSha256']?.toString(),
      expiresAt: json['expiresAt']?.toString(),
      secretRef: json['secretRef']?.toString(),
      identityStatus: (() {
        final value = json['identityStatus']?.toString();
        if (value == null) {
          throw FormatException('SigningIdentityResponse.identityStatus is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('SigningIdentityResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('SigningIdentityResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('SigningIdentityResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'identityName': identityName,
      'signingKind': signingKind,
      'platformTargetId': platformTargetId,
      'fingerprintSha256': fingerprintSha256,
      'expiresAt': expiresAt,
      'secretRef': secretRef,
      'identityStatus': identityStatus,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class UsageEventResponse {
  final String id;
  final String tenantId;
  final String? siteId;
  final String periodStart;
  final String dimension;
  final String quantity;
  final String unit;
  final String? sourceTargetUuid;
  final String? sourceWindowId;
  final String deduplicationKey;
  final String observedAt;
  final String createdAt;

  UsageEventResponse({
    required this.id,
    required this.tenantId,
    this.siteId,
    required this.periodStart,
    required this.dimension,
    required this.quantity,
    required this.unit,
    this.sourceTargetUuid,
    this.sourceWindowId,
    required this.deduplicationKey,
    required this.observedAt,
    required this.createdAt
  });

  factory UsageEventResponse.fromJson(Map<String, dynamic> json) {
    return UsageEventResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('UsageEventResponse.id is required');
        }
        return value;
      })(),
      tenantId: (() {
        final value = json['tenantId']?.toString();
        if (value == null) {
          throw FormatException('UsageEventResponse.tenantId is required');
        }
        return value;
      })(),
      siteId: json['siteId']?.toString(),
      periodStart: (() {
        final value = json['periodStart']?.toString();
        if (value == null) {
          throw FormatException('UsageEventResponse.periodStart is required');
        }
        return value;
      })(),
      dimension: (() {
        final value = json['dimension']?.toString();
        if (value == null) {
          throw FormatException('UsageEventResponse.dimension is required');
        }
        return value;
      })(),
      quantity: (() {
        final value = json['quantity']?.toString();
        if (value == null) {
          throw FormatException('UsageEventResponse.quantity is required');
        }
        return value;
      })(),
      unit: (() {
        final value = json['unit']?.toString();
        if (value == null) {
          throw FormatException('UsageEventResponse.unit is required');
        }
        return value;
      })(),
      sourceTargetUuid: json['sourceTargetUuid']?.toString(),
      sourceWindowId: json['sourceWindowId']?.toString(),
      deduplicationKey: (() {
        final value = json['deduplicationKey']?.toString();
        if (value == null) {
          throw FormatException('UsageEventResponse.deduplicationKey is required');
        }
        return value;
      })(),
      observedAt: (() {
        final value = json['observedAt']?.toString();
        if (value == null) {
          throw FormatException('UsageEventResponse.observedAt is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('UsageEventResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'tenantId': tenantId,
      'siteId': siteId,
      'periodStart': periodStart,
      'dimension': dimension,
      'quantity': quantity,
      'unit': unit,
      'sourceTargetUuid': sourceTargetUuid,
      'sourceWindowId': sourceWindowId,
      'deduplicationKey': deduplicationKey,
      'observedAt': observedAt,
      'createdAt': createdAt,
    };
  }
}

class CreateAppDatabaseProfileRequest {
  final String profileKey;
  final String dbEngine;
  final String catalogName;
  final String? schemaVersion;
  final String? baselineVersion;
  final String? migrationStrategy;

  CreateAppDatabaseProfileRequest({
    required this.profileKey,
    required this.dbEngine,
    required this.catalogName,
    this.schemaVersion,
    this.baselineVersion,
    this.migrationStrategy
  });

  factory CreateAppDatabaseProfileRequest.fromJson(Map<String, dynamic> json) {
    return CreateAppDatabaseProfileRequest(
      profileKey: (() {
        final value = json['profileKey']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDatabaseProfileRequest.profileKey is required');
        }
        return value;
      })(),
      dbEngine: (() {
        final value = json['dbEngine']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDatabaseProfileRequest.dbEngine is required');
        }
        return value;
      })(),
      catalogName: (() {
        final value = json['catalogName']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDatabaseProfileRequest.catalogName is required');
        }
        return value;
      })(),
      schemaVersion: json['schemaVersion']?.toString(),
      baselineVersion: json['baselineVersion']?.toString(),
      migrationStrategy: json['migrationStrategy']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'profileKey': profileKey,
      'dbEngine': dbEngine,
      'catalogName': catalogName,
      'schemaVersion': schemaVersion,
      'baselineVersion': baselineVersion,
      'migrationStrategy': migrationStrategy,
    };
  }
}

class UpdateAppDatabaseProfileRequest {
  final String? schemaVersion;
  final String? baselineVersion;
  final String? migrationStrategy;
  final String? profileStatus;

  UpdateAppDatabaseProfileRequest({
    this.schemaVersion,
    this.baselineVersion,
    this.migrationStrategy,
    this.profileStatus
  });

  factory UpdateAppDatabaseProfileRequest.fromJson(Map<String, dynamic> json) {
    return UpdateAppDatabaseProfileRequest(
      schemaVersion: json['schemaVersion']?.toString(),
      baselineVersion: json['baselineVersion']?.toString(),
      migrationStrategy: json['migrationStrategy']?.toString(),
      profileStatus: json['profileStatus']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'schemaVersion': schemaVersion,
      'baselineVersion': baselineVersion,
      'migrationStrategy': migrationStrategy,
      'profileStatus': profileStatus,
    };
  }
}

class AppDatabaseProfileResponse {
  final String id;
  final String appId;
  final String profileKey;
  final String dbEngine;
  final String catalogName;
  final String? schemaVersion;
  final String? baselineVersion;
  final String migrationStrategy;
  final String profileStatus;
  final String migrationCount;
  final String createdAt;
  final String updatedAt;
  final String version;

  AppDatabaseProfileResponse({
    required this.id,
    required this.appId,
    required this.profileKey,
    required this.dbEngine,
    required this.catalogName,
    this.schemaVersion,
    this.baselineVersion,
    required this.migrationStrategy,
    required this.profileStatus,
    required this.migrationCount,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory AppDatabaseProfileResponse.fromJson(Map<String, dynamic> json) {
    return AppDatabaseProfileResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.id is required');
        }
        return value;
      })(),
      appId: (() {
        final value = json['appId']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.appId is required');
        }
        return value;
      })(),
      profileKey: (() {
        final value = json['profileKey']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.profileKey is required');
        }
        return value;
      })(),
      dbEngine: (() {
        final value = json['dbEngine']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.dbEngine is required');
        }
        return value;
      })(),
      catalogName: (() {
        final value = json['catalogName']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.catalogName is required');
        }
        return value;
      })(),
      schemaVersion: json['schemaVersion']?.toString(),
      baselineVersion: json['baselineVersion']?.toString(),
      migrationStrategy: (() {
        final value = json['migrationStrategy']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.migrationStrategy is required');
        }
        return value;
      })(),
      profileStatus: (() {
        final value = json['profileStatus']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.profileStatus is required');
        }
        return value;
      })(),
      migrationCount: (() {
        final value = json['migrationCount']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.migrationCount is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfileResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'profileKey': profileKey,
      'dbEngine': dbEngine,
      'catalogName': catalogName,
      'schemaVersion': schemaVersion,
      'baselineVersion': baselineVersion,
      'migrationStrategy': migrationStrategy,
      'profileStatus': profileStatus,
      'migrationCount': migrationCount,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class CreateAppDatabaseMigrationRequest {
  final String migrationVersion;
  final String migrationName;
  final String checksumSha256;
  final String? scriptRef;

  CreateAppDatabaseMigrationRequest({
    required this.migrationVersion,
    required this.migrationName,
    required this.checksumSha256,
    this.scriptRef
  });

  factory CreateAppDatabaseMigrationRequest.fromJson(Map<String, dynamic> json) {
    return CreateAppDatabaseMigrationRequest(
      migrationVersion: (() {
        final value = json['migrationVersion']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDatabaseMigrationRequest.migrationVersion is required');
        }
        return value;
      })(),
      migrationName: (() {
        final value = json['migrationName']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDatabaseMigrationRequest.migrationName is required');
        }
        return value;
      })(),
      checksumSha256: (() {
        final value = json['checksumSha256']?.toString();
        if (value == null) {
          throw FormatException('CreateAppDatabaseMigrationRequest.checksumSha256 is required');
        }
        return value;
      })(),
      scriptRef: json['scriptRef']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'migrationVersion': migrationVersion,
      'migrationName': migrationName,
      'checksumSha256': checksumSha256,
      'scriptRef': scriptRef,
    };
  }
}

class AppDatabaseMigrationResponse {
  final String id;
  final String profileId;
  final String migrationVersion;
  final String migrationName;
  final String checksumSha256;
  final String? scriptRef;
  final String migrationStatus;
  final String? appliedAt;
  final String createdAt;
  final String updatedAt;
  final String version;

  AppDatabaseMigrationResponse({
    required this.id,
    required this.profileId,
    required this.migrationVersion,
    required this.migrationName,
    required this.checksumSha256,
    this.scriptRef,
    required this.migrationStatus,
    this.appliedAt,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory AppDatabaseMigrationResponse.fromJson(Map<String, dynamic> json) {
    return AppDatabaseMigrationResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationResponse.id is required');
        }
        return value;
      })(),
      profileId: (() {
        final value = json['profileId']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationResponse.profileId is required');
        }
        return value;
      })(),
      migrationVersion: (() {
        final value = json['migrationVersion']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationResponse.migrationVersion is required');
        }
        return value;
      })(),
      migrationName: (() {
        final value = json['migrationName']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationResponse.migrationName is required');
        }
        return value;
      })(),
      checksumSha256: (() {
        final value = json['checksumSha256']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationResponse.checksumSha256 is required');
        }
        return value;
      })(),
      scriptRef: json['scriptRef']?.toString(),
      migrationStatus: (() {
        final value = json['migrationStatus']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationResponse.migrationStatus is required');
        }
        return value;
      })(),
      appliedAt: json['appliedAt']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'profileId': profileId,
      'migrationVersion': migrationVersion,
      'migrationName': migrationName,
      'checksumSha256': checksumSha256,
      'scriptRef': scriptRef,
      'migrationStatus': migrationStatus,
      'appliedAt': appliedAt,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class CreateAppEnvironmentRequest {
  final String envKey;
  final String envName;
  final String envLevel;
  final bool? approvalRequired;

  CreateAppEnvironmentRequest({
    required this.envKey,
    required this.envName,
    required this.envLevel,
    this.approvalRequired
  });

  factory CreateAppEnvironmentRequest.fromJson(Map<String, dynamic> json) {
    return CreateAppEnvironmentRequest(
      envKey: (() {
        final value = json['envKey']?.toString();
        if (value == null) {
          throw FormatException('CreateAppEnvironmentRequest.envKey is required');
        }
        return value;
      })(),
      envName: (() {
        final value = json['envName']?.toString();
        if (value == null) {
          throw FormatException('CreateAppEnvironmentRequest.envName is required');
        }
        return value;
      })(),
      envLevel: (() {
        final value = json['envLevel']?.toString();
        if (value == null) {
          throw FormatException('CreateAppEnvironmentRequest.envLevel is required');
        }
        return value;
      })(),
      approvalRequired: json['approvalRequired'] is bool ? json['approvalRequired'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'envKey': envKey,
      'envName': envName,
      'envLevel': envLevel,
      'approvalRequired': approvalRequired,
    };
  }
}

class UpdateAppEnvironmentRequest {
  final String? envName;
  final bool? approvalRequired;
  final String? envStatus;

  UpdateAppEnvironmentRequest({
    this.envName,
    this.approvalRequired,
    this.envStatus
  });

  factory UpdateAppEnvironmentRequest.fromJson(Map<String, dynamic> json) {
    return UpdateAppEnvironmentRequest(
      envName: json['envName']?.toString(),
      approvalRequired: json['approvalRequired'] is bool ? json['approvalRequired'] : null,
      envStatus: json['envStatus']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'envName': envName,
      'approvalRequired': approvalRequired,
      'envStatus': envStatus,
    };
  }
}

class AppEnvironmentResponse {
  final String id;
  final String appId;
  final String envKey;
  final String envName;
  final String envLevel;
  final bool approvalRequired;
  final String? currentReleaseId;
  final String? currentReleaseVersion;
  final String envStatus;
  final String createdAt;
  final String updatedAt;
  final String version;

  AppEnvironmentResponse({
    required this.id,
    required this.appId,
    required this.envKey,
    required this.envName,
    required this.envLevel,
    required this.approvalRequired,
    this.currentReleaseId,
    this.currentReleaseVersion,
    required this.envStatus,
    required this.createdAt,
    required this.updatedAt,
    required this.version
  });

  factory AppEnvironmentResponse.fromJson(Map<String, dynamic> json) {
    return AppEnvironmentResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentResponse.id is required');
        }
        return value;
      })(),
      appId: (() {
        final value = json['appId']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentResponse.appId is required');
        }
        return value;
      })(),
      envKey: (() {
        final value = json['envKey']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentResponse.envKey is required');
        }
        return value;
      })(),
      envName: (() {
        final value = json['envName']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentResponse.envName is required');
        }
        return value;
      })(),
      envLevel: (() {
        final value = json['envLevel']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentResponse.envLevel is required');
        }
        return value;
      })(),
      approvalRequired: (() {
        final value = json['approvalRequired'];
        if (value is! bool) {
          throw FormatException('AppEnvironmentResponse.approvalRequired is required');
        }
        return value;
      })(),
      currentReleaseId: json['currentReleaseId']?.toString(),
      currentReleaseVersion: json['currentReleaseVersion']?.toString(),
      envStatus: (() {
        final value = json['envStatus']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentResponse.envStatus is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentResponse.updatedAt is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentResponse.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'envKey': envKey,
      'envName': envName,
      'envLevel': envLevel,
      'approvalRequired': approvalRequired,
      'currentReleaseId': currentReleaseId,
      'currentReleaseVersion': currentReleaseVersion,
      'envStatus': envStatus,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'version': version,
    };
  }
}

class PromoteEnvironmentRequest {
  final String releaseId;
  final String? fromEnvironmentId;
  final String? note;

  PromoteEnvironmentRequest({
    required this.releaseId,
    this.fromEnvironmentId,
    this.note
  });

  factory PromoteEnvironmentRequest.fromJson(Map<String, dynamic> json) {
    return PromoteEnvironmentRequest(
      releaseId: (() {
        final value = json['releaseId']?.toString();
        if (value == null) {
          throw FormatException('PromoteEnvironmentRequest.releaseId is required');
        }
        return value;
      })(),
      fromEnvironmentId: json['fromEnvironmentId']?.toString(),
      note: json['note']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'releaseId': releaseId,
      'fromEnvironmentId': fromEnvironmentId,
      'note': note,
    };
  }
}

class EnvironmentPromotionResponse {
  final String id;
  final String appId;
  final String environmentId;
  final String environmentKey;
  final String releaseId;
  final String releaseVersion;
  final String? fromEnvironmentId;
  final String? fromEnvironmentKey;
  final String? promotedBy;
  final String? note;
  final String createdAt;

  EnvironmentPromotionResponse({
    required this.id,
    required this.appId,
    required this.environmentId,
    required this.environmentKey,
    required this.releaseId,
    required this.releaseVersion,
    this.fromEnvironmentId,
    this.fromEnvironmentKey,
    this.promotedBy,
    this.note,
    required this.createdAt
  });

  factory EnvironmentPromotionResponse.fromJson(Map<String, dynamic> json) {
    return EnvironmentPromotionResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('EnvironmentPromotionResponse.id is required');
        }
        return value;
      })(),
      appId: (() {
        final value = json['appId']?.toString();
        if (value == null) {
          throw FormatException('EnvironmentPromotionResponse.appId is required');
        }
        return value;
      })(),
      environmentId: (() {
        final value = json['environmentId']?.toString();
        if (value == null) {
          throw FormatException('EnvironmentPromotionResponse.environmentId is required');
        }
        return value;
      })(),
      environmentKey: (() {
        final value = json['environmentKey']?.toString();
        if (value == null) {
          throw FormatException('EnvironmentPromotionResponse.environmentKey is required');
        }
        return value;
      })(),
      releaseId: (() {
        final value = json['releaseId']?.toString();
        if (value == null) {
          throw FormatException('EnvironmentPromotionResponse.releaseId is required');
        }
        return value;
      })(),
      releaseVersion: (() {
        final value = json['releaseVersion']?.toString();
        if (value == null) {
          throw FormatException('EnvironmentPromotionResponse.releaseVersion is required');
        }
        return value;
      })(),
      fromEnvironmentId: json['fromEnvironmentId']?.toString(),
      fromEnvironmentKey: json['fromEnvironmentKey']?.toString(),
      promotedBy: json['promotedBy']?.toString(),
      note: json['note']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('EnvironmentPromotionResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'environmentId': environmentId,
      'environmentKey': environmentKey,
      'releaseId': releaseId,
      'releaseVersion': releaseVersion,
      'fromEnvironmentId': fromEnvironmentId,
      'fromEnvironmentKey': fromEnvironmentKey,
      'promotedBy': promotedBy,
      'note': note,
      'createdAt': createdAt,
    };
  }
}

class DomainZonesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesListResponse.fromJson(Map<String, dynamic> json) {
    return DomainZonesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return DomainZonesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return DomainZonesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesUpdateResponse.fromJson(Map<String, dynamic> json) {
    return DomainZonesUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesHostnamesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesHostnamesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesHostnamesListResponse.fromJson(Map<String, dynamic> json) {
    return DomainZonesHostnamesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesHostnamesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesHostnamesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesHostnamesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesHostnamesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesHostnamesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesHostnamesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return DomainZonesHostnamesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesHostnamesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesHostnamesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesHostnamesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesHostnamesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesHostnamesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesHostnamesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return DomainZonesHostnamesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesHostnamesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesHostnamesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesHostnamesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesHostnamesUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesHostnamesUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesHostnamesUpdateResponse.fromJson(Map<String, dynamic> json) {
    return DomainZonesHostnamesUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesHostnamesUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesHostnamesUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesHostnamesUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesHostnamesVerifyResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesHostnamesVerifyResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesHostnamesVerifyResponse.fromJson(Map<String, dynamic> json) {
    return DomainZonesHostnamesVerifyResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesHostnamesVerifyResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesHostnamesVerifyResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesHostnamesVerifyResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesHostnameClaimsEnsureResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesHostnameClaimsEnsureResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesHostnameClaimsEnsureResponse.fromJson(Map<String, dynamic> json) {
    return DomainZonesHostnameClaimsEnsureResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesHostnameClaimsEnsureResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesHostnameClaimsEnsureResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesHostnameClaimsEnsureResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesDnsRecordsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesDnsRecordsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesDnsRecordsListResponse.fromJson(Map<String, dynamic> json) {
    return DomainZonesDnsRecordsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesDnsRecordsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesDnsRecordsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesDnsRecordsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainZonesDnsRecordsSyncResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainZonesDnsRecordsSyncResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainZonesDnsRecordsSyncResponse.fromJson(Map<String, dynamic> json) {
    return DomainZonesDnsRecordsSyncResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainZonesDnsRecordsSyncResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainZonesDnsRecordsSyncResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainZonesDnsRecordsSyncResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class CloudAccountsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  CloudAccountsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CloudAccountsListResponse.fromJson(Map<String, dynamic> json) {
    return CloudAccountsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CloudAccountsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CloudAccountsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CloudAccountsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class CloudAccountsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  CloudAccountsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CloudAccountsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return CloudAccountsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CloudAccountsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CloudAccountsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CloudAccountsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class CertificatesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesListResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CertificatesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class CertificatesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return CertificatesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CertificatesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class CertificatesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CertificatesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class CertificatesRenewResponse {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesRenewResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesRenewResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesRenewResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesRenewResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CertificatesRenewResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesRenewResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class CertificatesRenewalsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesRenewalsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesRenewalsListResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesRenewalsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesRenewalsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CertificatesRenewalsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesRenewalsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class UploadSessionsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  UploadSessionsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory UploadSessionsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return UploadSessionsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('UploadSessionsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('UploadSessionsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('UploadSessionsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class UploadSessionsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  UploadSessionsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory UploadSessionsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return UploadSessionsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('UploadSessionsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('UploadSessionsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('UploadSessionsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class UploadSessionsCompleteResponse {
  final int code;
  final dynamic data;
  final String traceId;

  UploadSessionsCompleteResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory UploadSessionsCompleteResponse.fromJson(Map<String, dynamic> json) {
    return UploadSessionsCompleteResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('UploadSessionsCompleteResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('UploadSessionsCompleteResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('UploadSessionsCompleteResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class UploadSessionsCancelResponse {
  final int code;
  final dynamic data;
  final String traceId;

  UploadSessionsCancelResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory UploadSessionsCancelResponse.fromJson(Map<String, dynamic> json) {
    return UploadSessionsCancelResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('UploadSessionsCancelResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('UploadSessionsCancelResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('UploadSessionsCancelResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ArtifactsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ArtifactsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ArtifactsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ArtifactsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ArtifactsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ArtifactsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ArtifactsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ArtifactsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ArtifactsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ArtifactsListResponse.fromJson(Map<String, dynamic> json) {
    return ArtifactsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ArtifactsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ArtifactsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ArtifactsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ArtifactsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ArtifactsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ArtifactsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ArtifactsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ArtifactsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ArtifactsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ArtifactsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsListResponse.fromJson(Map<String, dynamic> json) {
    return AppsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  AppsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return AppsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return AppsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return AppsUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsActivateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsActivateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsActivateResponse.fromJson(Map<String, dynamic> json) {
    return AppsActivateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsActivateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsActivateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsActivateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsDomainsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsDomainsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsDomainsListResponse.fromJson(Map<String, dynamic> json) {
    return AppsDomainsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsDomainsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsDomainsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsDomainsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsCompositionUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsCompositionUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsCompositionUpdateResponse.fromJson(Map<String, dynamic> json) {
    return AppsCompositionUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsCompositionUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsCompositionUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsCompositionUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsPauseResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsPauseResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsPauseResponse.fromJson(Map<String, dynamic> json) {
    return AppsPauseResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsPauseResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsPauseResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsPauseResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsEnvVariablesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsEnvVariablesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsEnvVariablesListResponse.fromJson(Map<String, dynamic> json) {
    return AppsEnvVariablesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsEnvVariablesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsEnvVariablesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsEnvVariablesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsEnvVariablesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  AppsEnvVariablesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsEnvVariablesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return AppsEnvVariablesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsEnvVariablesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsEnvVariablesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsEnvVariablesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsHealthChecksListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsHealthChecksListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsHealthChecksListResponse.fromJson(Map<String, dynamic> json) {
    return AppsHealthChecksListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsHealthChecksListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsHealthChecksListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsHealthChecksListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsHealthChecksCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  AppsHealthChecksCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsHealthChecksCreateResponse201.fromJson(Map<String, dynamic> json) {
    return AppsHealthChecksCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsHealthChecksCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsHealthChecksCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsHealthChecksCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsSourceSpecsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsSourceSpecsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsSourceSpecsListResponse.fromJson(Map<String, dynamic> json) {
    return AppsSourceSpecsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsSourceSpecsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsSourceSpecsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsSourceSpecsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsSourceSpecsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  AppsSourceSpecsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsSourceSpecsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return AppsSourceSpecsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsSourceSpecsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsSourceSpecsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsSourceSpecsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsSourceSpecsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsSourceSpecsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsSourceSpecsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return AppsSourceSpecsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsSourceSpecsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsSourceSpecsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsSourceSpecsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsSourceSpecsUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsSourceSpecsUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsSourceSpecsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return AppsSourceSpecsUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsSourceSpecsUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsSourceSpecsUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsSourceSpecsUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppsSourceSpecsBindResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppsSourceSpecsBindResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppsSourceSpecsBindResponse.fromJson(Map<String, dynamic> json) {
    return AppsSourceSpecsBindResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppsSourceSpecsBindResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppsSourceSpecsBindResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppsSourceSpecsBindResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class PlatformTargetsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  PlatformTargetsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory PlatformTargetsListResponse.fromJson(Map<String, dynamic> json) {
    return PlatformTargetsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('PlatformTargetsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('PlatformTargetsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class PlatformTargetsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  PlatformTargetsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory PlatformTargetsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return PlatformTargetsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('PlatformTargetsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('PlatformTargetsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class PlatformTargetsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  PlatformTargetsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory PlatformTargetsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return PlatformTargetsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('PlatformTargetsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('PlatformTargetsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('PlatformTargetsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SourceRepositoriesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SourceRepositoriesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SourceRepositoriesListResponse.fromJson(Map<String, dynamic> json) {
    return SourceRepositoriesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SourceRepositoriesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('SourceRepositoriesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoriesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SourceRepositoriesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  SourceRepositoriesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SourceRepositoriesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return SourceRepositoriesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SourceRepositoriesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('SourceRepositoriesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoriesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SourceRepositoriesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SourceRepositoriesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SourceRepositoriesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return SourceRepositoriesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SourceRepositoriesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('SourceRepositoriesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SourceRepositoriesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class BuildTemplatesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  BuildTemplatesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory BuildTemplatesListResponse.fromJson(Map<String, dynamic> json) {
    return BuildTemplatesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('BuildTemplatesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('BuildTemplatesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplatesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class BuildTemplatesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  BuildTemplatesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory BuildTemplatesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return BuildTemplatesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('BuildTemplatesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('BuildTemplatesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplatesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class BuildTemplatesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  BuildTemplatesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory BuildTemplatesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return BuildTemplatesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('BuildTemplatesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('BuildTemplatesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('BuildTemplatesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class BuildsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  BuildsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory BuildsListResponse.fromJson(Map<String, dynamic> json) {
    return BuildsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('BuildsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('BuildsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('BuildsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class BuildsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  BuildsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory BuildsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return BuildsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('BuildsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('BuildsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('BuildsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class BuildsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  BuildsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory BuildsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return BuildsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('BuildsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('BuildsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('BuildsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class BuildsUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  BuildsUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory BuildsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return BuildsUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('BuildsUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('BuildsUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('BuildsUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class PackagesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  PackagesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory PackagesListResponse.fromJson(Map<String, dynamic> json) {
    return PackagesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('PackagesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('PackagesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('PackagesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class PackagesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  PackagesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory PackagesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return PackagesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('PackagesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('PackagesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('PackagesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class PackagesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  PackagesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory PackagesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return PackagesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('PackagesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('PackagesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('PackagesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ReleasesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ReleasesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ReleasesListResponse.fromJson(Map<String, dynamic> json) {
    return ReleasesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ReleasesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ReleasesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ReleasesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ReleasesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ReleasesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ReleasesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ReleasesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ReleasesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ReleasesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ReleasesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ReleasesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ReleasesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ReleasesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ReleasesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ReleasesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ReleasesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ReleasesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ChannelsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ChannelsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ChannelsListResponse.fromJson(Map<String, dynamic> json) {
    return ChannelsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ChannelsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ChannelsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ChannelsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ChannelsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ChannelsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ChannelsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ChannelsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ChannelsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ChannelsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ChannelsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ChannelsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ChannelsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ChannelsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ChannelsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ChannelsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ChannelsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ChannelsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ChannelsRolloutsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ChannelsRolloutsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ChannelsRolloutsListResponse.fromJson(Map<String, dynamic> json) {
    return ChannelsRolloutsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ChannelsRolloutsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ChannelsRolloutsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ChannelsRolloutsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DeploymentsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DeploymentsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DeploymentsListResponse.fromJson(Map<String, dynamic> json) {
    return DeploymentsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DeploymentsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DeploymentsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DeploymentsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DeploymentsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  DeploymentsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DeploymentsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return DeploymentsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DeploymentsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DeploymentsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DeploymentsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DeploymentsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DeploymentsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DeploymentsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return DeploymentsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DeploymentsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DeploymentsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DeploymentsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SigningIdentitiesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SigningIdentitiesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SigningIdentitiesListResponse.fromJson(Map<String, dynamic> json) {
    return SigningIdentitiesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SigningIdentitiesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('SigningIdentitiesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SigningIdentitiesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SigningIdentitiesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  SigningIdentitiesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SigningIdentitiesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return SigningIdentitiesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SigningIdentitiesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('SigningIdentitiesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SigningIdentitiesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SigningIdentitiesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SigningIdentitiesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SigningIdentitiesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return SigningIdentitiesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SigningIdentitiesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('SigningIdentitiesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SigningIdentitiesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class UsageEventsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  UsageEventsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory UsageEventsListResponse.fromJson(Map<String, dynamic> json) {
    return UsageEventsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('UsageEventsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('UsageEventsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('UsageEventsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppDatabaseProfilesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppDatabaseProfilesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppDatabaseProfilesListResponse.fromJson(Map<String, dynamic> json) {
    return AppDatabaseProfilesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppDatabaseProfilesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppDatabaseProfilesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfilesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppDatabaseProfilesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  AppDatabaseProfilesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppDatabaseProfilesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return AppDatabaseProfilesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppDatabaseProfilesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppDatabaseProfilesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfilesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppDatabaseProfilesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppDatabaseProfilesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppDatabaseProfilesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return AppDatabaseProfilesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppDatabaseProfilesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppDatabaseProfilesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfilesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppDatabaseProfilesUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppDatabaseProfilesUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppDatabaseProfilesUpdateResponse.fromJson(Map<String, dynamic> json) {
    return AppDatabaseProfilesUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppDatabaseProfilesUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppDatabaseProfilesUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseProfilesUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppDatabaseMigrationsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppDatabaseMigrationsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppDatabaseMigrationsListResponse.fromJson(Map<String, dynamic> json) {
    return AppDatabaseMigrationsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppDatabaseMigrationsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppDatabaseMigrationsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppDatabaseMigrationsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  AppDatabaseMigrationsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppDatabaseMigrationsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return AppDatabaseMigrationsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppDatabaseMigrationsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppDatabaseMigrationsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppDatabaseMigrationsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppDatabaseMigrationsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppDatabaseMigrationsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return AppDatabaseMigrationsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppDatabaseMigrationsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppDatabaseMigrationsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppDatabaseMigrationsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppEnvironmentsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppEnvironmentsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppEnvironmentsListResponse.fromJson(Map<String, dynamic> json) {
    return AppEnvironmentsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppEnvironmentsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppEnvironmentsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppEnvironmentsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  AppEnvironmentsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppEnvironmentsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return AppEnvironmentsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppEnvironmentsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppEnvironmentsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppEnvironmentsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppEnvironmentsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppEnvironmentsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return AppEnvironmentsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppEnvironmentsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppEnvironmentsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppEnvironmentsUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppEnvironmentsUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppEnvironmentsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return AppEnvironmentsUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppEnvironmentsUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppEnvironmentsUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentsUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppEnvironmentsListGetResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppEnvironmentsListGetResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppEnvironmentsListGetResponse.fromJson(Map<String, dynamic> json) {
    return AppEnvironmentsListGetResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppEnvironmentsListGetResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppEnvironmentsListGetResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentsListGetResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppEnvironmentsCreatePostResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  AppEnvironmentsCreatePostResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppEnvironmentsCreatePostResponse201.fromJson(Map<String, dynamic> json) {
    return AppEnvironmentsCreatePostResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppEnvironmentsCreatePostResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppEnvironmentsCreatePostResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppEnvironmentsCreatePostResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class TemplateCategoriesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  TemplateCategoriesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory TemplateCategoriesListResponse.fromJson(Map<String, dynamic> json) {
    return TemplateCategoriesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('TemplateCategoriesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('TemplateCategoriesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('TemplateCategoriesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class MarketplaceTemplatesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  MarketplaceTemplatesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory MarketplaceTemplatesListResponse.fromJson(Map<String, dynamic> json) {
    return MarketplaceTemplatesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('MarketplaceTemplatesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('MarketplaceTemplatesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('MarketplaceTemplatesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class MarketplaceTemplatesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  MarketplaceTemplatesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory MarketplaceTemplatesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return MarketplaceTemplatesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('MarketplaceTemplatesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('MarketplaceTemplatesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('MarketplaceTemplatesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppTemplatesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppTemplatesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppTemplatesListResponse.fromJson(Map<String, dynamic> json) {
    return AppTemplatesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppTemplatesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppTemplatesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppTemplatesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppTemplatesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  AppTemplatesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppTemplatesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return AppTemplatesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppTemplatesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppTemplatesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppTemplatesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppTemplatesRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppTemplatesRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppTemplatesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return AppTemplatesRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppTemplatesRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppTemplatesRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppTemplatesRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppTemplatesUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppTemplatesUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppTemplatesUpdateResponse.fromJson(Map<String, dynamic> json) {
    return AppTemplatesUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppTemplatesUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppTemplatesUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppTemplatesUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppTemplatesSubmitResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppTemplatesSubmitResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppTemplatesSubmitResponse.fromJson(Map<String, dynamic> json) {
    return AppTemplatesSubmitResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppTemplatesSubmitResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppTemplatesSubmitResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppTemplatesSubmitResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppTemplateVersionsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AppTemplateVersionsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppTemplateVersionsListResponse.fromJson(Map<String, dynamic> json) {
    return AppTemplateVersionsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppTemplateVersionsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppTemplateVersionsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class AppTemplateVersionsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  AppTemplateVersionsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AppTemplateVersionsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return AppTemplateVersionsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AppTemplateVersionsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AppTemplateVersionsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AppTemplateVersionsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}
