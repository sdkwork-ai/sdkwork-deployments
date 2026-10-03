import type { DeploymentsH5Runtime } from "../bootstrap/runtime.ts";
import type {
  AppTemplateResponse,
  AppTemplateSummaryResponse,
  TemplateCategoryResponse,
  TemplatePurchaseResponse,
} from "@sdkwork/deployments-app-sdk";
import { uuid } from "@sdkwork/utils/id";

/**
 * H5 marketplace service over the injected app-SDK client. Construction stays
 * in bootstrap; this module is pure transport shaping (idempotency keys,
 * optional-parameter projection) so views stay declarative.
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

export function createH5MarketplaceService(deployClient: DeploymentsH5Runtime["deploy"]) {
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
    async acquire(templateUuid: string): Promise<TemplatePurchaseResponse> {
      return deployClient.template.templatePurchases.create(templateUuid, {}, { idempotencyKey: uuid() });
    },
    async myPurchases(page: number, pageSize: number): Promise<MarketplacePage<TemplatePurchaseResponse>> {
      return pageOf(await deployClient.template.templatePurchases.list({ page, pageSize }));
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
    async listVersions(templateUuid: string): Promise<readonly import("@sdkwork/deployments-app-sdk").AppTemplateVersionResponse[]> {
      const page = await deployClient.template.appTemplateVersions.list(templateUuid, { page: 1, pageSize: 50 });
      return page.items;
    },
  };
}

export type H5MarketplaceService = ReturnType<typeof createH5MarketplaceService>;
