/// Runtime configuration for the deployments Flutter client.
///
/// Values arrive as `--dart-define` compile-time constants so the same binary
/// can be built per profile without embedding secrets — the runtime document
/// is public by contract, mirroring the browser apps' `/runtime-env.json`.
library;

class DeploymentsRuntimeConfig {
  final String appApiBaseUrl;

  /// Auth tokens are injected by the host shell after the platform login
  /// flow; `--dart-define` supplies them for local development runs.
  final String authToken;
  final String accessToken;

  const DeploymentsRuntimeConfig({
    required this.appApiBaseUrl,
    this.authToken = '',
    this.accessToken = '',
  });

  const DeploymentsRuntimeConfig.dev()
      : appApiBaseUrl = const String.fromEnvironment(
          'SDKWORK_DEPLOY_APP_API_BASE_URL',
          defaultValue: 'http://127.0.0.1:3900',
        ),
        authToken = const String.fromEnvironment('SDKWORK_DEPLOY_AUTH_TOKEN'),
        accessToken = const String.fromEnvironment('SDKWORK_DEPLOY_ACCESS_TOKEN');
}
