import { describe, expect, it, vi } from 'vitest';
import type {
  DriveUploaderBlobLike,
  DriveUploaderRequest,
  DriveUploaderUploadResult,
} from '@sdkwork/drive-app-sdk';
import {
  createDeployApplicationPublisher,
  type ApplicationPublishDeployment,
  type ApplicationPublishProgress,
  type ApplicationPublishRequest,
  type ApplicationPublisherDeployClient,
  type ApplicationPublisherDriveClient,
} from '../composed/applicationPublisher';

const CHECKSUM_SHA256 = 'a'.repeat(64);

function packageFile(): DriveUploaderBlobLike {
  const bytes = new Uint8Array([1, 2, 3, 4]);
  return {
    name: 'application.zip',
    type: 'application/zip',
    size: bytes.byteLength,
    slice(start = 0, end = bytes.byteLength, contentType = 'application/zip') {
      return new Blob([bytes.slice(start, end)], { type: contentType });
    },
    async readRange(offsetBytes, lengthBytes) {
      return bytes.slice(offsetBytes, offsetBytes + lengthBytes).buffer;
    },
  };
}

function publishRequest(
  overrides: Omit<Partial<ApplicationPublishRequest>, 'deployment'> & {
    deployment?: ApplicationPublishDeployment | undefined;
  } = {},
): ApplicationPublishRequest {
  const { deployment, ...rest } = overrides;
  return {
    app: {
      kind: 'resolveOrCreate',
      name: 'BirdCoder',
      slug: 'birdcoder',
      appKind: 'SPA_WEB',
    },
    artifact: {
      file: packageFile(),
      packageType: 1,
      fileName: 'application.zip',
      contentType: 'application/zip',
      checksumSha256: CHECKSUM_SHA256,
      source: 'sdkwork-birdcoder-pc',
    },
    release: {
      platformTargetId: 'target-1',
      packageId: 'package-1',
      semanticVersion: '1.2.3',
    },
    ...(deployment !== undefined
      ? { deployment }
      : 'deployment' in overrides
        ? {}
        : {
            deployment: {
              platformTargetId: 'target-1',
              deploymentKind: 'ARTIFACT_RELEASE',
              deploymentTarget: 'WEB_NODE',
              environment: 'production',
            },
          }),
    ...rest,
  };
}

interface MockClientOptions {
  appList?: (page: number) => Promise<unknown>;
  appRetrieve?: () => Promise<unknown>;
  appCreate?: () => Promise<unknown>;
  upload?: (request: DriveUploaderRequest) => Promise<unknown>;
  artifactCreate?: () => Promise<unknown>;
  releaseCreate?: () => Promise<unknown>;
  deploymentCreate?: () => Promise<unknown>;
}

function createMockClients(options: MockClientOptions = {}) {
  const calls: string[] = [];
  const appList = vi.fn(async (params: { page?: number }) => {
    const page = params.page ?? 1;
    calls.push(`app.list:${page}`);
    return (
      (await options.appList?.(page)) ?? {
        items: [{ id: 'app-1', name: 'BirdCoder', slug: 'birdcoder' }],
        pageInfo: { mode: 'offset', page, pageSize: 50, hasMore: false },
      }
    );
  });
  const appRetrieve = vi.fn(async () => {
    calls.push('app.retrieve');
    return (await options.appRetrieve?.()) ?? { id: 'app-1', name: 'BirdCoder' };
  });
  const appCreate = vi.fn(async () => {
    calls.push('app.create');
    return (await options.appCreate?.()) ?? { id: 'app-1', name: 'BirdCoder' };
  });
  const uploadArchive = vi.fn(async (request: DriveUploaderRequest) => {
    calls.push('drive.uploadArchive');
    request.onProgress?.({
      taskId: 'task-1',
      uploadItemId: 'upload-item-1',
      uploadSessionId: 'upload-session-1',
      nodeId: 'node-1',
      uploadedBytes: request.file.size,
      totalBytes: request.file.size,
      uploadedPartsCount: 1,
      totalParts: 1,
      status: 'completed',
    });
    return (
      (await options.upload?.(request)) ?? {
        uploadItem: {
          id: 'upload-item-1',
          spaceId: 'space-1',
          nodeId: 'node-1',
        },
        uploadSession: { id: 'upload-session-1' },
        parts: [],
      }
    );
  });
  const artifactCreate = vi.fn(async () => {
    calls.push('artifact.create');
    return (await options.artifactCreate?.()) ?? { id: 'artifact-1' };
  });
  const releaseCreate = vi.fn(async () => {
    calls.push('release.create');
    return (await options.releaseCreate?.()) ?? { id: 'release-1' };
  });
  const deploymentCreate = vi.fn(async () => {
    calls.push('deployment.create');
    return (await options.deploymentCreate?.()) ?? { id: 'deployment-1' };
  });

  const deployClient = {
    app: {
      list: appList,
      retrieve: appRetrieve,
      create: appCreate,
    },
    artifact: { create: artifactCreate },
    release: { create: releaseCreate },
    deployment: { create: deploymentCreate },
  } as unknown as ApplicationPublisherDeployClient;
  const driveClient = {
    uploader: { uploadArchive },
  } as unknown as ApplicationPublisherDriveClient;

  return {
    calls,
    deployClient,
    driveClient,
    mocks: {
      appList,
      appRetrieve,
      appCreate,
      uploadArchive,
      artifactCreate,
      releaseCreate,
      deploymentCreate,
    },
  };
}

function publisher(
  clients: ReturnType<typeof createMockClients>,
) {
  let idempotencyIndex = 0;
  return createDeployApplicationPublisher({
    deployClient: clients.deployClient,
    driveClient: clients.driveClient,
    createIdempotencyKey: () => `idempotency-${++idempotencyIndex}`,
  });
}

