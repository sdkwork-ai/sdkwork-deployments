#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { parse as parseYaml } from 'yaml';

const repoRoot = process.cwd();
const surfaces = [
  {
    source: 'apis/app-api/deploy/openapi.yaml',
    authority: 'apis/app-api/deploy/deploy-app-api.openapi.json',
    sdk: 'sdks/sdkwork-deployments-app-sdk/openapi/deploy-app-api.openapi.json',
    sdkGenerationInput: 'sdks/sdkwork-deployments-app-sdk/openapi/deploy-app-api.sdkgen.json',
    sdkManifest: 'sdks/sdkwork-deployments-app-sdk/sdk-manifest.json',
    componentSpec: 'sdks/sdkwork-deployments-app-sdk/specs/component.spec.json',
    routeManifest: 'sdks/_route-manifests/app-api/sdkwork-routes-deploy-app-api.route-manifest.json',
    routePackageName: 'sdkwork-routes-deploy-app-api',
    sdkFamily: 'sdkwork-deployments-app-sdk',
    consumerPackageName: '@sdkwork/deployments-app-sdk',
    apiAuthority: 'sdkwork-deploy-app-api',
    apiSurface: 'app-api',
    apiPrefix: '/app/v3/api',
  },
  {
    source: 'apis/backend-api/deploy/openapi.yaml',
    authority: 'apis/backend-api/deploy/deploy-backend-api.openapi.json',
    sdk: 'sdks/sdkwork-deployments-backend-sdk/openapi/deploy-backend-api.openapi.json',
    sdkGenerationInput: 'sdks/sdkwork-deployments-backend-sdk/openapi/deploy-backend-api.sdkgen.json',
    sdkManifest: 'sdks/sdkwork-deployments-backend-sdk/sdk-manifest.json',
    componentSpec: 'sdks/sdkwork-deployments-backend-sdk/specs/component.spec.json',
    routeManifest: 'sdks/_route-manifests/backend-api/sdkwork-routes-deploy-backend-api.route-manifest.json',
    routePackageName: 'sdkwork-routes-deploy-backend-api',
    sdkFamily: 'sdkwork-deployments-backend-sdk',
    consumerPackageName: '@sdkwork/deployments-backend-sdk',
    apiAuthority: 'sdkwork-deploy-backend-api',
    apiSurface: 'backend-api',
    apiPrefix: '/backend/v3/api',
  },
];

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function assertWellFormedUnicode(value, sourcePath, pathSegments = []) {
  if (typeof value === 'string') {
    assert.doesNotMatch(
      value,
      /[\uD800-\uDFFF]/,
      `${sourcePath}/${pathSegments.join('/')} contains an unpaired UTF-16 surrogate`,
    );
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) =>
      assertWellFormedUnicode(item, sourcePath, [...pathSegments, String(index)]),
    );
    return;
  }
  if (value && typeof value === 'object') {
    for (const [key, item] of Object.entries(value)) {
      assertWellFormedUnicode(item, sourcePath, [...pathSegments, key]);
    }
  }
}

function collectDocumentation(value, pathSegments = [], result = new Map()) {
  if (Array.isArray(value)) {
    value.forEach((item, index) =>
      collectDocumentation(item, [...pathSegments, String(index)], result),
    );
    return result;
  }
  if (!value || typeof value !== 'object') {
    return result;
  }
  for (const [key, item] of Object.entries(value)) {
    const itemPath = [...pathSegments, key];
    if ((key === 'summary' || key === 'description') && typeof item === 'string') {
      result.set(itemPath.join('/'), item);
    }
    collectDocumentation(item, itemPath, result);
  }
  return result;
}

function assertMaterializedDocumentationMatchesSource(source, authority, sourcePath) {
  const sourceDocumentation = collectDocumentation(source);
  for (const [documentationPath, value] of collectDocumentation(authority)) {
    assert.equal(
      sourceDocumentation.get(documentationPath),
      value,
      `${sourcePath}/${documentationPath} must match its materialized authority`,
    );
  }
}

