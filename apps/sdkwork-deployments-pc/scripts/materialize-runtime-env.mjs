// Materializes the public runtime document (`public/runtime-env.json`) for one
// deployment lane from `etc/topology/<profile>.<environment>.env`.
//
// The document is public by contract — origin URLs and locale metadata only,
// never secrets. Standalone serves the SPAs and the API on one origin, so every
// base URL is the same-origin root `/`; cloud clients talk to the platform API
// gateway origin instead (`VITE_SDKWORK_DEPLOY_PLATFORM_API_GATEWAY_HTTP_URL`,
// falling back to its server-side twin).
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const { values } = parseArgs({
  options: {
    environment: { type: 'string', default: 'development' },
    'deployment-profile': { type: 'string', default: process.env.SDKWORK_DEPLOYMENT_PROFILE ?? 'standalone' },
    output: { type: 'string', default: 'public/runtime-env.json' },
  },
});

const environment = values.environment;
const deploymentProfile = values['deployment-profile'];
const profileId = `${deploymentProfile}.${environment}`;

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = path.resolve(appRoot, '..', '..');
const envPath = path.join(repoRoot, 'etc', 'topology', `${profileId}.env`);
const outputPath = path.resolve(appRoot, values.output);

function parseEnvFile(filePath) {
  const values = new Map();
  for (const line of fs.readFileSync(filePath, 'utf8').split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed === '' || trimmed.startsWith('#')) continue;
    const separator = trimmed.indexOf('=');
    if (separator <= 0) continue;
    values.set(trimmed.slice(0, separator), trimmed.slice(separator + 1));
  }
  return values;
}

const lane = parseEnvFile(envPath);
const readLaneValue = (key) => {
  const value = lane.get(key);
  if (value === undefined || value === '') {
    throw new Error(`${path.relative(repoRoot, envPath)} does not define ${key}`);
  }
  return value;
};

// Same-origin standalone: `/` for every API family (the edge serves the SPAs).
// Cloud: the platform API gateway origin for every API family.
const baseUrls =
  deploymentProfile === 'standalone'
    ? { appApiBaseUrl: '/', backendApiBaseUrl: '/', driveAppApiBaseUrl: '/', appbaseAppApiBaseUrl: '/', deployAppApiBaseUrl: '/', openApiBaseUrl: '/', sdkBaseUrl: '/' }
    : (() => {
        const gateway =
          lane.get('VITE_SDKWORK_DEPLOY_PLATFORM_API_GATEWAY_HTTP_URL') ??
          readLaneValue('SDKWORK_DEPLOY_PLATFORM_API_GATEWAY_HTTP_URL');
        return { appApiBaseUrl: gateway, backendApiBaseUrl: gateway, driveAppApiBaseUrl: gateway, appbaseAppApiBaseUrl: gateway, deployAppApiBaseUrl: gateway, openApiBaseUrl: gateway, sdkBaseUrl: gateway };
      })();

const document = {
  environment,
  deploymentProfile,
  profileId: readLaneValue('SDKWORK_DEPLOY_PROFILE_ID'),
  runtimeTarget: 'browser',
  browserOriginMode: deploymentProfile === 'standalone' ? 'same-origin' : 'cloud-gateway',
  defaultLocale: 'zh-CN',
  fallbackLocale: 'en-US',
  supportedLocales: ['zh-CN', 'en-US'],
  activeLocales: ['zh-CN', 'en-US'],
  ...baseUrls,
};

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify(document, null, 2)}\n`, 'utf8');
console.log(`materialized ${path.relative(repoRoot, outputPath)} for ${profileId}`);