describe('createDeployApplicationPublisher', () => {
  it('publishes in the frozen app, upload, artifact, release, deployment order', async () => {
    const clients = createMockClients();
    const progress: ApplicationPublishProgress[] = [];

    const result = await publisher(clients).publish(
      publishRequest({ onProgress: (event) => progress.push(event) }),
    );

    expect(clients.calls).toEqual([
      'app.list:1',
      'drive.uploadArchive',
      'artifact.create',
      'release.create',
      'deployment.create',
    ]);
    expect(result).toMatchObject({
      app: { id: 'app-1', resolution: 'existingBySlug' },
      upload: {
        uploadItemId: 'upload-item-1',
        uploadSessionId: 'upload-session-1',
        driveSpaceId: 'space-1',
        driveNodeId: 'node-1',
      },
      artifact: { id: 'artifact-1' },
      release: { id: 'release-1' },
      deployment: { id: 'deployment-1' },
    });
    expect(clients.mocks.uploadArchive).toHaveBeenCalledWith(
      expect.objectContaining({
        appResourceType: 'deploy.artifact',
        appResourceId: 'app-1',
        checksumSha256Hex: CHECKSUM_SHA256,
        source: 'sdkwork-birdcoder-pc',
      }),
    );
    expect(clients.mocks.artifactCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        // `CreateArtifactRequest.siteId` still carries the application id.
        siteId: 'app-1',
        driveUploadSessionId: 'upload-session-1',
        driveUploadItemId: 'upload-item-1',
        idempotencyKey: 'idempotency-1',
      }),
      { idempotencyKey: 'idempotency-1' },
      { signal: undefined, timeout: undefined },
    );
    expect(progress).toContainEqual(
      expect.objectContaining({
        kind: 'upload',
        stage: 'uploadArchive',
        status: 'completed',
        uploadedBytes: 4,
        totalBytes: 4,
      }),
    );
  });

  it('falls back from an exact slug match to an exact name match', async () => {
    const clients = createMockClients({
      // The replacement `app.list` has no `keyword` filter, so resolution pages
      // through once and filters both fields client-side.
      appList: async () => ({
        items: [{ id: 'app-by-name', name: 'BirdCoder', slug: 'legacy-slug' }],
        pageInfo: { mode: 'offset', page: 1, pageSize: 50, hasMore: false },
      }),
    });

    const result = await publisher(clients).publish(
      publishRequest({ deployment: undefined }),
    );

    expect(clients.calls.slice(0, 1)).toEqual(['app.list:1']);
    expect(result.app).toMatchObject({
      id: 'app-by-name',
      resolution: 'existingByName',
    });
    expect(result.deployment).toBeUndefined();
    expect(clients.mocks.deploymentCreate).not.toHaveBeenCalled();
  });

  it('creates an Application only after both exact lookups return no match', async () => {
    const clients = createMockClients({
      appList: async () => ({
        items: [],
        pageInfo: { mode: 'offset', page: 1, pageSize: 50, hasMore: false },
      }),
      appCreate: async () => ({ id: 'created-app' }),
    });

    const result = await publisher(clients).publish(publishRequest());

    expect(clients.calls.slice(0, 3)).toEqual([
      'app.list:1',
      'app.create',
      'drive.uploadArchive',
    ]);
    expect(result.app).toMatchObject({ id: 'created-app', resolution: 'created' });
    expect(clients.mocks.appCreate).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'BirdCoder', slug: 'birdcoder', appKind: 'SPA_WEB' }),
      { idempotencyKey: 'idempotency-1' },
      { signal: undefined, timeout: undefined },
    );
  });

  it('rejects an unbounded scan instead of creating a duplicate Application', async () => {
    const clients = createMockClients({
      appList: async (page) => ({
        items: [],
        pageInfo: { mode: 'offset', page, pageSize: 50, hasMore: true },
      }),
    });

    await expect(publisher(clients).publish(publishRequest())).rejects.toMatchObject({
      code: 'APP_RESOLUTION_AMBIGUOUS',
      stage: 'resolveApp',
    });
    expect(clients.mocks.appCreate).not.toHaveBeenCalled();
    expect(clients.mocks.uploadArchive).not.toHaveBeenCalled();
  });

  it('fails closed when a resolved Application response omits its id', async () => {
    const clients = createMockClients({ appRetrieve: async () => ({ name: 'BirdCoder' }) });

    await expect(
      publisher(clients).publish(
        publishRequest({ app: { kind: 'existing', appId: 'app-1' } }),
      ),
    ).rejects.toMatchObject({
      code: 'APP_RESPONSE_MISSING_ID',
      stage: 'resolveApp',
    });
    expect(clients.mocks.uploadArchive).not.toHaveBeenCalled();
  });

  it.each([
    {
      label: 'Artifact',
      options: { artifactCreate: async () => ({}) },
      code: 'ARTIFACT_RESPONSE_MISSING_ID',
      stage: 'registerArtifact',
    },
    {
      label: 'Release',
      options: { releaseCreate: async () => ({}) },
      code: 'RELEASE_RESPONSE_MISSING_ID',
      stage: 'createRelease',
    },
    {
      label: 'Deployment',
      options: { deploymentCreate: async () => ({}) },
      code: 'DEPLOYMENT_RESPONSE_MISSING_ID',
      stage: 'createDeployment',
    },
  ])('fails closed when the $label response omits its id', async ({ options, code, stage }) => {
    const clients = createMockClients(options);

    await expect(publisher(clients).publish(publishRequest())).rejects.toMatchObject({
      code,
      stage,
    });
  });
});

// Keep the mock boundary honest without reproducing dependency-owned DTOs.
void ({} as DriveUploaderUploadResult);
