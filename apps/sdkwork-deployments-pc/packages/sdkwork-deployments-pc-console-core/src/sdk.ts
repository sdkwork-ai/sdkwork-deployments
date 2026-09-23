/**
 * SDK surface of the deployments console core.
 *
 * Capability packages (`-console-publishing` and any future console feature
 * package) must not import the generated SDKs directly, so this subpath is the
 * single place where the console re-exports the generated clients, their
 * request/response types, and the application publisher the publishing console
 * drives.
 *
 * It mirrors only what feature packages consume: core keeps importing the SDKs
 * itself for the rest of the surface, and the two are free to diverge.
 */
export type {
  AppCompositionResponse,
  AppDomainResponse,
  AppKind,
  AppReleaseResponse,
  AppResponse,
  AppStatus,
  ArtifactResponse,
  CreateAppReleaseRequest,
  CreateAppRequest,
  CreateArtifactRequest,
  CreatePlatformTargetRequest,
  CreateSourceRepositoryRequest,
  DomainHostnameResponse,
  DomainZoneResponse,
  PageInfo,
  PackageResponse,
  Platform,
  PlatformTargetResponse,
  SdkworkDeployAppClient,
  SourceRepositoryResponse,
  TechStack,
} from "@sdkwork/deployments-app-sdk";

export type {
  ApplicationPublishProgress,
  ApplicationPublishResult,
  DeployApplicationPublisher,
} from "@sdkwork/deployments-app-sdk/application-publisher";

export { createDeployApplicationPublisher } from "@sdkwork/deployments-app-sdk/application-publisher";

export type {
  DriveUploaderBlobLike,
  DriveUploaderProgress,
  DriveUploaderUploadResult,
  SdkworkDriveAppClient,
} from "@sdkwork/drive-app-sdk";
