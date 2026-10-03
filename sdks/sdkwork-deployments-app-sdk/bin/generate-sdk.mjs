#!/usr/bin/env node

import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const scriptRoot = path.dirname(fileURLToPath(import.meta.url));
const familyRoot = path.resolve(scriptRoot, '..');
const applicationRoot = path.resolve(familyRoot, '..', '..');
const workspaceRoot = path.resolve(applicationRoot, '..');
const generator = path.join(workspaceRoot, 'sdkwork-sdk-generator', 'bin', 'sdkgen.js');
const materialize = spawnSync(process.execPath, [
  path.join(applicationRoot, 'tools', 'materialize_deploy_phase1_contracts.mjs'),
], { cwd: applicationRoot, stdio: 'inherit' });
if (materialize.status !== 0) process.exit(materialize.status ?? 1);

// One generator run per language. TypeScript feeds the PC/H5 web clients;
// flutter emits the Dart package the mobile client consumes
// (`apps/sdkwork-deployments-flutter`). Both read the same sdkgen input, so a
// contract change lands in every language at once.
const languages = [
  {
    language: 'typescript',
    output: path.join(
      familyRoot,
      'sdkwork-deployments-app-sdk-typescript',
      'generated',
      'server-openapi',
    ),
    packageName: '@sdkwork/deployments-app-sdk',
    extraArgs: ['--client-name', 'SdkworkDeployAppClient'],
  },
  {
    language: 'flutter',
    output: path.join(
      familyRoot,
      'sdkwork-deployments-app-sdk-flutter',
      'generated',
      'server-openapi',
    ),
    packageName: 'sdkwork_deployments_app_sdk',
    extraArgs: [],
  },
];

for (const target of languages) {
  const generated = spawnSync(process.execPath, [
    generator,
    'generate',
    '-i', path.join(familyRoot, 'openapi', 'deploy-app-api.sdkgen.json'),
    '-o', target.output,
    '-n', 'sdkwork-deployments-app-sdk',
    '-t', 'app',
    '-l', target.language,
    '--fixed-sdk-version', '0.1.0',
    '--base-url', 'http://127.0.0.1:3900',
    '--api-prefix', '/app/v3/api',
    '--package-name', target.packageName,
    '--standard-profile', 'sdkwork-v3',
    '--sdk-root', familyRoot,
    '--sdk-name', 'sdkwork-deployments-app-sdk',
    '--no-sync-published-version',
    ...target.extraArgs,
  ], { cwd: applicationRoot, stdio: 'inherit' });
  if (generated.status !== 0) process.exit(generated.status ?? 1);
}
