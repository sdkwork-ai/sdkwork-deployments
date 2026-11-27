/**
 * Deployment-application delivery read model.
 *
 * The detail page needs the application's own fact sheet plus the delivery state
 * that explains how it is being served right now: the factory readiness gate,
 * the publishing hostnames, the release history, and the deployment history.
 * Each of those is a separate read on the deploy app surface, and the page
 * renders them as independent regions so one source that is not reachable does
 * not blank the whole page.
 *
 * The calls arrive through an injected {@link DeployApplicationDetailTransport},
 * so this module performs no I/O of its own and knows no client constructor.
 * Every field is normalized here — the page never reads a wire field, and an
 * absent number stays absent instead of becoming a fabricated `0`.
 */
import type {
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
} from "../types/application-detail";
import { classifyReason } from "./application-usage";

/** Newest-first history windows the detail page renders. */
const HISTORY_PAGE_SIZE = 20;

/**
 * The read transport of the delivery plane.
 *
 * A consuming host implements it with its own deploy app SDK client
 * (`app.retrieve`, `app.readiness.retrieve`, `app.domains.list`, `release.list`,
 * `deployment.list`). Only the fact sheet is required: a method left out renders
 * that region as "not connected" instead of blanking the page.
 */
export interface DeployApplicationDetailTransport {
  /** `apps.retrieve` — the application fact sheet. */
  retrieveApp: (appId: string) => Promise<unknown>;
  /** `apps.readiness.retrieve` — the factory readiness gate. */
  retrieveReadiness?: ((appId: string) => Promise<unknown>) | undefined;
  /** `apps.domains.list` — the publishing hostnames. */
  listDomains?: ((appId: string) => Promise<unknown>) | undefined;
  /** `releases.list` — the release history. */
  listReleases?: ((appId: string, input: { page: number; pageSize: number }) => Promise<unknown>) | undefined;
  /** `deployments.list` — the deployment history. */
  listDeployments?: ((appId: string, input: { page: number; pageSize: number }) => Promise<unknown>) | undefined;
}

export interface DeployApplicationDetailQuery {
  /** Deploy application uuid (`AppResponse.id`, never the decimal id). */
  appId: string;
  /** The read transport; mandatory, because this package owns no SDK client. */
  transport: DeployApplicationDetailTransport;
}

/**
 * Read one application's fact sheet and delivery state.
 *
 * The fact sheet is mandatory: without it there is no page, so its failure is
 * thrown for the caller to localize. The delivery regions are optional and
 * degrade individually with a classified reason.
 */
export async function loadDeployApplicationDetail(
  query: DeployApplicationDetailQuery,
): Promise<DeployApplicationDetailRecord> {
  const transport = query.transport;
  const detail = mapApplicationDetail(
    asRecord(await transport.retrieveApp(query.appId)),
    query.appId,
  );

  const [readiness, domains, releases, deployments] = await Promise.all([
    settle(() => callOptional(transport.retrieveReadiness, (read) => read(query.appId))
      .then(mapReadiness)),
    settle(() => callOptional(transport.listDomains, (read) => read(query.appId))
      .then((payload) => readItems(payload).map(mapDomain))),
    settle(() => callOptional(transport.listReleases, (read) =>
      read(query.appId, { page: 1, pageSize: HISTORY_PAGE_SIZE }))
      .then((payload) => readItems(payload).map(mapRelease))),
    settle(() => callOptional(transport.listDeployments, (read) =>
      read(query.appId, { page: 1, pageSize: HISTORY_PAGE_SIZE }))
      .then((payload) => readItems(payload).map(mapDeployment))),
  ]);

  const delivery: ApplicationDeliveryDetail = {
    deployments: regionOf(deployments),
    domains: regionOf(domains),
    // The readiness gate answers with a status plus its per-step checks, so its
    // region keeps both instead of flattening to the check list.
    readiness: readiness.ok
      ? {
        availability: "available",
        items: readiness.value.checks,
        ...(readiness.value.blockingStep ? { blockingStep: readiness.value.blockingStep } : {}),
        status: readiness.value.status,
      }
      : {
        availability: "unavailable",
        availabilityReason: classifyReason(readiness.error),
        items: [],
      },
    releases: regionOf(releases),
  };
  return { delivery, detail };
}

type Settled<T> = { ok: true; value: T } | { ok: false; error: unknown };

async function settle<T>(load: () => Promise<T>): Promise<Settled<T>> {
  try {
    return { ok: true, value: await load() };
  } catch (error) {
    return { error, ok: false };
  }
}

/** Run an optional transport call, synthesizing "not connected" when absent. */
function callOptional<TRead extends (...args: never[]) => Promise<unknown>>(
  read: TRead | undefined,
  invoke: (read: TRead) => Promise<unknown>,
): Promise<unknown> {
  return read
    ? invoke(read)
    : Promise.reject(Object.assign(new Error("not connected"), { status: 501 }));
}

function regionOf<T>(settled: Settled<readonly T[]>): ApplicationDeliveryRegion<T> {
  return settled.ok
    ? { availability: "available", items: settled.value }
    : { availability: "unavailable", availabilityReason: classifyReason(settled.error), items: [] };
}

