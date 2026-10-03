import 'package:sdkwork_common_flutter/sdkwork_common_flutter.dart';
import 'src/http/client.dart';
import 'src/api/domain.dart';
import 'src/api/certificate.dart';
import 'src/api/upload_session.dart';
import 'src/api/artifact.dart';
import 'src/api/app.dart';
import 'src/api/env_variable.dart';
import 'src/api/monitor.dart';
import 'src/api/build.dart';
import 'src/api/package.dart';
import 'src/api/release.dart';
import 'src/api/deployment.dart';
import 'src/api/signing.dart';
import 'src/api/usage.dart';
import 'src/api/app_database.dart';
import 'src/api/app_environment.dart';
import 'src/api/template.dart';

class SdkworkAppClient {
  final HttpClient _httpClient;

  late final DomainApi domain;
  late final CertificateApi certificate;
  late final UploadSessionApi uploadSession;
  late final ArtifactApi artifact;
  late final AppApi app;
  late final EnvVariableApi envVariable;
  late final MonitorApi monitor;
  late final BuildApi build;
  late final PackageApi package;
  late final ReleaseApi release;
  late final DeploymentApi deployment;
  late final SigningApi signing;
  late final UsageApi usage;
  late final AppDatabaseApi appDatabase;
  late final AppEnvironmentApi appEnvironment;
  late final TemplateApi template;

  SdkworkAppClient({
    required SdkConfig config,
  }) : _httpClient = HttpClient(config: config) {
    domain = DomainApi(_httpClient);
    certificate = CertificateApi(_httpClient);
    uploadSession = UploadSessionApi(_httpClient);
    artifact = ArtifactApi(_httpClient);
    app = AppApi(_httpClient);
    envVariable = EnvVariableApi(_httpClient);
    monitor = MonitorApi(_httpClient);
    build = BuildApi(_httpClient);
    package = PackageApi(_httpClient);
    release = ReleaseApi(_httpClient);
    deployment = DeploymentApi(_httpClient);
    signing = SigningApi(_httpClient);
    usage = UsageApi(_httpClient);
    appDatabase = AppDatabaseApi(_httpClient);
    appEnvironment = AppEnvironmentApi(_httpClient);
    template = TemplateApi(_httpClient);
  }

  factory SdkworkAppClient.withBaseUrl({
    required String baseUrl,
    String? authToken,
    String? accessToken,
    Map<String, String>? headers,
    int timeout = 30000,
  }) {
    return SdkworkAppClient(
      config: SdkConfig(
        baseUrl: baseUrl,
        timeout: timeout,
        headers: headers ?? const {},
        authToken: authToken,
        accessToken: accessToken,
      ),
    );
  }

  void setAuthToken(String token) {
    _httpClient.setAuthToken(token);
  }

  void setAccessToken(String token) {
    _httpClient.setAccessToken(token);
  }

  void setHeader(String key, String value) {
    _httpClient.setHeader(key, value);
  }
}
