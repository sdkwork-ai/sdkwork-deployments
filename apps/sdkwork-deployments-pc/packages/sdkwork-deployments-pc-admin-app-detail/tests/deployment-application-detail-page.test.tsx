/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SdkworkI18nProvider } from "@sdkwork/i18n-pc-react";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  DeploymentApplicationDetailPage,
  type DeploymentApplicationDetailCapabilityPort,
  type DeploymentApplicationDetailDataSource,
  type DeployApplicationDetailRecord,
  type DeploymentApplicationUsageRead,
} from "../src";

/**
 * Deployment-application detail surface regression.
 *
 * The surface is the read view of one application: identity, three metered
 * planes, publish readiness, hostnames, and delivery history. The cases below pin
 * what an operator must be able to trust — the decimal application id is printed
 * verbatim, an unreachable meter says so instead of printing zero, the usage
 * window is operator-selectable, and every console-specific action goes through
 * the injected capability port rather than a hard-wired route.
 */

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const APP_UUID = "6c4c0b8e-8d5e-4d5c-9b39-1c1c3c9f8a11";

function createRecord(): DeployApplicationDetailRecord {
  return {
    delivery: {
      deployments: {
        availability: "available",
        items: [
          {
            createdAt: "2026-11-24T09:57:00Z",
            deploymentStatus: "LIVE",
            deploymentTarget: "WEB_NODE",
            environment: "production",
            id: "dep-1",
            releaseId: "rel-1",
            startedAt: "2026-11-24T09:55:00Z",
          },
        ],
      },
      domains: {
        availability: "available",
        items: [
          {
            bindingStatus: "ACTIVE",
            environment: "production",
            hostname: "skyline.example.com",
            isCanonical: true,
            kind: "DEFAULT",
            verificationStatus: "NOT_REQUIRED",
          },
        ],
      },
      readiness: {
        availability: "available",
        blockingStep: "IDENTITY",
        items: [
          { status: "READY", step: "REGISTRATION" },
          { status: "READY", step: "PLATFORM_TARGET" },
          { reason: "identity not provisioned", status: "BLOCKED", step: "IDENTITY" },
        ],
        status: "BLOCKED",
      },
      releases: {
        availability: "available",
        items: [
          {
            buildNumber: "12",
            createdAt: "2026-11-24T09:50:00Z",
            id: "rel-1",
            releaseStatus: "ACTIVE",
            semanticVersion: "1.4.0",
          },
        ],
      },
    },
    detail: {
      appDomainLabel: "skyline",
      appDomainSuffixes: [],
      appKind: "PC_WEB",
      appStatus: "ACTIVE",
      createdAt: "2026-11-20T02:00:00Z",
      defaultEnvironment: "production",
      // A snowflake past 2^53: it must be printed exactly as the wire carried it.
      deployApplicationId: "410000000000000001",
      id: APP_UUID,
      metadata: {},
      name: "Skyline Analytics",
      nginxConfigOverridden: false,
      organizationId: "42",
      ownershipScope: "ORGANIZATION",
      platformTargetCount: "1",
      slug: "skyline-analytics",
      tenantId: "7",
      updatedAt: "2026-11-24T09:57:00Z",
    },
  };
}

