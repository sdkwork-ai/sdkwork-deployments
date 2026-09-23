import { uuid } from '@sdkwork/utils/id';
import { ApplicationPublishError, toApplicationPublishError } from './errors';
import {
  createdApplicationPublishAppEvidence,
  findExactApplicationPublishApp,
  retrieveApplicationPublishApp,
} from './appResolver';
import type {
  ApplicationPublishArtifact,
  ApplicationPublishAppEvidence,
  ApplicationPublishProgress,
  ApplicationPublishProgressEvidence,
  ApplicationPublishRequest,
  ApplicationPublishResult,
  ApplicationPublishStage,
  DeployApplicationPublisher,
  DeployApplicationPublisherOptions,
} from './types';

const DEFAULT_UPLOAD_SCENE = 'deployment-package';
const DEFAULT_UPLOAD_SOURCE = '@sdkwork/deployments-app-sdk';
const SHA256_HEX_PATTERN = /^[a-fA-F0-9]{64}$/;

export function createDeployApplicationPublisher(
  options: DeployApplicationPublisherOptions,
): DeployApplicationPublisher {
  const createIdempotencyKey =
    options.createIdempotencyKey ?? createRuntimeIdempotencyKey;

  return {
    async publish(request): Promise<ApplicationPublishResult> {
      let currentStage: ApplicationPublishStage = 'resolveApp';
      const evidence: ApplicationPublishProgressEvidence = {};
      const emit = (progress: ApplicationPublishProgress): void => {
        try {
          request.onProgress?.(progress);
        } catch {
          // Progress observers cannot interrupt an operation after remote side effects begin.
        }
      };
      const startStage = (stage: ApplicationPublishStage): void => {
        currentStage = stage;
        emit({ kind: 'stage', stage, status: 'started', evidence: { ...evidence } });
      };
      const completeStage = (stage: ApplicationPublishStage): void => {
        emit({ kind: 'stage', stage, status: 'completed', evidence: { ...evidence } });
      };

      try {
        const normalizedArtifact = validateRequest(request);
        throwIfAborted(request.signal, currentStage);

        startStage('resolveApp');
        let appEvidence: ApplicationPublishAppEvidence;
        if (request.app.kind === 'existing') {
          appEvidence = await retrieveApplicationPublishApp(
            options.deployClient,
            request.app.appId.trim(),
            request.signal,
          );
        } else {
          const existing = await findExactApplicationPublishApp(
            options.deployClient,
            request.app,
            request.signal,
          );
          if (existing) {
            appEvidence = existing;
          } else {
            completeStage('resolveApp');
            startStage('createApp');
            const appName = request.app.name.trim();
            const appSlug = normalizedOptionalText(request.app.slug);
            const appDescription = normalizedOptionalText(request.app.description);
            const appDefaultEnvironment = normalizedOptionalText(
              request.app.defaultEnvironment,
            );
            const appMetadata = request.app.metadata;
            // The body's `idempotencyKey` and the `Idempotency-Key` header MUST carry
            // the same value, so resolve it once and reuse it for both.
            const appIdempotencyKey = resolveIdempotencyKey(
              request.idempotencyKeys?.app,
              createIdempotencyKey,
              'createApp',
            );
            const created = await options.deployClient.app.create(
              {
                name: appName,
                appKind: request.app.appKind,
                ...(appSlug !== undefined ? { slug: appSlug } : {}),
                ...(appDescription !== undefined ? { description: appDescription } : {}),
                ...(appDefaultEnvironment !== undefined
                  ? { defaultEnvironment: appDefaultEnvironment }
                  : {}),
                ...(appMetadata !== undefined ? { metadata: appMetadata } : {}),
                idempotencyKey: appIdempotencyKey,
              },
              { idempotencyKey: appIdempotencyKey },
              apiRequestOptions(request.signal),
            );
            appEvidence = createdApplicationPublishAppEvidence(created);
            evidence.appId = appEvidence.id;
            completeStage('createApp');
          }
        }
        evidence.appId = appEvidence.id;
        if (currentStage === 'resolveApp') {
          completeStage('resolveApp');
        }

        throwIfAborted(request.signal, 'uploadArchive');
        startStage('uploadArchive');
        const uploadTaskId = normalizedOptionalText(normalizedArtifact.taskId);
        const upload = await options.driveClient.uploader.uploadArchive({
          file: normalizedArtifact.file,
          appResourceType: 'deploy.artifact',
          appResourceId: appEvidence.id,
          scene: normalizedOptionalText(normalizedArtifact.scene) ?? DEFAULT_UPLOAD_SCENE,
          source: normalizedOptionalText(normalizedArtifact.source) ?? DEFAULT_UPLOAD_SOURCE,
          originalFileName: normalizedArtifact.fileName,
          contentType: normalizedArtifact.contentType,
          checksumSha256Hex: normalizedArtifact.checksumSha256,
          ...(normalizedArtifact.chunkSizeBytes !== undefined
            ? { chunkSizeBytes: normalizedArtifact.chunkSizeBytes }
            : {}),
          ...(uploadTaskId !== undefined ? { taskId: uploadTaskId } : {}),
          ...(request.signal !== undefined ? { signal: request.signal } : {}),
          onProgress: (progress) => {
            evidence.uploadItemId = progress.uploadItemId;
            evidence.uploadSessionId = progress.uploadSessionId;
            emit({
              kind: 'upload',
              stage: 'uploadArchive',
              status: progress.status,
              uploadedBytes: progress.uploadedBytes,
              totalBytes: progress.totalBytes,
              uploadedPartsCount: progress.uploadedPartsCount,
              totalParts: progress.totalParts,
              ...(progress.partNo !== undefined ? { partNo: progress.partNo } : {}),
              evidence: { ...evidence },
            });
          },
        });
        const uploadEvidence = requireUploadEvidence(upload);
        evidence.uploadItemId = uploadEvidence.uploadItemId;
        evidence.uploadSessionId = uploadEvidence.uploadSessionId;
        completeStage('uploadArchive');

        throwIfAborted(request.signal, 'registerArtifact');
        startStage('registerArtifact');
        const artifactIdempotencyKey = resolveIdempotencyKey(
          request.idempotencyKeys?.artifact,
          createIdempotencyKey,
          'registerArtifact',
        );
        const artifact = await options.deployClient.artifact.create(
          {
            // `CreateArtifactRequest.siteId` still carries the owning application id:
            // the backend DTO has not been renamed yet (see § applications convergence).
            siteId: appEvidence.id,
            packageType: normalizedArtifact.packageType,
            fileName: normalizedArtifact.fileName,
            contentType: normalizedArtifact.contentType,
            contentLength: String(normalizedArtifact.file.size),
            checksumSha256: normalizedArtifact.checksumSha256,
            driveUploadSessionId: uploadEvidence.uploadSessionId,
            driveUploadItemId: uploadEvidence.uploadItemId,
            driveSpaceId: uploadEvidence.driveSpaceId,
            driveNodeId: uploadEvidence.driveNodeId,
            idempotencyKey: artifactIdempotencyKey,
          },
          { idempotencyKey: artifactIdempotencyKey },
          apiRequestOptions(request.signal),
        );
        const artifactId = requireResponseId(
          artifact.id,
          'ARTIFACT_RESPONSE_MISSING_ID',
          'registerArtifact',
          'Artifact',
        );
        evidence.artifactId = artifactId;
        completeStage('registerArtifact');

        throwIfAborted(request.signal, 'createRelease');
        startStage('createRelease');
        const releaseIdempotencyKey = resolveIdempotencyKey(
          request.idempotencyKeys?.release,
          createIdempotencyKey,
          'createRelease',
        );
        const release = await options.deployClient.release.create(
          appEvidence.id,
          {
            platformTargetId: requireText(
              request.release?.platformTargetId ?? '',
              'release.platformTargetId',
            ),
            packageId: requireText(
              request.release?.packageId ?? artifactId,
              'release.packageId',
            ),
            semanticVersion: requireText(
              request.release?.semanticVersion ?? '',
              'release.semanticVersion',
            ),
            ...(request.release?.releaseNotes !== undefined
              ? { releaseNotes: request.release.releaseNotes }
              : {}),
            ...(request.release?.releaseStatus !== undefined
              ? { releaseStatus: request.release.releaseStatus }
              : {}),
            idempotencyKey: releaseIdempotencyKey,
          },
          { idempotencyKey: releaseIdempotencyKey },
          apiRequestOptions(request.signal),
        );
        const releaseId = requireResponseId(
          release.id,
          'RELEASE_RESPONSE_MISSING_ID',
          'createRelease',
          'Release',
        );
        evidence.releaseId = releaseId;
        completeStage('createRelease');

        let deploymentEvidence: ApplicationPublishResult['deployment'];
        if (request.deployment) {
          throwIfAborted(request.signal, 'createDeployment');
          startStage('createDeployment');
          const deploymentIdempotencyKey = resolveIdempotencyKey(
            request.idempotencyKeys?.deployment,
            createIdempotencyKey,
            'createDeployment',
          );
          const deployment = await options.deployClient.deployment.create(
            appEvidence.id,
            {
              ...request.deployment,
              releaseId,
              idempotencyKey: deploymentIdempotencyKey,
            },
            { idempotencyKey: deploymentIdempotencyKey },
            apiRequestOptions(request.signal),
          );
          const deploymentId = requireResponseId(
            deployment.id,
            'DEPLOYMENT_RESPONSE_MISSING_ID',
            'createDeployment',
            'Deployment',
          );
          evidence.deploymentId = deploymentId;
          deploymentEvidence = { id: deploymentId, value: deployment };
          completeStage('createDeployment');
        }

        startStage('complete');
        const result: ApplicationPublishResult = {
          app: appEvidence,
          upload: uploadEvidence,
          artifact: { id: artifactId, value: artifact },
          release: { id: releaseId, value: release },
          ...(deploymentEvidence !== undefined ? { deployment: deploymentEvidence } : {}),
        };
        completeStage('complete');
        return result;
      } catch (cause) {
        const error = toApplicationPublishError(
          cause,
          currentStage,
          request.signal?.aborted === true,
        );
        emit({
          kind: 'failure',
          stage: error.stage,
          status: 'failed',
          error: { code: error.code, message: error.message },
          evidence: { ...evidence },
        });
        throw error;
      }
    },
  };
}

