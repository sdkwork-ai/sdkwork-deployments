import { describe, expect, it, vi } from "vitest";

import type { SdkworkDeployAppClient, SdkworkOrderAppClient } from "@sdkwork/deployments-pc-console-core/sdk";

import { createMarketplaceService, createMyTemplatesService } from "../src/service/marketplace.ts";

interface Recorded {
  readonly browse: ReturnType<typeof vi.fn>;
  readonly categoryList: ReturnType<typeof vi.fn>;
  readonly orderCreate: ReturnType<typeof vi.fn>;
  readonly orderList: ReturnType<typeof vi.fn>;
  readonly templateCreate: ReturnType<typeof vi.fn>;
  readonly templateRetrieve: ReturnType<typeof vi.fn>;
  readonly templateUpdate: ReturnType<typeof vi.fn>;
  readonly versionCreate: ReturnType<typeof vi.fn>;
}

/**
 * Structural client stubs: the storefront service reaches the deploy client for
 * the catalog and the order app client for app-template trade, so the fakes
 * carry exactly those families and nothing else. `as unknown as` mirrors the
 * repository's existing SDK stubs (see `deploy-app-row-operations.spec.ts`).
 */
function stubClients(recorded: Partial<Recorded> = {}) {
  const fn = <T>(value: T): ReturnType<typeof vi.fn> => vi.fn().mockResolvedValue(value);
  const browse = recorded.browse ?? fn({ items: [], pageInfo: { totalItems: "0", hasMore: false } });
  const categoryList = recorded.categoryList ?? fn({ items: [], pageInfo: { totalItems: "0", hasMore: false } });
  const orderCreate = recorded.orderCreate ?? fn({ orderId: "order-1", status: "pending_payment" });
  const orderList = recorded.orderList ?? fn({ items: [], pageInfo: { totalItems: "0", hasMore: false } });
  const templateCreate = recorded.templateCreate ?? fn({ id: "template-1" });
  const templateRetrieve = recorded.templateRetrieve ?? fn({ id: "template-1" });
  const templateUpdate = recorded.templateUpdate ?? fn({ id: "template-1" });
  const versionCreate = recorded.versionCreate ?? fn({ id: "version-1" });
  const deploy = {
    app: { list: fn({ items: [], pageInfo: { totalItems: "0", hasMore: false } }) },
    template: {
      appTemplateVersions: { create: versionCreate, list: fn({ items: [], pageInfo: {} }) },
      appTemplates: {
        create: templateCreate,
        delete: fn(undefined),
        list: fn({ items: [], pageInfo: { totalItems: "0", hasMore: false } }),
        retrieve: templateRetrieve,
        submit: fn({ id: "template-1" }),
        update: templateUpdate,
      },
      marketplaceTemplates: { list: browse, retrieve: fn({ id: "template-1" }) },
      templateCategories: { list: categoryList },
    },
  };
  const order = { orderAppTemplates: { appTemplateOrders: { create: orderCreate, list: orderList } } };
  return {
    deployClient: deploy as unknown as SdkworkDeployAppClient,
    orderClient: order as unknown as SdkworkOrderAppClient,
    recorded: { browse, categoryList, orderCreate, orderList, templateCreate, templateRetrieve, templateUpdate, versionCreate },
  };
}

/** The arguments of a stub's first call, or a failure that names the stub. */
function firstCall(fn: ReturnType<typeof vi.fn>): readonly unknown[] {
  const call = fn.mock.calls[0];
  if (call === undefined) throw new Error("expected the stub to have been called");
  return call as readonly unknown[];
}

function bodyOf(fn: ReturnType<typeof vi.fn>, index: number): Record<string, unknown> {
  return firstCall(fn)[index] as Record<string, unknown>;
}

function paramsOf(fn: ReturnType<typeof vi.fn>, index: number): { idempotencyKey: string } {
  return firstCall(fn)[index] as { idempotencyKey: string };
}