function createUsage(): DeploymentApplicationUsageRead {
  return {
    failures: [],
    snapshot: {
      ledger: [
        {
          deduplicationKey: `traffic:window:7:${APP_UUID}:1:skyline.example.com:10.0.0.1:traffic.requests`,
          dimension: "traffic.requests",
          id: "ev-1",
          periodStart: new Date().toISOString(),
          quantity: "3600",
          unit: "REQUEST",
        },
      ],
      service: {
        apiSource: "cloudrouter.ai_routing_usage",
        availability: "available",
        usage: {
          // The gateway meters this tenant wide; the page must label it.
          apiAttribution: "tenant",
          apiRequests: {
            dimension: "api.requests",
            quantity: "25",
            sourceLabel: "cloudrouter.ai_routing_usage",
            unit: "REQUEST",
          },
          apiTokens: {
            dimension: "api.requests",
            quantity: "5000",
            sourceLabel: "cloudrouter.ai_routing_usage",
            unit: "TOKEN",
          },
          buildMinutes: {
            dimension: "build_minutes",
            quantity: "14",
            sourceLabel: "deploy.usage_events",
            unit: "MINUTE",
          },
        },
      },
      sources: ["deploy.usage_events", "cloudrouter.ai_routing_usage"],
      storage: {
        availability: "available",
        completeness: "complete",
        items: [
          {
            dimension: "package_storage_bytes",
            label: "package",
            sourceLabel: "deploy.packages",
            usedBytes: "1048576",
          },
        ],
        packages: [{ id: "pkg-1", semanticVersion: "1.4.0", sizeBytes: "1048576", status: "READY" }],
        quotas: [],
      },
      traffic: {
        attributed: true,
        availability: "available",
        byHostname: [{ egressBytes: "3145728", hostname: "skyline.example.com", requests: "3600" }],
        completeness: "complete",
        dimensions: ["traffic.egress_bytes", "traffic.requests"],
        measured: true,
        totals: {
          egressBytes: {
            dimension: "traffic.egress_bytes",
            quantity: "3145728",
            sourceLabel: "deploy.usage_events",
            unit: "BYTE",
          },
          requests: {
            dimension: "traffic.requests",
            quantity: "3600",
            sourceLabel: "deploy.usage_events",
            unit: "REQUEST",
          },
        },
        trend: [{ date: "2026-11-26", egressBytes: "3145728", ingressBytes: "0", requests: "3600" }],
      },
      windowEnd: "2026-11-28",
      windowStart: "2026-11-22",
    },
  };
}

function createDataSource(
  overrides: Partial<DeploymentApplicationDetailDataSource> = {},
): DeploymentApplicationDetailDataSource {
  return {
    loadDetail: async () => createRecord(),
    loadUsage: async () => createUsage(),
    ...overrides,
  };
}

function renderDetail(
  props: {
    capabilities?: DeploymentApplicationDetailCapabilityPort;
    dataSource?: DeploymentApplicationDetailDataSource;
    onBack?: () => void;
    permissions?: { sourceRead?: () => boolean; sourceWrite?: () => boolean };
  } = {},
) {
  return render(
    <SdkworkI18nProvider locale="zh-CN">
      <DeploymentApplicationDetailPage
        appId={APP_UUID}
        backTo="/applications"
        {...(props.capabilities ? { capabilities: props.capabilities } : {})}
        dataSource={props.dataSource ?? createDataSource()}
        {...(props.onBack ? { onBack: props.onBack } : {})}
        {...(props.permissions ? { permissions: props.permissions } : {})}
      />
    </SdkworkI18nProvider>,
  );
}

