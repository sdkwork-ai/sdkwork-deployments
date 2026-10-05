import { RefreshCw, Search, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";

import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";

import type {
  AppTemplateOrderCreateResult,
  AppTemplateOrderSummary,
  AppTemplateResponse,
  AppTemplateSummaryResponse,
  SdkworkDeployAppClient,
  SdkworkOrderAppClient,
  TemplateCategoryResponse,
} from "@sdkwork/deployments-pc-console-core/sdk";
import type { DeploymentsLocale } from "@sdkwork/deployments-pc-commons";

import { marketplaceTranslator, type MarketplaceMessageKey } from "./i18n.ts";
import { createMarketplaceService, type MarketplacePage as MarketplaceLedger, type TemplateType } from "./service/marketplace.ts";

export interface MarketplacePageProps {
  readonly deployClient: SdkworkDeployAppClient;
  readonly orderClient: SdkworkOrderAppClient;
  readonly locale: DeploymentsLocale;
}

const PAGE_SIZE = 20;

/** The degraded ledger for a host without the order center (see `load`). */
const EMPTY_ORDERS: MarketplaceLedger<AppTemplateOrderSummary> = { items: [], total: 0, hasMore: false };

/**
 * An order the buyer still has to pay, held in component state so the dialog can
 * show what to do next. The listings are free or paid: a free one is owned the
 * moment the order comes back `paid`, and a paid one leaves the buyer on a
 * cashier page or with a provider payload to scan.
 */
interface PendingPayment {
  readonly kind: "cashier" | "qr";
  readonly orderNo: string;
  readonly value: string;
}

/**
 * Storefront for the tenant marketplace: category/pricing facets over the
 * `PUBLIC` + `PUBLISHED` listings, a detail dialog, and the acquire action.
 * Acquire asks the platform order center for an app-template order: the deploy
 * module owns the catalog only, so a FREE listing comes back owned in one click
 * and a PAID listing hands the buyer to the cashier the order center returned.
 */
export function MarketplacePage({ deployClient, orderClient, locale }: MarketplacePageProps) {
  // The translator must be reference-stable per locale: `load` names `t` in its
  // dependency list, and a fresh closure every render would re-create `load`
  // every render, re-fire the load effect, and refetch the whole page in a loop.
  const t = useMemo(() => marketplaceTranslator(locale), [locale]);
  const service = useMemo(() => createMarketplaceService(deployClient, orderClient), [deployClient, orderClient]);
  const [categories, setCategories] = useState<readonly TemplateCategoryResponse[]>([]);
  const [items, setItems] = useState<readonly AppTemplateSummaryResponse[]>([]);
  const [total, setTotal] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [page, setPage] = useState(1);
  const [keyword, setKeyword] = useState("");
  // The draft keyword lives in a ref so typing stays off the load path: `load`
  // reads the ref at call time, and because `keyword` is not in its dependency
  // list the load effect does not fire per keystroke. Only the search submit
  // (or a page reset from it) commits the term.
  const keywordRef = useRef("");
  const [categoryUuid, setCategoryUuid] = useState("");
  const [pricingModel, setPricingModel] = useState<"" | "FREE" | "PAID">("");
  const [templateType, setTemplateType] = useState<"" | TemplateType>("");
  const [sort, setSort] = useState<"NEWEST" | "POPULAR">("NEWEST");
  const [detail, setDetail] = useState<AppTemplateResponse>();
  const [orders, setOrders] = useState<readonly AppTemplateOrderSummary[]>([]);
  const [pendingPayment, setPendingPayment] = useState<PendingPayment>();
  const [busy, setBusy] = useState(false);
  const [acquiring, setAcquiring] = useState(false);
  const [error, setError] = useState<string>();
  const [actionError, setActionError] = useState<string>();

  // Ownership is the order center's verdict: a `paid` order for a listing is
  // that listing's install entitlement, keyed by the template it was bought for.
  const ownedTemplates = useMemo(
    () => new Set(orders.filter((order) => order.status === "paid").map((order) => order.templateUuid)),
    [orders],
  );

  const load = useCallback(async (): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      // The order read is an optional plane: a host that does not mount the
      // platform order center answers 404 for `app_template_orders`, and that
      // gap must not take the storefront down with it. The catalog calls stay
      // page-fatal; the ledger degrades to an empty set, which only clears the
      // "owned" acquire state — an acquire attempt still reports its own
      // failure through `actionError`.
      const [categoryList, listings, ownership] = await Promise.all([
        service.listCategories(),
        service.browse({
          page,
          pageSize: PAGE_SIZE,
          categoryUuid: categoryUuid === "" ? undefined : categoryUuid,
          keyword: keywordRef.current.trim() === "" ? undefined : keywordRef.current.trim(),
          pricingModel: pricingModel === "" ? undefined : pricingModel,
          templateType: templateType === "" ? undefined : templateType,
          sort,
        }),
        service.myPurchases(1, 100).catch(() => EMPTY_ORDERS),
      ]);
      setCategories(categoryList);
      setItems(listings.items);
      setTotal(listings.total);
      setHasMore(listings.hasMore);
      setOrders(ownership.items);
    } catch {
      setError(t("marketplace.loadFailed"));
    } finally {
      setBusy(false);
    }
  }, [service, page, categoryUuid, pricingModel, templateType, sort, t]);

  useEffect(() => {
    void load();
  }, [load]);

  const openDetail = useCallback(
    async (templateUuid: string): Promise<void> => {
      setActionError(undefined);
      setPendingPayment(undefined);
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
      setPendingPayment(undefined);
      setAcquiring(true);
      try {
        const order = await service.acquire(templateUuid);
        if (order.status === "paid") {
          setDetail(undefined);
          await load();
          return;
        }
        // The order center owns the payment page: hand the buyer to it rather
        // than reporting success for an order that is still unpaid.
        setPendingPayment(pendingPaymentOf(order));
      } catch {
        setActionError(t("marketplace.acquireFailed"));
      } finally {
        setAcquiring(false);
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
    // Resetting the page re-runs the load effect through its new closure; when
    // the page is already 1 no state changes, so the submit loads directly.
    // Doing both would race a page>1 fetch against the page-1 refetch.
    if (page === 1) {
      await load();
    } else {
      setPage(1);
    }
  }

  function updateKeyword(value: string): void {
    keywordRef.current = value;
    setKeyword(value);
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
            onChange={(event) => updateKeyword(event.target.value)}
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
            <dl className="fact-grid">
              <dt>{t("marketplace.type")}</dt>
              <dd>{templateTypeLabel(detail.templateType, t)}</dd>
              <dt>{t("marketplace.pricing")}</dt>
              <dd>{formatPricing(detail, t)}</dd>
              <dt>{t("common.visibility")}</dt>
              <dd>{detail.visibility}</dd>
              <dt>{t("marketplace.latestVersion")}</dt>
              <dd>{detail.latestVersionUuid ?? t("marketplace.noVersion")}</dd>
              <dt>{t("marketplace.sort.POPULAR")}</dt>
              <dd>{t("marketplace.installCount", { count: detail.installCount })}</dd>
              <dt>{t("marketplace.views")}</dt>
              <dd>{t("marketplace.viewCount", { count: detail.viewCount })}</dd>
              <dt>{t("common.updatedAt")}</dt>
              <dd>{detail.updatedAt}</dd>
            </dl>
            <p>{detail.summary}</p>
            {detail.description
              .split("\n")
              .filter((line) => line.trim() !== "")
              .map((line, index) => (
                <p key={index}>{line}</p>
              ))}
            {actionError && <div className="error-banner" role="alert">{actionError}</div>}
            {pendingPayment && (
              <div className="warning" role="status">
                <span>
                  {pendingPayment.kind === "cashier"
                    ? t("marketplace.paymentPending", { orderNo: pendingPayment.orderNo })
                    : t("marketplace.paymentQr", { orderNo: pendingPayment.orderNo, qrCode: pendingPayment.value })}
                </span>
                {pendingPayment.kind === "cashier" && (
                  <a className="link-button" href={pendingPayment.value} target="_blank" rel="noreferrer">
                    {t("marketplace.openCashier")}
                  </a>
                )}
              </div>
            )}
            <footer>
              <button className="secondary-button" type="button" onClick={() => setDetail(undefined)}>
                {t("common.close")}
              </button>
              <button
                className="command-button"
                disabled={ownedTemplates.has(detail.id) || acquiring}
                type="button"
                onClick={() => void acquire(detail.id)}
              >
                {ownedTemplates.has(detail.id)
                  ? t("marketplace.acquired")
                  : detail.pricingModel === "PAID"
                    ? t("marketplace.acquirePaid")
                    : t("marketplace.acquire")}
              </button>
            </footer>
          </div>
        </div>
      )}
    </section>
  );
}

