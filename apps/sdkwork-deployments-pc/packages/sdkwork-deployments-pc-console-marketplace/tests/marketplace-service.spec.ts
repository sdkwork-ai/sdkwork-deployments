import { describe, expect, it, vi } from "vitest";

import type { SdkworkDeployAppClient } from "@sdkwork/deployments-pc-console-core/sdk";

import { createMarketplaceService, createMyTemplatesService } from "../src/service/marketplace.ts";

interface Recorded {
  readonly browse: ReturnType<typeof vi.fn>;
  readonly categoryList: ReturnType<typeof vi.fn>;
  readonly purchaseCreate: ReturnType<typeof vi.fn>;
  readonly templateCreate: ReturnType<typeof vi.fn>;
  readonly templateRetrieve: ReturnType<typeof vi.fn>;
  readonly templateUpdate: ReturnType<typeof vi.fn>;
  readonly versionCreate: ReturnType<typeof vi.fn>;
}

/**
 * Structural client stub: the services only reach the template families, so
 * the fake carries exactly those and nothing else. `as unknown as` mirrors the
 * repository's existing SDK stubs (see `deploy-app-row-operations.spec.ts`).
 */
function stubClient(recorded: Partial<Recorded> = {}) {
  const fn = <T>(value: T): ReturnType<typeof vi.fn> => vi.fn().mockResolvedValue(value);
  const browse = recorded.browse ?? fn({ items: [], pageInfo: { totalItems: "0", hasMore: false } });
  const categoryList = recorded.categoryList ?? fn({ items: [], pageInfo: { totalItems: "0", hasMore: false } });
  const purchaseCreate = recorded.purchaseCreate ?? fn({ id: "purchase-1" });
  const templateCreate = recorded.templateCreate ?? fn({ id: "template-1" });
  const templateRetrieve = recorded.templateRetrieve ?? fn({ id: "template-1" });
  const templateUpdate = recorded.templateUpdate ?? fn({ id: "template-1" });
  const versionCreate = recorded.versionCreate ?? fn({ id: "version-1" });
  const client = {
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
      templatePurchases: { create: purchaseCreate, list: fn({ items: [], pageInfo: {} }) },
    },
  };
  return {
    client: client as unknown as SdkworkDeployAppClient,
    recorded: { browse, categoryList, purchaseCreate, templateCreate, templateRetrieve, templateUpdate, versionCreate },
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
    const { client, recorded } = stubClient();
    await createMarketplaceService(client).browse({
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
    const { client, recorded } = stubClient();
    await createMarketplaceService(client).browse({ page: 1, pageSize: 20, keyword: "  rag  " });
    expect(bodyOf(recorded.browse, 0).keyword).toBe("rag");
  });

  it("acquires with an empty body and an idempotency key when no version is pinned", async () => {
    const { client, recorded } = stubClient();
    await createMarketplaceService(client).acquire("template-1");
    expect(firstCall(recorded.purchaseCreate)[0]).toBe("template-1");
    expect(bodyOf(recorded.purchaseCreate, 1)).toEqual({});
    expect(paramsOf(recorded.purchaseCreate, 2).idempotencyKey).toMatch(/\S/);
  });

  it("pins the requested version when the caller supplies one", async () => {
    const { client, recorded } = stubClient();
    await createMarketplaceService(client).acquire("template-1", "version-9");
    expect(bodyOf(recorded.purchaseCreate, 1)).toEqual({ versionUuid: "version-9" });
  });
});

describe("my-templates author service", () => {
  it("keeps a FREE draft priceless even when a price is supplied", async () => {
    const { client, recorded } = stubClient();
    await createMyTemplatesService(client).create({
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
    const { client, recorded } = stubClient();
    await createMyTemplatesService(client).update("template-1", {
      displayName: "  Renamed  ",
      currency: "cny",
    });
    expect(firstCall(recorded.templateUpdate)[0]).toBe("template-1");
    expect(bodyOf(recorded.templateUpdate, 1)).toEqual({ displayName: "Renamed", currency: "CNY" });
    expect(paramsOf(recorded.templateUpdate, 2).idempotencyKey).toMatch(/\S/);
  });

  it("reads a listing back before editing it", async () => {
    const { client, recorded } = stubClient();
    await createMyTemplatesService(client).retrieve("template-7");
    expect(recorded.templateRetrieve).toHaveBeenCalledWith("template-7");
  });

  it("maps every optional version field and drops the blank ones", async () => {
    const { client, recorded } = stubClient();
    await createMyTemplatesService(client).createVersion("template-1", {
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