describe("deployment application detail surface", () => {
  it("prints the fact sheet including the exact decimal application id", async () => {
    renderDetail();

    expect((await screen.findAllByText("Skyline Analytics")).length).toBeGreaterThan(0);
    expect(screen.getByText("410000000000000001")).toBeTruthy();
    expect(screen.getByText("skyline-analytics")).toBeTruthy();
    // The endpoint kind resolves through the localized vocabulary, not the raw
    // wire enum.
    expect(screen.getAllByText("PC 站点").length).toBeGreaterThan(0);
    expect(screen.getAllByText("运行中").length).toBeGreaterThan(0);
  });

  it("shows traffic, tenant-scoped API usage, and package storage on their own planes", async () => {
    renderDetail();

    await waitFor(() => {
      expect(screen.getAllByText("3 MiB").length).toBeGreaterThan(0);
    });
    expect(screen.getAllByText(/3,600/).length).toBeGreaterThan(0);
    // The gateway snapshot is tenant wide and the surface must say so.
    expect(screen.getAllByText("租户维度").length).toBeGreaterThan(0);
    expect(screen.getAllByText("1 MiB").length).toBeGreaterThan(0);
  });

  it("renders an unreachable meter as not connected instead of zero", async () => {
    renderDetail({
      dataSource: createDataSource({
        loadUsage: async () => ({
          ...createUsage(),
          failures: [{ reason: "forbidden", source: "cloudrouter.ai_routing_usage" }],
          snapshot: {
            ...createUsage().snapshot,
            service: { availability: "unavailable", availabilityReason: "forbidden", usage: {} },
          },
        }),
      }),
    });

    await waitFor(() => {
      expect(screen.getAllByText("无权限读取").length).toBeGreaterThan(0);
    });
  });

  it("reports the readiness gate's blocking step and the delivery history", async () => {
    renderDetail();

    await waitFor(() => {
      expect(screen.getByText(/identity not provisioned/)).toBeTruthy();
    });
    expect(screen.getAllByText("被阻塞").length).toBeGreaterThan(0);
    expect(screen.getAllByText("skyline.example.com").length).toBeGreaterThan(0);
    expect(screen.getAllByText("1.4.0").length).toBeGreaterThan(0);
  });

  it("opens host commands through the capability port, not a hard-wired route", async () => {
    const openAccessToken = vi.fn();
    const openSource = vi.fn();
    renderDetail({
      capabilities: { openAccessToken, openSource },
      permissions: { sourceRead: () => true },
    });

    await screen.findAllByText("Skyline Analytics");
    fireEvent.click(screen.getByRole("button", { name: "访问令牌" }));
    expect(openAccessToken).toHaveBeenCalledWith(APP_UUID);

    fireEvent.click(screen.getByRole("button", { name: "源码与衍生" }));
    expect(openSource).toHaveBeenCalledWith(APP_UUID);
  });

  it("hides a command whose capability or permission the host did not supply", async () => {
    renderDetail({ permissions: { sourceRead: () => false } });

    await screen.findAllByText("Skyline Analytics");
    expect(screen.queryByRole("button", { name: "访问令牌" })).toBeNull();
    expect(screen.queryByRole("button", { name: "源码与衍生" })).toBeNull();
  });

  it("lets the operator change the usage window", async () => {
    const loadUsage = vi.fn(async () => createUsage());
    renderDetail({ dataSource: createDataSource({ loadUsage }) });

    await screen.findAllByText("Skyline Analytics");
    await waitFor(() => expect(loadUsage).toHaveBeenCalled());
    const firstCall = loadUsage.mock.calls.at(0) as [{ window?: string }] | undefined;
    expect(firstCall?.[0]).toMatchObject({ appId: APP_UUID, window: "7d" });

    // The window strip is marked per trigger; the trend chart has its own metric
    // strip, so a role query would be ambiguous. Radix enables a trigger on
    // `mouseDown` and switches on `click`, so both are dispatched.
    const windowTab = document.querySelector('[data-usage-window="30d"]') as HTMLElement | null;
    expect(windowTab).not.toBeNull();
    fireEvent.mouseDown(windowTab as HTMLElement, { button: 0, ctrlKey: false });
    fireEvent.click(windowTab as HTMLElement);
    await waitFor(
      () => {
        expect(loadUsage).toHaveBeenCalledWith(expect.objectContaining({ window: "30d" }));
      },
      { timeout: 3000 },
    );
  }, 15000);

  it("explains a missing application instead of rendering an empty sheet", async () => {
    renderDetail({
      dataSource: createDataSource({
        loadDetail: async () => {
          throw Object.assign(new Error("not found"), { status: 404 });
        },
      }),
    });

    await waitFor(() => {
      expect(screen.getAllByText(/未找到该应用/).length).toBeGreaterThan(0);
    });
  });

  it("degrades the usage plane alone when the host has no usage read", async () => {
    renderDetail({ dataSource: { loadDetail: async () => createRecord() } });

    await screen.findAllByText("Skyline Analytics");
    // The fact sheet still renders while the consumption plane reports why.
    expect(screen.getAllByText("410000000000000001").length).toBeGreaterThan(0);
    await waitFor(() => {
      expect(screen.getAllByText("该计量未接入").length).toBeGreaterThan(0);
    });
  });
});
