# sdkwork-deployments-app-sdk (Flutter)

Generated SDKWork v3 dual-token transport SDK.

## Installation

Add to `pubspec.yaml`:

```yaml
dependencies:
  sdkwork_deployments_app_sdk: ^0.1.0
```

## Quick Start

```dart
import 'package:sdkwork_deployments_app_sdk/sdkwork_deployments_app_sdk.dart';

final client = SdkworkAppClient.withBaseUrl(baseUrl: 'http://127.0.0.1:3900');
client.setAuthToken('your-auth-token');
client.setAccessToken('your-access-token');

// Use the SDK
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
  'keyword': 'keyword',
  'scope': 'PLATFORM',
};
final result = await client.app.appsList(params);
print(result);
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```dart
final client = SdkworkAppClient.withBaseUrl(baseUrl: 'http://127.0.0.1:3900');

// Set custom headers
client.setHeader('X-Custom-Header', 'value');
```

## API Modules

- `client.domain` - domain API
- `client.certificate` - certificate API
- `client.uploadSession` - upload_session API
- `client.artifact` - artifact API
- `client.app` - app API
- `client.envVariable` - env_variable API
- `client.monitor` - monitor API
- `client.build` - build API
- `client.package` - package API
- `client.release` - release API
- `client.deployment` - deployment API
- `client.signing` - signing API
- `client.usage` - usage API
- `client.appDatabase` - app_database API
- `client.appEnvironment` - app_environment API
- `client.template` - template API

## Usage Examples

### domain
```dart
// List root domain zones
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
  'status': 'ACTIVE',
  'keyword': 'keyword',
  'scope': 'USER',
  'provider_account_id': '1',
};
final result = await client.domain.zonesList(params);
print(result);
```

### certificate
```dart
// 获取证书列表
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.certificate.certificatesList(params);
print(result);
```

### upload_session
```dart
// 获取上传会话
final uploadSessionId = '1';
final result = await client.uploadSession.uploadSessionsRetrieve(uploadSessionId);
print(result);
```

### artifact
```dart
// 获取租户制品列表
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.artifact.artifactsList(params);
print(result);
```

### app
```dart
// List the apps the caller may reach
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
  'keyword': 'keyword',
  'scope': 'PLATFORM',
};
final result = await client.app.appsList(params);
print(result);
```

### env_variable
```dart
// 获取环境变量列表
final appId = '1';
final params = <String, dynamic>{
  'environment': 'environment',
};
final result = await client.envVariable.appsEnvVariablesList(appId, params);
print(result);
```

### monitor
```dart
// 获取健康检查配置
final appId = '1';
final result = await client.monitor.appsHealthChecksList(appId);
print(result);
```

### build
```dart
// List tenant build templates
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.build.templatesList(params);
print(result);
```

### package
```dart
// List deployment packages of an app
final appId = '1';
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.package.packagesList(appId, params);
print(result);
```

### release
```dart
// List release channels of an app
final appId = '1';
final result = await client.release.channelsList(appId);
print(result);
```

### deployment
```dart
// List deployments of an app
final appId = '1';
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.deployment.deploymentsList(appId, params);
print(result);
```

### signing
```dart
// List tenant signing identities
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.signing.identitiesList(params);
print(result);
```

### usage
```dart
// List tenant usage metering events
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.usage.eventsList(params);
print(result);
```

### app_database
```dart
// List the database structure contracts of an app
final appId = '1';
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.appDatabase.profilesList(appId, params);
print(result);
```

### app_environment
```dart
// List the environments of an app
final appId = '1';
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.appEnvironment.getAppEnvironmentsList(appId, params);
print(result);
```

### template
```dart
// List template categories
final params = <String, dynamic>{
  'include_disabled': true,
};
final result = await client.template.categoriesList(params);
print(result);
```

## Error Handling

```dart
try {
  final params = <String, dynamic>{
    'page': 1,
    'page_size': 2,
    'keyword': 'keyword',
    'scope': 'PLATFORM',
  };
  final result = await client.app.appsList(params);
  print(result);
} catch (e) {
  print('Error: $e');
}
```

## Publishing

This SDK includes cross-platform publish scripts in `bin/`:
- `bin/publish-core.mjs`
- `bin/publish.sh`
- `bin/publish.ps1`

### Check

```bash
./bin/publish.sh --action check
```

### Publish

```bash
./bin/publish.sh --action publish --channel release
```

```powershell
.\bin\publish.ps1 --action publish --channel test --dry-run
```

> Ensure `dart pub publish --dry-run` passes before release publish.

## License

MIT

## Regeneration Contract

- HTTP/OpenAPI generator-owned files are tracked in `.sdkwork/sdkwork-generator-manifest.json`.
- HTTP/OpenAPI generation also writes `.sdkwork/sdkwork-generator-changes.json` so automation can inspect created, updated, deleted, unchanged, scaffolded, and backed-up files plus the classified impact areas, verification plan, and execution decision for the latest generation.
- HTTP/OpenAPI apply mode also writes `.sdkwork/sdkwork-generator-report.json` with the full execution report, including `schemaVersion`, `generator`, stable artifact paths, and the execution handoff commands that match CLI `--json` output.
- CLI JSON output also includes an execution handoff with concrete next commands, including reviewed apply commands for dry-run flows.
- Put HTTP/OpenAPI hand-written wrappers, adapters, and orchestration in `custom/`.
- Files scaffolded under `custom/` are created once and preserved across HTTP/OpenAPI regenerations.
- If an HTTP/OpenAPI generated-owned file was modified locally, its previous content is copied to `.sdkwork/manual-backups/` before overwrite or removal.
- RPC SDK source workspaces use convention-first evidence by default: RPC SDK family naming, language workspace naming, `rpc/*.manifest.json`, proto source references, generated client source, and native package manifests.
- Use `sdkgen inspect --protocol rpc` to verify RPC convention evidence. Request persisted generator evidence only with `--emit-control-plane` for release, CI, audit, or migration workflows; evidence paths are derived by generator convention.
