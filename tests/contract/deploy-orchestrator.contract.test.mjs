// WORKSPACE-PATH:allow-fixture - this file is a test fixture that simulates a foreign
// checkout root, so the sdkwork-<name> segment below is the value under assertion rather
// than a binding to a real sibling checkout. PORTABILITY_SPEC.md section 5.2 governs it.
import assert from 'node:assert/strict';
import { parseSdkworkDeployBinding } from '../../../sdkwork-specs/tools/deploy/site-binding.mjs';

const binding = parseSdkworkDeployBinding(
  {
    sdkworkDeploy: {
      appRoot: 'E:/sdkwork-space/sdkwork-im',
      profileId: 'cloud.production',
    },
  },
  'im.sdkwork.com',
);

assert.equal(binding.appRoot, 'E:/sdkwork-space/sdkwork-im');
assert.equal(binding.domain, 'im.sdkwork.com');
assert.equal(binding.profileId, 'cloud.production');

process.stdout.write('deploy-orchestrator.contract.test.mjs passed\n');
