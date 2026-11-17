#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  inspectSdkPackages,
  loadEnvFile,
  needsTypeScriptTransform,
  parseKeyValues,
  readSession,
  redactSession,
  removeSession,
  requireTokens,
  resolveBaseUrls,
  resolveProfile,
  resolveSessionTokens,
  sessionFilePath,
  writeSession,
} from './env.mjs';
import {
  callApi,
  importFromCwd,
  listOperations,
  login,
  logout,
  problemDetails,
  refresh,
  resolveOperation,
  uploadArtifact,
} from './runtime.mjs';

const USAGE = `Usage:
  run.mjs login   --iam-base-url <url> --account <id> --password-stdin [--profile <p>]
  run.mjs refresh [--profile <p>]
  run.mjs logout  [--profile <p>]
  run.mjs api     --operation <operationId> --path-param k=v --query k=v --input <file|->
  run.mjs api     --api app|backend --method GET --path /app/v3/api/apps
  run.mjs upload  --app-id <id> --file <archive> [--package-type 1] [--drive-base-url <url>]
  run.mjs ops     [--api app|backend] [--filter <text>]
  run.mjs doctor  [--profile <p>]

Global options:
  --cwd <path>   application root used to resolve SDK packages (default: current directory)

Base URLs (flag / environment):
  app      --base-url          SDKWORK_DEPLOY_APP_API_BASE_URL
  backend  --backend-base-url  SDKWORK_DEPLOY_BACKEND_API_BASE_URL
  drive    --drive-base-url    SDKWORK_DRIVE_APP_API_BASE_URL
  iam      --iam-base-url      SDKWORK_IAM_APP_API_BASE_URL

Credentials:
  SDKWORK_ACCESS_TOKEN   public application access token (safe to share, required for login)
  SDKWORK_AUTH_TOKEN     session bearer token
  SDKWORK_LOGIN_PASSWORD password for the login command (or use --password-stdin)
  Session cache: ~/.sdkwork/agent-skills/<profile>.json (0600, user-private)`;

function parseArgs(argv) {
  const args = { _: [], 'path-param': [], query: [] };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      args.help = true;
    } else if (arg === '--dry-run') {
      args['dry-run'] = true;
    } else if (arg === '--json') {
      args.json = true;
    } else if (arg === '--password-stdin') {
      args['password-stdin'] = true;
    } else if (arg === '--path-param' || arg === '--query') {
      args[arg.slice(2)].push(argv[++index]);
    } else if (arg.startsWith('--')) {
      const key = arg.slice(2);
      const next = argv[index + 1];
      if (next === undefined || next.startsWith('--')) {
        args[key] = true;
      } else {
        args[key] = next;
        index += 1;
      }
    } else {
      args._.push(arg);
    }
  }
  return args;
}

function readStdin() {
  return fs.readFileSync(0, 'utf8').trim();
}

function readJsonInput(value, cwd) {
  if (value === undefined) return undefined;
  const text = value === '-' ? readStdin() : fs.readFileSync(path.resolve(cwd, value), 'utf8');
  if (text.trim() === '') return undefined;
  return JSON.parse(text);
}

function print(value) {
  console.log(JSON.stringify(value, null, 2));
}

function mergeBaseUrls(session, resolved) {
  const merged = { ...(session?.baseUrls ?? {}) };
  for (const [surface, value] of Object.entries(resolved)) {
    if (value) merged[surface] = value;
  }
  return merged;
}

function contextFromArgs(args) {
  const cwd = path.resolve(args.cwd ?? process.cwd());
  const profile = resolveProfile({ explicit: args.profile, env: process.env });
  const envFile = args['env-file'] ? loadEnvFile(path.resolve(args['env-file'])) : {};
  const session = readSession(profile);
  const baseUrls = mergeBaseUrls(session, resolveBaseUrls({
    env: process.env,
    envFile,
    overrides: {
      app: args['base-url'],
      backend: args['backend-base-url'],
      drive: args['drive-base-url'],
      iam: args['iam-base-url'],
    },
  }));
  const tokens = resolveSessionTokens({ env: process.env, session });
  return { cwd, profile, baseUrls, session, tokens };
}

