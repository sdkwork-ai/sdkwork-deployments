/**
 * Publishing capability surface: the reusable create-deploy-app components and
 * services. `src/index.ts` re-exports this barrel alongside the module
 * registration, so hosts can import the whole capability from the package root
 * (console shell + BirdCoder plugin) without reaching into package internals.
 */
/**
 * The routing kernel is part of the capability's public surface, not an internal
 * detail: the backend-admin operations view needs the same "declared vs
 * effective" answer, and the app shell projects it into that surface's port.
 * Re-exporting it here is what keeps a single implementation of the rule — see
 * `projectSourceSpecLedger`'s doc for why a second one is unacceptable.
 */
export {
  APP_CLIENT_CLASSES,
  APP_PUBLISH_ENVIRONMENTS,
  describeClientClassRouting,
  describeRoutingOverview,
  projectSourceSpecLedger,
  resolveAppEnvironment,
  SOURCE_BINDING_LABEL_KEYS,
  /**
   * `STATIC | SPA | WIKI` — the third column of a spec row, alongside its
   * runtime target and client architecture. Exported for the same reason as the
   * other three tables: the admin ledger renders the same row.
   */
  SOURCE_SPEC_HANDLER_LABEL_KEYS,
  SOURCE_SPEC_STATUS_LABEL_KEYS,
  CLIENT_ARCHITECTURE_LABEL_KEYS,
  CLIENT_CLASS_LABEL_KEYS,
  RUNTIME_TARGET_LABEL_KEYS,
} from "./service/app-source-spec-routing.ts";
export type {
  ClientClassRouting,
  SourceSpecLedgerEntry,
  SourceSpecLedgerInput,
  SourceSpecRouteFact,
  SourceSpecRoutingInput,
  SourceSpecRoutingOverview,
} from "./service/app-source-spec-routing.ts";
export { CreateDeployAppDialog } from "./components/CreateDeployAppDialog.tsx";
export type { CreateDeployAppDialogProps, DeployAppPublishResult } from "./components/CreateDeployAppDialog.tsx";
export { CreateAppDialog } from "./components/CreateAppDialog.tsx";
export type { CreateAppDialogProps } from "./components/CreateAppDialog.tsx";
export { BuildProgressDialog } from "./components/BuildProgressDialog.tsx";
export type { BuildProgressDialogProps, DeployDialogBuildFrame, DeployDialogBuildPort } from "./components/BuildProgressDialog.tsx";
export { PublishingAppsPage, primaryHostname } from "./components/PublishingAppsPage.tsx";
export type { PublishingAppsPageProps } from "./components/PublishingAppsPage.tsx";
export { AppArchiveDialog, AppEditDialog, AppPublishDialog, AppSourceDialog } from "./components/AppOperationsDialogs.tsx";
export type {
  AppArchiveDialogProps,
  AppEditDialogProps,
  AppPublishDialogProps,
  AppSourceDialogProps,
} from "./components/AppOperationsDialogs.tsx";
export { CategoryCascadeSelect } from "./components/CategoryCascadeSelect.tsx";
export { DeployAppTypeSelect } from "./components/DeployAppTypeSelect.tsx";
export { DeployAppTypeGrid } from "./components/DeployAppTypeGrid.tsx";
export { LockedAppTypeField } from "./components/LockedAppTypeField.tsx";
export type { LockedAppTypeFieldProps } from "./components/LockedAppTypeField.tsx";
export { DeployAppTypeIcon } from "./components/DeployAppTypeIcon.tsx";
export { DeployFrameworkSelect } from "./components/DeployFrameworkSelect.tsx";
export { DeployProjectPathBar } from "./components/DeployProjectPathBar.tsx";
export type { DeployProjectPathBarProps } from "./components/DeployProjectPathBar.tsx";
export { DeployProjectDirectoryFields } from "./components/DeployProjectDirectoryFields.tsx";
export { DeployEnvironmentSelect } from "./components/DeployEnvironmentSelect.tsx";
export { DeployAppMediaFields } from "./components/DeployAppMediaFields.tsx";
export type { DeployAppMediaFiles, DeployAppMediaFieldsProps } from "./components/DeployAppMediaFields.tsx";
export { UploadSourceDialog } from "./components/UploadSourceDialog.tsx";
export type { UploadSourceDialogProps } from "./components/UploadSourceDialog.tsx";
export { AppDomainDialog } from "./components/AppDomainDialog.tsx";
export type { AppDomainDialogProps } from "./components/AppDomainDialog.tsx";
export { AppDetailDrawer } from "./components/AppDetailDrawer.tsx";
export type { AppDetailDrawerProps } from "./components/AppDetailDrawer.tsx";
export { AppReleaseHistoryDrawer } from "./components/AppReleaseHistoryDrawer.tsx";
export type { AppReleaseHistoryDrawerProps } from "./components/AppReleaseHistoryDrawer.tsx";
/**
 * 发布历史的判定内核：合并三段读结果、判定这一版能否回滚。
 *
 * 与 `app-source-spec-routing` 同理，这是能力本身的规则而不是内部细节 —— 后端管理
 * 视图要给出同一个「哪一版在线上 / 能不能退回去」的答案，而第二份实现必然与第一份
 * 漂移。导出的是**纯函数**，调用方不必构造任何客户端。
 */
