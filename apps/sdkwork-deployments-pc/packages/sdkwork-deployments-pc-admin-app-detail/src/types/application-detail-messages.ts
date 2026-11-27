import type { ApplicationUsageWindow } from "./application-usage";
import type { ApplicationReadinessStep } from "./application-detail";
import type { DeployApplicationKind, DeployApplicationStatus } from "./deploy-application-kind";

/**
 * Copy of the deployment-application detail surface.
 *
 * The package is read-only: it explains what the application is, whether it can
 * be published, what is live, and what it consumes. The commands it offers are
 * host-owned, so their wording is supplied by the console that owns them and is
 * not repeated here.
 */
export interface ApplicationDetailMessages {
  /** Page chrome. */
  title: string;
  subtitle: string;
  notFound: string;
  loadError: string;
  loadErrorForbidden: string;
  loadErrorUnavailable: string;
  retry: string;
  refresh: string;
  back: string;
  /** Action group in the header. */
  actions: {
    configure: string;
    accessToken: string;
    source: string;
    correctKind: string;
    copyLink: string;
    copied: string;
  };
  /** The fact sheet. */
  overview: {
    title: string;
    description: string;
    fields: {
      applicationId: string;
      slug: string;
      kind: string;
      status: string;
      environment: string;
      ownership: string;
      organization: string;
      tenant: string;
      platformTargets: string;
      latestRelease: string;
      createdAt: string;
      updatedAt: string;
      revision: string;
      revisionPending: string;
      description: string;
      webConfig: string;
      webConfigOverridden: string;
      webConfigGenerated: string;
    };
    kinds: Record<DeployApplicationKind, string> & { UNKNOWN: string };
    statuses: Record<DeployApplicationStatus, string> & { UNKNOWN: string };
    ownershipFallback: string;
  };
  /** Factory readiness gate. */
  readiness: {
    title: string;
    description: string;
    status: { READY: string; PENDING: string; BLOCKED: string; UNKNOWN: string };
    blocking: string;
    empty: string;
    steps: Record<ApplicationReadinessStep, string>;
    stepStatuses: {
      READY: string;
      PENDING: string;
      BLOCKED: string;
      EXTERNAL: string;
      NOT_APPLICABLE: string;
      UNKNOWN: string;
    };
  };
  /** Publishing hostnames. */
  domains: {
    title: string;
    description: string;
    empty: string;
    canonical: string;
    columns: {
      hostname: string;
      kind: string;
      environment: string;
      binding: string;
      verification: string;
    };
    kinds: { DEFAULT: string; CUSTOM: string; UNKNOWN: string };
    dnsHint: string;
    cnameHint: string;
    copy: string;
    copied: string;
  };
  /** Delivery history. */
  history: {
    title: string;
    description: string;
    empty: string;
    releases: string;
    deployments: string;
    columnsRelease: { version: string; status: string; createdAt: string };
    columnsDeployment: {
      status: string;
      environment: string;
      kind: string;
      startedAt: string;
      completedAt: string;
      rollbackOf: string;
    };
    rollbackBadge: string;
  };
  /** Resource consumption. */
  usage: {
    title: string;
    description: string;
    window: string;
    windows: Record<ApplicationUsageWindow, string>;
    windowRange: string;
    refresh: string;
    attribution: { application: string; tenant: string; tenantHint: string };
    completeness: { complete: string; partial: string; partialHint: string };
    kpi: {
      egress: string;
      egressHint: string;
      requests: string;
      requestsHint: string;
      apiCalls: string;
      apiCallsHint: string;
      apiCallsTenant: string;
      storage: string;
      storageHint: string;
    };
    trend: {
      title: string;
      description: string;
      requests: string;
      egress: string;
      empty: string;
      maxDay: string;
      total: string;
    };
    hostnames: {
      title: string;
      description: string;
      hostname: string;
      requests: string;
      egress: string;
      share: string;
      empty: string;
    };
    storage: {
      title: string;
      description: string;
      packages: string;
      packageCount: string;
      quotas: string;
      quotaScope: string;
      quotaUsed: string;
      quotaLimit: string;
      quotaSingleFile: string;
      quotaEmpty: string;
      packageEmpty: string;
      scopes: string;
    };
    ledger: {
      title: string;
      description: string;
      dimension: string;
      quantity: string;
      unit: string;
      period: string;
      source: string;
      empty: string;
      dimensions: Record<string, string>;
    };
    unavailable: {
      forbidden: string;
      "not-found": string;
      unavailable: string;
      error: string;
    };
    sources: string;
    noData: string;
    /** Explains that quantities are metered facts, not prices. */
    billingHint: string;
  };
}
