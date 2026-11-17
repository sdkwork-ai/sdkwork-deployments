import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

import {
  requireBaseUrl,
  requireTokens,
  resolveSessionTokens,
  substitutePath,
} from './env.mjs';

const OPERATIONS = JSON.parse(
  fs.readFileSync(new URL('./operations.json', import.meta.url), 'utf8'),
);

export class SkillRuntimeError extends Error {
  constructor(message, details = {}) {
    super(message);
    this.name = 'SkillRuntimeError';
    this.details = details;
  }
}

export function listOperations({ api, filter } = {}) {
  return Object.values(OPERATIONS)
    .filter((entry) => !api || entry.api === api)
    .filter((entry) => {
      if (!filter) return true;
      const needle = filter.toLowerCase();
      return entry.operationId.toLowerCase().includes(needle)
        || entry.path.toLowerCase().includes(needle);
    })
    .sort((left, right) => left.operationId.localeCompare(right.operationId, 'en'));
}

export function resolveOperation({ operationId, api } = {}) {
  if (!operationId) throw new SkillRuntimeError('missing operationId');
  if (api) {
    const entry = OPERATIONS[`${api}:${operationId}`];
    if (!entry) throw new SkillRuntimeError(`unknown ${api} operation "${operationId}"`);
    return entry;
  }
  const matches = Object.values(OPERATIONS).filter((entry) => entry.operationId === operationId);
  if (matches.length === 1) return matches[0];
  if (matches.length === 0) {
    const suggestions = listOperations({ filter: operationId.split('.').at(-1) })
      .slice(0, 5)
      .map((entry) => entry.operationId);
    throw new SkillRuntimeError(
      `unknown operation "${operationId}"${suggestions.length ? `; did you mean ${suggestions.join(', ')}?` : ''}`,
    );
  }
  throw new SkillRuntimeError(
    `operation "${operationId}" exists on both surfaces; pass --api ${matches.map((entry) => entry.api).join(' or --api ')}`,
  );
}

export function importFromCwd(cwd, specifier) {
  const require = createRequire(path.join(cwd, 'package.json'));
  let resolved;
  try {
    resolved = require.resolve(specifier);
  } catch {
    throw new SkillRuntimeError(
      `cannot resolve ${specifier} from ${cwd}; install the published npm SDK in that app root per APP_SDK_INTEGRATION_SPEC.md`,
    );
  }
  return import(pathToFileURL(resolved).href);
}

async function createTokenManager(cwd, tokens) {
  const common = await importFromCwd(cwd, '@sdkwork/sdk-common');
  return common.createTokenManager({
    authToken: tokens.authToken,
    accessToken: tokens.accessToken,
    refreshToken: tokens.refreshToken,
  });
}

export async function createSurfaceClient({ cwd, api, baseUrls, tokens }) {
  const tokenManager = await createTokenManager(cwd, tokens);
  const baseUrl = requireBaseUrl(api, baseUrls);
  const config = { baseUrl, authMode: 'dual-token', platform: 'agent', tokenManager };
  const sdk = api === 'app'
    ? await importFromCwd(cwd, '@sdkwork/deployments-app-sdk')
    : await importFromCwd(cwd, '@sdkwork/deployments-backend-sdk');
  return { client: sdk.createClient(config), tokenManager, baseUrl };
}

export async function createDriveClient({ cwd, baseUrls, tokens }) {
  const tokenManager = await createTokenManager(cwd, tokens);
  const baseUrl = requireBaseUrl('drive', baseUrls);
  const sdk = await importFromCwd(cwd, '@sdkwork/drive-app-sdk');
  return {
    client: sdk.createClient({ baseUrl, authMode: 'dual-token', platform: 'agent', tokenManager }),
    baseUrl,
  };
}

export async function createIamClient({ cwd, baseUrls, appAccessToken }) {
  const baseUrl = requireBaseUrl('iam', baseUrls);
  const sdk = await importFromCwd(cwd, '@sdkwork/iam-app-sdk');
  return {
    client: sdk.createClient({
      baseUrl,
      accessToken: appAccessToken,
      authMode: 'apikey',
      platform: 'agent',
    }),
    baseUrl,
  };
}

export async function callApi({
  cwd,
  api,
  operationId,
  method,
  path,
  pathParams = {},
  query,
  body,
  unwrap = 'data',
  baseUrls,
  tokens,
}) {
  let entry = null;
  if (operationId) {
    entry = resolveOperation({ operationId, api });
    api = entry.api;
  }
  if (!path) path = entry?.path;
  if (!method) method = entry?.method ?? 'GET';
  if (!path) throw new SkillRuntimeError('missing --operation or --path');
  if (!api) throw new SkillRuntimeError('missing --api for a raw --method/--path request');

  const resolvedPath = substitutePath(path, pathParams);
  const { client } = await createSurfaceClient({ cwd, api, baseUrls, tokens });
  const options = {
    method,
    params: query,
    sdkworkUnwrapKind: unwrap,
  };
  if (body !== undefined && method !== 'GET' && method !== 'HEAD' && method !== 'DELETE') {
    options.body = body;
  }
  return client.http.request(resolvedPath, options);
}