export {
  DEPLOYABLE_RELEASE_STATUSES,
  deploymentStatusTone,
  formatByteSize,
  formatHistoryTime,
  IN_FLIGHT_DEPLOYMENT_STATUSES,
  mergeReleaseHistory,
  packageStatusTone,
  RELEASE_HISTORY_PAGE_SIZE,
  releaseStatusTone,
  RETIRED_PACKAGE_STATUSES,
  rollbackAvailability,
  SERVING_DEPLOYMENT_STATUSES,
  shortDigest,
} from "./service/app-release-history.ts";
export type {
  DeployAppHistorySection,
  DeployAppReleaseHistory,
  DeployAppReleaseHistoryEntry,
  DeployAppReleaseHistoryInput,
  DeployAppRollbackAvailability,
  DeployAppRollbackBlockReason,
  DeployAppRollbackPlan,
  DeployStatusTone,
} from "./service/app-release-history.ts";
export {
  compositionKey,
  createDeployAppOperationsService,
  DEPLOY_PACKAGE_TYPE_OPTIONS,
  relativeNameInZone,
} from "./service/deploy-app-operations.ts";
export type {
  DeployAppDetail,
  DeployAppDomain,
  DeployAppDomainState,
  DeployAppOperationsService,
  DeployAppOperationsServiceOptions,
  DeployAppReleaseHistorySnapshot,
  DeployCodeArchiveUpload,
  DeployCodeSource,
  DeployCustomHostnameInput,
  DeployCustomHostnameResult,
  DeployDriveArchiveOption,
  DeployGitSourceInput,
  DeployPackageTypeOption,
  DeployUploadCodeFromArchiveInput,
  DeployUploadCodeResult,
  DeployUploadProgress,
} from "./service/deploy-app-operations.ts";
export {
  appKindOfCard,
  cardIdsOfAppKind,
  cardsOfAppKind,
  classifyAppTypeCards,
  classifyFrameworks,
  createDeployAppPublishingService,
  deployAppMediaRefFromImageValue,
  detectFrameworkId,
  deriveAppSlug,
  frameworksOfCard,
  isValidSemver,
  requiredSurfaceDirectory,
  resolveDeployAppType,
  CARD_APP_KIND,
  DEPLOY_APP_TYPE_CARDS,
  DEPLOY_APP_TYPE_OPTIONS,
} from "./service/deploy-app-publishing.ts";
export type {
  CreateDeployAppInput,
  DeployAppCategorySelection,
  DeployAppMediaGroup,
  DeployAppMediaRef,
  DeployAppMediaUpload,
  DeployAppPublishingService,
  DeployAppPublishingServiceOptions,
  DeployAppTypeAvailability,
  DeployAppTypeCard,
  DeployAppTypeIconId,
  DeployAppTypeOption,
  DeployFrameworkAvailability,
  DeployFrameworkOption,
} from "./service/deploy-app-publishing.ts";export {
  APP_SURFACE_DIRECTORY_CAPABILITIES,
  APP_SURFACE_DIRECTORY_SUFFIX,
  browserDistOutputPath,
  BROWSER_DIST_ENV_ALIASES,
  buildOutputExists,
  canonicalEnvironment,
  deriveSurfaceDirectory,
  detectBuildOutputCandidates,
  detectSdkworkProject,
  detectedSurfaceIds,
  findDetectedSurface,
  DEPLOY_DEPLOYMENT_MODES,
  DEPLOY_ENVIRONMENT_ALIASES,
  DEPLOY_ENVIRONMENT_IDS,
  deployProfileId,
  joinPath,
  KNOWN_BUILD_OUTPUT_DIRECTORY_NAMES,
  projectProfile,
  repositoryRootOf,
  resolveSourceDirectory,
  shouldSyncEnvironmentBuildOutput,
  surfacesOfDirectoryName,
} from "./service/project-detection.ts";
export type {
  AppSurfaceId,
  DeployDeploymentMode,
  DeployDetectedSurface,
  DeployEnvironmentId,
  DeployProjectConformance,
  DeployProjectDetection,
  DeployProjectInspection,
  DeployProjectProfile,
} from "./service/project-detection.ts";
// 通用类（零依赖，可被控制台 / BirdCoder / CLI / 服务端直接复用）。
export {
  SDKWORK_LAYOUT_MARKERS,
  SDKWORK_NON_SURFACE_SUFFIXES,
  SDKWORK_SURFACE_ARCHITECTURES,
  SdkworkProject,
} from "./service/sdkwork-project.ts";
export type {
  SdkworkAppSurface,
  SdkworkProjectConformance,
  SdkworkProjectInput,
  SdkworkSurfaceArchitecture,
  SdkworkSurfaceDirectoryName,
} from "./service/sdkwork-project.ts";
export {
  DEPLOY_APP_CATEGORY_TREE,
  categoriesForAppKind,
  findCategoryNode,
  categoryPathTo,
} from "./service/app-categories.ts";
export type { DeployAppCategoryNode } from "./service/app-categories.ts";
export {
  APP_ICON_SPEC,
  APP_STORE_PREVIEW_TARGETS,
  COVER_SPEC,
  MAX_SCREENSHOTS_TOTAL,
  MEDIA_ACCEPTED_TYPES,
  PREVIEW_ASPECT_TOLERANCE,
  countScreenshots,
  mediaSpecForAppKind,
  missingRequiredScreenshots,
  previewTargetsForAppKind,
  validateCover,
  validateIcon,
  validatePreviewSize,
} from "./service/app-store-preview-spec.ts";
export type {
  AppMediaSpec,
  AspectBand,
  CoverSpec,
  IconSpec,
  PreviewDevice,
  PreviewFailureReason,
  PreviewSizeTarget,
  PreviewValidationResult,
  StoreId,
} from "./service/app-store-preview-spec.ts";
export {
  publishingText,
  publishingTranslator,
  APP_KIND_LABEL_KEYS,
  /**
   * `deploy_app.app_status` → copy. The admin ledger shows an application's own
   * status above its specs, and re-declaring these six keys there would make a
   * seventh status render as a raw token on one surface and as a label on the
   * other.
   */
  APP_STATUS_LABEL_KEYS,
  APP_SURFACE_LABEL_KEYS,
  /**
   * 发布历史的六个枚举词表一并导出，理由同上：`TAR_GZ` / `PENDING_REVIEW` 这类裸
   * 枚举值一旦在某个面上漏出来，就会和另一个面已经本地化的同一状态并列显示，
   * 读起来像两个不同的东西。
   */
  DEPLOYMENT_KIND_LABEL_KEYS,
  DEPLOYMENT_STATUS_LABEL_KEYS,
  DEPLOYMENT_TARGET_LABEL_KEYS,
  PACKAGE_FORMAT_LABEL_KEYS,
  PACKAGE_STATUS_LABEL_KEYS,
  RELEASE_STATUS_LABEL_KEYS,
} from "./i18n.ts";
export type { PublishingMessageKey, PublishingTranslator } from "./i18n.ts";
