import type { DeploymentsH5Runtime } from "../bootstrap/runtime.ts";
import type {
  AppTemplateOrderCreateResult,
  AppTemplateOrderSummary,
} from "@sdkwork/order-app-sdk";
import type {
  AppTemplateResponse,
  AppTemplateSummaryResponse,
  AppTemplateVersionResponse,
  TemplateCategoryResponse,
} from "@sdkwork/deployments-app-sdk";
import { uuid } from "@sdkwork/utils/id";

/**
 * H5 marketplace service over the injected app-SDK clients. The deploy client
 * reads the module-owned catalog; the order client drives app-template trade,
 * which the platform order center owns. Construction stays in bootstrap; this
 * module is pure transport shaping (idempotency keys, optional-parameter
 * projection) so views stay declarative.
 */
export type TemplateType = "APP" | "PPT" | "VIDEO";

export interface MarketplaceBrowseParams {
  readonly page: number;
  readonly pageSize: number;
  readonly categoryUuid?: string | undefined;
  readonly keyword?: string | undefined;
  readonly pricingModel?: "FREE" | "PAID" | undefined;
  readonly templateType?: TemplateType | undefined;
  readonly sort?: "NEWEST" | "POPULAR" | undefined;
}

export interface MarketplacePage<TItem> {
  readonly items: readonly TItem[];
  readonly total: number;
  readonly hasMore: boolean;
}

function omitBlank<T extends object>(value: T): T {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => !(typeof entry === "string" && entry.trim() === "")),
  ) as T;
}

function pageOf<T>(result: { items: T[]; pageInfo: { totalItems?: string; hasMore?: boolean } }): MarketplacePage<T> {
  return {
    items: result.items,
    total: Number(result.pageInfo.totalItems ?? "0"),
    hasMore: result.pageInfo.hasMore ?? false,
  };
}

export function createH5MarketplaceService(deployClient: DeploymentsH5Runtime["deploy"], orderClient: DeploymentsH5Runtime["order"]) {
  return {
    async listCategories(): Promise<readonly TemplateCategoryResponse[]> {
      const page = await deployClient.template.templateCategories.list({ includeDisabled: false });
      return page.items;
    },
    async browse(params: MarketplaceBrowseParams): Promise<MarketplacePage<AppTemplateSummaryResponse>> {
      const page = await deployClient.template.marketplaceTemplates.list(
        omitBlank({
          page: params.page,
          pageSize: params.pageSize,
          ...(params.categoryUuid === undefined ? {} : { categoryUuid: params.categoryUuid }),
          ...(params.keyword === undefined || params.keyword.trim() === "" ? {} : { keyword: params.keyword.trim() }),
          ...(params.pricingModel === undefined ? {} : { pricingModel: params.pricingModel }),
          ...(params.templateType === undefined ? {} : { templateType: params.templateType }),
          ...(params.sort === undefined ? {} : { sort: params.sort }),
        }),
      );
      return pageOf(page);
    },
    async retrieve(templateUuid: string): Promise<AppTemplateResponse> {
      return deployClient.template.marketplaceTemplates.retrieve(templateUuid);
    },
    /**
     * Starts (or reuses) the order-center purchase of one listing.
     *
     * The order command carries no version — the server snapshots whichever
     * version the listing publishes — so the service does not offer one either.
     * A FREE listing comes back `status: "paid"` with nothing left to do; a PAID
     * listing comes back `pending_payment` with the cashier URL (or provider
     * payload) the view must surface.
     */
    async acquire(templateUuid: string): Promise<AppTemplateOrderCreateResult> {
      return orderClient.orderAppTemplates.appTemplateOrders.create(
        { templateUuid },
        { idempotencyKey: uuid() },
      );
    },
    /** The caller's app-template orders; the order center's `paid` is the install entitlement. */
    async myPurchases(page: number, pageSize: number): Promise<MarketplacePage<AppTemplateOrderSummary>> {
      return pageOf(await orderClient.orderAppTemplates.appTemplateOrders.list({ page, pageSize }));
    },
    async myTemplates(page: number, pageSize: number): Promise<MarketplacePage<AppTemplateResponse>> {
      return pageOf(await deployClient.template.appTemplates.list({ page, pageSize }));
    },
    submit(templateUuid: string): Promise<AppTemplateResponse> {
      return deployClient.template.appTemplates.submit(templateUuid);
    },
    remove(templateUuid: string): Promise<void> {
      return deployClient.template.appTemplates.delete(templateUuid);
    },
    async listVersions(templateUuid: string): Promise<readonly AppTemplateVersionResponse[]> {
      const page = await deployClient.template.appTemplateVersions.list(templateUuid, { page: 1, pageSize: 50 });
      return page.items;
    },
  };
}

export type H5MarketplaceService = ReturnType<typeof createH5MarketplaceService>;
