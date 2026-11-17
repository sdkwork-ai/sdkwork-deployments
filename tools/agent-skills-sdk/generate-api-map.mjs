#!/usr/bin/env node
/**
 * Generate the Deploy API map used by the skills from the operation registry.
 * Platform administration surfaces (domain zones, certificates, cloud accounts,
 * download catalog) are excluded; the skills never call them.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '../..');
const OPERATIONS = JSON.parse(fs.readFileSync(path.join(HERE, 'operations.json'), 'utf8'));
const TARGET = path.join(REPO_ROOT, 'skills/sdkwork-app-deployments/references/api-map.md');

const EXCLUDED_PATH_SEGMENTS = [
  '/domain_zones',
  '/certificates',
  '/cloud_accounts',
  '/downloads/',
];

const entries = Object.values(OPERATIONS)
  .filter((entry) => !EXCLUDED_PATH_SEGMENTS.some((segment) => entry.path.includes(segment)))
  .sort((left, right) => (
    left.api.localeCompare(right.api, 'en')
    || left.operationId.localeCompare(right.operationId, 'en')
  ));

const rows = entries.map((entry) => (
  `| ${entry.api} | \`${entry.operationId}\` | ${entry.method} | \`${entry.path}\` | ${entry.summary.replaceAll('|', '\\|')} |`
));

const content = [
  '# Deploy API Map',
  '',
  'Generated from the authored OpenAPI contracts by',
  '`node tools/agent-skills-sdk/generate-api-map.mjs`. Do not hand-edit.',
  '',
  'Auth on every operation: `Authorization: Bearer <authToken>` plus',
  '`Access-Token: <accessToken>` (`authMode: "dual-token"`). The `iam` surface is',
  'credential-entry only and accepts the public application access token.',
  '',
  '| Surface | Operation | Method | Path | Summary |',
  '| --- | --- | --- | --- | --- |',
  ...rows,
  '',
  `Operations: ${entries.length}.`,
  '',
].join('\n');

const check = process.argv.includes('--check');
if (check) {
  const current = fs.existsSync(TARGET) ? fs.readFileSync(TARGET, 'utf8') : '';
  if (current !== content) {
    console.error('agent-skills-sdk: api-map.md is stale; run node tools/agent-skills-sdk/generate-api-map.mjs');
    process.exitCode = 1;
  } else {
    console.log('agent-skills-sdk: api-map.md is current');
  }
} else {
  fs.writeFileSync(TARGET, content);
  console.log(`agent-skills-sdk: wrote ${path.relative(REPO_ROOT, TARGET)} (${entries.length} operations)`);
}
