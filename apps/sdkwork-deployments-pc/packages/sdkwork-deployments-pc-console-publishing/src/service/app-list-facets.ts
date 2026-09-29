/**
 * Applications-ledger facets: the two decisions that decide *which rows the
 * ledger is showing* — the application-type facet and the keyword — plus the
 * paging rule that makes them trustworthy.
 *
 * Why this lives in the service layer and not in the page: both are plain
 * predicates over a fetched row set, and a predicate written inline inside a
 * component is a unit-test blind spot — it can only be exercised through a
 * rendered table. Extracted here, each one is driven directly.
 *
 * **Why the page loads the whole scope instead of filtering the page it has.**
 * The application-type facet is client-side and there is no server parameter for
 * it (see the note below), so filtering only the rows of the current page would
 * make the answer depend on which page the operator happens to be on: the same
 * "Android" selection would return different apps after turning a page. Loading
 * every page of the active ownership scope once removes that drift, and it also
 * makes the per-type counts exact rather than "counts of what happened to load".
 * {@link loadAllApps} is that rule, with a hard page ceiling so a runaway tenant
 * cannot turn one page view into an unbounded request loop.
 *
 * **Why the facet is `deploy_app.app_kind` and not the surface.** The server
 * distinguishes H5 from PC web by the *surface root* (`apps/sdkwork-<code>-h5`
 * vs `..-pc`), and that is recorded in `metadata.surface` — but only by the
 * publish flow. `apps.create` writes no surface, so an app that was registered
 * and never published carries no端 information at all, and a surface-split facet
 * would have to invent a "unknown end" bucket for it. `app_kind` is present on
 * every row (it is a `NOT NULL` column serialized by every list response), so it
 * is the only facet that is complete today. `SPA_WEB` therefore covers both ends;
 * the page says so under the facet legend rather than pretending otherwise.
 */
import type { AppKind, AppResponse, PageInfo } from "@sdkwork/deployments-pc-console-core/sdk";

/**
 * Display order of the application-type facet.
 *
 * A **rank table, not an ordered list**, on purpose: the facet is rendered from
 * the exhaustive `AppKind` label map, so a kind the contract adds shows up in the
 * facet without anybody editing this file. A missing rank therefore has to mean
 * "put it at the end", never "leave it out" — otherwise adding an enum member
 * would silently make apps of that kind unfilterable (they would render as a row
 * whose type can never be selected).
 */
const APP_KIND_FILTER_RANK: Readonly<Partial<Record<AppKind, number>>> = {
  SPA_WEB: 0,
  STATIC_WEB: 1,
  DESKTOP_APP: 2,
  WECHAT_MINIPROGRAM: 3,
  DOUYIN_MINIPROGRAM: 4,
  ANDROID_APP: 5,
  IOS_APP: 6,
  HARMONYOS_APP: 7,
  API_SERVICE: 8,
};

/**
 * Order the facet options for display, keeping every kind the caller passed.
 *
 * Ties (and unranked kinds) keep their input order, which is the order of the
 * label map — so the facet is deterministic without this function owning the
 * membership of the list.
 */
export function orderAppKindFacets(kinds: readonly AppKind[]): readonly AppKind[] {
  return kinds
    .map((kind, index) => ({ kind, index }))
    .sort((left, right) => {
      const leftRank = APP_KIND_FILTER_RANK[left.kind] ?? Number.MAX_SAFE_INTEGER
      const rightRank = APP_KIND_FILTER_RANK[right.kind] ?? Number.MAX_SAFE_INTEGER
      return leftRank === rightRank ? left.index - right.index : leftRank - rightRank
    })
    .map((entry) => entry.kind)
}

/** Keyword normalization: trim, then case-fold — the same fold the server uses. */
export function normalizeKeyword(raw: string): string {
  return raw.trim().toLowerCase()
}

/**
 * Keyword match, mirroring `apps.list`'s server-side rule
 * (`LOWER(name) LIKE %kw% OR LOWER(slug) LIKE %kw%`) so the local and remote
 * answers cannot disagree about what a keyword means.
 */
export function appMatchesKeyword(app: AppResponse, keyword: string): boolean {
  if (keyword === "") return true
  return app.name.toLowerCase().includes(keyword) || app.slug.toLowerCase().includes(keyword)
}

