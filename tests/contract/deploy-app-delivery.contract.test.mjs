// Contract test: unified app delivery vocabulary, package standard rules,
// version management invariants, and deployment target compatibility must
// agree between the design documents, the OpenAPI contract, and the
// migration schema (REQ-2026-0002).

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

const readJson = (relative) =>
  JSON.parse(fs.readFileSync(path.join(root, relative), 'utf-8'));

// ---------------------------------------------------------------------------
// OpenAPI surface
// ---------------------------------------------------------------------------

const openapi = readJson('apis/app-api/deploy/deploy-app-api.openapi.json');
const appApi = openapi;

const operations = new Set();
for (const pathItem of Object.values(appApi.paths ?? {})) {
  for (const method of Object.keys(pathItem ?? {})) {
    if (['get', 'post', 'put', 'patch', 'delete'].includes(method)) {
      operations.add(pathItem[method]?.operationId);
    }
  }
}

// Required app-delivery resource groups are present.
for (const operation of [
  'apps.list',
  'apps.create',
  'apps.retrieve',
  'apps.sourceSpecs.list',
  'apps.sourceSpecs.create',
  'apps.sourceSpecs.retrieve',
  'apps.sourceSpecs.update',
  'apps.sourceSpecs.delete',
  'apps.sourceSpecs.bindSource',
  'platformTargets.create',
  'sourceRepositories.create',
  'buildTemplates.create',
  'builds.create',
  'builds.stateUpdate',
  'packages.register',
  'releases.create',
  'channels.promote',
  'channels.rollouts.list',
  'deployments.create',
  'signingIdentities.create',
]) {
  assert.ok(operations.has(operation), `missing OpenAPI operation ${operation}`);
}

// App kind vocabulary matches the design.
const appKind = appApi.components?.schemas?.AppKind;
assert.ok(appKind, 'AppKind schema is missing');
assert.deepEqual(appKind.enum, [
  'STATIC_WEB',
  'SPA_WEB',
  'API_SERVICE',
  'WECHAT_MINIPROGRAM',
  'DOUYIN_MINIPROGRAM',
  'IOS_APP',
  'ANDROID_APP',
  'HARMONYOS_APP',
]);

const deploymentKind = appApi.components?.schemas?.DeploymentKind;
assert.ok(deploymentKind, 'DeploymentKind schema is missing');
for (const kind of [
  'ARTIFACT_RELEASE',
  'SITE_CONFIG',
  'TLS_CONFIG',
  'MINIPROGRAM_REVIEW',
  'STORE_SUBMISSION',
  'OTA_DISTRIBUTION',
  'ENTERPRISE_DISTRIBUTION',
  'CONTAINER_ROLLOUT',
]) {
  assert.ok(deploymentKind.enum.includes(kind), `missing DeploymentKind ${kind}`);
}

// ---------------------------------------------------------------------------
// Baseline schema: new tables exist and legacy tables gain additive columns.
// Pre-launch the greenfield migration inventory is consolidated on the single
// baseline snapshot, so the app-delivery contract is asserted against the
// baseline instead of the folded 0007 migration.
// ---------------------------------------------------------------------------

const migration = fs.readFileSync(
  path.join(root, 'database/ddl/baseline/postgres/0001_deploy_baseline.sql'),
  'utf-8',
);

const expectedTables = [
  'deploy_app',
  'deploy_app_platform_target',
  'deploy_app_source_spec',
  'deploy_app_variant',
  'deploy_app_variant_rule',
  'deploy_app_mount',
  'deploy_app_binding',
  'deploy_source_repository',
  'deploy_build_template',
  'deploy_build',
  'deploy_package',
  'deploy_release_channel',
  'deploy_channel_rollout',
  'deploy_signing_identity',
];
for (const table of expectedTables) {
  assert.ok(
    migration.includes(`CREATE TABLE IF NOT EXISTS ${table}`),
    `baseline is missing table ${table}`,
  );
}

// Additive compatibility columns on legacy tables.
for (const column of ['app_id', 'semantic_version', 'deployment_kind']) {
  assert.ok(
    migration.includes(`ADD COLUMN IF NOT EXISTS ${column}`),
    `baseline is missing additive column ${column}`,
  );
}

// Semantic version uniqueness index is present.
assert.ok(
  migration.includes('uk_deploy_release_app_target_version'),
  'baseline is missing the (app, target, version) uniqueness index',
);

// Table registry and schema contract agree.
const tableRegistry = readJson('database/contract/table-registry.json');
const registered = new Set(tableRegistry.tables.map((row) => row.table_name));
for (const table of expectedTables) {
  assert.ok(registered.has(table), `table registry is missing ${table}`);
}

const schemaContract = fs.readFileSync(
  path.join(root, 'database/contract/schema.yaml'),
  'utf-8',
);
for (const table of expectedTables) {
  assert.ok(
    schemaContract.includes(`- name: ${table}`),
    `schema.yaml is missing ${table}`,
  );
}

// ---------------------------------------------------------------------------
// Design documents agree on vocabulary
// ---------------------------------------------------------------------------

const techDoc = fs.readFileSync(
  path.join(root, 'docs/architecture/tech/TECH-unified-app-delivery-platform.md'),
  'utf-8',
);
const reqDoc = fs.readFileSync(
  path.join(root, 'docs/product/requirements/REQ-2026-0002-unified-app-delivery-platform.md'),
  'utf-8',
);

for (const token of [
  'sdkwork.deploy-package.v1',
  'WECHAT_MINIPROGRAM',
  'DOUYIN_MINIPROGRAM',
  'HARMONYOS_APP',
  'build_number',
  'deploy_channel_rollout',
  'MINIPROGRAM_REVIEW',
  'CONTAINER_ROLLOUT',
]) {
  assert.ok(
    techDoc.includes(token) && reqDoc.includes(token),
    `design documents disagree on vocabulary ${token}`,
  );
}

// ---------------------------------------------------------------------------
// Deployment kind/target compatibility pairs from the service validation
// ---------------------------------------------------------------------------

const compatiblePairs = [
  ['MINIPROGRAM_REVIEW', '"WECHAT_REVIEW" | "DOUYIN_REVIEW"'],
  ['STORE_SUBMISSION', '"APP_STORE_CONNECT" | "TESTFLIGHT" | "HARMONYOS_STORE"'],
  ['OTA_DISTRIBUTION', '"OTA"'],
  ['ENTERPRISE_DISTRIBUTION', '"ENTERPRISE"'],
  ['CONTAINER_ROLLOUT', '"CONTAINER"'],
  ['ARTIFACT_RELEASE', '"WEB_NODE"'],
  ['SITE_CONFIG', '"WEB_NODE"'],
  ['TLS_CONFIG', '"WEB_NODE"'],
];
const serviceSource = fs.readFileSync(
  path.join(root, 'crates/sdkwork-intelligence-deploy-service/src/app_delivery.rs'),
  'utf-8',
);
const pairBlock = serviceSource.slice(serviceSource.indexOf('validate_deployment_pair'));
// rustfmt may wrap matches! patterns across lines; compare whitespace-compact.
const compactBlock = pairBlock.replace(/\s+/g, '');
for (const [kind, targets] of compatiblePairs) {
  const needle = `("${kind}", ${targets})`.replace(/\s+/g, '');
  assert.ok(
    compactBlock.includes(needle),
    `service validation is missing compatible pair ${kind}/${targets}`,
  );
}

process.stdout.write('deploy-app-delivery.contract.test.mjs passed\n');
