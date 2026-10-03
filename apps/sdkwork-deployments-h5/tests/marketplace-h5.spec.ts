import { describe, expect, it } from "vitest";

import { translateH5, type DeploymentsH5MessageKey } from "../src/marketplace/i18n.ts";
import { createH5MarketplaceService } from "../src/marketplace/service.ts";
import type { SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";

describe("h5 marketplace i18n", () => {
  it("renders zh-CN and en-US for every key without leaking placeholders", () => {
    const samples: DeploymentsH5MessageKey[] = [
      "app.title",
      "tab.marketplace",
      "tab.myTemplates",
      "marketplace.acquire",
      "marketplace.acquireCommerce",
      "marketplace.versions",
      "marketplace.versions.empty",
      "myTemplates.submit",
      "common.retry",
    ];
    for (const key of samples) {
      const zh = translateH5("zh-CN", key);
      const en = translateH5("en-US", key);
      expect(zh.length).toBeGreaterThan(0);
      expect(en.length).toBeGreaterThan(0);
      expect(zh).not.toMatch(/\{[a-zA-Z]+\}/);
      expect(en).not.toMatch(/\{[a-zA-Z]+\}/);
    }
  });

  it("interpolates install counts", () => {
    expect(translateH5("zh-CN", "marketplace.installs", { count: 7 })).toContain("7");
  });
});

describe("h5 marketplace service", () => {
  function clientWithSpy() {
    // Mirrors the generated client method shapes so the recorded first
    // arguments can be asserted on (same rationale as the PC publishing test).
    const marketplaceList = async (_params?: unknown, _options?: unknown) => ({
      items: [
        {
          id: "tmpl-1",
          templateKey: "blog",
          displayName: "Blog starter",
          summary: "A blog",
          categoryUuid: "cat-1",
          visibility: "PUBLIC",
          pricingModel: "FREE",
          priceMinor: "0",
          currency: "CNY",
          status: "PUBLISHED",
          isFeatured: false,
          installCount: "3",
          viewCount: "9",
          updatedAt: "2026-10-03T00:00:00Z",
          version: "1",
        },
      ],
      pageInfo: { mode: "offset", page: 1, pageSize: 10, totalItems: "1", totalPages: 1, hasMore: false, nextCursor: null },
    });
    let browseArgs: unknown;
    const deploy = {
      template: {
        templateCategories: { list: async () => ({ items: [], pageInfo: { totalItems: "0", hasMore: false } }) },
        marketplaceTemplates: {
          list: async (params?: unknown, options?: unknown) => {
            browseArgs = params;
            return marketplaceList(params, options);
          },
        },
      },
    } as unknown as SdkworkDeployAppClient;
    return { service: createH5MarketplaceService(deploy), browseArgs: () => browseArgs };
  }

  it("projects blank optional facets as absent query parameters", async () => {
    const { service, browseArgs } = clientWithSpy();
    const page = await service.browse({ page: 1, pageSize: 10, keyword: "  ", categoryUuid: "" });
    expect(page.items).toHaveLength(1);
    expect(page.total).toBe(1);
    expect(browseArgs()).toEqual({ page: 1, pageSize: 10 });
  });

  it("keeps provided facets on the wire with trimmed keywords", async () => {
    const { service, browseArgs } = clientWithSpy();
    await service.browse({ page: 2, pageSize: 10, keyword: " blog ", categoryUuid: "cat-1", sort: "POPULAR" });
    expect(browseArgs()).toEqual({ page: 2, pageSize: 10, keyword: "blog", categoryUuid: "cat-1", sort: "POPULAR" });
  });

  it("carries the pricing and type facets the storefront offers", async () => {
    const { service, browseArgs } = clientWithSpy();
    await service.browse({ page: 1, pageSize: 10, pricingModel: "PAID", templateType: "PPT" });
    expect(browseArgs()).toEqual({ page: 1, pageSize: 10, pricingModel: "PAID", templateType: "PPT" });
  });
});

describe("h5 marketplace version and acquisition reads", () => {
  function clientWithSpy() {
    let acquireArgs: unknown[] | undefined;
    let versionArgs: unknown[] | undefined;
    const deploy = {
      template: {
        appTemplateVersions: {
          list: async (...args: unknown[]) => {
            versionArgs = args;
            return {
              items: [
                {
                  id: "v-2",
                  templateUuid: "tmpl-1",
                  templateVersion: "1.2.0",
                  changelog: "",
                  platformTargets: [],
                  packageSizeBytes: "0",
                  status: "PUBLISHED",
                  publishedAt: "2026-10-01T00:00:00Z",
                  createdAt: "2026-10-01T00:00:00Z",
                  updatedAt: "2026-10-01T00:00:00Z",
                  version: "1",
                },
              ],
              pageInfo: { totalItems: "1", hasMore: false },
            };
          },
        },
        templatePurchases: {
          create: async (...args: unknown[]) => {
            acquireArgs = args;
            return { id: "purchase-1", status: "ACTIVE" };
          },
        },
      },
    } as unknown as SdkworkDeployAppClient;
    return {
      service: createH5MarketplaceService(deploy),
      acquireArgs: () => acquireArgs,
      versionArgs: () => versionArgs,
    };
  }

  it("acquires with an empty body and a fresh idempotency key", async () => {
    const { service, acquireArgs } = clientWithSpy();
    await service.acquire("tmpl-1");
    const args = acquireArgs();
    expect(args?.[0]).toBe("tmpl-1");
    expect(args?.[1]).toEqual({});
    const params = args?.[2] as { idempotencyKey?: string } | undefined;
    expect(params?.idempotencyKey).toMatch(/\S/);
  });

  it("scopes the version read to the listing and returns its rows", async () => {
    const { service, versionArgs } = clientWithSpy();
    const versions = await service.listVersions("tmpl-1");
    expect(versionArgs()?.[0]).toBe("tmpl-1");
    expect(versionArgs()?.[1]).toEqual({ page: 1, pageSize: 50 });
    expect(versions).toHaveLength(1);
    expect(versions[0].templateVersion).toBe("1.2.0");
  });
});