function mapApplicationDetail(
  record: Record<string, unknown>,
  requestedId: string,
): DeployApplicationDetail {
  return {
    appDomainSuffixes: readStringArray(record.appDomainSuffixes),
    appKind: readString(record.appKind) ?? "UNKNOWN",
    appStatus: readString(record.appStatus) ?? "UNKNOWN",
    createdAt: readString(record.createdAt),
    currentRevisionId: readString(record.currentRevisionId),
    defaultEnvironment: readString(record.defaultEnvironment),
    deployApplicationId: readString(record.deployApplicationId),
    description: readString(record.description),
    desiredRevisionId: readString(record.desiredRevisionId),
    id: readString(record.id) ?? requestedId,
    latestReleaseTag: readString(record.latestReleaseTag),
    metadata: asRecord(record.metadata),
    name: readString(record.name) ?? requestedId,
    nginxConfigOverridden: readBoolean(record.nginxConfigOverridden) ?? false,
    organizationId: readString(record.organizationId),
    ownerType: readString(record.ownerType),
    ownershipScope: readString(record.ownershipScope),
    platformTargetCount: readString(record.platformTargetCount),
    slug: readString(record.slug),
    tenantId: readString(record.tenantId),
    updatedAt: readString(record.updatedAt),
    version: readString(record.version),
  };
}

function mapReadiness(value: unknown): ApplicationFactoryReadiness {
  const record = asRecord(value);
  const blockingStep = readString(record.blockingStep);
  return {
    ...(blockingStep ? { blockingStep } : {}),
    checks: (Array.isArray(record.checks) ? record.checks : [])
      .map((entry) => mapReadinessCheck(asRecord(entry)))
      .filter((entry) => entry.step !== "UNKNOWN"),
    status: readString(record.status) ?? "PENDING",
  };
}

function mapReadinessCheck(record: Record<string, unknown>): ApplicationReadinessCheck {
  const step = readString(record.step) ?? "UNKNOWN";
  return {
    detail: readString(record.detail),
    reason: readString(record.reason),
    status: readString(record.status) ?? "PENDING",
    step: isReadinessStep(step) ? step : "UNKNOWN",
  };
}

function mapDomain(value: unknown): ApplicationDomainRecord {
  const record = asRecord(value);
  return {
    bindingStatus: readString(record.bindingStatus) ?? "UNKNOWN",
    cnameTarget: readString(record.cnameTarget),
    domainId: readString(record.domainId),
    dnsRecordName: readString(record.dnsRecordName),
    dnsRecordValue: readString(record.dnsRecordValue),
    environment: readString(record.environment) ?? "-",
    hostname: readString(record.hostname) ?? "-",
    isCanonical: readBoolean(record.isCanonical) ?? false,
    kind: readString(record.kind) ?? "DEFAULT",
    pathPrefix: readString(record.pathPrefix),
    verificationStatus: readString(record.verificationStatus) ?? "NOT_REQUIRED",
  };
}

function mapRelease(value: unknown): ApplicationReleaseRecord {
  const record = asRecord(value);
  return {
    buildNumber: readString(record.buildNumber),
    createdAt: readString(record.createdAt),
    id: readString(record.id) ?? "",
    platformTargetId: readString(record.platformTargetId),
    releaseStatus: readString(record.releaseStatus) ?? "UNKNOWN",
    semanticVersion: readString(record.semanticVersion) ?? "-",
    updatedAt: readString(record.updatedAt),
  };
}

function mapDeployment(value: unknown): ApplicationDeploymentRecord {
  const record = asRecord(value);
  const percentage = readNumber(record.percentage);
  return {
    completedAt: readString(record.completedAt),
    createdAt: readString(record.createdAt),
    deploymentKind: readString(record.deploymentKind),
    deploymentStatus: readString(record.deploymentStatus) ?? "UNKNOWN",
    deploymentTarget: readString(record.deploymentTarget),
    environment: readString(record.environment) ?? "-",
    id: readString(record.id) ?? "",
    ...(percentage === undefined ? {} : { percentage }),
    platformReviewRef: readString(record.platformReviewRef),
    platformTargetId: readString(record.platformTargetId),
    releaseId: readString(record.releaseId),
    rollbackFromDeploymentId: readString(record.rollbackFromDeploymentId),
    startedAt: readString(record.startedAt),
    strategy: readString(record.strategy),
  };
}

const READINESS_STEPS: readonly ApplicationReadinessStep[] = [
  "REGISTRATION",
  "PLATFORM_TARGET",
  "ENVIRONMENT",
  "IDENTITY",
  "SOURCE",
  "HOSTING_CARRIER",
];

function isReadinessStep(value: string): value is ApplicationReadinessStep {
  return (READINESS_STEPS as readonly string[]).includes(value);
}

function readItems(payload: unknown): readonly unknown[] {
  if (Array.isArray(payload)) {
    return payload;
  }
  const record = asRecord(payload);
  if (Array.isArray(record.items)) {
    return record.items;
  }
  const data = asRecord(record.data);
  return Array.isArray(data.items) ? data.items : [];
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" ? (value as Record<string, unknown>) : {};
}

function readString(value: unknown): string | undefined {
  if (typeof value === "string") {
    return value.trim() === "" ? undefined : value;
  }
  if (typeof value === "number" && Number.isFinite(value)) {
    return String(value);
  }
  return undefined;
}

function readBoolean(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function readNumber(value: unknown): number | undefined {
  const parsed = typeof value === "number" ? value : Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
}

function readStringArray(value: unknown): readonly string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}
