#!/usr/bin/env node
/**
 * Skill-local launcher. Delegates to the shared agent-skills-sdk runtime so both
 * skills use one auth and API implementation.
 */
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const runner = path.resolve(here, '../../../tools/agent-skills-sdk/run.mjs');
const result = spawnSync(process.execPath, [runner, ...process.argv.slice(2)], { stdio: 'inherit' });
if (result.error) throw result.error;
process.exit(result.status ?? 1);
