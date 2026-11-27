/**
 * SDKWork Deployments PC — deployment application detail.
 *
 * Owns the read model, the page, and the copy of one deployment application for
 * the `backend-admin` surface. The package belongs to the `deployment` domain
 * repository, which is what makes it the *domain's* view of a deployment
 * application rather than one console's private page: any console that manages
 * deployment applications consumes this package and supplies its own SDK client
 * and commands through the ports.
 */
export { DeploymentApplicationDetailPage } from "./pages/DeploymentApplicationDetailPage";
export { TrafficTrendChart, peakTrendPoint, sumTrend } from "./components/UsageTrendChart";
export type { TrafficTrendMetric } from "./components/UsageTrendChart";

export type {
  DeploymentApplicationDetailCapabilityPort,
  DeploymentApplicationDetailDataSource,
  DeploymentApplicationDetailPermissionPort,
  DeploymentApplicationDetailReadQuery,
  DeploymentApplicationUsageRead,
} from "./services/ports";

export { classifyReason, loadApplicationUsage, resolveUsageWindow } from "./services/application-usage";
export type {
  ApplicationUsageQuery,
  ApplicationUsageResult,
  ApplicationUsageTransport,
} from "./services/application-usage";
export { loadDeployApplicationDetail } from "./services/deploy-application-detail";
export { createDeployApplicationDetailTransport } from "./services/client-transport";
export type { DeployApplicationDetailClientTransport } from "./services/client-transport";
export type {
  DeployApplicationDetailQuery,
  DeployApplicationDetailTransport,
} from "./services/deploy-application-detail";

export { DEPLOYMENTS_APP_DETAIL_I18N, useApplicationDetailMessages } from "./i18n";
export type { ApplicationDetailMessages } from "./i18n";

export {
  formatByteSize,
  formatDate,
  formatDateTime,
  formatRatio,
  formatUsageCount,
  formatUsageDimensionLabel,
  formatUsageMeasure,
  formatUsageWindowRange,
  formatUtcDay,
  usageRatio,
} from "./services/application-usage-format";

export {
  APPLICATION_READINESS_STEPS,
} from "./types/application-detail";
export type {
  ApplicationDeliveryDetail,
  ApplicationDeliveryRegion,
  ApplicationDeploymentRecord,
  ApplicationDomainRecord,
  ApplicationFactoryReadiness,
  ApplicationReadinessCheck,
  ApplicationReadinessStep,
  ApplicationReleaseRecord,
  DeployApplicationDetail,
  DeployApplicationDetailRecord,
} from "./types/application-detail";
export {
  APPLICATION_USAGE_DIMENSIONS,
  APPLICATION_USAGE_WINDOWS,
  USAGE_PLANES,
} from "./types/application-usage";
export type {
  ApplicationPackageFootprint,
  ApplicationServiceSnapshot,
  ApplicationServiceUsage,
  ApplicationStorageQuota,
  ApplicationStorageSnapshot,
  ApplicationStorageUsage,
  ApplicationTrafficByHostname,
  ApplicationTrafficSnapshot,
  ApplicationTrafficTrendPoint,
  ApplicationTrafficUsage,
  ApplicationUsageAvailability,
  ApplicationUsageAvailabilityReason,
  ApplicationUsageCompleteness,
  ApplicationUsageDimension,
  ApplicationUsageLedgerEntry,
  ApplicationUsageMeasure,
  ApplicationUsageSnapshot,
  ApplicationUsageSourceFailure,
  ApplicationUsageWindow,
  UsagePlane,
} from "./types/application-usage";
export {
  DEPLOY_APPLICATION_KINDS,
  DEPLOY_APPLICATION_STATUSES,
} from "./types/deploy-application-kind";
export type {
  DeployApplicationKind,
  DeployApplicationStatus,
} from "./types/deploy-application-kind";
