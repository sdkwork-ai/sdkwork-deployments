/**
 * Publishing capability surface: the reusable create-deploy-app components and
 * services. `src/index.ts` re-exports this barrel alongside the module
 * registration, so hosts can import the whole capability from the package root
 * (console shell + BirdCoder plugin) without reaching into package internals.
 */
export { CreateDeployAppDialog } from "./components/CreateDeployAppDialog.tsx";
export type { CreateDeployAppDialogProps, DeployAppPublishResult } from "./components/CreateDeployAppDialog.tsx";
export { BuildProgressDialog } from "./components/BuildProgressDialog.tsx";
export type { BuildProgressDialogProps, DeployDialogBuildFrame, DeployDialogBuildPort } from "./components/BuildProgressDialog.tsx";
export { PublishingAppsPage } from "./components/PublishingAppsPage.tsx";
export type { PublishingAppsPageProps } from "./components/PublishingAppsPage.tsx";
export { CategoryCascadeSelect } from "./components/CategoryCascadeSelect.tsx";
export { DeployAppTypeSelect } from "./components/DeployAppTypeSelect.tsx";
export { DeployAppTypeGrid } from "./components/DeployAppTypeGrid.tsx";
export { DeployAppTypeIcon } from "./components/DeployAppTypeIcon.tsx";
export { DeployFrameworkSelect } from "./components/DeployFrameworkSelect.tsx";
export { DeployProjectPathBar } from "./components/DeployProjectPathBar.tsx";
export type { DeployProjectPathBarProps } from "./components/DeployProjectPathBar.tsx";
export { DeployProjectDirectoryFields } from "./components/DeployProjectDirectoryFields.tsx";
export { DeployEnvironmentSelect } from "./components/DeployEnvironmentSelect.tsx";
export { DeployAppMediaFields, screenshotTargets } from "./components/DeployAppMediaFields.tsx";
export type { DeployAppMediaFiles, DeployAppMediaFieldsProps } from "./components/DeployAppMediaFields.tsx";
export {
  classifyAppTypeCards,
  classifyFrameworks,
  createDeployAppPublishingService,
  detectFrameworkId,
  deriveAppSlug,
  frameworksOfCard,
  isValidSemver,
  requiredSurfaceDirectory,
  resolveDeployAppType,
  toDeployAppMediaRef,
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
} from "./service/deploy-app-publishing.ts";
export {
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
  previewTargetsForAppKind,
  validatePreviewSize,
} from "./service/app-store-preview-spec.ts";
export type { PreviewSizeTarget, PreviewValidationResult } from "./service/app-store-preview-spec.ts";
export {
  publishingText,
  publishingTranslator,
  APP_KIND_LABEL_KEYS,
  APP_STATUS_LABEL_KEYS,
  APP_SURFACE_LABEL_KEYS,
} from "./i18n.ts";
export type { PublishingMessageKey, PublishingTranslator } from "./i18n.ts";