/**
 * Projects a created order onto what the buyer still has to do. A `cashier_url`
 * order is paid on the order center's page, so it opens in a new tab and the
 * dialog keeps a link in case the browser blocked the popup; a provider-native
 * order has no page to open, so the payload itself is surfaced for scanning.
 */
function pendingPaymentOf(order: AppTemplateOrderCreateResult): PendingPayment {
  if (order.qrCodeType === "cashier_url") {
    window.open(order.cashierUrl, "_blank", "noopener,noreferrer");
    return { kind: "cashier", orderNo: order.orderNo, value: order.cashierUrl };
  }
  return { kind: "qr", orderNo: order.orderNo, value: order.qrCode };
}

function pricingLabel(pricingModel: "FREE" | "PAID", t: (key: MarketplaceMessageKey, values?: Record<string, string | number>) => string): string {
  return pricingModel === "PAID" ? t("marketplace.pricing.PAID") : t("marketplace.pricing.FREE");
}

/** Localized facet label for a listing's artifact kind. */
function templateTypeLabel(
  templateType: AppTemplateSummaryResponse["templateType"],
  t: (key: MarketplaceMessageKey, values?: Record<string, string | number>) => string,
): string {
  switch (templateType) {
    case "PPT":
      return t("marketplace.type.PPT");
    case "VIDEO":
      return t("marketplace.type.VIDEO");
    default:
      return t("marketplace.type.APP");
  }
}

function formatPricing(
  item: AppTemplateSummaryResponse,
  t: (key: MarketplaceMessageKey, values?: Record<string, string | number>) => string,
): string {
  const label = pricingLabel(item.pricingModel, t);
  return item.pricingModel === "PAID" ? `${label} ${item.priceMinor} ${item.currency}` : label;
}
