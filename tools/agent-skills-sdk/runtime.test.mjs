import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  inspectSdkPackage,
  inspectSdkPackages,
  needsTypeScriptTransform,
  parseEnvFile,
  redactSession,
  resolveBaseUrls,
  resolveSessionTokens,
  sanitizeProfile,
  substitutePath,
} from './env.mjs';
import {
  extractTokens,
  listOperations,
  problemDetails,
  resolveOperation,
} from './runtime.mjs';

test('parses env files with comments, quotes, and blank lines', () => {
  const values = parseEnvFile([
    '# comment',
    'SDKWORK_DEPLOY_APPLICATION_APP_HTTP_URL="http://127.0.0.1:3905"',
    "SDKWORK_DRIVE_APP_API_BASE_URL='http://127.0.0.1:3906'",
    '',
    'EMPTY=',
  ].join('\n'));
  assert.equal(values.SDKWORK_DEPLOY_APPLICATION_APP_HTTP_URL, 'http://127.0.0.1:3905');
  assert.equal(values.SDKWORK_DRIVE_APP_API_BASE_URL, 'http://127.0.0.1:3906');
  assert.equal(values.EMPTY, '');
});

test('resolves base URLs with flag > env > env file precedence', () => {
  const baseUrls = resolveBaseUrls({
    env: { SDKWORK_DEPLOY_APP_API_BASE_URL: 'http://env-app', SDKWORK_DEPLOY_BACKEND_API_BASE_URL: 'http://env-backend' },
    envFile: { SDKWORK_DEPLOY_APPLICATION_APP_HTTP_URL: 'http://file-app' },
    overrides: { app: 'http://flag-app' },
  });
  assert.equal(baseUrls.app, 'http://flag-app');
  assert.equal(baseUrls.backend, 'http://env-backend');
  assert.equal(baseUrls.drive, undefined);
});

test('substitutes and encodes path parameters', () => {
  assert.equal(
    substitutePath('/app/v3/api/apps/{appId}/source', { appId: 'a b' }),
    '/app/v3/api/apps/a%20b/source',
  );
  assert.throws(() => substitutePath('/apps/{appId}', {}), /missing path parameter "appId"/u);
});

test('rejects unsafe profile names', () => {
  assert.equal(sanitizeProfile('Standalone.Development'), 'standalone.development');
  assert.throws(() => sanitizeProfile('../escape'), /invalid profile/u);
});

test('environment tokens win over the cached session', () => {
  const tokens = resolveSessionTokens({
    env: { SDKWORK_AUTH_TOKEN: 'env-auth', SDKWORK_ACCESS_TOKEN: 'env-access' },
    session: { authToken: 'file-auth', accessToken: 'file-access', refreshToken: 'file-refresh' },
  });
  assert.equal(tokens.authToken, 'env-auth');
  assert.equal(tokens.accessToken, 'env-access');
  assert.equal(tokens.refreshToken, 'file-refresh');
});

test('redacts token values but keeps presence flags', () => {
  const redacted = redactSession({
    profile: 'standalone.development',
    authToken: 'secret-auth',
    accessToken: 'secret-access',
    refreshToken: 'secret-refresh',
  });
  assert.equal(redacted.hasAuthToken, true);
  assert.equal(redacted.hasAccessToken, true);
  assert.equal(redacted.hasRefreshToken, true);
  assert.equal(JSON.stringify(redacted).includes('secret'), false);
});

test('resolves a unique operation and requires an api for collisions', () => {
  assert.equal(resolveOperation({ operationId: 'apps.source.update' }).api, 'app');
  assert.equal(
    resolveOperation({ operationId: 'appTemplates.list', api: 'backend' }).path,
    '/backend/v3/api/app_templates',
  );
  assert.throws(
    () => resolveOperation({ operationId: 'appTemplates.list' }),
    /exists on both surfaces/u,
  );
});

test('lists operations by surface and filter', () => {
  const backendTemplates = listOperations({ api: 'backend', filter: 'appTemplates' });
  assert.ok(backendTemplates.length > 0);
  assert.ok(backendTemplates.every((entry) => entry.api === 'backend'));
  assert.ok(backendTemplates.every((entry) => entry.operationId.includes('appTemplates')));
});

test('extracts tokens from nested login response shapes', () => {
  const tokens = extractTokens({
    data: {
      tokens: {
        authToken: 'auth',
        accessToken: 'access',
        refreshToken: 'refresh',
        expiresIn: '3600',
      },
      tenantId: '100001',
      organizationId: '0',
    },
  });
  assert.equal(tokens.authToken, 'auth');
  assert.equal(tokens.accessToken, 'access');
  assert.equal(tokens.refreshToken, 'refresh');
  assert.equal(tokens.tenantId, '100001');
  assert.equal(tokens.organizationId, '0');
  assert.match(tokens.expiresAt, /^\d{4}-\d{2}-\d{2}T/u);
});

test('maps ProblemDetail error fields without leaking the raw payload', () => {
  const details = problemDetails({
    code: 40001,
    traceId: 'trace-1',
    i18nKey: 'deploy.invalid',
    locale: 'zh-CN',
    title: 'Invalid',
    detail: 'bad input',
    data: { token: 'secret' },
  });
  assert.deepEqual(details, {
    code: 40001,
    traceId: 'trace-1',
    title: 'Invalid',
    detail: 'bad input',
    i18nKey: 'deploy.invalid',
    locale: 'zh-CN',
  });
});

function sdkFixture(entryFile, { name = '@sdkwork/test-sdk', version = '1.2.3' } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sdkwork-skill-sdk-'));
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ name: 'consumer' }));
  const packageDir = path.join(root, 'node_modules', ...name.split('/'));
  fs.mkdirSync(packageDir, { recursive: true });
  fs.writeFileSync(
    path.join(packageDir, 'package.json'),
    JSON.stringify({ name, version, main: entryFile }),
  );
  fs.writeFileSync(path.join(packageDir, entryFile), 'export const ok = true;\n');
  return { root, name, entryFile };
}

test('detects a published npm SDK as built JavaScript', () => {
  const fixture = sdkFixture('index.js');
  const entry = inspectSdkPackage(fixture.root, fixture.name);
  assert.equal(entry.status, 'resolved');
  assert.equal(entry.kind, 'javascript');
  assert.equal(entry.version, '1.2.3');
  assert.equal(needsTypeScriptTransform(inspectSdkPackages(fixture.root, [fixture.name])), false);
});

test('detects a workspace SDK as TypeScript source', () => {
  const fixture = sdkFixture('index.ts');
  const entry = inspectSdkPackage(fixture.root, fixture.name);
  assert.equal(entry.kind, 'typescript-source');
  assert.equal(needsTypeScriptTransform(inspectSdkPackages(fixture.root, [fixture.name])), true);
});

test('reports a missing SDK without throwing', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sdkwork-skill-sdk-'));
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ name: 'consumer' }));
  assert.deepEqual(inspectSdkPackage(root, '@sdkwork/missing-sdk'), {
    name: '@sdkwork/missing-sdk',
    status: 'missing',
  });
});