function publicAppToken(args) {
  if (args['access-token-file']) {
    return fs.readFileSync(path.resolve(args['access-token-file']), 'utf8').trim();
  }
  return process.env.SDKWORK_ACCESS_TOKEN;
}

async function runLogin(args) {
  const { cwd, profile, baseUrls } = contextFromArgs(args);
  const password = args['password-stdin']
    ? readStdin()
    : process.env.SDKWORK_LOGIN_PASSWORD;
  const result = await login({
    cwd,
    baseUrls,
    appAccessToken: publicAppToken(args),
    grantType: args['grant-type'] ?? 'password',
    account: args.account,
    email: args.email,
    username: args.username,
    phone: args.phone,
    password,
    deviceId: args['device-id'],
  });
  const session = {
    profile,
    baseUrls,
    authToken: result.authToken,
    accessToken: result.accessToken,
    refreshToken: result.refreshToken,
    tenantId: result.tenantId,
    organizationId: result.organizationId,
    expiresAt: result.expiresAt,
  };
  const filePath = args['no-save'] ? null : writeSession(profile, session);
  return {
    profile,
    sessionFile: filePath,
    saved: Boolean(filePath),
    iamBaseUrl: result.iamBaseUrl,
    tenantId: result.tenantId,
    organizationId: result.organizationId,
    expiresAt: result.expiresAt,
  };
}

async function runRefresh(args) {
  const { profile, baseUrls, session, tokens } = contextFromArgs(args);
  if (!session) throw new Error(`no saved session for profile "${profile}"`);
  const result = await refresh({
    cwd,
    baseUrls,
    appAccessToken: publicAppToken(args),
    refreshToken: tokens.refreshToken,
  });
  const next = {
    ...session,
    ...result,
    profile,
    baseUrls,
  };
  writeSession(profile, next);
  return redactSession(next);
}

async function runLogout(args) {
  const { cwd, profile, baseUrls, session, tokens } = contextFromArgs(args);
  if (session?.refreshToken || tokens.refreshToken) {
    try {
      await logout({
        cwd,
        baseUrls,
        appAccessToken: publicAppToken(args),
        refreshToken: tokens.refreshToken ?? session.refreshToken,
      });
    } catch (error) {
      console.error(`agent-skills-sdk: remote logout failed, clearing local session only: ${error.message}`);
    }
  }
  removeSession(profile);
  return { profile, sessionFile: sessionFilePath(profile), cleared: true };
}

async function runApi(args) {
  const { cwd, baseUrls, tokens } = contextFromArgs(args);
  const pathParams = parseKeyValues(args['path-param']);
  const query = parseKeyValues(args.query);
  const body = readJsonInput(args.input, cwd);
  let operation = null;
  if (args.operation) {
    operation = resolveOperation({ operationId: args.operation, api: args.api });
  }
  const api = args.api ?? operation?.api;
  const method = args.method ?? operation?.method ?? 'GET';
  const requestPath = args.path ?? operation?.path;

  if (args['dry-run']) {
    return {
      dryRun: true,
      api,
      operation: args.operation,
      method,
      path: requestPath,
      pathParams,
      query,
      bodyKeys: body === undefined ? [] : Object.keys(body),
      hasAuthToken: Boolean(tokens.authToken),
      hasAccessToken: Boolean(tokens.accessToken),
    };
  }

  requireTokens(tokens);
  return callApi({
    cwd,
    api,
    operationId: args.operation,
    method: args.method,
    path: args.path,
    pathParams,
    query,
    body,
    unwrap: args.unwrap ?? 'data',
    baseUrls,
    tokens,
  });
}

