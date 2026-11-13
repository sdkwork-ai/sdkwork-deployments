#!/usr/bin/env node
/**
 * Regression contract for the application-template category bootstrap.
 *
 * A fresh tenant cannot publish a template without an ACTIVE category. The
 * category seed is therefore part of every seed profile, must carry stable
 * reserved ids, and must remain idempotent. This test catches a profile that
 * silently drops the bootstrap or a taxonomy edit that duplicates a key.
 */
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const root = process.cwd();
const seedPath = path.join('database', 'seeds', 'common', '015_deploy_app_template_categories.sql');
assert.equal(existsSync(path.join(root, seedPath)), true, `${seedPath} must exist`);

const manifest = JSON.parse(readFileSync(path.join(root, 'database', 'seeds', 'seed.manifest.json'), 'utf8'));
for (const [profile, config] of Object.entries(manifest.profiles)) {
  assert.equal(
    config.common.includes('common/015_deploy_app_template_categories.sql'),
    true,
    `seed profile "${profile}" must initialize template categories`,
  );
}

const sql = readFileSync(path.join(root, seedPath), 'utf8');
const rows = [...sql.matchAll(
  /\(\s*(9100013000\d{2}),\s*'([0-9a-f-]{36})',\s*'([a-z0-9-]+)',/gu,
)];
const ids = rows.map(row => row[1]);
const uuids = rows.map(row => row[2]);
const keys = rows.map(row => row[3]);
assert.equal(ids.length, 10, 'the bootstrap taxonomy must contain ten categories');
assert.equal(new Set(ids).size, ids.length, 'reserved category ids must be unique');
assert.equal(new Set(uuids).size, uuids.length, 'category uuids must be unique');
assert.equal(new Set(keys).size, keys.length, 'category keys must be unique');
assert.deepEqual(keys, [
  'web-h5',
  'pc-desktop',
  'mobile-native',
  'cross-platform',
  'mini-program',
  'api-service',
  'full-stack',
  'ai-agent',
  'developer-tooling',
  'other',
]);
assert.equal(sql.includes('ON CONFLICT (tenant_id, category_key) WHERE deleted_at IS NULL'), true,
  'the seed must be idempotent by tenant/key');
assert.equal(sql.includes('100001'), true, 'the platform tenant must own the bootstrap taxonomy');

process.stdout.write('template-category-seed.contract.test.mjs passed\n');
