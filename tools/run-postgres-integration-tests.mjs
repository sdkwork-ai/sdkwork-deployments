#!/usr/bin/env node
// Run the repository's `#[ignore]`d PostgreSQL integration tests.
//
// Every database-backed integration test in this repository is declared
// `#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL"]` so that a plain
// `cargo test` stays runnable without a database. The cost of that default is
// that nothing runs them: `cargo test --workspace` skips all of them, and no
// gate in the repository passed `--ignored` or exported
// `SDKWORK_DATABASE_TEST_POSTGRES_URL` at all.
//
// That blind spot is not theoretical. The per-app domain-suffix override was
// written as `app_domain_suffixes JSONB` and bound a bare `Vec<String>` at both
// write sites, so `apps.create` / `apps.update` returned a masked 500 for any
// payload carrying `appDomainSuffixes`; the read site then swallowed the
// matching decode failure with `.ok()`, so the value silently degraded to the
// platform catalog. Every one of those defects lived entirely inside the
// `--ignored` set and shipped through a green CI.
//
// Behaviour:
//   - Without `SDKWORK_DATABASE_TEST_POSTGRES_URL`, this is a documented no-op
//     that exits 0, so environments without a database are unaffected.
//   - With it set, the ignored tests run and a failure fails the gate.
//   - `certificate_issuance_pebble` is skipped: it needs an external ACME CA
//     (`SDKWORK_DEPLOY_PEBBLE_DIRECTORY_URL`) and is exercised by its own
//     end-to-end script, not by this gate.
//
// Usage:
//   node tools/run-postgres-integration-tests.mjs
//   node tools/run-postgres-integration-tests.mjs --package <crate>

import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const DATABASE_URL_ENV = 'SDKWORK_DATABASE_TEST_POSTGRES_URL';

/// Test files that need more than a PostgreSQL instance. Each entry is a reason
/// the gate must not run it.
const EXTERNAL_DEPENDENCY_TESTS = new Map([
  [
    'certificate_issuance_pebble',
    'requires an external ACME CA (SDKWORK_DEPLOY_PEBBLE_DIRECTORY_URL)',
  ],
]);

/// `common` is a shared module, not a test binary.
const NON_TEST_MODULES = new Set(['common']);

const args = process.argv.slice(2);
const packageFlag = args.indexOf('--package');
const packageName =
  packageFlag >= 0 && args[packageFlag + 1]
    ? args[packageFlag + 1]
    : 'sdkwork-intelligence-deploy-repository-sqlx';

function log(message) {
  process.stdout.write(`${message}\n`);
}

const databaseUrl = process.env[DATABASE_URL_ENV];
if (!databaseUrl) {
  log(
    `${DATABASE_URL_ENV} is not set; skipping the PostgreSQL integration gate ` +
      '(set it to a disposable database URL to run the #[ignore]d tests).',
  );
  process.exit(0);
}

const testsDir = path.join('crates', packageName, 'tests');
if (!existsSync(testsDir)) {
  log(`no tests directory at ${testsDir}; nothing to run.`);
  process.exit(0);
}

const testBinaries = readdirSync(testsDir)
  .filter((name) => name.endsWith('.rs'))
  .map((name) => name.slice(0, -'.rs'.length))
  .filter((name) => !NON_TEST_MODULES.has(name))
  .sort();

const runnable = [];
const skipped = [];
for (const name of testBinaries) {
  const reason = EXTERNAL_DEPENDENCY_TESTS.get(name);
  if (reason) skipped.push([name, reason]);
  else runnable.push(name);
}

if (skipped.length > 0) {
  log('Skipping tests that need an external service:');
  for (const [name, reason] of skipped) log(`  - ${name}: ${reason}`);
}

log(
  `Running ${runnable.length} PostgreSQL integration binaries from ${testsDir} ` +
    '(the #[ignore]d set)…',
);

let passed = 0;
let failed = 0;
const failures = [];

for (const binary of runnable) {
  // Serial per binary: the harness gives every test its own randomly-named
  // schema, but test binaries in this crate create schemas concurrently and
  // contention on `CREATE SCHEMA` produces spurious failures. Serialising is
  // the documented way to run this suite, and it keeps the gate deterministic.
  const result = spawnSync(
    'cargo',
    [
      'test',
      '-p',
      packageName,
      '--test',
      binary,
      '--',
      '--ignored',
      '--test-threads=1',
    ],
    { stdio: 'inherit', shell: process.platform === 'win32' },
  );

  const combined = result.status ?? 1;
  if (result.error) {
    failed += 1;
    failures.push(`${binary}: ${result.error.message}`);
    continue;
  }
  if (combined === 0) {
    passed += 1;
  } else {
    failed += 1;
    failures.push(binary);
  }
}

log('');
log(`PostgreSQL integration gate: ${passed} binaries passed, ${failed} failed.`);
if (failed > 0) {
  log('Failing binaries:');
  for (const failure of failures) log(`  - ${failure}`);
  process.exit(1);
}
