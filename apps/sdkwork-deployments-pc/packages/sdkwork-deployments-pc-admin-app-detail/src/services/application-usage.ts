/**
 * Deployment-application resource consumption read model.
 *
 * This module turns the provider answers of the usage plane into one normalized
 * snapshot ({@link ApplicationUsageSnapshot}) so the page never reads a wire
 * field. It performs no I/O of its own: the calls arrive through an injected
 * {@link ApplicationUsageTransport}, which the deployments console implements
 * with its own generated SDK client (`FRONTEND_SPEC.md` §3):
 *
 * - usage ledger and package registry: `usage.usageEvents.list`, `package.list`;
 * - object-store footprint and storage quotas: `storage.oss.usage.list`,
 *   `storage.oss.quotas.list`;
 * - AI gateway service usage: `ai.routing.usage.retrieve`.
 *
 * A meter the transport cannot serve is reported as unavailable with a reason,
 * never as zero.
 *
 * Contract limits this module encodes (they are also why the page labels a
 * figure "tenant" instead of "application"):
 *
 * 1. `GET /app/v3/api/usage_events` is tenant-scoped — it accepts only
 *    `page`/`page_size`, so traffic attribution is recovered client-side from the
 *    `deduplicationKey` the data plane emits
 *    (`traffic:<window>:<tenant>:<uuid>:<binding>:<host>:<ip>:<dim>`). The
 *    aggregate read model belongs to the Web Server operations surface, so this
 *    page samples the append-only facts and marks the result `partial` rather
 *    than claiming a total.
 * 2. The AI routing usage snapshot is likewise tenant wide.
 * 3. Package bytes are per application; the object-store footprint and its
 *    quotas are per storage scope.
 */
import type {
  ApplicationPackageFootprint,
  ApplicationStorageQuota,
  ApplicationStorageUsage,
  ApplicationTrafficByHostname,
  ApplicationTrafficSnapshot,
  ApplicationTrafficTrendPoint,
  ApplicationTrafficUsage,
  ApplicationUsageAvailabilityReason,
  ApplicationUsageCompleteness,
  ApplicationUsageDimension,
  ApplicationUsageLedgerEntry,
  ApplicationUsageMeasure,
  ApplicationUsageSnapshot,
  ApplicationUsageSourceFailure,
  ApplicationUsageWindow,
  ApplicationServiceSnapshot,
  ApplicationServiceUsage,
} from "../types/application-usage";

/** Days covered by each selectable window; `24h` still reads whole UTC days. */
export const APPLICATION_USAGE_WINDOW_DAYS: Record<ApplicationUsageWindow, number> = {
  "24h": 1,
  "7d": 7,
  "30d": 30,
};

/** Dimensions whose quantity is a byte count, for storage formatting. */
export const BYTE_USAGE_DIMENSIONS: ReadonlySet<string> = new Set([
  "traffic.ingress_bytes",
  "traffic.egress_bytes",
  "package_storage_bytes",
]);

/**
 * Bounds on the usage-ledger read.
 *
 * The deploy surface answers one offset page at a time and publishes no
 * aggregate here, so ten full pages bound the work and the snapshot records the
 * result as sampled (`partial`) instead of presenting it as a total.
 */
const LEDGER_MAX_PAGES = 10;
const LEDGER_PAGE_SIZE = 200;
const PACKAGE_MAX_PAGES = 5;
const PACKAGE_PAGE_SIZE = 200;
const INFRASTRUCTURE_PAGE_SIZE = 200;

/** Canonical deploy usage dimensions of the data plane, for the ledger. */
const TRAFFIC_DIMENSION_PREFIX = "traffic.";

/**
 * The read transport of the usage plane.
 *
 * A consuming host implements it with its own generated SDK client. This module
 * defines the shape only — it never constructs a client, reads a base URL, or
 * attaches credentials (`UI_ARCHITECTURE_SPEC.md` §3).
 *
 * Every method answers with the provider response as-is; normalization happens
 * once in {@link loadApplicationUsage}, so a host adapter stays a thin mapping.
 */
