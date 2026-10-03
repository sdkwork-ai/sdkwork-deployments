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
  AppTemplateResponse,
  AppTemplateSummaryResponse,
  AppTemplateVersionResponse,
  ArtifactResponse,
  BindAppSourceSpecSourceRequest,
  CreateAppDeploymentRequest,
  CreateAppReleaseRequest,
  CreateAppRequest,
  CreateAppSourceSpecRequest,
  CreateAppTemplateRequest,
  CreateAppTemplateVersionRequest,
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
  TemplateCategoryResponse,
  UpdateAppSourceSpecRequest,
  UpdateAppTemplateRequest,
} from "@sdkwork/deployments-app-sdk";

/**
 * App-template trade belongs to the platform order center, not to this module:
 * the deployments app API is catalog-only, and `sdkwork-order` owns the order
 * and its payment. The console funnels the order app SDK's surface the same way
 * it funnels the deploy one — feature packages import it from here instead of
 * reaching for the generated package — and aliases the client so a reader can
 * tell which app API it talks to.
 */
export type {
  AppTemplateOrderCreateResult,
  AppTemplateOrderSummary,
  SdkworkAppClient as SdkworkOrderAppClient,
} from "@sdkwork/order-app-sdk";

export type {
  ApplicationPublishProgress,
  ApplicationPublishResult,
  DeployApplicationPublisher,
} from "@sdkwork/deployments-app-sdk/application-publisher";

export { createDeployApplicationPublisher } from "@sdkwork/deployments-app-sdk/application-publisher";

export type {
  /**
   * 应用头像媒体的另一半：把 `deploy_app.metadata.media` 里的 Drive 节点引用
   * 换成可展示的签名下载 URL。不镜像它，能力包就只能拿裸字符串与响应
   * 对赌 —— 而签名 URL 有过期时间，过期语义必须按契约承载。
   */
  CreateDownloadUrlResponse,
  DriveUploaderBlobLike,
  DriveUploaderProgress,
  DriveUploaderUploadResult,
  SdkworkDriveAppClient,
} from "@sdkwork/drive-app-sdk";