function validateRequest(request: ApplicationPublishRequest): ApplicationPublishArtifact {
  if (request.app.kind === 'existing') {
    requireText(request.app.appId, 'app.appId');
  } else {
    requireText(request.app.name, 'app.name');
    requireText(request.app.appKind, 'app.appKind');
  }

  const artifact = request.artifact;
  if (!artifact.file || !Number.isFinite(artifact.file.size) || artifact.file.size <= 0) {
    throw invalidRequest('artifact.file must contain a non-empty package.');
  }
  if (!Number.isInteger(artifact.packageType) || artifact.packageType <= 0) {
    throw invalidRequest('artifact.packageType must be a positive integer.');
  }
  const fileName = requireText(artifact.fileName, 'artifact.fileName');
  const contentType = requireText(artifact.contentType, 'artifact.contentType');
  const checksumSha256 = requireText(
    artifact.checksumSha256,
    'artifact.checksumSha256',
  );
  if (!SHA256_HEX_PATTERN.test(checksumSha256)) {
    throw invalidRequest('artifact.checksumSha256 must be a 64-character SHA-256 hex digest.');
  }
  if (
    artifact.chunkSizeBytes !== undefined &&
    (!Number.isInteger(artifact.chunkSizeBytes) || artifact.chunkSizeBytes <= 0)
  ) {
    throw invalidRequest('artifact.chunkSizeBytes must be a positive integer when provided.');
  }

  return {
    ...artifact,
    fileName,
    contentType,
    checksumSha256: checksumSha256.toLowerCase(),
  };
}

