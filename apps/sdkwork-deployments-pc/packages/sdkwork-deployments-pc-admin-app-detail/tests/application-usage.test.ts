import { describe, expect, it } from "vitest";

import {
  classifyReason,
  loadApplicationUsage,
  resolveUsageWindow,
  type ApplicationUsageTransport,
} from "../src/services/application-usage";

/**
 * Usage read-model regression.
 *
 * The deploy usage ledger is tenant-scoped, so this read model is the only place
 * that recovers per-application attribution and per-plane availability. The cases
 * below pin the three properties a cloud console must not get wrong: attribution
 * comes from the data plane's deduplication key; a window is whole UTC days; and
 * a meter that cannot answer is never rendered as zero.
 */

const APP_UUID = "0f2f6f1e-1e5a-4b1c-9a41-2f4f6f1e1e5a";
const NOW = new Date("2026-11-27T13:00:00.000Z");

/** The identity the data plane builds (`traffic:<window>:<tenant>:<app>:<binding>:<host>:<ip>:<dim>`). */
function trafficKey(app: string, host: string, dimension: string): string {
  return `traffic:window:1:${app}:7:${host}:10.0.0.1:${dimension}`;
}

function ledgerEvent(input: {
  id: string;
  dimension: string;
  quantity: string;
  unit: string;
  periodStart: string;
  deduplicationKey?: string;
}) {
  return {
    createdAt: input.periodStart,
    ...(input.deduplicationKey ? { deduplicationKey: input.deduplicationKey } : {}),
    dimension: input.dimension,
    id: input.id,
    observedAt: input.periodStart,
    periodStart: input.periodStart,
    quantity: input.quantity,
    unit: input.unit,
  };
}

function createTransport(overrides: Partial<ApplicationUsageTransport> = {}): ApplicationUsageTransport {
  return {
    listDomains: async () => ({ items: [{ hostname: "skyline.example.com" }] }),
    listPackages: async () => ({
      items: [
        { id: "pkg-1", packageSizeBytes: "1048576", packageStatus: "READY", semanticVersion: "1.4.0" },
        { id: "pkg-2", packageSizeBytes: "524288", packageStatus: "READY", semanticVersion: "1.3.0" },
      ],
      pageInfo: { hasMore: false, mode: "offset" },
    }),
    listStorageQuotas: async () => ({
      items: [
        {
          id: "q-1",
          quotaLimitBytes: "1073741824",
          scopeId: "tenant-1",
          scopeType: "TENANT",
          status: "ACTIVE",
          usedBytes: "2048",
        },
      ],
    }),
    listStorageUsage: async () => ({
      items: [{ id: "u-1", scope: "tenant-1.default", usedBytes: "4096" }],
    }),
    listUsageEvents: async () => ({
      items: [
        ledgerEvent({
          deduplicationKey: trafficKey(APP_UUID, "skyline.example.com", "traffic.requests"),
          dimension: "traffic.requests",
          id: "ev-1",
          periodStart: "2026-11-26T00:00:00Z",
          quantity: "1200",
          unit: "REQUEST",
        }),
        ledgerEvent({
          deduplicationKey: trafficKey(APP_UUID, "skyline.example.com", "traffic.egress_bytes"),
          dimension: "traffic.egress_bytes",
          id: "ev-2",
          periodStart: "2026-11-26T00:00:00Z",
          quantity: "2097152",
          unit: "BYTE",
        }),
        // Another application in the same tenant: it must not leak in.
        ledgerEvent({
          deduplicationKey: trafficKey("other-app", "other.example.com", "traffic.egress_bytes"),
          dimension: "traffic.egress_bytes",
          id: "ev-3",
          periodStart: "2026-11-26T00:00:00Z",
          quantity: "999999999",
          unit: "BYTE",
        }),
        ledgerEvent({
          dimension: "build_minutes",
          id: "ev-4",
          periodStart: "2026-11-26T00:00:00Z",
          quantity: "14",
          unit: "MINUTE",
        }),
        ledgerEvent({
          dimension: "deployment_count",
          id: "ev-5",
          periodStart: "2026-11-26T00:00:00Z",
          quantity: "2",
          unit: "COUNT",
        }),
      ],
      pageInfo: { hasMore: false, mode: "offset", totalItems: "5" },
    }),
    retrieveRoutingUsage: async () => ({
      chartData: [{ latency: "12", requests: "40", time: "2026-11-26T00:00:00Z" }],
      modelStats: [{ lat: "10", m: "gpt", req: "40", sr: "1", tok: "8000" }],
    }),
    ...overrides,
  };
}

