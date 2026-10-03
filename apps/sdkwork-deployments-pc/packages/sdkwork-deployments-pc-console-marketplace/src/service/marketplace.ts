/**
 * Console service layer for the app template marketplace.
 *
 * The injected app-SDK client arrives from the host (clients-as-props, same
 * contract as the publishing package); this module is the only place the
 * pages touch transport shape, so the acquire idempotency key and the
 * optional-parameter projection live here rather than in the components.
 */
import { uuid } from "@sdkwork/utils/id";

import type {
  AppTemplateResponse,
  AppTemplateSummaryResponse,
  AppTemplateVersionResponse,
  SdkworkDeployAppClient,
  TemplateCategoryResponse,
  TemplatePurchaseResponse,
} from "@sdkwork/deployments-pc-console-core/sdk";

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

export function createMarketplaceService(deployClient: SdkworkDeployAppClient) {
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
      return {
        items: page.items,
        total: Number(page.pageInfo.totalItems ?? "0"),
        hasMore: page.pageInfo.hasMore ?? false,
      };
    },
    async retrieve(templateUuid: string): Promise<AppTemplateResponse> {
      return deployClient.template.marketplaceTemplates.retrieve(templateUuid);
    },
    /** Acquires the template; FREE settles instantly, PAID parks in PENDING. */
    async acquire(templateUuid: string, versionUuid?: string | undefined): Promise<TemplatePurchaseResponse> {
      return deployClient.template.templatePurchases.create(
        templateUuid,
        versionUuid === undefined ? {} : { versionUuid },
        { idempotencyKey: uuid() },
      );
    },
    async myPurchases(page: number, pageSize: number): Promise<MarketplacePage<TemplatePurchaseResponse>> {
      const result = await deployClient.template.templatePurchases.list({ page, pageSize });
      return {
        items: result.items,
        total: Number(result.pageInfo.totalItems ?? "0"),
        hasMore: result.pageInfo.hasMore ?? false,
      };
    },
  };
}

export type MarketplaceService = ReturnType<typeof createMarketplaceService>;

export interface CreateTemplateDraft {
  readonly appUuid: string;
  readonly templateType: TemplateType;
  readonly categoryUuid: string;
  readonly templateKey: string;
  readonly displayName: string;
  readonly summary: string;
  readonly description?: string | undefined;
  readonly visibility: "PUBLIC" | "PRIVATE";
  readonly pricingModel: "FREE" | "PAID";
  readonly priceMinor?: string | undefined;
  readonly initialVersion?: string | undefined;
}

export interface CreateTemplateVersionDraft {
  readonly version: string;
  readonly changelog?: string | undefined;
  readonly artifactUuid?: string | undefined;
}

export function createMyTemplatesService(deployClient: SdkworkDeployAppClient) {
  return {
    async list(page: number, pageSize: number, keyword?: string | undefined): Promise<MarketplacePage<AppTemplateResponse>> {
      const result = await deployClient.template.appTemplates.list(
        omitBlank({ page, pageSize, ...(keyword === undefined || keyword.trim() === "" ? {} : { keyword: keyword.trim() }) }),
      );
      return {
        items: result.items,
        total: Number(result.pageInfo.totalItems ?? "0"),
        hasMore: result.pageInfo.hasMore ?? false,
      };
    },
    async listApps(): Promise<readonly { id: string; name: string }[]> {
      const page = await deployClient.app.list({ page: 1, pageSize: 100 });
      return page.items.map((app) => ({ id: app.id, name: app.name }));
    },
    async create(draft: CreateTemplateDraft): Promise<AppTemplateResponse> {
      return deployClient.template.appTemplates.create(
        {
          appUuid: draft.appUuid,
          templateType: draft.templateType,
          categoryUuid: draft.categoryUuid,
          templateKey: draft.templateKey.trim(),
          displayName: draft.displayName.trim(),
          summary: draft.summary.trim(),
          ...(draft.description === undefined || draft.description.trim() === "" ? {} : { description: draft.description }),
          visibility: draft.visibility,
          pricingModel: draft.pricingModel,
          ...(draft.pricingModel === "PAID" && draft.priceMinor !== undefined && draft.priceMinor.trim() !== ""
            ? { priceMinor: draft.priceMinor.trim() }
            : {}),
          ...(draft.initialVersion === undefined || draft.initialVersion.trim() === ""
            ? {}
            : { initialVersion: { version: draft.initialVersion.trim() } }),
        },
        { idempotencyKey: uuid() },
      );
    },
    remove(templateUuid: string): Promise<void> {
      return deployClient.template.appTemplates.delete(templateUuid);
    },
    submit(templateUuid: string): Promise<AppTemplateResponse> {
      return deployClient.template.appTemplates.submit(templateUuid);
    },
    async listVersions(templateUuid: string): Promise<readonly AppTemplateVersionResponse[]> {
      const page = await deployClient.template.appTemplateVersions.list(templateUuid, { page: 1, pageSize: 100 });
      return page.items;
    },
    async createVersion(templateUuid: string, draft: CreateTemplateVersionDraft): Promise<AppTemplateVersionResponse> {
      return deployClient.template.appTemplateVersions.create(
        templateUuid,
        {
          version: draft.version.trim(),
          ...(draft.changelog === undefined || draft.changelog.trim() === "" ? {} : { changelog: draft.changelog }),
          ...(draft.artifactUuid === undefined || draft.artifactUuid.trim() === "" ? {} : { artifactUuid: draft.artifactUuid.trim() }),
        },
        { idempotencyKey: uuid() },
      );
    },
  };
}

export type MyTemplatesService = ReturnType<typeof createMyTemplatesService>;
