/**
 * Deployment-application resource and usage read model.
 *
 * Cloud providers report consumption on independent planes and never fold them
 * into one meter:
 *
 * - **traffic** — the chargeable egress/ingress volume and request count the
 *   delivery plane served for the application's hostnames;
 * - **service / API usage** — the metered calls the platform performed on the
 *   application's behalf (AI gateway routing, plus the control plane's own build
 *   and deployment commands);
 * - **storage** — the bytes the application keeps resident (published package
 *   bytes plus its object-store footprint and quotas).
 *
 * Every figure therefore carries the dimension it was measured in, the provider
 * surface it came from, and whether it is an exact sum or a sampled part. A
 * surface that cannot answer says `availability: "unavailable"` with a reason
 * instead of rendering `0`: a fabricated zero reads as "this application used
 * nothing" rather than "this meter is not wired for this installation", and a
 * cloud console must never make that distinction silently.
 */

/** Which plane one figure belongs to. */
export const USAGE_PLANES = ["traffic", "service", "storage"] as const;

export type UsagePlane = (typeof USAGE_PLANES)[number];

/**
 * Canonical usage dimensions this platform meters.
 *
 * `traffic.*` and the control-plane dimensions come from the deploy usage
 * ledger (`deploy_usage_event`); `api.requests` is Cloud Router service usage.
 */
export const APPLICATION_USAGE_DIMENSIONS = [
  "traffic.requests",
  "traffic.ingress_bytes",
  "traffic.egress_bytes",
  "api.requests",
  "build_minutes",
  "package_storage_bytes",
  "deployment_count",
] as const;

export type ApplicationUsageDimension = (typeof APPLICATION_USAGE_DIMENSIONS)[number];

/** Whether a reported figure is an exact sum or a bounded sample. */
export type ApplicationUsageCompleteness = "complete" | "partial";

/** Why one region of the usage page has no figure. */
export type ApplicationUsageAvailabilityReason =
  | "forbidden"
  | "not-found"
  | "unavailable"
  | "error";

/** Whether a plane answered at all. */
export type ApplicationUsageAvailability = "available" | "unavailable";

/** One metered quantity with the unit it was measured in. */
export interface ApplicationUsageMeasure {
  dimension: ApplicationUsageDimension;
  /** Exact decimal quantity as the wire carried it (never a rounded float). */
  quantity: string;
  /**
   * Unit of `quantity`: `BYTE`, `REQUEST`, `MINUTE`, `COUNT`, or `TOKEN`.
   * Empty when the provider declared none.
   */
  unit: string;
  /** Provider surface that produced the figure (`deploy.usage_events`, …). */
  sourceLabel: string;
}

/** Traffic totals for the selected window. */
export interface ApplicationTrafficUsage {
  requests?: ApplicationUsageMeasure | undefined;
  ingressBytes?: ApplicationUsageMeasure | undefined;
  egressBytes?: ApplicationUsageMeasure | undefined;
}

/** Control-plane and gateway service usage for the selected window. */
export interface ApplicationServiceUsage {
  buildMinutes?: ApplicationUsageMeasure | undefined;
  deploymentCount?: ApplicationUsageMeasure | undefined;
  apiRequests?: ApplicationUsageMeasure | undefined;
  apiTokens?: ApplicationUsageMeasure | undefined;
  /**
   * `tenant` means the API figures describe the whole tenant rather than this
   * application alone, and the page must label them as such.
   */
  apiAttribution?: "tenant" | "application" | undefined;
}

/** A point on the daily traffic trend. */
export interface ApplicationTrafficTrendPoint {
  /** UTC calendar day (`YYYY-MM-DD`). */
  date: string;
  requests: string;
  ingressBytes: string;
  egressBytes: string;
}

/** One traffic dimension broken down by the hostname that served it. */
export interface ApplicationTrafficByHostname {
  hostname: string;
  requests: string;
  egressBytes: string;
}

/** Storage footprint of one measured resource class. */
export interface ApplicationStorageUsage {
  /** `package_storage_bytes` for the package registry, or a scope label. */
  dimension: ApplicationUsageDimension;
  label: string;
  usedBytes: string;
  sourceLabel: string;
}