export interface ApplicationUsageTransport {
  /** The application's canonical hostnames (`apps.domains.list`). */
  listDomains: (appId: string) => Promise<unknown>;
  /** One page of an application's registered packages (`package.list`). */
  listPackages: (appId: string, input: { page: number; pageSize: number }) => Promise<unknown>;
  /** One page of the tenant usage ledger (`usage.usageEvents.list`). */
  listUsageEvents: (input: { page: number; pageSize: number }) => Promise<unknown>;
  /** Object-store usage scopes (`storage.oss.usage.list`). */
  listStorageUsage?: ((input: { pageSize: number }) => Promise<unknown>) | undefined;
  /** Object-store quota policies (`storage.oss.quotas.list`). */
  listStorageQuotas?: ((input: { pageSize: number }) => Promise<unknown>) | undefined;
  /** The tenant-wide AI routing usage snapshot (`ai.routing.usage.retrieve`). */
  retrieveRoutingUsage?: (() => Promise<unknown>) | undefined;
}

export interface ApplicationUsageQuery {
  /** Deploy application uuid (`AppResponse.id`). */
  appId: string;
  /** Hostnames already known by the caller; the transport also reads them. */
  hostnames?: readonly string[];
  window: ApplicationUsageWindow;
  /** The read transport; mandatory, because this package owns no SDK client. */
  transport: ApplicationUsageTransport;
  /** Injectable clock so the window is deterministic in tests. */
  now?: Date;
}

export interface ApplicationUsageResult {
  snapshot: ApplicationUsageSnapshot;
  failures: readonly ApplicationUsageSourceFailure[];
}

/**
 * Assemble the usage snapshot for one application and window.
 *
 * Sources settle independently: a failing object-store probe must not erase a
 * correct traffic figure, and each plane reports its own availability. The
 * snapshot's `sources` names every provider that answered.
 */
export async function loadApplicationUsage(
  query: ApplicationUsageQuery,
): Promise<ApplicationUsageResult> {
  const transport = query.transport;
  const failures: ApplicationUsageSourceFailure[] = [];
  const sources: string[] = [];
  const window = resolveUsageWindow(query.window, query.now ?? new Date());

  const hostnames = await readHostnames(transport, query, failures, sources);
  const ledger = await readLedger(transport, failures, sources);
  const packages = await readPackages(transport, query.appId, failures, sources);
  const storage = await readStorage(transport, failures, sources);
  const service = await readServiceUsage(transport, ledger.events, failures, sources);

  const storageItems: ApplicationStorageUsage[] = [];
  if (packages.availability === "available") {
    storageItems.push({
      dimension: "package_storage_bytes",
      label: "package",
      sourceLabel: "deploy.packages",
      usedBytes: String(packages.items.reduce((total, item) => total + toSafeNumber(item.sizeBytes), 0)),
    });
  }
  storageItems.push(...storage.items);

  const storageAvailable = packages.availability === "available" || storage.available;
  const snapshot: ApplicationUsageSnapshot = {
    ledger: ledger.events,
    service,
    sources,
    storage: {
      availability: storageAvailable ? "available" : "unavailable",
      ...(storageAvailable
        ? {}
        : { availabilityReason: resolveReason(failures, "storage.usage") }),
      completeness: packages.complete && storage.complete ? "complete" : "partial",
      items: storageItems,
      packages: packages.items,
      quotas: storage.quotas,
    },
    traffic: buildTraffic(
      ledger.events,
      window,
      buildAttributionKeys(query.appId, hostnames),
      ledger.complete,
    ),
    windowEnd: window.end,
    windowStart: window.start,
  };
  return { failures, snapshot };
}

/**
 * Inclusive / exclusive UTC day window for a rolling selection.
 *
 * The ledger partitions by UTC calendar day, so a window is expressed as whole
 * days: `30d` today covers exactly thirty days ending today inclusive.
 */
export function resolveUsageWindow(
  selection: ApplicationUsageWindow,
  now: Date,
): { start: string; end: string } {
  const days = APPLICATION_USAGE_WINDOW_DAYS[selection] ?? APPLICATION_USAGE_WINDOW_DAYS["7d"];
  const endDay = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate()));
  return {
    end: formatUtcDay(new Date(endDay.getTime() + 86_400_000)),
    start: formatUtcDay(new Date(endDay.getTime() - (days - 1) * 86_400_000)),
  };
}

async function readHostnames(
  transport: ApplicationUsageTransport,
  query: ApplicationUsageQuery,
  failures: ApplicationUsageSourceFailure[],
  sources: string[],
): Promise<readonly string[]> {
  const known = [...(query.hostnames ?? [])].filter(Boolean);
  try {
    const payload = await transport.listDomains(query.appId);
    const discovered = readItems(payload)
      .map((row) => readString(asRecord(row).hostname))
      .filter((hostname): hostname is string => Boolean(hostname));
    sources.push("deploy.apps.domains");
    return [...new Set([...known, ...discovered])];
  } catch (error) {
    // A missing domain read only narrows attribution; the ledger is still read.
    failures.push({ error, reason: classifyReason(error), source: "deploy.apps.domains" });
    return known;
  }
}

