import { RefreshCw, Search, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";

import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";

import type {
  AppTemplateResponse,
  AppTemplateSummaryResponse,
  SdkworkDeployAppClient,
  TemplateCategoryResponse,
  TemplatePurchaseResponse,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";

import { marketplaceTranslator, type MarketplaceMessageKey } from "./i18n.ts";
import { createMarketplaceService, type TemplateType } from "./service/marketplace.ts";

export interface MarketplacePageProps {
  readonly deployClient: SdkworkDeployAppClient;
  readonly locale: DeploymentsLocale;
}

const PAGE_SIZE = 20;

/**
 * Storefront for the tenant marketplace: category/pricing facets over the
 * `PUBLIC` + `PUBLISHED` listings, a detail dialog, and the acquire action.
 * Acquire grants FREE listings only — a PAID listing routes the buyer to the
 * commerce checkout, so the button says so instead of implying this surface
 * can mint the entitlement.
 */
export function MarketplacePage({ deployClient, locale }: MarketplacePageProps) {
  const t = marketplaceTranslator(locale);
  const service = useMemo(() => createMarketplaceService(deployClient), [deployClient]);
  const [categories, setCategories] = useState<readonly TemplateCategoryResponse[]>([]);
  const [items, setItems] = useState<readonly AppTemplateSummaryResponse[]>([]);
  const [total, setTotal] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [page, setPage] = useState(1);
  const [keyword, setKeyword] = useState("");
  const [categoryUuid, setCategoryUuid] = useState("");
  const [pricingModel, setPricingModel] = useState<"" | "FREE" | "PAID">("");
  const [templateType, setTemplateType] = useState<"" | TemplateType>("");
  const [sort, setSort] = useState<"NEWEST" | "POPULAR">("NEWEST");
  const [detail, setDetail] = useState<AppTemplateResponse>();
  const [purchases, setPurchases] = useState<readonly TemplatePurchaseResponse[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [actionError, setActionError] = useState<string>();

  const activeEntitlements = useMemo(
    () => new Set(purchases.filter((purchase) => purchase.status === "ACTIVE").map((purchase) => purchase.templateUuid)),
    [purchases],
  );

  const load = useCallback(async (): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const [categoryList, listings, ownership] = await Promise.all([
        service.listCategories(),
        service.browse({
          page,
          pageSize: PAGE_SIZE,
          categoryUuid: categoryUuid === "" ? undefined : categoryUuid,
          keyword: keyword === "" ? undefined : keyword,
          pricingModel: pricingModel === "" ? undefined : pricingModel,
          templateType: templateType === "" ? undefined : templateType,
          sort,
        }),
        service.myPurchases(1, 100),
      ]);
      setCategories(categoryList);
      setItems(listings.items);
      setTotal(listings.total);
      setHasMore(listings.hasMore);
      setPurchases(ownership.items);
    } catch {
      setError(t("marketplace.loadFailed"));
    } finally {
      setBusy(false);
    }
  }, [service, page, categoryUuid, keyword, pricingModel, templateType, sort, t]);

  useEffect(() => {
    void load();
  }, [load]);

  const openDetail = useCallback(
    async (templateUuid: string): Promise<void> => {
      setActionError(undefined);
      try {
        setDetail(await service.retrieve(templateUuid));
      } catch {
        setActionError(t("marketplace.loadFailed"));
      }
    },
    [service, t],
  );

  const acquire = useCallback(
    async (templateUuid: string): Promise<void> => {
      setActionError(undefined);
      try {
        await service.acquire(templateUuid);
        setDetail(undefined);
        await load();
      } catch {
        setActionError(t("marketplace.acquireFailed"));
      }
    },
    [service, load, t],
  );

  const columns = useMemo<DataTableColumn<AppTemplateSummaryResponse>[]>(
    () => [
      {
        id: "displayName",
        header: t("myTemplates.create.displayName"),
        cell: (item) => (
          <button className="link-button" type="button" onClick={() => void openDetail(item.id)}>
            {item.displayName}
          </button>
        ),
      },
      { id: "summary", header: t("myTemplates.create.summary"), cell: (item) => item.summary },
      { id: "pricingModel", header: t("common.pricing"), cell: (item) => formatPricing(item, t) },
      { id: "installCount", header: t("marketplace.sort.POPULAR"), cell: (item) => t("marketplace.installCount", { count: item.installCount }) },
      {
        id: "status",
        header: t("common.status"),
        cell: (item) => <span className={`status-badge status-${item.status.toLowerCase()}`}>{item.status}</span>,
      },
    ],
    [t, openDetail],
  );

  async function submitSearch(event: FormEvent): Promise<void> {
    event.preventDefault();
    setPage(1);
    await load();
  }

  return (
    <section className="resource-page">
      <header className="page-header">
        <div>
          <span className="eyebrow">marketplace</span>
          <h1>{t("marketplace.title")}</h1>
          <p>{t("marketplace.description")}</p>
        </div>
        <button className="icon-button" type="button" disabled={busy} title={t("common.refresh")} onClick={() => void load()}>
          <RefreshCw size={18} />
        </button>
      </header>
      <div className="toolbar">
        <form className="search-box" onSubmit={(event) => void submitSearch(event)}>
          <Search size={16} />
          <input
            aria-label={t("marketplace.search")}
            value={keyword}
            placeholder={t("marketplace.search")}
            onChange={(event) => setKeyword(event.target.value)}
          />
        </form>
        <label className="scope-input">
          <select
            aria-label={t("marketplace.category")}
            value={categoryUuid}
            onChange={(event) => {
              setCategoryUuid(event.target.value);
              setPage(1);
            }}
          >
            <option value="">{t("marketplace.category.all")}</option>
            {categories.map((category) => (
              <option key={category.id} value={category.id}>
                {category.displayName}
              </option>
            ))}
          </select>
        </label>
        <label className="scope-input">
          <select
            aria-label={t("marketplace.pricing")}
            value={pricingModel}
            onChange={(event) => {
              setPricingModel(event.target.value as "" | "FREE" | "PAID");
              setPage(1);
            }}
          >
            <option value="">{t("marketplace.pricing.all")}</option>
            <option value="FREE">{t("marketplace.pricing.FREE")}</option>
            <option value="PAID">{t("marketplace.pricing.PAID")}</option>
          </select>
        </label>
        <label className="scope-input">
          <select
            aria-label={t("marketplace.type")}
            value={templateType}
            onChange={(event) => {
              setTemplateType(event.target.value as "" | TemplateType);
              setPage(1);
            }}
          >
            <option value="">{t("marketplace.type.all")}</option>
            <option value="APP">{t("marketplace.type.APP")}</option>
            <option value="PPT">{t("marketplace.type.PPT")}</option>
            <option value="VIDEO">{t("marketplace.type.VIDEO")}</option>
          </select>
        </label>
        <label className="scope-input">
          <select
            aria-label={t("marketplace.sort")}
            value={sort}
            onChange={(event) => {
              setSort(event.target.value as "NEWEST" | "POPULAR");
              setPage(1);
            }}
          >
            <option value="NEWEST">{t("marketplace.sort.NEWEST")}</option>
            <option value="POPULAR">{t("marketplace.sort.POPULAR")}</option>
          </select>
        </label>
      </div>
      {error && (
        <div className="error-banner" role="alert">
          {error}
          <button className="icon-button" type="button" title={t("common.close")} onClick={() => setError(undefined)}>
            <X size={16} />
          </button>
        </div>
      )}
      <DataTable<AppTemplateSummaryResponse>
        columns={columns}
        density="compact"
        emptyState={<span>{t("marketplace.empty")}</span>}
        getRowId={(item) => item.id}
        loading={busy && items.length === 0}
        pagination={{
          hasMore,
          mode: "server",
          onPageChange: (next: number) => {
            if (!busy && next >= 1 && next !== page) setPage(next);
          },
          onPageSizeChange: () => undefined,
          page,
          pageSize: PAGE_SIZE,
          pageSizeOptions: [PAGE_SIZE],
          ...(total === 0 ? {} : { rowCount: total }),
        }}
        rows={items as AppTemplateSummaryResponse[]}
        stickyHeader
      />
      <h2 className="section-title">{t("marketplace.purchases")}</h2>
      <PurchaseTable emptyLabel={t("marketplace.purchases.empty")} locale={locale} purchases={purchases} />
      {detail && (
        <div
          className="dialog-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setDetail(undefined);
          }}
        >
          <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="marketplace-detail-title">
            <header>
              <div>
                <span className="eyebrow">{t("marketplace.detail")}</span>
                <h2 id="marketplace-detail-title">{detail.displayName}</h2>
              </div>
              <button className="icon-button" title={t("common.close")} type="button" onClick={() => setDetail(undefined)}>
                <X size={18} />
              </button>
            </header>
            {detail.isFeatured && <div className="warning">{t("marketplace.featured")}</div>}
            <p>{detail.summary}</p>
            {detail.description
              .split("\n")
              .filter((line) => line.trim() !== "")
              .map((line, index) => (
                <p key={index}>{line}</p>
              ))}
            {actionError && <div className="error-banner" role="alert">{actionError}</div>}
            <footer>
              <button className="secondary-button" type="button" onClick={() => setDetail(undefined)}>
                {t("common.close")}
              </button>
              <button
                className="command-button"
                disabled={activeEntitlements.has(detail.id) || detail.pricingModel === "PAID"}
                type="button"
                onClick={() => void acquire(detail.id)}
              >
                {activeEntitlements.has(detail.id)
                  ? t("marketplace.acquired")
                  : detail.pricingModel === "PAID"
                    ? t("marketplace.acquireCommerce")
                    : t("marketplace.acquire")}
              </button>
            </footer>
          </div>
        </div>
      )}
    </section>
  );
}

