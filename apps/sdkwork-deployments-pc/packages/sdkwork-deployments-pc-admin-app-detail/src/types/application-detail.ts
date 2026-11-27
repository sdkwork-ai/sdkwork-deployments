/**
 * Deployment-application detail read model.
 *
 * One deployment application (`deploy_app`) is one endpoint. The detail page
 * answers three operator questions with it:
 *
 * 1. *what is this?* — the fact sheet: endpoint kind, slug, ownership, tenant
 *    and organization scope, the decimal application id identity provisioning
 *    anchors on, and the lifecycle timestamps;
 * 2. *is it actually publishable/serving?* — the factory readiness gate and the
 *    publishing hostnames with their binding and DNS verification state;
 * 3. *how did it get here, and what is live?* — the release history and the
 *    deployment history, including rollbacks and review gates.
 *
 * Resource consumption is a separate read model (see `application-usage.ts`); a
 * cloud console keeps "what is it" and "what does it consume" apart so a
 * metering outage can never make an application look unconfigured.
 */
import type { ApplicationUsageAvailabilityReason } from "./application-usage";

/** The deploy application fact sheet (`apps.retrieve`). */
export interface DeployApplicationDetail {
  /** `<appId>` prefix of the default publishing hostnames. */
  appDomainLabel?: string | undefined;
  /** Suffix catalog the app publishes on. */
  appDomainSuffixes: readonly string[];
  /** `AppKind` on the wire; `UNKNOWN` when the backend sent a newer value. */
  appKind: string;
  /** `AppStatus` on the wire (`DRAFT`/`READY`/`ACTIVE`/`PAUSED`/`ARCHIVED`/`FAILED`). */
  appStatus: string;
  createdAt?: string | undefined;
  currentRevisionId?: string | undefined;
  defaultEnvironment?: string | undefined;
  /**
   * The **decimal** primary key (`deploy_app.id`) and the identity-provisioning
   * anchor. Kept as the decimal string the wire carried: it is an int64 past
   * 2^53 and must never pass through `Number`.
   */
  deployApplicationId?: string | undefined;
  description?: string | undefined;
  desiredRevisionId?: string | undefined;
  /** Public uuid (`deploy_app.uuid`) every app endpoint resolves against. */
  id: string;
  /**
   * `deploy_app.metadata`, which carries the application-integration binding
   * document (`integrations`) read by the configuration drawer.
   */
  metadata: Readonly<Record<string, unknown>>;
  latestReleaseTag?: string | undefined;
  name: string;
  /** An operator has overridden the generated app-level web configuration. */
  nginxConfigOverridden: boolean;
  /** `"0"` is the tenant-level encoding; a positive value is an organization. */
  organizationId?: string | undefined;
  ownerType?: string | undefined;
  ownershipScope?: string | undefined;
  platformTargetCount?: string | undefined;
  slug?: string | undefined;
  tenantId?: string | undefined;
  updatedAt?: string | undefined;
  version?: string | undefined;
}

/** One step of the factory readiness gate. */
export const APPLICATION_READINESS_STEPS = [
  "REGISTRATION",
  "PLATFORM_TARGET",
  "ENVIRONMENT",
  "IDENTITY",
  "SOURCE",
  "HOSTING_CARRIER",
] as const;

export type ApplicationReadinessStep = (typeof APPLICATION_READINESS_STEPS)[number];

/** One readiness step's verdict (`READY`/`PENDING`/`BLOCKED`/`EXTERNAL`/`NOT_APPLICABLE`). */
export interface ApplicationReadinessCheck {
  step: ApplicationReadinessStep | "UNKNOWN";
  status: string;
  reason?: string | undefined;
  detail?: string | undefined;
}

/** `apps.readiness.retrieve`: whether the application can be published end to end. */
export interface ApplicationFactoryReadiness {
  status: string;
  blockingStep?: string | undefined;
  checks: readonly ApplicationReadinessCheck[];
}

/** One publishing hostname and its delivery state (`apps.domains.list`). */
export interface ApplicationDomainRecord {
  hostname: string;
  /** `DEFAULT` for a platform-provisioned hostname, `CUSTOM` for a registered one. */
  kind: string;
  environment: string;
  /** `PENDING`/`VERIFIED`/`ACTIVE`/`PAUSED`/`FAILED`/`ARCHIVED`. */
  bindingStatus: string;
  /** `NOT_REQUIRED`/`PENDING`/`VERIFIED`/`FAILED`/`EXPIRED`. */
  verificationStatus: string;
  isCanonical: boolean;
  pathPrefix?: string | undefined;
  domainId?: string | undefined;
  /** TXT record a custom hostname must publish to prove ownership. */
  dnsRecordName?: string | undefined;
  dnsRecordValue?: string | undefined;
  /** Platform hostname a custom hostname must be pointed at. */
  cnameTarget?: string | undefined;
}

/** One release in the application's release history. */
export interface ApplicationReleaseRecord {
  id: string;
  semanticVersion: string;
  buildNumber?: string | undefined;
  /** `DRAFT`/`ACTIVE`/`SUPERSEDED`/`DEPRECATED`/`RETIRED`/`ARCHIVED`. */
  releaseStatus: string;
  platformTargetId?: string | undefined;
  createdAt?: string | undefined;
  updatedAt?: string | undefined;
}

/** One deployment attempt in the application's deployment history. */
export interface ApplicationDeploymentRecord {
  id: string;
  environment: string;
  /** `PENDING`…`LIVE`/`DEGRADED`/`FAILED`/`ROLLED_BACK`/`CANCELLED`. */
  deploymentStatus: string;
  deploymentKind?: string | undefined;
  deploymentTarget?: string | undefined;
  strategy?: string | undefined;
  percentage?: number | undefined;
  releaseId?: string | undefined;
  platformTargetId?: string | undefined;
  platformReviewRef?: string | undefined;
  rollbackFromDeploymentId?: string | undefined;
  startedAt?: string | undefined;
  completedAt?: string | undefined;
  createdAt?: string | undefined;
}

/** One independently-loadable delivery region. */
export interface ApplicationDeliveryRegion<T> {
  availability: "available" | "unavailable";
  availabilityReason?: ApplicationUsageAvailabilityReason | undefined;
  items: readonly T[];
}

/** Delivery state shown beside the fact sheet. */
export interface ApplicationDeliveryDetail {
  readiness: ApplicationDeliveryRegion<ApplicationReadinessCheck> & {
    /** Step the gate reports as blocking, when any. */
    blockingStep?: string | undefined;
    /** `READY`/`PENDING`/`BLOCKED`, when the gate answered. */
    status?: string | undefined;
  };
  domains: ApplicationDeliveryRegion<ApplicationDomainRecord>;
  releases: ApplicationDeliveryRegion<ApplicationReleaseRecord>;
  deployments: ApplicationDeliveryRegion<ApplicationDeploymentRecord>;
}

/** The fact sheet together with the delivery state that belongs to it. */
export interface DeployApplicationDetailRecord {
  detail: DeployApplicationDetail;
  delivery: ApplicationDeliveryDetail;
}
