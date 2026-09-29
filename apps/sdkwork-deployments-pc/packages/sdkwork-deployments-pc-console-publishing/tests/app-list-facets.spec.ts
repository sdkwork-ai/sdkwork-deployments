/**
 * The applications ledger's local facets, driven directly.
 *
 * These predicates decide which rows the ledger shows, and every one of them is a
 * one-line branch — which is exactly the shape a unit test cannot reach once it
 * lives inside a component. They are extracted to `service/app-list-facets.ts` and
 * exercised here so that a mutation to any of them turns this file red rather than
 * an operator's table.
 *
 * The two rules worth stating out loud, because both read as bugs when reversed:
 *
 * - an **empty** type selection means "the operator has not narrowed the type
 *   dimension", not "nothing matches";
 * - a zero-count type entry stays **listed**, because an entry that disappears at
 *   zero cannot be switched off again once it has been switched on.
 */
import { describe, expect, it, vi } from "vitest";
import type { AppKind, AppResponse, PageInfo } from "@sdkwork/deployments-pc-console-core/sdk";
import {
  APP_LIST_PAGE_SIZE,
  appMatchesKeyword,
  appMatchesKindFacet,
  countAppKinds,
  filterApps,
  loadAllApps,
  normalizeKeyword,
  orderAppKindFacets,
} from "../src/service/app-list-facets.ts";

/** A row shaped like the contract — every required `AppResponse` field is present. */
function appFixture(name: string, appKind: AppKind, slug = name.toLowerCase()): AppResponse {
  return {
    id: `app-${name}`,
    name,
    slug,
    appKind,
    appStatus: "DRAFT",
    defaultEnvironment: "production",
    ownerType: "USER",
    tenantId: "tenant-1",
    nginxConfigOverridden: false,
    createdAt: "2026-09-23T04:00:00Z",
    updatedAt: "2026-09-23T04:00:00Z",
    version: "1",
  } as AppResponse;
}

function emptyPageInfo(overrides: Partial<PageInfo> = {}): PageInfo {
  return { mode: "offset", page: 1, pageSize: APP_LIST_PAGE_SIZE, hasMore: false, ...overrides }
}

describe("application-type facet ordering", () => {
  it("orders by the rank table and drops nothing", () => {
    expect(orderAppKindFacets(["API_SERVICE", "SPA_WEB", "ANDROID_APP"])).toEqual([
      "SPA_WEB",
      "ANDROID_APP",
      "API_SERVICE",
    ]);
  });

  it("keeps a kind the rank table does not know, at the end", () => {
    // The facet is rendered from the exhaustive label map, so a kind the contract
    // adds arrives here before this repository knows about it. Dropping it would
    // make apps of that kind unfilterable — they would render as a row whose type
    // can never be selected.
    const unknown = "SOMETHING_NEW" as AppKind;
    expect(orderAppKindFacets(["API_SERVICE", unknown, "SPA_WEB"])).toEqual([
      "SPA_WEB",
      "API_SERVICE",
      unknown,
    ]);
  });

  it("keeps the input order for equally ranked kinds", () => {
    const unknownA = "AAA_NEW" as AppKind;
    const unknownB = "BBB_NEW" as AppKind;
    expect(orderAppKindFacets([unknownA, unknownB])).toEqual([unknownA, unknownB]);
  });
});

describe("keyword facet", () => {
  it("trims and case-folds the raw input", () => {
    expect(normalizeKeyword("  Store FRONT  ")).toBe("store front");
  });

  it("matches the name or the slug, case-insensitively", () => {
    const row = appFixture("Store Front", "SPA_WEB", "shop-web");
    expect(appMatchesKeyword(row, normalizeKeyword("STORE"))).toBe(true);
    expect(appMatchesKeyword(row, normalizeKeyword("shop"))).toBe(true);
    expect(appMatchesKeyword(row, ""), "an empty keyword narrows nothing").toBe(true);
    expect(appMatchesKeyword(row, normalizeKeyword("checkout"))).toBe(false);
  });
});