function PurchaseTable({
  emptyLabel,
  locale,
  purchases,
}: {
  emptyLabel: string;
  locale: DeploymentsLocale;
  purchases: readonly TemplatePurchaseResponse[];
}) {
  const t = marketplaceTranslator(locale);
  const columns = useMemo<DataTableColumn<TemplatePurchaseResponse>[]>(
    () => [
      { id: "templateUuid", header: "Template", cell: (item) => item.templateUuid },
      { id: "pricingModel", header: t("common.pricing"), cell: (item) => pricingLabel(item.pricingModel, t) },
      {
        id: "status",
        header: t("common.status"),
        cell: (item) => <span className={`status-badge status-${item.status.toLowerCase()}`}>{item.status}</span>,
      },
    ],
    [t],
  );
  return (
    <DataTable<TemplatePurchaseResponse>
      columns={columns}
      density="compact"
      emptyState={<span>{emptyLabel}</span>}
      getRowId={(item) => item.id}
      pagination={{
        hasMore: false,
        mode: "server",
        onPageChange: () => undefined,
        onPageSizeChange: () => undefined,
        page: 1,
        pageSize: 100,
        pageSizeOptions: [100],
      }}
      rows={purchases as TemplatePurchaseResponse[]}
      stickyHeader
    />
  );
}

function pricingLabel(pricingModel: "FREE" | "PAID", t: (key: MarketplaceMessageKey, values?: Record<string, string | number>) => string): string {
  return pricingModel === "PAID" ? t("marketplace.pricing.PAID") : t("marketplace.pricing.FREE");
}

function formatPricing(
  item: AppTemplateSummaryResponse,
  t: (key: MarketplaceMessageKey, values?: Record<string, string | number>) => string,
): string {
  const label = pricingLabel(item.pricingModel, t);
  return item.pricingModel === "PAID" ? `${label} ${item.priceMinor} ${item.currency}` : label;
}
