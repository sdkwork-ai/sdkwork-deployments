import { useCallback, useEffect, useMemo, useState } from "react";

import type {
  AppTemplateResponse,
  AppTemplateSummaryResponse,
  TemplateCategoryResponse,
  TemplatePurchaseResponse,
} from "@sdkwork/deployments-app-sdk";

import { translateH5, type DeploymentsH5MessageKey } from "./i18n.ts";
import { createH5MarketplaceService } from "./service.ts";
import type { DeploymentsH5Runtime } from "../bootstrap/runtime.ts";

const PAGE_SIZE = 10;

function translator(locale: string) {
  return (key: DeploymentsH5MessageKey, values?: Record<string, string | number>) => translateH5(locale, key, values);
}

/**
 * H5 storefront: category chips over the `PUBLIC` + `PUBLISHED` listings,
 * card list, detail sheet, and the idempotent acquire command. `PAID`
 * templates report the honest `PENDING` settlement state instead of implying
 * the entitlement is live.
 */
export function MarketplaceView({ runtime }: { runtime: DeploymentsH5Runtime }) {
  const t = translator(runtime.locale);
  const service = useMemo(() => createH5MarketplaceService(runtime.deploy), [runtime.deploy]);
  const [categories, setCategories] = useState<readonly TemplateCategoryResponse[]>([]);
  const [items, setItems] = useState<readonly AppTemplateSummaryResponse[]>([]);
  const [purchases, setPurchases] = useState<readonly TemplatePurchaseResponse[]>([]);
  const [detail, setDetail] = useState<AppTemplateResponse>();
  const [categoryUuid, setCategoryUuid] = useState("");
  const [keyword, setKeyword] = useState("");
  const [templateType, setTemplateType] = useState<"" | "APP" | "PPT" | "VIDEO">("");
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [actionError, setActionError] = useState<string>();

  const entitled = useMemo(
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
          templateType: templateType === "" ? undefined : templateType,
        }),
        service.myPurchases(1, 100),
      ]);
      setCategories(categoryList);
      setItems(listings.items);
      setHasMore(listings.hasMore);
      setPurchases(ownership.items);
    } catch {
      setError(t("marketplace.loadFailed"));
    } finally {
      setBusy(false);
    }
  }, [service, page, categoryUuid, keyword, templateType, t]);

  useEffect(() => {
    void load();
  }, [load]);

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
              setActionError(undefined);
              void service
                .retrieve(item.id)
                .then(setDetail)
                .catch(() => setActionError(t("marketplace.loadFailed")));
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
      {purchases.length === 0 ? (
        <div className="h5-empty">{t("marketplace.purchases.empty")}</div>
      ) : (
        <ul className="h5-list">
          {purchases.map((purchase) => (
            <li key={purchase.id}>
              <span className={`h5-status h5-status-${purchase.status.toLowerCase()}`}>{purchase.status}</span>
              <span className="h5-list-main">{purchase.templateUuid}</span>
              <span>{purchase.pricingModel === "PAID" ? t("marketplace.pricing.PAID") : t("marketplace.pricing.FREE")}</span>
            </li>
          ))}
        </ul>
      )}
      {detail && (
        <div
          className="h5-sheet-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setDetail(undefined);
          }}
        >
          <div className="h5-sheet" role="dialog" aria-modal="true" aria-label={t("marketplace.detail")}>
            <header>
              <h2>{detail.displayName}</h2>
              <button className="h5-sheet-close" type="button" onClick={() => setDetail(undefined)}>
                ×
              </button>
            </header>
            <p className="h5-sheet-summary">{detail.summary}</p>
            {detail.description
              .split("\n")
              .filter((line) => line.trim() !== "")
              .map((line, index) => (
                <p key={index}>{line}</p>
              ))}
            {actionError && <div className="h5-error" role="alert">{actionError}</div>}
            <footer>
              <button className="h5-secondary" type="button" onClick={() => setDetail(undefined)}>
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
                    ? t("marketplace.acquirePending")
                    : t("marketplace.acquire")}
              </button>
            </footer>
          </div>
        </div>
      )}
    </section>
  );
}