describe("resolveUsageWindow", () => {
  it("covers whole UTC days with an exclusive upper bound", () => {
    expect(resolveUsageWindow("7d", NOW)).toEqual({ end: "2026-11-28", start: "2026-11-21" });
    expect(resolveUsageWindow("24h", NOW)).toEqual({ end: "2026-11-28", start: "2026-11-27" });
    expect(resolveUsageWindow("30d", NOW)).toEqual({ end: "2026-11-28", start: "2026-10-29" });
  });
});

describe("loadApplicationUsage", () => {
  it("attributes traffic through the deduplication key and ignores other applications", async () => {
    const { snapshot } = await loadApplicationUsage({
      appId: APP_UUID,
      now: NOW,
      transport: createTransport(),
      window: "7d",
    });

    expect(snapshot.traffic.attributed).toBe(true);
    expect(snapshot.traffic.measured).toBe(true);
    expect(snapshot.traffic.totals.requests?.quantity).toBe("1200");
    expect(snapshot.traffic.totals.egressBytes?.quantity).toBe("2097152");
    expect(snapshot.traffic.completeness).toBe("complete");
    expect(snapshot.traffic.byHostname).toEqual([
      { egressBytes: "2097152", hostname: "skyline.example.com", requests: "1200" },
    ]);
    expect(snapshot.traffic.trend).toHaveLength(7);
    const busiest = snapshot.traffic.trend.find((point) => point.date === "2026-11-26");
    expect(busiest?.requests).toBe("1200");
    expect(snapshot.windowStart).toBe("2026-11-21");
    expect(snapshot.windowEnd).toBe("2026-11-28");
  });

  it("records control-plane and gateway usage on the service plane", async () => {
    const { snapshot } = await loadApplicationUsage({
      appId: APP_UUID,
      now: NOW,
      transport: createTransport(),
      window: "7d",
    });

    expect(snapshot.service.availability).toBe("available");
    expect(snapshot.service.usage.buildMinutes?.quantity).toBe("14");
    expect(snapshot.service.usage.deploymentCount?.quantity).toBe("2");
    // The routing snapshot is tenant wide; the page must know that.
    expect(snapshot.service.usage.apiAttribution).toBe("tenant");
    expect(snapshot.service.usage.apiRequests?.quantity).toBe("40");
    expect(snapshot.service.usage.apiTokens?.quantity).toBe("8000");
  });

  it("sums package bytes and keeps object-store scopes separate", async () => {
    const { snapshot } = await loadApplicationUsage({
      appId: APP_UUID,
      now: NOW,
      transport: createTransport(),
      window: "7d",
    });

    expect(snapshot.storage.availability).toBe("available");
    expect(snapshot.storage.packages).toHaveLength(2);
    const packages = snapshot.storage.items.find((item) => item.sourceLabel === "deploy.packages");
    expect(packages?.usedBytes).toBe(String(1048576 + 524288));
    expect(snapshot.storage.items.some((item) => item.sourceLabel === "storage.usage")).toBe(true);
    expect(snapshot.storage.quotas[0]?.limitBytes).toBe("1073741824");
    expect(snapshot.storage.completeness).toBe("complete");
    expect(snapshot.sources).toContain("deploy.usage_events");
    expect(snapshot.sources).toContain("cloudrouter.ai_routing_usage");
  });

  it("falls back to the tenant figures and says so when nothing is attributed", async () => {
    const transport = createTransport({
      listUsageEvents: async () => ({
        items: [
          ledgerEvent({
            dimension: "traffic.egress_bytes",
            id: "ev-1",
            periodStart: "2026-11-26T00:00:00Z",
            quantity: "512",
            unit: "BYTE",
          }),
        ],
        pageInfo: { hasMore: false, mode: "offset" },
      }),
    });
    const { snapshot } = await loadApplicationUsage({
      appId: APP_UUID,
      now: NOW,
      transport,
      window: "7d",
    });

    expect(snapshot.traffic.attributed).toBe(false);
    expect(snapshot.traffic.totals.egressBytes?.quantity).toBe("512");
  });

  it("reports a sampled window as partial instead of claiming a total", async () => {
    const page = await createTransport().listUsageEvents({ page: 1, pageSize: 200 });
    const rows = (page as { items: unknown[] }).items;
    const fullPage = Array.from({ length: 200 }, (_, index) => ({
      ...(rows[0] as Record<string, unknown>),
      id: `ev-${index}`,
    }));
    const transport = createTransport({
      listUsageEvents: async () => ({ items: fullPage, pageInfo: { hasMore: true, mode: "offset" } }),
    });
    const { snapshot } = await loadApplicationUsage({
      appId: APP_UUID,
      now: NOW,
      transport,
      window: "7d",
    });

    expect(snapshot.traffic.completeness).toBe("partial");
  });

  it("does not report zero when the usage surface is forbidden", async () => {
    const { failures, snapshot } = await loadApplicationUsage({
      appId: APP_UUID,
      now: NOW,
      transport: createTransport({
        listUsageEvents: async () => {
          throw Object.assign(new Error("forbidden"), { status: 403 });
        },
      }),
      window: "7d",
    });

    expect(failures.some((failure) => failure.source === "deploy.usage_events")).toBe(true);
    expect(snapshot.traffic.measured).toBe(false);
    expect(snapshot.traffic.dimensions).toHaveLength(0);
    expect(snapshot.traffic.totals).toEqual({});
  });

  it("degrades only the storage plane when the object store is not wired", async () => {
    const { failures, snapshot } = await loadApplicationUsage({
      appId: APP_UUID,
      now: NOW,
      transport: createTransport({
        listPackages: async () => {
          throw Object.assign(new Error("not registered"), { status: 404 });
        },
        listStorageQuotas: undefined,
        listStorageUsage: undefined,
      }),
      window: "7d",
    });

    expect(snapshot.storage.availability).toBe("unavailable");
    expect(failures.some((failure) => failure.source === "storage.usage")).toBe(true);
    // The traffic plane is unaffected by a storage failure.
    expect(snapshot.traffic.totals.egressBytes?.quantity).toBe("2097152");
  });

  it("reports the service plane unavailable when the gateway snapshot is not wired", async () => {
    const { snapshot } = await loadApplicationUsage({
      appId: APP_UUID,
      now: NOW,
      transport: createTransport({ retrieveRoutingUsage: undefined }),
      window: "7d",
    });

    expect(snapshot.service.availability).toBe("unavailable");
    expect(snapshot.service.availabilityReason).toBe("unavailable");
    // Control-plane facts still answer.
    expect(snapshot.service.usage.buildMinutes?.quantity).toBe("14");
  });
});

describe("classifyReason", () => {
  it("maps the provider status onto the reason the page renders", () => {
    expect(classifyReason({ status: 403 })).toBe("forbidden");
    expect(classifyReason({ status: 404 })).toBe("not-found");
    expect(classifyReason({ status: 503 })).toBe("unavailable");
    expect(classifyReason(new Error("network"))).toBe("error");
  });
});