/**
 * Application-type facet: a multi-select with **OR** semantics.
 *
 * An empty selection is not "nothing matches" but "the operator has not narrowed
 * the type dimension yet" — the facet's own "all types" state. Reading it the
 * other way is how a filter rail ends up looking broken on first paint.
 */
export function appMatchesKindFacet(app: AppResponse, selectedKinds: ReadonlySet<AppKind>): boolean {
  return selectedKinds.size === 0 || selectedKinds.has(app.appKind)
}

/** One row of the application-type facet: the kind, and how many loaded rows it has. */
export interface AppKindFacetCount {
  readonly appKind: AppKind
  readonly count: number
}

/**
 * Count the loaded rows per `app_kind`, in facet display order.
 *
 * Counts are taken over the rows the caller passes in — for the ledger that is
 * the complete set of the active ownership scope (see {@link loadAllApps}), which
 * is what makes them worth printing next to a checkbox.
 *
 * Kinds with no rows are still returned: a facet entry that disappears when its
 * count reaches zero cannot be switched off again once the operator selects it.
 */
export function countAppKinds(
  apps: readonly AppResponse[],
  facetOrder: readonly AppKind[],
): readonly AppKindFacetCount[] {
  const counts = new Map<string, number>()
  for (const app of apps) {
    counts.set(app.appKind, (counts.get(app.appKind) ?? 0) + 1)
  }
  return facetOrder.map((appKind) => ({ appKind, count: counts.get(appKind) ?? 0 }))
}

/** The two narrowing dimensions the ledger applies locally. */
export interface AppLedgerFacets {
  readonly kinds: ReadonlySet<AppKind>
  readonly keyword: string
}

/**
 * Apply both facets. Order is irrelevant — the two predicates are independent.
 *
 * The result is mutable even though the input is not: `Array.prototype.filter`
 * already hands back a fresh array, and the table component takes `AppResponse[]`,
 * so widening the type here would force a copy at every call site for nothing.
 * The input stays `readonly` — that is the side that would actually be a mistake.
 */
export function filterApps(
  apps: readonly AppResponse[],
  facets: AppLedgerFacets,
): AppResponse[] {
  return apps.filter((app) =>
    appMatchesKindFacet(app, facets.kinds) && appMatchesKeyword(app, facets.keyword))
}

/** Rows are pulled this many at a time — the contract caps `page_size` at 200. */
export const APP_LIST_PAGE_SIZE = 200
/** Ceiling on {@link loadAllApps}: 5 × 200 rows, and the page says when it bites. */
export const APP_LIST_MAX_PAGES = 5

/** One page of the app ledger, as the generated client returns it. */
export interface AppListPage {
  readonly items: AppResponse[]
  readonly pageInfo: PageInfo
}

/** The complete scope, plus whether the ceiling above cut it short. */
export interface AppListSnapshot {
  readonly items: AppResponse[]
  /** True when the ceiling stopped the walk while the server still had rows. */
  readonly truncated: boolean
}

/**
 * Walk every page of a list call and return the whole set.
 *
 * Termination is decided by the server's own `hasMore` when it sends one, and
 * otherwise by the short-page rule (`items.length < pageSize`), so a host that
 * omits `pageInfo` still terminates instead of looping forever. An empty page
 * always terminates, which is the guard that survives every other signal being
 * wrong.
 */
export async function loadAllApps(
  loadPage: (page: number) => Promise<AppListPage>,
  options: { readonly pageSize?: number; readonly maxPages?: number } = {},
): Promise<AppListSnapshot> {
  const pageSize = options.pageSize ?? APP_LIST_PAGE_SIZE
  const maxPages = options.maxPages ?? APP_LIST_MAX_PAGES
  const items: AppResponse[] = []
  for (let page = 1; page <= maxPages; page += 1) {
    const result = await loadPage(page)
    items.push(...result.items)
    if (result.items.length === 0) return { items, truncated: false }
    // Widened to `| undefined` before reading: the generated envelope declares
    // `pageInfo` required, but a host that wraps the client may not forward it,
    // and the fallback (a short page means the last page) has to survive that.
    const pageInfo: PageInfo | undefined = result.pageInfo
    const hasMore = pageInfo?.hasMore ?? result.items.length >= pageSize
    if (!hasMore) return { items, truncated: false }
    if (page === maxPages) return { items, truncated: true }
  }
  return { items, truncated: false }
}