/** A storage quota policy and its headroom. */
export interface ApplicationStorageQuota {
  id: string;
  scopeType: string;
  scopeId: string;
  label: string;
  limitBytes?: string | undefined;
  usedBytes?: string | undefined;
  singleFileLimitBytes?: string | undefined;
  enforcement?: string | undefined;
  status?: string | undefined;
}

/** One published package counted toward the package footprint. */
export interface ApplicationPackageFootprint {
  id: string;
  semanticVersion: string;
  packageFormat?: string | undefined;
  sizeBytes: string;
  status?: string | undefined;
  createdAt?: string | undefined;
}

/** One row of the raw usage ledger. */
export interface ApplicationUsageLedgerEntry {
  id: string;
  /** Window start of the fact (RFC 3339). */
  periodStart?: string | undefined;
  dimension: string;
  quantity: string;
  unit: string;
  /**
   * Idempotency identity the data plane emitted. It also carries the traffic
   * attribution (`traffic:<window>:<tenant>:<app>:<binding>:<host>:<ip>:<dim>`),
   * which is how this read model recovers the serving hostname.
   */
  deduplicationKey?: string | undefined;
  observedAt?: string | undefined;
  createdAt?: string | undefined;
}

/** Traffic plane of the usage snapshot. */
export interface ApplicationTrafficSnapshot {
  availability: ApplicationUsageAvailability;
  availabilityReason?: ApplicationUsageAvailabilityReason | undefined;
  /**
   * `true` when at least one fact names this application (or one of its
   * hostnames); `false` means the figures describe the whole tenant.
   */
  attributed: boolean;
  /** `false` when the window has no traffic fact at all (reported, not zeroed). */
  measured: boolean;
  totals: ApplicationTrafficUsage;
  trend: readonly ApplicationTrafficTrendPoint[];
  byHostname: readonly ApplicationTrafficByHostname[];
  /** Traffic dimensions the ledger returned, for the ledger table's facets. */
  dimensions: readonly string[];
  completeness: ApplicationUsageCompleteness;
}

/** Storage plane of the usage snapshot. */
export interface ApplicationStorageSnapshot {
  availability: ApplicationUsageAvailability;
  availabilityReason?: ApplicationUsageAvailabilityReason | undefined;
  items: readonly ApplicationStorageUsage[];
  quotas: readonly ApplicationStorageQuota[];
  packages: readonly ApplicationPackageFootprint[];
  completeness: ApplicationUsageCompleteness;
}

/** Service plane of the usage snapshot. */
export interface ApplicationServiceSnapshot {
  availability: ApplicationUsageAvailability;
  availabilityReason?: ApplicationUsageAvailabilityReason | undefined;
  usage: ApplicationServiceUsage;
  /** Provider that produced the API figures, when it answered. */
  apiSource?: string | undefined;
}

/**
 * The complete usage read model for one deployment application.
 *
 * Each plane carries its own `availability` so one failing provider degrades
 * one card while the rest of the picture stays readable.
 */
export interface ApplicationUsageSnapshot {
  /** Inclusive UTC day the window starts on (`YYYY-MM-DD`). */
  windowStart: string;
  /** Exclusive UTC day the window ends on (`YYYY-MM-DD`). */
  windowEnd: string;
  traffic: ApplicationTrafficSnapshot;
  service: ApplicationServiceSnapshot;
  storage: ApplicationStorageSnapshot;
  /** Raw ledger rows in the order the provider returned them (newest first). */
  ledger: readonly ApplicationUsageLedgerEntry[];
  /** Providers that answered while building this snapshot, in call order. */
  sources: readonly string[];
}

/** The rolling windows the usage page can request. */
export const APPLICATION_USAGE_WINDOWS = ["24h", "7d", "30d"] as const;

export type ApplicationUsageWindow = (typeof APPLICATION_USAGE_WINDOWS)[number];

/** One provider response that failed while the snapshot was assembled. */
export interface ApplicationUsageSourceFailure {
  source: string;
  reason: ApplicationUsageAvailabilityReason;
  error?: unknown | undefined;
}
