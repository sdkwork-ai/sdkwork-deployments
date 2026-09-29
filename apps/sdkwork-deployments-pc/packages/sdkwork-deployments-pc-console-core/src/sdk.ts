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
  AppClientArchitecture,
  AppClientClass,
  AppCompositionResponse,
  AppDeploymentResponse,
  AppDomainResponse,
  AppKind,
  AppOwnerType,
  AppPublishEnvironment,
  AppReleaseResponse,
  AppResponse,
  AppSourceBindingResponse,
  AppSourceBindingStatus,
  AppSourceSpecResponse,
  AppSourceSpecRoute,
  AppSourceSpecStatus,
  AppStatus,
  ArtifactResponse,
  BindAppSourceSpecSourceRequest,
  CreateAppDeploymentRequest,
  CreateAppReleaseRequest,
  CreateAppRequest,
  CreateAppSourceSpecRequest,
  CreateArtifactRequest,
  CreatePlatformTargetRequest,
  CreateSourceRepositoryRequest,
  /**
   * 发布历史的三个轴：`deploymentKind` / `deploymentTarget` 是回滚请求要复现的
   * 部署形状，`rollbackFromDeploymentId` 对应的策略枚举由 `RolloutStrategy` 承载。
   * 不导出它们，历史视图就只能拿字符串与契约对赌 —— 而回滚正是一次写操作。
   */
  DeploymentKind,
  DeploymentStatus,
  DeploymentTarget,
  DomainHostnameResponse,
  DomainZoneResponse,
  DriveDirectorySource,
  DriveFolderSelector,
  DriveSpaceRootSelector,
  /**
   * 规格来源的另一半：知识库 Wiki。
   *
   * `BindAppSourceSpecSourceRequest["source"]` 是一个二选一联合，只导出 Drive
   * 那一半会让调用方无法为另一支构造一个类型正确的值 —— 而「来源」正是本能力
   * 的核心输入，两半必须在同一个出口上。
   */
  KnowledgebaseWikiSource,
  PageInfo,
  PackageFormat,
  PackageResponse,
  PackageStatus,
  Platform,
  PlatformTargetResponse,
  ReleaseStatus,
  SdkworkDeployAppClient,
  SdkworkRuntimeTarget,
  SourceRepositoryResponse,
  TechStack,
  UpdateAppSourceSpecRequest,
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