interface LedgerRead {
  events: readonly ApplicationUsageLedgerEntry[];
  complete: boolean;
}

async function readLedger(
  transport: ApplicationUsageTransport,
  failures: ApplicationUsageSourceFailure[],
  sources: string[],
): Promise<LedgerRead> {
  const events: ApplicationUsageLedgerEntry[] = [];
  let complete = false;
  try {
    for (let page = 1; page <= LEDGER_MAX_PAGES; page += 1) {
      const payload = await transport.listUsageEvents({ page, pageSize: LEDGER_PAGE_SIZE });
      const rows = readItems(payload);
      events.push(...rows.map(mapLedgerEntry));
      const pageInfo = asRecord(asRecord(payload).pageInfo);
      const hasMore = readBoolean(pageInfo.hasMore);
      const totalItems = readCount(pageInfo.totalItems);
      if (hasMore === false || rows.length < LEDGER_PAGE_SIZE) {
        complete = true;
        break;
      }
      if (totalItems !== undefined && events.length >= totalItems) {
        complete = true;
        break;
      }
    }
    sources.push("deploy.usage_events");
  } catch (error) {
    failures.push({ error, reason: classifyReason(error), source: "deploy.usage_events" });
  }
  return { complete, events };
}

function buildTraffic(
  events: readonly ApplicationUsageLedgerEntry[],
  window: { start: string; end: string },
  attributionKeys: readonly string[],
  ledgerComplete: boolean,
): ApplicationTrafficSnapshot {
  const inWindow = events.filter((event) => {
    const day = toUtcDay(event.periodStart ?? event.observedAt ?? event.createdAt);
    return day !== undefined && day >= window.start && day < window.end;
  });
  // Attribution: the data plane names the app by its public uuid (and the
  // serving hostname); when nothing matches — an older fact, or a deployment
  // whose data plane does not emit attribution — the tenant's facts are shown as
  // a labelled tenant-wide figure instead of an unexplained zero.
  const attributed = inWindow.filter((event) =>
    attributionKeys.some((key) => eventMatchesKey(event, key)));
  const scoped = attributed.length > 0 ? attributed : inWindow;
  const totalsByDimension = new Map<string, number>();
  const trendByDay = new Map<string, { requests: number; ingress: number; egress: number }>();
  const hostByKey = new Map<string, { requests: number; egressBytes: number }>();
  for (const event of scoped) {
    const quantity = toSafeNumber(event.quantity);
    totalsByDimension.set(event.dimension, (totalsByDimension.get(event.dimension) ?? 0) + quantity);
    const day = toUtcDay(event.periodStart ?? event.observedAt ?? event.createdAt);
    if (day) {
      const point = trendByDay.get(day) ?? { egress: 0, ingress: 0, requests: 0 };
      if (event.dimension === "traffic.requests") point.requests += quantity;
      if (event.dimension === "traffic.ingress_bytes") point.ingress += quantity;
      if (event.dimension === "traffic.egress_bytes") point.egress += quantity;
      trendByDay.set(day, point);
    }
    const hostname = readAttributionHostname(event);
    if (hostname) {
      const row = hostByKey.get(hostname) ?? { egressBytes: 0, requests: 0 };
      if (event.dimension === "traffic.requests") row.requests += quantity;
      if (event.dimension === "traffic.egress_bytes") row.egressBytes += quantity;
      hostByKey.set(hostname, row);
    }
  }
  const completeness: ApplicationUsageCompleteness = ledgerComplete ? "complete" : "partial";
  const byHostname: ApplicationTrafficByHostname[] = [...hostByKey.entries()]
    .map(([hostname, row]) => ({
      egressBytes: String(row.egressBytes),
      hostname,
      requests: String(row.requests),
    }))
    .sort((left, right) => toSafeNumber(right.egressBytes) - toSafeNumber(left.egressBytes));
  return {
    attributed: attributed.length > 0,
    availability: "available",
    byHostname,
    completeness,
    dimensions: [...totalsByDimension.keys()]
      .filter((dimension) => dimension.startsWith(TRAFFIC_DIMENSION_PREFIX))
      .sort(),
    // `measured` distinguishes "the window carried no traffic" from "the window
    // has not been metered yet": both render as an explicit statement.
    measured: inWindow.length > 0,
    totals: toTrafficTotals(totalsByDimension),
    trend: renderTrafficTrend(trendByDay, window),
  };
}