function assertOwnerOnlyOperations(authority, surface) {
  let operationCount = 0;
  for (const [operationPath, pathItem] of Object.entries(authority.paths ?? {})) {
    assert.ok(
      operationPath.startsWith(surface.apiPrefix),
      `${surface.authority} contains an operation outside ${surface.apiPrefix}: ${operationPath}`,
    );
    for (const [method, operation] of Object.entries(pathItem)) {
      if (!['get', 'post', 'put', 'patch', 'delete'].includes(method)) {
        continue;
      }
      operationCount += 1;
      assert.equal(operation['x-sdkwork-owner'], 'sdkwork-deploy');
      assert.equal(operation['x-sdkwork-api-authority'], surface.apiAuthority);
      assert.equal(operation['x-sdkwork-api-surface'], surface.apiSurface);
    }
  }
  return operationCount;
}

function assertLocalResponseReferencesResolve(authority, sourcePath) {
  const responses = authority.components?.responses ?? {};
  for (const [operationPath, pathItem] of Object.entries(authority.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem)) {
      if (!['get', 'post', 'put', 'patch', 'delete'].includes(method)) {
        continue;
      }
      for (const response of Object.values(operation.responses ?? {})) {
        if (!response?.$ref?.startsWith('#/components/responses/')) {
          continue;
        }
        const responseName = response.$ref.slice('#/components/responses/'.length);
        assert.ok(
          responses[responseName],
          `${sourcePath} ${method.toUpperCase()} ${operationPath} has unresolved response ${response.$ref}`,
        );
      }
    }
  }
}

for (const surface of surfaces) {
  const source = parseYaml(fs.readFileSync(path.join(repoRoot, surface.source), 'utf8'));
  const authority = readJson(surface.authority);
  const sdk = readJson(surface.sdk);
  const sdkGenerationInput = readJson(surface.sdkGenerationInput);
  const sdkManifest = readJson(surface.sdkManifest);
  const componentSpec = readJson(surface.componentSpec);
  const routeManifest = readJson(surface.routeManifest);

  assertWellFormedUnicode(source, surface.source);
  assertMaterializedDocumentationMatchesSource(source, authority, surface.source);
  const ownerOnlyOperationCount = assertOwnerOnlyOperations(authority, surface);
  assertLocalResponseReferencesResolve(authority, surface.authority);
  assert.deepEqual(sdk, authority, `${surface.sdk} must match ${surface.authority}`);
  assert.deepEqual(
    sdkGenerationInput,
    authority,
    `${surface.sdkGenerationInput} must be deterministically derived from ${surface.authority}`,
  );
  assert.equal(routeManifest.packageName, surface.routePackageName);
  assert.equal(routeManifest.source.crateImport, surface.routePackageName.replaceAll('-', '_'));
  assert.equal(sdkManifest.sdkOwner, 'sdkwork-deploy');
  assert.equal(sdkManifest.apiAuthority, surface.apiAuthority);
  assert.equal(sdkManifest.sdkFamily, surface.sdkFamily);
  assert.equal(sdkManifest.packageName, surface.consumerPackageName);
  assert.equal(sdkManifest.ownerOnlyOperationCount, ownerOnlyOperationCount);
  assert.equal(sdkManifest.generationInputSpec, path.posix.relative(
    path.posix.dirname(surface.sdkManifest),
    surface.sdkGenerationInput,
  ));
  assert.deepEqual(sdkManifest.derivedSpecs, { default: sdkManifest.generationInputSpec });
  assert.equal(
    sdkManifest.typescript.composedRoot,
    `${surface.sdkFamily}-typescript`,
  );
  assert.equal(
    sdkManifest.typescript.transportRoot,
    `${surface.sdkFamily}-typescript/generated/server-openapi`,
  );
  assert.deepEqual(
    sdkManifest.sdkDependencies,
    componentSpec.contracts.sdkDependencies,
    `${surface.sdkManifest} sdkDependencies must match ${surface.componentSpec}`,
  );
}

const appRouteManifest = readJson(
  'sdks/_route-manifests/app-api/sdkwork-routes-deploy-app-api.route-manifest.json',
);
const compositionRoute = appRouteManifest.routes.find(
  (route) => route.operationId === 'apps.composition.update',
);
assert.ok(compositionRoute, 'apps.composition.update must be materialized into the app route manifest');
assert.equal(compositionRoute.idempotent, true);
assert.equal(compositionRoute.permission, 'deploy.apps.write');
assert.deepEqual(compositionRoute.auth, { mode: 'dual-token', required: true });

process.stdout.write('openapi-materialization.contract.test.mjs passed\n');
