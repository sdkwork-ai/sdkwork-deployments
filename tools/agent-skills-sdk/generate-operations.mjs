#!/usr/bin/env node
/**
 * Regenerate the skill runtime operation registry from the authored OpenAPI
 * contracts that this repository already owns.
 *
 * Operation ids collide between the app and backend surfaces (for example
 * `appTemplates.list`), so the registry is keyed `<api>:<operationId>` and callers
 * select the surface explicitly when an id exists on both.
 *
 * Usage:
 *   node tools/agent-skills-sdk/generate-operations.mjs
 *   node tools/agent-skills-sdk/generate-operations.mjs --check
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '../..');

const SOURCES = [
  { api: 'app', file: 'apis/app-api/deploy/deploy-app-api.openapi.json' },
  { api: 'backend', file: 'apis/backend-api/deploy/deploy-backend-api.openapi.json' },
];

const HTTP_METHODS = ['get', 'post', 'put', 'patch', 'delete'];

function buildRegistry() {
  const registry = {};
  for (const source of SOURCES) {
    const absolute = path.join(REPO_ROOT, source.file);
    const document = JSON.parse(fs.readFileSync(absolute, 'utf8'));
    for (const [openApiPath, item] of Object.entries(document.paths ?? {})) {
      for (const method of HTTP_METHODS) {
        const operation = item[method];
        if (!operation?.operationId) continue;
        registry[`${source.api}:${operation.operationId}`] = {
          api: source.api,
          operationId: operation.operationId,
          method: method.toUpperCase(),
          path: openApiPath,
          summary: operation.summary ?? '',
        };
      }
    }
  }
  return Object.fromEntries(
    Object.entries(registry).sort(([left], [right]) => left.localeCompare(right, 'en')),
  );
}

function serialize(registry) {
  return `${JSON.stringify(registry, null, 2)}\n`;
}

function main(argv) {
  const check = argv.includes('--check');
  const target = path.join(HERE, 'operations.json');
  const registry = buildRegistry();
  const next = serialize(registry);
  if (check) {
    const current = fs.existsSync(target) ? fs.readFileSync(target, 'utf8') : '';
    if (current !== next) {
      console.error('agent-skills-sdk: operations.json is stale; run node tools/agent-skills-sdk/generate-operations.mjs');
      process.exitCode = 1;
      return;
    }
    console.log('agent-skills-sdk: operations.json is current');
    return;
  }
  fs.writeFileSync(target, next);
  console.log(`agent-skills-sdk: wrote ${path.relative(REPO_ROOT, target)} (${Object.keys(registry).length} operations)`);
}

main(process.argv.slice(2));