function toTrafficTotals(totals: ReadonlyMap<string, number>): ApplicationTrafficUsage {
  const measure = (
    dimension: ApplicationUsageDimension,
    unit: string,
  ): ApplicationUsageMeasure | undefined => {
    const quantity = totals.get(dimension);
    return quantity === undefined
      ? undefined
      : { dimension, quantity: String(quantity), sourceLabel: "deploy.usage_events", unit };
  };
  const requests = measure("traffic.requests", "REQUEST");
  const ingressBytes = measure("traffic.ingress_bytes", "BYTE");
  const egressBytes = measure("traffic.egress_bytes", "BYTE");
  return {
    ...(requests ? { requests } : {}),
    ...(ingressBytes ? { ingressBytes } : {}),
    ...(egressBytes ? { egressBytes } : {}),
  };
}

/** Every UTC day of the window, so the trend renders a zero baseline. */
function renderTrafficTrend(
  trendByDay: ReadonlyMap<string, { requests: number; ingress: number; egress: number }>,
  window: { start: string; end: string },
): ApplicationTrafficTrendPoint[] {
  const points: ApplicationTrafficTrendPoint[] = [];
  let cursor = parseUtcDay(window.start);
  const end = parseUtcDay(window.end);
  if (!cursor || !end) {
    return [...trendByDay.entries()].map(([date, value]) => ({
      date,
      egressBytes: String(value.egress),
      ingressBytes: String(value.ingress),
      requests: String(value.requests),
    }));
  }
  while (cursor.getTime() < end.getTime() && points.length < 400) {
    const day = formatUtcDay(cursor);
    const value = trendByDay.get(day);
    points.push({
      date: day,
      egressBytes: String(value?.egress ?? 0),
      ingressBytes: String(value?.ingress ?? 0),
      requests: String(value?.requests ?? 0),
    });
    cursor = new Date(cursor.getTime() + 86_400_000);
  }
  return points;
}

interface PackageRead {
  availability: "available" | "unavailable";
  availabilityReason?: ApplicationUsageAvailabilityReason;
  complete: boolean;
  items: readonly ApplicationPackageFootprint[];
}

async function readPackages(
  transport: ApplicationUsageTransport,
  appId: string,
  failures: ApplicationUsageSourceFailure[],
  sources: string[],
): Promise<PackageRead> {
  const items: ApplicationPackageFootprint[] = [];
  let complete = false;
  try {
    for (let page = 1; page <= PACKAGE_MAX_PAGES; page += 1) {
      const payload = await transport.listPackages(appId, { page, pageSize: PACKAGE_PAGE_SIZE });
      const rows = readItems(payload);
      items.push(...rows.map(mapPackage));
      if (rows.length < PACKAGE_PAGE_SIZE) {
        complete = true;
        break;
      }
    }
    sources.push("deploy.packages");
    return { availability: "available", complete, items };
  } catch (error) {
    const reason = classifyReason(error);
    failures.push({ error, reason, source: "deploy.packages" });
    return { availability: "unavailable", availabilityReason: reason, complete: false, items };
  }
}

interface StorageRead {
  available: boolean;
  complete: boolean;
  items: readonly ApplicationStorageUsage[];
  quotas: readonly ApplicationStorageQuota[];
}

async function readStorage(
  transport: ApplicationUsageTransport,
  failures: ApplicationUsageSourceFailure[],
  sources: string[],
): Promise<StorageRead> {
  const listStorageUsage = transport.listStorageUsage;
  const listStorageQuotas = transport.listStorageQuotas;
  if (!listStorageUsage) {
    failures.push({ reason: "unavailable", source: "storage.usage" });
    return { available: false, complete: false, items: [], quotas: [] };
  }
  let items: ApplicationStorageUsage[] = [];
  let quotas: ApplicationStorageQuota[] = [];
  let complete = true;
  try {
    items = readItems(await listStorageUsage({ pageSize: INFRASTRUCTURE_PAGE_SIZE }))
      .map(mapStorageUsage);
    sources.push("storage.usage");
  } catch (error) {
    complete = false;
    failures.push({ error, reason: classifyReason(error), source: "storage.usage" });
  }
  try {
    if (listStorageQuotas) {
      quotas = readItems(await listStorageQuotas({ pageSize: INFRASTRUCTURE_PAGE_SIZE }))
        .map(mapStorageQuota);
      sources.push("storage.quotas");
    }
  } catch (error) {
    complete = false;
    failures.push({ error, reason: classifyReason(error), source: "storage.quotas" });
  }
  return { available: items.length > 0 || quotas.length > 0, complete, items, quotas };
}

