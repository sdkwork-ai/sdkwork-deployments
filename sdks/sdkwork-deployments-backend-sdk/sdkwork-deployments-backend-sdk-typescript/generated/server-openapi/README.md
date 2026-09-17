# sdkwork-deployments-backend-sdk

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
npm install @sdkwork/deployments-backend-sdk
# or
yarn add @sdkwork/deployments-backend-sdk
# or
pnpm add @sdkwork/deployments-backend-sdk
```

## Quick Start

```typescript
import { SdkworkDeployBackendClient } from '@sdkwork/deployments-backend-sdk';

const client = new SdkworkDeployBackendClient({
  baseUrl: 'http://127.0.0.1:3900',
  timeout: 30000,
});

// Authentication
client.setAuthToken('your-auth-token');
client.setAccessToken('your-access-token');

// Use the SDK
const result = await client.nginx.runtime.retrieve();
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```typescript
import { SdkworkDeployBackendClient } from '@sdkwork/deployments-backend-sdk';

const client = new SdkworkDeployBackendClient({
  baseUrl: 'http://127.0.0.1:3900',
  timeout: 30000, // Request timeout in ms
  headers: {      // Custom headers
    'X-Custom-Header': 'value',
  },
});
```

## API Modules

- `client.nginx` - nginx API
- `client.server` - server API
- `client.cluster` - cluster API
- `client.audit` - audit API
- `client.entitlement` - entitlement API
- `client.buildQueue` - build_queue API
- `client.runners` - runners API
- `client.tls` - tls API
- `client.retention` - retention API
- `client.usage` - usage API
- `client.signingHealth` - signing_health API
- `client.sourceEvents` - source_events API

## Usage Examples

### nginx

```typescript
// 获取 Nginx 状态
const result = await client.nginx.runtime.retrieve();
```

### server

```typescript
// 获取服务器节点列表
const params = {
  page: 1,
  page_size: 2,
  cluster_id: 'cluster_id',
};
const result = await client.server.list(params);
```

### cluster

```typescript
// 获取节点集群列表
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.cluster.list(params);
```

### audit

```typescript
// 获取审计日志列表
const params = {
  page: 1,
  page_size: 2,
  target_type: 'target_type',
  action: 'action',
  operator_id: 'operator_id',
  start_date: 'start_date',
  end_date: 'end_date',
  cursor: 'cursor',
};
const result = await client.audit.auditLogs.list(params);
```

### entitlement

```typescript
// List Commerce-backed entitlement projections
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.entitlement.list(params);
```

### build_queue

```typescript
// List queued builds waiting for or claimed by runners
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.buildQueue.list(params);
```

### runners

```typescript
// List runner liveness and workload health
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.runners.list(params);
```

### tls

```typescript
// List ACME accounts of the tenant
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.tls.tlsAccounts.list(params);
```

### retention

```typescript
// Apply platform retention policies
const body = {
  dryRun: true,
};
const result = await client.retention.run.create(body);
```

### usage

```typescript
// Ingest Web Server traffic usage events (per-domain / per-server-IP)
const body = {
  nodeUuid: 'nodeUuid',
  events: [
    {},
  ],
};
const result = await client.usage.ingest.create(body);
```

### signing_health

```typescript
// List signing identity expiry health
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.signingHealth.signingIdentityHealth.list(params);
```

### source_events

```typescript
// List ingested Git webhook events
const params = {
  page: 1,
  page_size: 2,
};
const result = await client.sourceEvents.list(params);
```

## Error Handling

```typescript
import { SdkworkDeployBackendClient, NetworkError, TimeoutError, AuthenticationError } from '@sdkwork/deployments-backend-sdk';

try {
  const result = await client.nginx.runtime.retrieve();
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