describe("marketplace storefront service", () => {
  it("drops blank filters from the browse query", async () => {
    const { deployClient, orderClient, recorded } = stubClients();
    await createMarketplaceService(deployClient, orderClient).browse({
      page: 2,
      pageSize: 20,
      categoryUuid: undefined,
      keyword: "   ",
      pricingModel: "FREE",
      templateType: undefined,
      sort: "POPULAR",
    });
    expect(recorded.browse).toHaveBeenCalledWith({
      page: 2,
      pageSize: 20,
      pricingModel: "FREE",
      sort: "POPULAR",
    });
  });

  it("trims a keyword that carries surrounding whitespace", async () => {
    const { deployClient, orderClient, recorded } = stubClients();
    await createMarketplaceService(deployClient, orderClient).browse({ page: 1, pageSize: 20, keyword: "  rag  " });
    expect(bodyOf(recorded.browse, 0).keyword).toBe("rag");
  });

  it("orders the template through the order center with a fresh idempotency key", async () => {
    const { deployClient, orderClient, recorded } = stubClients();
    await createMarketplaceService(deployClient, orderClient).acquire("template-1");
    expect(recorded.orderCreate).toHaveBeenCalledTimes(1);
    expect(bodyOf(recorded.orderCreate, 0)).toEqual({ templateUuid: "template-1" });
    expect(paramsOf(recorded.orderCreate, 1).idempotencyKey).toMatch(/\S/);
  });

  it("maps the order page onto the shared page shape", async () => {
    const orderList = vi.fn().mockResolvedValue({
      items: [{ orderId: "order-1", orderNo: "T-1", templateUuid: "template-1", templateName: "Starter", amount: "0", currencyCode: "CNY", status: "paid", fulfillmentStatus: "FULFILLED", createdAt: "2026-10-03T00:00:00Z" }],
      pageInfo: { totalItems: "3", hasMore: true },
    });
    const { deployClient, orderClient } = stubClients({ orderList });
    const page = await createMarketplaceService(deployClient, orderClient).myPurchases(2, 20);
    expect(orderList).toHaveBeenCalledWith({ page: 2, pageSize: 20 });
    expect(page.total).toBe(3);
    expect(page.hasMore).toBe(true);
    expect(page.items[0]?.status).toBe("paid");
  });
});

describe("my-templates author service", () => {
  it("keeps a FREE draft priceless even when a price is supplied", async () => {
    const { deployClient, recorded } = stubClients();
    await createMyTemplatesService(deployClient).create({
      appUuid: "app-1",
      templateType: "APP",
      categoryUuid: "category-1",
      templateKey: "saas-starter",
      displayName: "SaaS Starter",
      summary: "Starter listing",
      visibility: "PUBLIC",
      pricingModel: "FREE",
      priceMinor: "500",
    });
    const body = bodyOf(recorded.templateCreate, 0);
    expect(body.pricingModel).toBe("FREE");
    expect(body).not.toHaveProperty("priceMinor");
    expect(body).not.toHaveProperty("currency");
  });

  it("sends only the patched fields on update and normalizes their casing", async () => {
    const { deployClient, recorded } = stubClients();
    await createMyTemplatesService(deployClient).update("template-1", {
      displayName: "  Renamed  ",
      currency: "cny",
    });
    expect(firstCall(recorded.templateUpdate)[0]).toBe("template-1");
    expect(bodyOf(recorded.templateUpdate, 1)).toEqual({ displayName: "Renamed", currency: "CNY" });
    expect(paramsOf(recorded.templateUpdate, 2).idempotencyKey).toMatch(/\S/);
  });

  it("reads a listing back before editing it", async () => {
    const { deployClient, recorded } = stubClients();
    await createMyTemplatesService(deployClient).retrieve("template-7");
    expect(recorded.templateRetrieve).toHaveBeenCalledWith("template-7");
  });

  it("maps every optional version field and drops the blank ones", async () => {
    const { deployClient, recorded } = stubClients();
    await createMyTemplatesService(deployClient).createVersion("template-1", {
      version: " 1.2.0 ",
      changelog: "",
      artifactUuid: " artifact-1 ",
      sourceAppVersion: "   ",
      platformTargets: ["pc", "h5"],
      packageSizeBytes: "4096",
      checksumSha256: undefined,
    });
    expect(bodyOf(recorded.versionCreate, 1)).toEqual({
      version: "1.2.0",
      artifactUuid: "artifact-1",
      platformTargets: ["pc", "h5"],
      packageSizeBytes: "4096",
    });
  });
});