export async function uploadArtifact({
  cwd,
  baseUrls,
  tokens,
  appId,
  filePath,
  packageType = 1,
  contentType = 'application/zip',
  scene = 'deployment-package',
  idempotencyKey,
  onProgress,
}) {
  requireTokens(tokens);
  const bytes = await readFile(filePath);
  if (bytes.byteLength === 0) throw new SkillRuntimeError(`artifact file is empty: ${filePath}`);
  const digest = createHash('sha256').update(bytes).digest('hex');
  const fileName = path.basename(filePath);
  const idempotency = idempotencyKey
    ?? `artifact-${createHash('sha256').update(`${appId}:${digest}`).digest('hex').slice(0, 32)}`;

  const drive = await createDriveClient({ cwd, baseUrls, tokens });
  const blob = {
    size: bytes.byteLength,
    type: contentType,
    name: fileName,
    arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  };
  const upload = await drive.client.uploader.uploadArchive({
    file: blob,
    appResourceType: 'deploy.artifact',
    appResourceId: appId,
    scene,
    originalFileName: fileName,
    contentType,
    checksumSha256Hex: digest,
    ...(onProgress ? { onProgress } : {}),
  });

  const { client: deploy } = await createSurfaceClient({ cwd, api: 'app', baseUrls, tokens });
  const artifact = await deploy.artifact.create(
    {
      siteId: appId,
      packageType,
      fileName,
      contentType,
      contentLength: String(bytes.byteLength),
      checksumSha256: digest,
      driveUploadSessionId: upload.uploadSession?.id,
      driveUploadItemId: upload.uploadItem?.id,
      driveSpaceId: upload.uploadItem?.spaceId,
      driveNodeId: upload.uploadItem?.nodeId,
      idempotencyKey: idempotency,
    },
    { idempotencyKey: idempotency },
  );

  return {
    artifact,
    upload: {
      uploadItemId: upload.uploadItem?.id,
      uploadSessionId: upload.uploadSession?.id,
      driveSpaceId: upload.uploadItem?.spaceId,
      driveNodeId: upload.uploadItem?.nodeId,
    },
    checksumSha256: digest,
    byteSize: bytes.byteLength,
    fileName,
  };
}

function isRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function extractTokens(response) {
  const layers = [
    response,
    response?.tokens,
    response?.session,
    response?.data,
    response?.result,
    response?.item,
    response?.data?.tokens,
    response?.data?.session,
  ].filter(isRecord);

  const pick = (...keys) => {
    for (const layer of layers) {
      for (const key of keys) {
        const value = layer[key];
        if (typeof value === 'string' && value.trim() !== '') return value.trim();
      }
    }
    return undefined;
  };

  const tokens = {
    authToken: pick('authToken', 'auth_token'),
    accessToken: pick('accessToken', 'access_token'),
    refreshToken: pick('refreshToken', 'refresh_token'),
  };
  if (!tokens.authToken && !tokens.accessToken) {
    throw new SkillRuntimeError('login response did not contain authToken or accessToken', {
      responseKeys: isRecord(response) ? Object.keys(response) : typeof response,
    });
  }
  const tenantId = pick('tenantId', 'tenant_id');
  const organizationId = pick('organizationId', 'organization_id');
  const expiresInRaw = pick('expiresIn', 'expires_in');
  const expiresIn = expiresInRaw && /^\d+$/u.test(expiresInRaw) ? Number(expiresInRaw) : undefined;
  return {
    ...tokens,
    ...(tenantId ? { tenantId } : {}),
    ...(organizationId ? { organizationId } : {}),
    ...(expiresIn ? { expiresAt: new Date(Date.now() + expiresIn * 1000).toISOString() } : {}),
  };
}

export async function login({
  cwd,
  baseUrls,
  appAccessToken,
  grantType = 'password',
  account,
  email,
  username,
  phone,
  password,
  deviceId,
}) {
  if (!appAccessToken) {
    throw new SkillRuntimeError(
      'missing public application access token; set SDKWORK_ACCESS_TOKEN (public app token) or pass --access-token-file',
    );
  }
  if (!password) throw new SkillRuntimeError('missing password; pass --password-stdin or set SDKWORK_LOGIN_PASSWORD');
  const { client, baseUrl } = await createIamClient({ cwd, baseUrls, appAccessToken });
  const response = await client.auth.sessions.create({
    grantType,
    ...(account ? { account } : {}),
    ...(email ? { email } : {}),
    ...(username ? { username } : {}),
    ...(phone ? { phone } : {}),
    password,
    ...(deviceId ? { deviceId } : {}),
  });
  return { ...extractTokens(response), iamBaseUrl: baseUrl };
}

export async function refresh({ cwd, baseUrls, appAccessToken, refreshToken }) {
  if (!refreshToken) throw new SkillRuntimeError('missing refresh token');
  const { client } = await createIamClient({ cwd, baseUrls, appAccessToken });
  const response = await client.auth.sessions.refresh({ refreshToken });
  return { ...extractTokens(response), refreshToken };
}

export async function logout({ cwd, baseUrls, appAccessToken, refreshToken }) {
  if (!refreshToken) throw new SkillRuntimeError('missing refresh token');
  const { client } = await createIamClient({ cwd, baseUrls, appAccessToken });
  await client.auth.sessions.current.logout({ refreshToken });
}

export function problemDetails(error) {
  const candidate = isRecord(error) ? error : {};
  const problem = isRecord(candidate.problem) ? candidate.problem : candidate;
  const details = {
    code: problem.code ?? candidate.code,
    traceId: problem.traceId ?? candidate.traceId,
    status: problem.status ?? candidate.status,
    title: problem.title,
    detail: problem.detail ?? candidate.message,
    i18nKey: problem.i18nKey,
    locale: problem.locale,
  };
  return Object.fromEntries(Object.entries(details).filter(([, value]) => value !== undefined));
}

export function buildTokens({ env, session }) {
  return resolveSessionTokens({ env, session });
}