function requireUploadEvidence(
  value: Awaited<
    ReturnType<DeployApplicationPublisherOptions['driveClient']['uploader']['uploadArchive']>
  >,
): ApplicationPublishResult['upload'] {
  const uploadItemId = normalizedOptionalText(value.uploadItem?.id);
  const uploadSessionId = normalizedOptionalText(value.uploadSession?.id);
  const driveSpaceId = normalizedOptionalText(value.uploadItem?.spaceId);
  const driveNodeId = normalizedOptionalText(value.uploadItem?.nodeId);
  if (!uploadItemId || !uploadSessionId || !driveSpaceId || !driveNodeId) {
    throw new ApplicationPublishError(
      'UPLOAD_RESPONSE_INCOMPLETE',
      'uploadArchive',
      'Drive upload response did not include stable item, session, space, and node references.',
    );
  }
  return {
    uploadItemId,
    uploadSessionId,
    driveSpaceId,
    driveNodeId,
    value,
  };
}

function requireResponseId(
  value: string | undefined,
  code:
    | 'ARTIFACT_RESPONSE_MISSING_ID'
    | 'RELEASE_RESPONSE_MISSING_ID'
    | 'DEPLOYMENT_RESPONSE_MISSING_ID',
  stage: 'registerArtifact' | 'createRelease' | 'createDeployment',
  resource: string,
): string {
  const id = normalizedOptionalText(value);
  if (!id) {
    throw new ApplicationPublishError(
      code,
      stage,
      `Deploy ${resource} response did not include an id.`,
    );
  }
  return id;
}

function resolveIdempotencyKey(
  value: string | undefined,
  createIdempotencyKey: () => string,
  stage: 'createApp' | 'registerArtifact' | 'createRelease' | 'createDeployment',
): string {
  if (value !== undefined) {
    return requireText(value, 'idempotency key');
  }
  const generated = normalizedOptionalText(createIdempotencyKey());
  if (!generated) {
    throw new ApplicationPublishError(
      'IDEMPOTENCY_KEY_UNAVAILABLE',
      stage,
      'The idempotency key factory returned an empty value.',
    );
  }
  return generated;
}

function createRuntimeIdempotencyKey(): string {
  return uuid();
}

function throwIfAborted(
  signal: AbortSignal | undefined,
  stage: ApplicationPublishStage,
): void {
  if (signal?.aborted) {
    throw new ApplicationPublishError(
      'ABORTED',
      stage,
      'Application publishing was cancelled.',
      signal.reason,
    );
  }
}

function requireText(value: string, field: string): string {
  const normalized = value.trim();
  if (!normalized) {
    throw invalidRequest(`${field} is required.`);
  }
  return normalized;
}

function normalizedOptionalText(value: string | undefined): string | undefined {
  const normalized = value?.trim();
  return normalized || undefined;
}

function apiRequestOptions(
  signal: AbortSignal | undefined,
): { signal?: AbortSignal } {
  return signal !== undefined ? { signal } : {};
}

function invalidRequest(message: string): ApplicationPublishError {
  return new ApplicationPublishError(
    'INVALID_REQUEST',
    'resolveApp',
    message,
  );
}
