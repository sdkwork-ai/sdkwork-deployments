#!/usr/bin/env node
/**
 * Launch the skill CLI.
 *
 * SDKWork SDKs are published to npm and resolve to built JavaScript. In that case
 * the CLI runs on plain Node. A local sdkwork-space checkout resolves the public
 * package export to `src/index.ts`, so the launcher switches to the TypeScript
 * transform and resolver only for that development layout.
 */
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

import { inspectSdkPackages, needsTypeScriptTransform } from './env.mjs';

function resolveCwd(argv) {
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--cwd') return path.resolve(argv[index + 1] ?? '.');
    if (arg.startsWith('--cwd=')) return path.resolve(arg.slice('--cwd='.length));
  }
  return process.cwd();
}

const argv = process.argv.slice(2);
const cwd = resolveCwd(argv);
const cli = fileURLToPath(new URL('./cli.mjs', import.meta.url));
const register = pathToFileURL(fileURLToPath(new URL('./register-ts.mjs', import.meta.url))).href;

const workspaceMode = needsTypeScriptTransform(inspectSdkPackages(cwd));
const nodeArgs = workspaceMode
  ? ['--experimental-transform-types', '--no-warnings', '--import', register, cli, ...argv]
  : [cli, ...argv];

if (workspaceMode) {
  console.error('agent-skills-sdk: local workspace TypeScript SDK sources detected; using the development transform');
}

const result = spawnSync(process.execPath, nodeArgs, { stdio: 'inherit' });
if (result.error) throw result.error;
process.exit(result.status ?? 1);