async function readServiceUsage(
  transport: ApplicationUsageTransport,
  events: readonly ApplicationUsageLedgerEntry[],
  failures: ApplicationUsageSourceFailure[],
  sources: string[],
): Promise<ApplicationServiceSnapshot> {
  const usage: ApplicationServiceUsage = {};
  const buildMinutes = sumDimension(events, "build_minutes");
  const deploymentCount = sumDimension(events, "deployment_count");
  if (buildMinutes !== undefined) {
    usage.buildMinutes = {
      dimension: "build_minutes",
      quantity: String(buildMinutes),
      sourceLabel: "deploy.usage_events",
      unit: "MINUTE",
    };
  }
  if (deploymentCount !== undefined) {
    usage.deploymentCount = {
      dimension: "deployment_count",
      quantity: String(deploymentCount),
      sourceLabel: "deploy.usage_events",
      unit: "COUNT",
    };
  }
  const retrieveRoutingUsage = transport.retrieveRoutingUsage;
  if (!retrieveRoutingUsage) {
    failures.push({ reason: "unavailable", source: "cloudrouter.ai_routing_usage" });
    return { availability: "unavailable", availabilityReason: "unavailable", usage };
  }
  try {
    const payload = asRecord(await retrieveRoutingUsage());
    const chart = Array.isArray(payload.chartData) ? payload.chartData : [];
    const requests = chart.reduce(
      (total, point) => total + toSafeNumber(readString(asRecord(point).requests) ?? "0"),
      0,
    );
    const tokens = (Array.isArray(payload.modelStats) ? payload.modelStats : []).reduce(
      (total, row) => total + toSafeNumber(readString(asRecord(row).tok) ?? "0"),
      0,
    );
    // The routing snapshot is tenant wide: it names no application, so the page
    // prints these figures under a tenant label rather than claiming them.
    usage.apiAttribution = "tenant";
    usage.apiRequests = {
      dimension: "api.requests",
      quantity: String(requests),
      sourceLabel: "cloudrouter.ai_routing_usage",
      unit: "REQUEST",
    };
    usage.apiTokens = {
      dimension: "api.requests",
      quantity: String(tokens),
      sourceLabel: "cloudrouter.ai_routing_usage",
      unit: "TOKEN",
    };
    sources.push("cloudrouter.ai_routing_usage");
    return { apiSource: "cloudrouter.ai_routing_usage", availability: "available", usage };
  } catch (error) {
    const reason = classifyReason(error);
    failures.push({ error, reason, source: "cloudrouter.ai_routing_usage" });
    return { availability: "unavailable", availabilityReason: reason, usage };
  }
}

function sumDimension(
  events: readonly ApplicationUsageLedgerEntry[],
  dimension: string,
): number | undefined {
  const matching = events.filter((event) => event.dimension === dimension);
  return matching.length === 0
    ? undefined
    : matching.reduce((total, event) => total + toSafeNumber(event.quantity), 0);
}

/**
 * Keys a usage fact's deduplication identity is matched against.
 *
 * The data plane emits `traffic:<window>:<tenant>:<app-uuid>:<binding>:<host>:<ip>:<dim>`;
 * the decimal application id and the serving hostnames are accepted because a
 * node may attribute a window by whichever of them it resolved first.
 */
function buildAttributionKeys(appId: string, hostnames: readonly string[]): string[] {
  return [appId, ...hostnames]
    .map((value) => value.trim().toLowerCase())
    .filter((value) => value.length > 0);
}

function eventMatchesKey(event: ApplicationUsageLedgerEntry, key: string): boolean {
  return event.deduplicationKey !== undefined
    && event.deduplicationKey.toLowerCase().includes(key);
}

/** `traffic:<window>:<tenant>:<app>:<binding>:<host>:<ip>:<dim>` → host. */
function readAttributionHostname(event: ApplicationUsageLedgerEntry): string | undefined {
  const key = event.deduplicationKey;
  if (!key) {
    return undefined;
  }
  const parts = key.split(":");
  return parts.length >= 6 && parts[0] === "traffic" ? parts[5] : undefined;
}

