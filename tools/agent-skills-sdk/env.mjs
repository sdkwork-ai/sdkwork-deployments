import fs from 'node:fs';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';

export const SDK_PACKAGES = [
  '@sdkwork/deployments-app-sdk',
  '@sdkwork/deployments-backend-sdk',
  '@sdkwork/drive-app-sdk',
  '@sdkwork/iam-app-sdk',
];

export const BASE_URL_ENV_KEYS = {
  app: ['SDKWORK_DEPLOY_APP_API_BASE_URL', 'SDKWORK_DEPLOY_APPLICATION_APP_HTTP_URL'],
  backend: ['SDKWORK_DEPLOY_BACKEND_API_BASE_URL', 'SDKWORK_DEPLOY_APPLICATION_BACKEND_HTTP_URL'],
  drive: ['SDKWORK_DRIVE_APP_API_BASE_URL', 'SDKWORK_DRIVE_APPLICATION_APP_HTTP_URL'],
  iam: ['SDKWORK_IAM_APP_API_BASE_URL', 'SDKWORK_APPBASE_APP_API_BASE_URL'],
};

export const SESSION_ENV_KEYS = {
  authToken: ['SDKWORK_AUTH_TOKEN'],
  accessToken: ['SDKWORK_ACCESS_TOKEN'],
  refreshToken: ['SDKWORK_REFRESH_TOKEN'],
};

const PROFILE_PATTERN = /^[a-z0-9][a-z0-9._-]{0,63}$/u;

export function parseEnvFile(text) {
  const values = {};
  for (const rawLine of String(text ?? '').split(/\r?\n/u)) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) continue;
    const separator = line.indexOf('=');
    if (separator <= 0) continue;
    const key = line.slice(0, separator).trim();
    let value = line.slice(separator + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"'))
      || (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    values[key] = value;
  }
  return values;
}

export function loadEnvFile(filePath) {
  if (!filePath) return {};
  return parseEnvFile(fs.readFileSync(filePath, 'utf8'));
}

export function firstDefined(values) {
  for (const value of values) {
    if (typeof value === 'string' && value.trim() !== '') return value.trim();
  }
  return undefined;
}

export function resolveBaseUrls({ env = process.env, envFile = {}, overrides = {} } = {}) {
  const merged = { ...envFile, ...env };
  const resolved = {};
  for (const [surface, keys] of Object.entries(BASE_URL_ENV_KEYS)) {
    resolved[surface] = firstDefined([overrides[surface], ...keys.map((key) => merged[key])]);
  }
  return resolved;
}

export function resolveProfile({ explicit, env = process.env } = {}) {
  if (explicit) return sanitizeProfile(explicit);
  const deploymentProfile = firstDefined([env.SDKWORK_DEPLOYMENT_PROFILE]);
  const environment = firstDefined([env.SDKWORK_ENVIRONMENT]);
  if (deploymentProfile && environment) return sanitizeProfile(`${deploymentProfile}.${environment}`);
  return sanitizeProfile(firstDefined([env.SDKWORK_DEPLOY_PROFILE]) ?? 'default');
}

export function sanitizeProfile(value) {
  const normalized = String(value ?? '').toLowerCase().trim();
  if (!PROFILE_PATTERN.test(normalized)) {
    throw new Error(`invalid profile "${value}"; use lowercase letters, digits, dot, underscore, or hyphen`);
  }
  return normalized;
}

export function sessionDirectory(home = os.homedir()) {
  return path.join(home, '.sdkwork', 'agent-skills');
}

export function sessionFilePath(profile, home = os.homedir()) {
  return path.join(sessionDirectory(home), `${profile}.json`);
}

export function readSession(profile, home = os.homedir()) {
  const filePath = sessionFilePath(profile, home);
  if (!fs.existsSync(filePath)) return null;
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch {
    throw new Error(`session file is not valid JSON: ${filePath}`);
  }
}

export function writeSession(profile, session, home = os.homedir()) {
  const directory = sessionDirectory(home);
  fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
  const filePath = sessionFilePath(profile, home);
  const payload = {
    ...session,
    schemaVersion: 1,
    profile,
    updatedAt: new Date().toISOString(),
  };
  fs.writeFileSync(filePath, `${JSON.stringify(payload, null, 2)}\n`, { mode: 0o600 });
  try {
    fs.chmodSync(filePath, 0o600);
  } catch {
    // Windows does not implement POSIX modes; the file is still user-private.
  }
  return filePath;
}