async function runUpload(args) {
  const { cwd, baseUrls, tokens } = contextFromArgs(args);
  if (!args['app-id']) throw new Error('missing --app-id');
  if (!args.file) throw new Error('missing --file');
  const resolved = path.resolve(cwd, args.file);
  if (!fs.existsSync(resolved)) throw new Error(`artifact file does not exist: ${resolved}`);
  if (args['dry-run']) {
    const size = fs.statSync(resolved).size;
    return {
      dryRun: true,
      appId: args['app-id'],
      file: resolved,
      byteSize: size,
      packageType: Number(args['package-type'] ?? 1),
      contentType: args['content-type'] ?? 'application/zip',
      scene: args.scene ?? 'deployment-package',
      hasAuthToken: Boolean(tokens.authToken),
      hasAccessToken: Boolean(tokens.accessToken),
      hasDriveBaseUrl: Boolean(baseUrls.drive),
    };
  }
  requireTokens(tokens);
  return uploadArtifact({
    cwd,
    baseUrls,
    tokens,
    appId: args['app-id'],
    filePath: resolved,
    packageType: Number(args['package-type'] ?? 1),
    contentType: args['content-type'] ?? 'application/zip',
    scene: args.scene ?? 'deployment-package',
    idempotencyKey: args['idempotency-key'],
  });
}

function runOps(args) {
  return listOperations({ api: args.api, filter: args.filter }).map((entry) => ({
    api: entry.api,
    operationId: entry.operationId,
    method: entry.method,
    path: entry.path,
  }));
}

async function runDoctor(args) {
  const { cwd, profile, baseUrls, session, tokens } = contextFromArgs(args);
  const sdkPackages = inspectSdkPackages(cwd);
  const runtimeMode = needsTypeScriptTransform(sdkPackages) ? 'workspace-typescript' : 'npm-javascript';
  const contractVersions = readContractVersions();
  const versionChecks = {};
  for (const [name, entry] of Object.entries(sdkPackages)) {
    const contract = contractVersions[name];
    if (entry.status !== 'resolved' || !contract) continue;
    versionChecks[name] = entry.version === contract
      ? 'match'
      : entry.version
        ? `mismatch (installed ${entry.version}, contract ${contract})`
        : 'unknown';
  }
  if (args['check-sdk-import']) {
    for (const [name, entry] of Object.entries(sdkPackages)) {
      if (entry.status !== 'resolved') continue;
      try {
        await importFromCwd(cwd, name);
        entry.importStatus = 'imported';
      } catch (error) {
        entry.importStatus = `import-failed: ${error.message}`;
      }
    }
  }
  return {
    cwd,
    profile,
    sessionFile: sessionFilePath(profile),
    session: redactSession(session),
    baseUrls,
    runtimeMode,
    tokens: {
      authToken: Boolean(tokens.authToken),
      accessToken: Boolean(tokens.accessToken),
      refreshToken: Boolean(tokens.refreshToken),
    },
    sdkPackages,
    versionChecks,
  };
}

function readContractVersions() {
  const repoRoot = fileURLToPath(new URL('../../', import.meta.url));
  const candidates = [
    'sdks/sdkwork-deployments-app-sdk/sdkwork-deployments-app-sdk-typescript/package.json',
    'sdks/sdkwork-deployments-backend-sdk/sdkwork-deployments-backend-sdk-typescript/package.json',
  ];
  const versions = {};
  for (const candidate of candidates) {
    const manifest = path.join(repoRoot, candidate);
    if (!fs.existsSync(manifest)) continue;
    try {
      const parsed = JSON.parse(fs.readFileSync(manifest, 'utf8'));
      if (parsed.name && parsed.version) versions[parsed.name] = parsed.version;
    } catch {
      // A missing contract version only weakens the diagnostic.
    }
  }
  return versions;
}

async function main(argv) {
  const [command, ...rest] = argv;
  if (!command || command === '--help' || command === '-h' || command === 'help') {
    console.log(USAGE);
    return;
  }
  const args = parseArgs(rest);
  if (args.help) {
    console.log(USAGE);
    return;
  }
  const handlers = {
    login: runLogin,
    refresh: runRefresh,
    logout: runLogout,
    api: runApi,
    upload: runUpload,
    ops: runOps,
    doctor: runDoctor,
  };
  const handler = handlers[command];
  if (!handler) throw new Error(`unknown command "${command}"`);
  print(await handler(args));
}

try {
  await main(process.argv.slice(2));
} catch (error) {
  const details = problemDetails(error);
  console.error(JSON.stringify({
    error: error.message,
    ...(Object.keys(details).length > 0 ? details : {}),
    ...(error.details && Object.keys(error.details).length > 0 ? { context: error.details } : {}),
  }, null, 2));
  process.exitCode = 1;
}