function mapLedgerEntry(value: unknown): ApplicationUsageLedgerEntry {
  const record = asRecord(value);
  return {
    ...optionalEntry("createdAt", readString(record.createdAt)),
    ...optionalEntry("deduplicationKey", readString(record.deduplicationKey)),
    dimension: readString(record.dimension) ?? "unknown",
    id: readString(record.id) ?? "",
    ...optionalEntry("observedAt", readString(record.observedAt)),
    ...optionalEntry("periodStart", readString(record.periodStart)),
    quantity: readString(record.quantity) ?? "0",
    unit: readString(record.unit) ?? "",
  };
}

function mapPackage(value: unknown): ApplicationPackageFootprint {
  const record = asRecord(value);
  return {
    ...optionalEntry("createdAt", readString(record.createdAt)),
    id: readString(record.id) ?? "",
    ...optionalEntry("packageFormat", readString(record.packageFormat)),
    semanticVersion: readString(record.semanticVersion) ?? "-",
    sizeBytes: readString(record.packageSizeBytes) ?? "0",
    ...optionalEntry("status", readString(record.packageStatus)),
  };
}

function mapStorageUsage(value: unknown): ApplicationStorageUsage {
  const record = asRecord(value);
  return {
    dimension: "package_storage_bytes",
    label: readString(record.scope) ?? readString(record.scopeType) ?? "object-store",
    sourceLabel: "storage.usage",
    usedBytes: readString(record.usedBytes) ?? "0",
  };
}

function mapStorageQuota(value: unknown): ApplicationStorageQuota {
  const record = asRecord(value);
  return {
    ...optionalEntry("enforcement", readString(record.enforcement)),
    id: readString(record.id) ?? "",
    label: `${readString(record.scopeType) ?? "scope"}:${readString(record.scopeId) ?? "-"}`,
    ...optionalEntry("limitBytes", readString(record.quotaLimitBytes)),
    scopeId: readString(record.scopeId) ?? "",
    scopeType: readString(record.scopeType) ?? "",
    ...optionalEntry("singleFileLimitBytes", readString(record.singleFileLimitBytes)),
    ...optionalEntry("status", readString(record.status)),
    ...optionalEntry("usedBytes", readString(record.usedBytes)),
  };
}

/**
 * Spread helper for an optional property.
 *
 * The deployments app compiles with `exactOptionalPropertyTypes`, so an absent
 * value must be an absent key rather than a `undefined` one.
 */
function optionalEntry<K extends string, V>(key: K, value: V | undefined): Record<K, V> | Record<string, never> {
  return value === undefined ? {} : ({ [key]: value } as Record<K, V>);
}

function resolveReason(
  failures: readonly ApplicationUsageSourceFailure[],
  source: string,
): ApplicationUsageAvailabilityReason {
  return failures.find((failure) => failure.source === source)?.reason
    ?? failures[0]?.reason
    ?? "unavailable";
}

/**
 * Classify a provider failure into the reason the page renders.
 *
 * The generated SDKs surface `ProblemDetail`-shaped errors, so the numeric
 * status is authoritative; a transport failure with no status is `error`.
 */
export function classifyReason(error: unknown): ApplicationUsageAvailabilityReason {
  const record = asRecord(error);
  const status = Number(record.status ?? record.statusCode ?? record.code);
  if (status === 401 || status === 403) {
    return "forbidden";
  }
  if (status === 404) {
    return "not-found";
  }
  if (status === 501 || status === 502 || status === 503) {
    return "unavailable";
  }
  return "error";
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

function readCount(value: unknown): number | undefined {
  const text = readString(value);
  if (text === undefined || !/^[0-9]+$/u.test(text)) {
    return undefined;
  }
  const parsed = Number(text);
  return Number.isSafeInteger(parsed) ? parsed : undefined;
}

/**
 * Exact decimal → number for arithmetic on the page.
 *
 * A byte total past 2^53 cannot be summed exactly in a browser number; the UI
 * formats single figures from the decimal string, and this helper only
 * saturates an aggregate that would already have lost precision.
 */
function toSafeNumber(value: string): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : Number.MAX_SAFE_INTEGER;
}

function formatUtcDay(day: Date): string {
  return day.toISOString().slice(0, 10);
}

function parseUtcDay(value: string): Date | undefined {
  if (!/^\d{4}-\d{2}-\d{2}$/u.test(value)) {
    return undefined;
  }
  const parsed = new Date(`${value}T00:00:00.000Z`);
  return Number.isNaN(parsed.getTime()) ? undefined : parsed;
}

function toUtcDay(value: string | undefined): string | undefined {
  if (!value) {
    return undefined;
  }
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? undefined : parsed.toISOString().slice(0, 10);
}