describe("application-type facet", () => {
  it("treats an empty selection as 'every type', not 'no type'", () => {
    const row = appFixture("Store Front", "SPA_WEB");
    expect(appMatchesKindFacet(row, new Set())).toBe(true);
  });

  it("keeps a row whose kind is selected and drops the rest", () => {
    const android = appFixture("Mobile", "ANDROID_APP");
    expect(appMatchesKindFacet(android, new Set<AppKind>(["ANDROID_APP", "SPA_WEB"]))).toBe(true);
    expect(appMatchesKindFacet(android, new Set<AppKind>(["IOS_APP"]))).toBe(false);
  });

  it("lists every kind it was asked about, counting zero for the empty ones", () => {
    const counts = countAppKinds(
      [appFixture("a", "SPA_WEB"), appFixture("b", "SPA_WEB"), appFixture("c", "ANDROID_APP")],
      ["SPA_WEB", "ANDROID_APP", "IOS_APP"],
    );
    expect(counts).toEqual([
      { appKind: "SPA_WEB", count: 2 },
      { appKind: "ANDROID_APP", count: 1 },
      { appKind: "IOS_APP", count: 0 },
    ]);
  });
});

describe("applying both facets at once", () => {
  it("keeps rows matching the selected kinds and the keyword", () => {
    const rows = [
      appFixture("Store H5", "SPA_WEB", "store-h5"),
      appFixture("Store Android", "ANDROID_APP", "store-android"),
      appFixture("Admin Android", "ANDROID_APP", "admin-android"),
    ];
    expect(filterApps(rows, { kinds: new Set(), keyword: "store" }).map((row) => row.name))
      .toEqual(["Store H5", "Store Android"]);
    expect(filterApps(rows, { kinds: new Set<AppKind>(["ANDROID_APP"]), keyword: "store" })
      .map((row) => row.name)).toEqual(["Store Android"]);
    expect(filterApps(rows, { kinds: new Set(), keyword: "" })).toHaveLength(3);
  });
});

describe("loading the whole ownership scope", () => {
  it("walks pages while the server says there is more", async () => {
    const load = vi.fn(async (page: number) => ({
      items: [appFixture(`app-${page}`, "SPA_WEB")],
      pageInfo: emptyPageInfo({ page, hasMore: page < 3 }),
    }));

    const snapshot = await loadAllApps(load, { pageSize: 1 });

    expect(snapshot.items.map((row) => row.name)).toEqual(["app-1", "app-2", "app-3"]);
    expect(snapshot.truncated).toBe(false);
    expect(load).toHaveBeenCalledTimes(3);
  });

  it("stops on a short page when the envelope carries no `hasMore`", async () => {
    const load = vi.fn(async (page: number) => ({
      items: page === 1 ? [appFixture("a", "SPA_WEB"), appFixture("b", "SPA_WEB")] : [],
      pageInfo: { mode: "offset" as const, page, pageSize: 3 },
    }));

    const snapshot = await loadAllApps(load, { pageSize: 3 });

    expect(snapshot.items).toHaveLength(2);
    expect(snapshot.truncated).toBe(false);
    expect(load, "a short page is the last page").toHaveBeenCalledTimes(1);
  });

  it("stops on an empty page even when the server claims there is more", async () => {
    // The guard that has to survive every other signal being wrong: a server that
    // keeps answering `hasMore: true` with nothing in it must not spin forever.
    const load = vi.fn(async () => ({ items: [], pageInfo: emptyPageInfo({ hasMore: true }) }));

    const snapshot = await loadAllApps(load, { pageSize: 10 });

    expect(snapshot.items).toEqual([]);
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("reports truncation instead of walking forever", async () => {
    const load = vi.fn(async (page: number) => ({
      items: [appFixture(`app-${page}`, "SPA_WEB")],
      pageInfo: emptyPageInfo({ page, hasMore: true }),
    }));

    const snapshot = await loadAllApps(load, { pageSize: 1, maxPages: 4 });

    expect(snapshot.items).toHaveLength(4);
    expect(snapshot.truncated, "the ceiling bit, and the page says so").toBe(true);
    expect(load).toHaveBeenCalledTimes(4);
  });

  it("asks for the contract's own page ceiling by default", () => {
    // `page_size` tops out at 200 in the served contract; asking for more would be
    // a 422, not a bigger page.
    expect(APP_LIST_PAGE_SIZE).toBe(200);
  });
});