export function removeSession(profile, home = os.homedir()) {
  const filePath = sessionFilePath(profile, home);
  if (fs.existsSync(filePath)) fs.unlinkSync(filePath);
  return filePath;
}

export function redactSession(session) {
  if (!session) return null;
  return {
    profile: session.profile,
    baseUrls: session.baseUrls,
    tenantId: session.tenantId,
    organizationId: session.organizationId,
    hasAuthToken: typeof session.authToken === 'string' && session.authToken.length > 0,
    hasAccessToken: typeof session.accessToken === 'string' && session.accessToken.length > 0,
    hasRefreshToken: typeof session.refreshToken === 'string' && session.refreshToken.length > 0,
    expiresAt: session.expiresAt,
    updatedAt: session.updatedAt,
  };
}

export function substitutePath(template, pathParams = {}) {
  return template.replace(/\{([^}]+)\}/gu, (match, name) => {
    if (pathParams[name] === undefined) {
      throw new Error(`missing path parameter "${name}" for ${template}`);
    }
    return encodeURIComponent(String(pathParams[name]));
  });
}

export function parseKeyValues(values) {
  const result = {};
  for (const entry of values ?? []) {
    const separator = entry.indexOf('=');
    if (separator <= 0) throw new Error(`expected key=value, got "${entry}"`);
    result[entry.slice(0, separator).trim()] = entry.slice(separator + 1).trim();
  }
  return result;
}

export function resolveSessionTokens({ env = process.env, session = null } = {}) {
  return {
    authToken: firstDefined([...SESSION_ENV_KEYS.authToken.map((key) => env[key]), session?.authToken]),
    accessToken: firstDefined([...SESSION_ENV_KEYS.accessToken.map((key) => env[key]), session?.accessToken]),
    refreshToken: firstDefined([...SESSION_ENV_KEYS.refreshToken.map((key) => env[key]), session?.refreshToken]),
  };
}

export function requireBaseUrl(surface, baseUrls) {
  const value = baseUrls?.[surface];
  if (!value) {
    const keys = BASE_URL_ENV_KEYS[surface] ?? [];
    throw new Error(
      `missing ${surface} API base URL; pass --${surface}-base-url or set ${keys.join(' / ')}`,
    );
  }
  return value;
}

export function requireTokens(tokens, { requireAuthToken = true } = {}) {
  if (requireAuthToken && !tokens.authToken) {
    throw new Error(
      'missing session auth token; run the skill login command or set SDKWORK_AUTH_TOKEN',
    );
  }
  if (!tokens.accessToken) {
    throw new Error(
      'missing access token; run the skill login command or set SDKWORK_ACCESS_TOKEN (public application access token)',
    );
  }
  return tokens;
}

/**
 * Inspect the installed SDK package that the target app root resolves.
 *
 * Published packages export built JavaScript (`dist/*.js`) and never need the
 * TypeScript transform. A local SDKWork workspace checkout exposes `src/index.ts`,
 * which the launcher supports as a development fallback.
 */
export function inspectSdkPackage(cwd, name) {
  let resolved;
  try {
    resolved = createRequire(path.join(cwd, 'package.json')).resolve(name);
  } catch {
    return { name, status: 'missing' };
  }
  const kind = resolved.endsWith('.ts') ? 'typescript-source' : 'javascript';
  let version;
  let directory = path.dirname(resolved);
  const stop = path.parse(directory).root;
  while (directory !== stop) {
    const manifest = path.join(directory, 'package.json');
    if (fs.existsSync(manifest)) {
      try {
        const parsed = JSON.parse(fs.readFileSync(manifest, 'utf8'));
        if (parsed.name === name) {
          version = parsed.version;
          break;
        }
      } catch {
        // Keep walking to the package root.
      }
    }
    directory = path.dirname(directory);
  }
  return {
    name,
    status: 'resolved',
    kind,
    resolved,
    ...(version ? { version } : {}),
  };
}

export function inspectSdkPackages(cwd, names = SDK_PACKAGES) {
  return Object.fromEntries(names.map((name) => [name, inspectSdkPackage(cwd, name)]));
}

export function needsTypeScriptTransform(packages) {
  return Object.values(packages).some((entry) => entry.kind === 'typescript-source');
}

