import { useCallback, useEffect, useMemo, useState } from "react";

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

import { translateH5, type DeploymentsH5MessageKey } from "./i18n.ts";
import { createH5MarketplaceService } from "./service.ts";
import type { DeploymentsH5Runtime } from "../bootstrap/runtime.ts";

const PAGE_SIZE = 10;

function translator(locale: string) {
  return (key: DeploymentsH5MessageKey, values?: Record<string, string | number>) => translateH5(locale, key, values);
}

/**
 * An order the buyer still has to pay, held in view state so the sheet can show
 * what to do next. A free listing is owned the moment the order comes back
 * `paid`; a paid one leaves the buyer on a cashier page or with a provider
 * payload to scan.
 */
interface PendingPayment {
  readonly kind: "cashier" | "qr";
  readonly orderNo: string;
  readonly value: string;
}

/**
 * H5 storefront: category chips over the `PUBLIC` + `PUBLISHED` listings,
 * card list, detail sheet, and the acquire command. Acquire asks the platform
 * order center for an app-template order — the deploy module owns the catalog
 * only — so a FREE listing is owned in one tap and a PAID listing hands the
 * buyer to the cashier the order center returned.
 */
export function MarketplaceView({ runtime }: { runtime: DeploymentsH5Runtime }) {
  const t = translator(runtime.locale);
  const service = useMemo(() => createH5MarketplaceService(runtime.deploy, runtime.order), [runtime.deploy, runtime.order]);
  const [categories, setCategories] = useState<readonly TemplateCategoryResponse[]>([]);
  const [items, setItems] = useState<readonly AppTemplateSummaryResponse[]>([]);
  const [orders, setOrders] = useState<readonly AppTemplateOrderSummary[]>([]);
  const [pendingPayment, setPendingPayment] = useState<PendingPayment>();
  const [detail, setDetail] = useState<AppTemplateResponse>();
  const [categoryUuid, setCategoryUuid] = useState("");
  const [keyword, setKeyword] = useState("");
  const [templateType, setTemplateType] = useState<"" | "APP" | "PPT" | "VIDEO">("");
  const [pricingModel, setPricingModel] = useState<"" | "FREE" | "PAID">("");
  const [sort, setSort] = useState<"NEWEST" | "POPULAR">("NEWEST");
  const [versions, setVersions] = useState<readonly AppTemplateVersionResponse[]>([]);
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [actionError, setActionError] = useState<string>();

  // Ownership is the order center's verdict: a `paid` order for a listing is
  // that listing's install entitlement, keyed by the template it was bought for.
  const entitled = useMemo(
    () => new Set(orders.filter((order) => order.status === "paid").map((order) => order.templateUuid)),
    [orders],
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
          templateType: templateType === "" ? undefined : templateType,
          pricingModel: pricingModel === "" ? undefined : pricingModel,
          sort,
        }),
        service.myPurchases(1, 100),
      ]);
      setCategories(categoryList);
      setItems(listings.items);
      setHasMore(listings.hasMore);
      setOrders(ownership.items);
    } catch {
      setError(t("marketplace.loadFailed"));
    } finally {
      setBusy(false);
    }
  }, [service, page, categoryUuid, keyword, templateType, pricingModel, sort, t]);

  useEffect(() => {
    void load();
  }, [load]);

  const openDetail = useCallback(
    async (templateUuid: string): Promise<void> => {
      setActionError(undefined);
      setPendingPayment(undefined);
      // The sheet opens on the detail read; the version list is a follow-up
      // because the app-api only exposes versions to the listing's author, so a
      // buyer's request legitimately comes back empty or refused. A failed
      // version read must not blank the sheet.
      try {
        setDetail(await service.retrieve(templateUuid));
      } catch {
        setActionError(t("marketplace.loadFailed"));
        return;
      }
      try {
        setVersions(await service.listVersions(templateUuid));
      } catch {
        setVersions([]);
      }
    },
    [service, t],
  );

  const closeDetail = useCallback((): void => {
    setDetail(undefined);
    setVersions([]);
    setPendingPayment(undefined);
  }, []);

  const acquire = useCallback(
    async (templateUuid: string): Promise<void> => {
      setActionError(undefined);
      setPendingPayment(undefined);
      try {
        const order = await service.acquire(templateUuid);
        if (order.status === "paid") {
          // The ownership set and the install count both move on acquire, so the
          // sheet closes onto a reloaded list rather than a stale card.
          closeDetail();
          await load();
          return;
        }
        // The order center owns the payment page: hand the buyer to it rather
        // than reporting success for an order that is still unpaid.
        setPendingPayment(pendingPaymentOf(order));
      } catch {
        setActionError(t("marketplace.acquireFailed"));
      }
    },
    [service, load, t, closeDetail],
  );

  return (
    <section className="h5-view">
      <form
        className="h5-search"
        onSubmit={(event) => {
          event.preventDefault();
          setPage(1);
          void load();
        }}
      >
        <input
          aria-label={t("marketplace.search")}
          value={keyword}
          placeholder={t("marketplace.search")}
          onChange={(event) => setKeyword(event.target.value)}
          type="search"
        />
      </form>
      <div className="h5-chips" role="tablist" aria-label={t("marketplace.type")}>
        {(["", "APP", "PPT", "VIDEO"] as const).map((type) => (
          <button
            className={templateType === type ? "h5-chip h5-chip-active" : "h5-chip"}
            key={type || "all"}
            type="button"
            onClick={() => {
              setTemplateType(type);
              setPage(1);
            }}
          >
            {type === "" ? t("marketplace.type.all") : t(`marketplace.type.${type}` as const)}
          </button>
        ))}
      </div>
      <div className="h5-chips" role="tablist" aria-label={t("marketplace.category.all")}>
        <button
          className={categoryUuid === "" ? "h5-chip h5-chip-active" : "h5-chip"}
          type="button"
          onClick={() => {
            setCategoryUuid("");
            setPage(1);
          }}
        >
          {t("marketplace.category.all")}
        </button>
        {categories.map((category) => (
          <button
            className={categoryUuid === category.id ? "h5-chip h5-chip-active" : "h5-chip"}
            key={category.id}
            type="button"
            onClick={() => {
              setCategoryUuid(category.id);
              setPage(1);
            }}
          >
            {category.displayName}
          </button>
        ))}
      </div>
      <div className="h5-chips" role="tablist" aria-label={t("marketplace.pricing.all")}>
        {(["", "FREE", "PAID"] as const).map((pricing) => (
          <button
            className={pricingModel === pricing ? "h5-chip h5-chip-active" : "h5-chip"}
            key={pricing || "all-pricing"}
            type="button"
            onClick={() => {
              setPricingModel(pricing);
              setPage(1);
            }}
          >
            {pricing === "" ? t("marketplace.pricing.all") : t(pricing === "PAID" ? "marketplace.pricing.PAID" : "marketplace.pricing.FREE")}
          </button>
        ))}
      </div>
      <div className="h5-chips" role="tablist" aria-label={t("marketplace.sort.NEWEST")}>
        {(["NEWEST", "POPULAR"] as const).map((option) => (
          <button
            className={sort === option ? "h5-chip h5-chip-active" : "h5-chip"}
            key={option}
            type="button"
            onClick={() => {
              setSort(option);
              setPage(1);
            }}
          >
            {t(option === "POPULAR" ? "marketplace.sort.POPULAR" : "marketplace.sort.NEWEST")}
          </button>
        ))}
      </div>
      {error && (
        <div className="h5-error" role="alert">
          {error}
          <button type="button" onClick={() => void load()}>
            {t("common.retry")}
          </button>
        </div>
      )}
      <div className="h5-cards">
        {items.map((item) => (
          <button
            className="h5-card"
            key={item.id}
            type="button"
            onClick={() => {
              void openDetail(item.id);
            }}
          >
            <span className="h5-card-title">
              {item.isFeatured && <em className="h5-badge h5-badge-featured">{t("marketplace.featured")}</em>}
              {item.displayName}
            </span>
            <span className="h5-card-summary">{item.summary}</span>
            <span className="h5-card-meta">
              <b className={item.pricingModel === "PAID" ? "h5-price" : "h5-price h5-price-free"}>
                {item.pricingModel === "PAID" ? `${t("marketplace.pricing.PAID")} ${item.priceMinor} ${item.currency}` : t("marketplace.pricing.FREE")}
              </b>
              <span>{t("marketplace.installs", { count: item.installCount })}</span>
              {entitled.has(item.id) && <em className="h5-badge">{t("marketplace.acquired")}</em>}
            </span>
          </button>
        ))}
        {items.length === 0 && !busy && <div className="h5-empty">{t("marketplace.empty")}</div>}
      </div>
      {hasMore && (
        <button className="h5-more" disabled={busy} type="button" onClick={() => setPage((current) => current + 1)}>
          {busy ? t("common.working") : "···"}
        </button>
      )}
      <h2 className="h5-section">{t("marketplace.purchases")}</h2>
      {orders.length === 0 ? (
        <div className="h5-empty">{t("marketplace.purchases.empty")}</div>
      ) : (
        <ul className="h5-list">
          {orders.map((order) => (
            <li key={order.orderId}>
              <span className={`h5-status h5-status-${order.status.toLowerCase()}`}>{order.status}</span>
              <span className="h5-list-main">{order.templateName}</span>
              <span>{`${order.amount} ${order.currencyCode}`}</span>
            </li>
          ))}
        </ul>
      )}
      {detail && (
        <div
          className="h5-sheet-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) closeDetail();
          }}
        >
          <div className="h5-sheet" role="dialog" aria-modal="true" aria-label={t("marketplace.detail")}>
            <header>
              <h2>{detail.displayName}</h2>
              <button className="h5-sheet-close" type="button" onClick={closeDetail}>
                ×
              </button>
            </header>
            <p className="h5-sheet-summary">{detail.summary}</p>
            <p className="h5-sheet-meta">
              {t("marketplace.installs", { count: detail.installCount })} · {t("marketplace.updatedAt", { value: detail.updatedAt })}
            </p>
            {detail.description
              .split("\n")
              .filter((line) => line.trim() !== "")
              .map((line, index) => (
                <p key={index}>{line}</p>
              ))}
            <h3 className="h5-section">{t("marketplace.versions")}</h3>
            {detail.latestVersionUuid === undefined && versions.length === 0 ? (
              <div className="h5-empty">{t("marketplace.versions.empty")}</div>
            ) : (
              <ul className="h5-list">
                {versions.map((version) => (
                  <li key={version.id}>
                    <span className={`h5-status h5-status-${version.status.toLowerCase()}`}>{version.status}</span>
                    <span className="h5-list-main">{version.templateVersion}</span>
                    <span>{version.publishedAt ?? version.createdAt}</span>
                  </li>
                ))}
              </ul>
            )}
            {actionError && <div className="h5-error" role="alert">{actionError}</div>}
            {pendingPayment && (
              <div className="h5-notice" role="status">
                <p>
                  {pendingPayment.kind === "cashier"
                    ? t("marketplace.paymentPending", { orderNo: pendingPayment.orderNo })
                    : t("marketplace.paymentQr", { orderNo: pendingPayment.orderNo, qrCode: pendingPayment.value })}
                </p>
                {pendingPayment.kind === "cashier" && (
                  <a className="h5-secondary" href={pendingPayment.value} target="_blank" rel="noreferrer">
                    {t("marketplace.openCashier")}
                  </a>
                )}
              </div>
            )}
            <footer>
              <button className="h5-secondary" type="button" onClick={closeDetail}>
                {t("common.close")}
              </button>
              <button
                className="h5-primary"
                disabled={entitled.has(detail.id) || busy}
                type="button"
                onClick={() => void acquire(detail.id)}
              >
                {entitled.has(detail.id)
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
 * sheet keeps a link in case the browser blocked the popup; a provider-native
 * order has no page to open, so the payload itself is surfaced for scanning.
 */
function pendingPaymentOf(order: AppTemplateOrderCreateResult): PendingPayment {
  if (order.qrCodeType === "cashier_url") {
    window.open(order.cashierUrl, "_blank", "noopener,noreferrer");
    return { kind: "cashier", orderNo: order.orderNo, value: order.cashierUrl };
  }
  return { kind: "qr", orderNo: order.orderNo, value: order.qrCode };
}
