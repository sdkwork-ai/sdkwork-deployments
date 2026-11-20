#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { buildDeployContext, validateDeploy } from '../../../sdkwork-specs/tools/deploy/validate.mjs';
import { planDeploy } from '../../../sdkwork-specs/tools/deploy/plan.mjs';
import { renderNginxSite } from '../../../sdkwork-specs/tools/deploy/nginx-render.mjs';
import { validateDeploySchema } from '../../../sdkwork-specs/tools/deploy/schema-validate.mjs';
import { inspectNginxConfig } from '../../../sdkwork-specs/tools/deploy/nginx-lifecycle.mjs';
import { resolveDomainSurfaceId } from '../../../sdkwork-specs/tools/deploy/topology-env.mjs';

const repoRoot = process.cwd();

const deployResult = validateDeploy(repoRoot);
assert.equal(
  deployResult.ok,
  true,
  `deploy validation failed: ${(deployResult.errors ?? []).join('; ')}`,
);
assert.equal(deployResult.appId, 'sdkwork-deployments');
assert.equal(deployResult.runtimeCode, 'deploy');

const plan = planDeploy(repoRoot);
assert.equal(plan.ok, true, `deploy plan failed: ${(plan.errors ?? []).join('; ')}`);
assert.ok(plan.topology, 'plan must include topology for nginx render');
assert.ok(plan.overrides !== undefined, 'plan must include overrides for nginx render');

// The repository moved from an nginx-served deployment to a container-image
// (kubernetes) driver; `resolveUpstreams` deliberately answers `{}` for a
// non-nginx driver. The nginx-render contract below still has to be exercised,
// but only when the descriptor actually declares the nginx driver — asserting
// loopback upstreams unconditionally failed on every run after that migration and
// blocked the whole `pnpm test` gate. The branch keeps both models honest: the
// nginx model is fully asserted where it applies, and the container model asserts
// that no nginx upstream was fabricated.
const deployContext = buildDeployContext(repoRoot);
const deploymentDriver = deployContext.deployment?.deploymentDriver ?? 'nginx';
if (deploymentDriver !== 'nginx') {
  assert.deepEqual(
    plan.upstreams,
    {},
    'a non-nginx deployment driver must not fabricate nginx upstreams',
  );
  assert.equal(
    deployContext.deployment?.deliveryKind,
    'container-image',
    'the kubernetes driver is a container-image delivery',
  );
}

// Nginx rendering is a shared-tooling contract, not a property of this
// repository's own descriptor. When the descriptor is nginx-driven the plan
// already carries the loopback upstream; when it is container-image driven the
// plan deliberately has none, so the fixture supplies the upstream the renderer
// under test consumes. This keeps `renderNginxSite` fully covered without
// asserting that this repository deploys through nginx.
const renderPlan = deploymentDriver === 'nginx'
  ? plan
  : {
      ...plan,
      upstreams: { ...plan.upstreams, application: 'http://127.0.0.1:3900' },
};
assert.equal(
  renderPlan.upstreams.application,
  'http://127.0.0.1:3900',
  'the nginx render fixture must resolve a loopback upstream',
);

// The repository's `cloud.production` profile declares one `mode: api` expose
// entry per brand domain with app/public aliases. The renderer resolves the
// surface from that mode (`api` -> `application.backend-http`) and puts every
// alias in `server_name`, so the contract checks that real shape instead of the
// retired three-separate-expose-entries shape.
const primaryDomain = renderPlan.expose[0].domain;
const backendSite = renderNginxSite(renderPlan, primaryDomain);
assert.match(backendSite.mainConfig, /# surface: application\.backend-http/);
assert.match(backendSite.mainConfig, /location \/backend\/v3\/api\//);
assert.doesNotMatch(backendSite.mainConfig, /location \/app\/v3\/api\//);
assert.match(
  backendSite.mainConfig,
  new RegExp(`server_name ${primaryDomain.replaceAll('.', '\\.')}`),
  'the primary domain must be the first server_name entry',
);
for (const alias of renderPlan.expose[0].aliases ?? []) {
  assert.match(
    backendSite.mainConfig,
    new RegExp(alias.replaceAll('.', '\\.')),
    `alias ${alias} must appear in the rendered server_name`,
  );
}
assert.match(
  backendSite.mainConfig,
  /proxy_pass http:\/\/127\.0\.0\.1:3900/,
  'the backend surface must proxy to the resolved upstream',
);
assert.doesNotMatch(
  backendSite.mainConfig,
  /127\.0\.0\.1:8080/,
  'the rendered config must not fall back to a legacy development port',
);

// strip-prefix keeps the API prefix in nginx and rewrites it out of the
// upstream request.
const stripPlan = {
  ...renderPlan,
  expose: renderPlan.expose.map((item) =>
    item.domain === primaryDomain ? { ...item, apiPathStyle: 'strip-prefix' } : item,
  ),
};
const stripSite = renderNginxSite(stripPlan, primaryDomain);
assert.match(
  stripSite.mainConfig,
  /location \/backend\/v3\/api\/[\s\S]*proxy_pass http:\/\/127\.0\.0\.1:3900\/;/,
  'strip-prefix must proxy to the upstream root',
);

assert.deepEqual(
  validateDeploySchema({ version: 3, profile: 'cloud.production', expose: [] }),
  ['version: must be 1 or 2'],
  'schema validation must reject invalid version',
);

const nginxInspection = inspectNginxConfig(backendSite.mainConfig);
assert.equal(nginxInspection.valid, true, nginxInspection.errors?.join('; '));

// Adaptive Web: the same entry promoted to `web+api` with two client surfaces
// must render a UA map and a variable snippet selector, and must never use a
// variable `root` SPA fallback.
const adaptiveDomain = primaryDomain;
const adaptivePlan = {
  ...renderPlan,
  expose: renderPlan.expose.map((item) =>
    item.domain === adaptiveDomain
      ? {
          ...item,
          mode: 'web+api',
          web: { mode: 'adaptive', surfaces: ['pc', 'h5'], warnings: [] },
          webRoots: [
            { surface: 'pc', path: '/usr/share/sdkwork/deploy/web/pc/' },
            { surface: 'h5', path: '/usr/share/sdkwork/deploy/web/h5/' },
          ],
        }
      : item,
  ),
};
const adaptiveSite = renderNginxSite(adaptivePlan, adaptiveDomain);
assert.match(
  adaptiveSite.mainConfig,
  /map \$http_user_agent \$sdkwork_sdkwork_deployments_surface \{/,
  'adaptive nginx must define a UA map',
);
assert.match(
  adaptiveSite.mainConfig,
  /try_files\s+\/__sdkwork_adaptive_dispatch__\s+\$sdkwork_sdkwork_deployments_surface_final;/,
  'adaptive nginx must dispatch through the resolved surface via try_files',
);
assert.doesNotMatch(
  adaptiveSite.mainConfig,
  /set\s+\$root\s+/,
  'adaptive nginx must not use a variable root SPA fallback',
);

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'sdkwork-deploy-contract-'));
try {
  const rendered = renderNginxSite(adaptivePlan, adaptiveDomain, { snippetDir: tmpDir });
  assert.ok(rendered.snippets.length >= 2, 'nginx render must emit pc and h5 web snippets');
} finally {
  fs.rmSync(tmpDir, { recursive: true, force: true });
}

process.stdout.write('deploy-standard.contract.test.mjs passed\n');