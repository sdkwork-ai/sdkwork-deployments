import type {
  AppDeploymentResponse,
  AppReleaseResponse,
  AppResponse,
  ArtifactResponse,
  CreateAppDeploymentRequest,
  CreateAppReleaseRequest,
  CreateAppRequest,
} from '../../generated/server-openapi/src/types';
import type { SdkworkDeployAppClient } from '../../generated/server-openapi/src/sdk';
import type {
  DriveUploaderBlobLike,
  DriveUploaderProgress,
  DriveUploaderUploadResult,
  SdkworkDriveAppClient,
} from '@sdkwork/drive-app-sdk';

export type ApplicationPublishStage =
  | 'resolveApp'
  | 'createApp'
  | 'uploadArchive'
  | 'registerArtifact'
  | 'createRelease'
  | 'createDeployment'
  | 'complete';

export type ApplicationPublishErrorCode =
  | 'INVALID_REQUEST'
  | 'APP_RESOLUTION_AMBIGUOUS'
  | 'APP_RESPONSE_MISSING_ID'
  | 'APP_ID_MISMATCH'
  | 'UPLOAD_RESPONSE_INCOMPLETE'
  | 'ARTIFACT_RESPONSE_MISSING_ID'
  | 'RELEASE_RESPONSE_MISSING_ID'
  | 'DEPLOYMENT_RESPONSE_MISSING_ID'
  | 'IDEMPOTENCY_KEY_UNAVAILABLE'
  | 'ABORTED'
  | 'STAGE_FAILED';

export interface ExistingApplicationPublishApp {
  kind: 'existing';
  appId: string;
}

export interface ResolveOrCreateApplicationPublishApp extends CreateAppRequest {
  kind: 'resolveOrCreate';
}

export type ApplicationPublishApp =
  | ExistingApplicationPublishApp
  | ResolveOrCreateApplicationPublishApp;

export interface ApplicationPublishArtifact {
  file: DriveUploaderBlobLike;
  packageType: number;
  fileName: string;
  contentType: string;
  checksumSha256: string;
  taskId?: string;
  chunkSizeBytes?: number;
  scene?: string;
  source?: string;
}

export type ApplicationPublishRelease = Omit<CreateAppReleaseRequest, 'idempotencyKey'>;

export type ApplicationPublishDeployment = Omit<
  CreateAppDeploymentRequest,
  'releaseId' | 'idempotencyKey'
>;

export interface ApplicationPublishIdempotencyKeys {
  app?: string;
  artifact?: string;
  release?: string;
  deployment?: string;
}

export interface ApplicationPublishRequest {
  app: ApplicationPublishApp;
  artifact: ApplicationPublishArtifact;
  release?: ApplicationPublishRelease;
  deployment?: ApplicationPublishDeployment;
  idempotencyKeys?: ApplicationPublishIdempotencyKeys;
  signal?: AbortSignal;
  onProgress?: ApplicationPublishProgressCallback;
}

export type ApplicationPublishAppResolution =
  | 'existingById'
  | 'existingBySlug'
  | 'existingByName'
  | 'created';

export interface ApplicationPublishAppEvidence {
  id: string;
  resolution: ApplicationPublishAppResolution;
  value: AppResponse;
}

export interface ApplicationPublishUploadEvidence {
  uploadItemId: string;
  uploadSessionId: string;
  driveSpaceId: string;
  driveNodeId: string;
  value: DriveUploaderUploadResult;
}

export interface ApplicationPublishArtifactEvidence {
  id: string;
  value: ArtifactResponse;
}

export interface ApplicationPublishReleaseEvidence {
  id: string;
  value: AppReleaseResponse;
}

export interface ApplicationPublishDeploymentEvidence {
  id: string;
  value: AppDeploymentResponse;
}

export interface ApplicationPublishResult {
  app: ApplicationPublishAppEvidence;
  upload: ApplicationPublishUploadEvidence;
  artifact: ApplicationPublishArtifactEvidence;
  release: ApplicationPublishReleaseEvidence;
  deployment?: ApplicationPublishDeploymentEvidence;
}

export interface ApplicationPublishProgressEvidence {
  appId?: string;
  uploadItemId?: string;
  uploadSessionId?: string;
  artifactId?: string;
  releaseId?: string;
  deploymentId?: string;
}

export interface ApplicationPublishStageProgress {
  kind: 'stage';
  stage: ApplicationPublishStage;
  status: 'started' | 'completed';
  evidence: ApplicationPublishProgressEvidence;
}

export interface ApplicationPublishUploadProgress {
  kind: 'upload';
  stage: 'uploadArchive';
  status: DriveUploaderProgress['status'];
  uploadedBytes: number;
  totalBytes: number;
  uploadedPartsCount: number;
  totalParts: number;
  partNo?: number;
  evidence: ApplicationPublishProgressEvidence;
}

export interface ApplicationPublishFailureProgress {
  kind: 'failure';
  stage: ApplicationPublishStage;
  status: 'failed';
  error: {
    code: ApplicationPublishErrorCode;
    message: string;
  };
  evidence: ApplicationPublishProgressEvidence;
}

export type ApplicationPublishProgress =
  | ApplicationPublishStageProgress
  | ApplicationPublishUploadProgress
  | ApplicationPublishFailureProgress;

export type ApplicationPublishProgressCallback = (
  progress: ApplicationPublishProgress,
) => void;

export interface ApplicationPublisherDeployClient {
  readonly app: Pick<SdkworkDeployAppClient['app'], 'create' | 'list' | 'retrieve'>;
  readonly artifact: Pick<SdkworkDeployAppClient['artifact'], 'create'>;
  readonly release: Pick<SdkworkDeployAppClient['release'], 'create'>;
  readonly deployment: Pick<SdkworkDeployAppClient['deployment'], 'create'>;
}

export interface ApplicationPublisherDriveClient {
  readonly uploader: Pick<SdkworkDriveAppClient['uploader'], 'uploadArchive'>;
}

export interface DeployApplicationPublisherOptions {
  deployClient: ApplicationPublisherDeployClient;
  driveClient: ApplicationPublisherDriveClient;
  createIdempotencyKey?: () => string;
}

export interface DeployApplicationPublisher {
  publish(request: ApplicationPublishRequest): Promise<ApplicationPublishResult>;
}
