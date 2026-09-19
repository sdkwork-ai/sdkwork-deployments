# sdkwork-deployments-app-sdk

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
npm install @sdkwork/deployments-app-sdk
# or
yarn add @sdkwork/deployments-app-sdk
# or
pnpm add @sdkwork/deployments-app-sdk
```

## Quick Start

```typescript
import { SdkworkDeployAppClient } from '@sdkwork/deployments-app-sdk';

const client = new SdkworkDeployAppClient({
  baseUrl: 'http://127.0.0.1:3900',
  timeout: 30000,
});

// Authentication
client.setAuthToken('your-auth-token');
client.setAccessToken('your-access-token');

// Use the SDK
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.app.list(params);
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```typescript
import { SdkworkDeployAppClient } from '@sdkwork/deployments-app-sdk';

const client = new SdkworkDeployAppClient({
  baseUrl: 'http://127.0.0.1:3900',
  timeout: 30000, // Request timeout in ms
  headers: {      // Custom headers
    'X-Custom-Header': 'value',
  },
});
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

## Usage Examples

### domain

```typescript
// List root domain zones
const params = {
  page: 1,
  page_size: 2,
  status: 'ACTIVE',
  keyword: 'keyword',
};
const result = await client.domain.domainZones.list(params);
```

### certificate

```typescript
// 获取证书列表
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.certificate.list(params);
```

### upload_session

```typescript
// 获取上传会话
const uploadSessionId = '1';
const result = await client.uploadSession.retrieve(uploadSessionId);
```

### artifact

```typescript
// 获取租户制品列表
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.artifact.list(params);
```

### app

```typescript
// List tenant apps
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.app.list(params);
```

### env_variable

```typescript
// 获取环境变量列表
const appId = '1';
const params = {
  environment: 'environment',
};
const result = await client.envVariable.apps.envVariables.list(appId, params);
```

### monitor

```typescript
// 获取健康检查配置
const appId = '1';
const result = await client.monitor.apps.healthChecks.list(appId);
```

### build

```typescript
// List tenant build templates
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.build.buildTemplates.list(params);
```

### package

```typescript
// List deployment packages of an app
const appId = '1';
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.package.list(appId, params);
```

### release

```typescript
// List release channels of an app
const appId = '1';
const result = await client.release.channels.list(appId);
```

### deployment

```typescript
// List deployments of an app
const appId = '1';
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.deployment.list(appId, params);
```

### signing

```typescript
// List tenant signing identities
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.signing.signingIdentities.list(params);
```

### usage

```typescript
// List tenant usage metering events
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.usage.usageEvents.list(params);
```

### app_database

```typescript
// List the database structure contracts of an app
const appId = '1';
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.appDatabase.appDatabaseProfiles.list(appId, params);
```

### app_environment

```typescript
// List the environments of an app
const appId = '1';
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.appEnvironment.list(appId, params);
```

## Error Handling

```typescript
import { SdkworkDeployAppClient, NetworkError, TimeoutError, AuthenticationError } from '@sdkwork/deployments-app-sdk';

try {
  const params = {
    page: 1,
    page_size: 2,
  };
  const result = await client.app.list(params);
} catch (error) {
  if (error instanceof AuthenticationError) {
    console.error('Authentication failed:', error.message);
  } else if (error instanceof TimeoutError) {
    console.error('Request timed out:', error.message);
  } else if (error instanceof NetworkError) {
    console.error('Network error:', error.message);
  } else {
    throw error;
  }
}
```

## Publishing

This SDK includes cross-platform publish scripts in `bin/`:
- `bin/publish-core.mjs`
- `bin/publish.sh`
- `bin/publish.ps1`

TypeScript check and publish commands materialize workspace dependency versions in a temporary tarball with pnpm. They reject local-only dependency protocols before npm publication and do not rewrite the source `package.json`.

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

> Configure npm registry credentials before release publish.

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
