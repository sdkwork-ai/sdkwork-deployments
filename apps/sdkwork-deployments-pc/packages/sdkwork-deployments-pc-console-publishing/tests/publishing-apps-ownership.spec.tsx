// @vitest-environment jsdom
/**
 * Page-level tests for the applications ledger's **ownership** face.
 *
 * Ownership is the one axis on which the two surfaces of this page disagree, and
 * `surface` is what says which one is rendering:
 *
 * - the **admin** surface reaches every ownership level, so it gets the axis as a
 *   single-select tab row, and that facet is applied **by the server** — a tab is
 *   pushed down as `apps.list`'s `scope`. It has to be server-side: the row set
 *   really changes, and a local filter over one page would return different rows
 *   after turning a page.
 * - the **console** surface reaches one level (the caller's own apps), so it gets
 *   **no control at all** rather than a selector with one answer. The tab row must
 *   not come back there, and neither may a `scope` reach the request.
 *
 * What must not be lost with the control is the *information*: `apps.list` is
 * tenant-wide, so without the two columns a platform-operated app, a tenant's
 * shared app and one person's own app are indistinguishable in the ledger. The
 * host gate (`sdkwork-webserver`'s `applications-operations-column.test.tsx`)
 * asserts that the columns render through the bridge; this file asserts the
 * values the columns actually carry, and which surface gets a control for them.
 *
 * The client is a stub rather than a `fetch` double on purpose: what is under
 * test is the page's own contract with the service, so the assertion names the
 * parameter object instead of re-parsing a query string.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppOwnerType, AppResponse, SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import { PublishingAppsPage } from "../src/components/PublishingAppsPage.tsx";
import { APP_LIST_PAGE_SIZE } from "../src/service/app-list-facets.ts";

declare global {
  // eslint-disable-next-line no-var
  var IS_REACT_ACT_ENVIRONMENT: boolean | undefined;
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

/** A row shaped like the contract — every required `AppResponse` field is present. */
function appFixture(overrides: Partial<AppResponse> = {}): AppResponse {
  return {
    id: "app-1",
    name: "Store Front",
    slug: "store-front",
    appKind: "SPA_WEB",
    appStatus: "DRAFT",
    ownerType: "PLATFORM",
    tenantId: "tenant-1",
    nginxConfigOverridden: false,
    defaultEnvironment: "production",
    description: "",
    createdAt: "2026-09-23T04:00:00Z",
    updatedAt: "2026-09-23T04:00:00Z",
    version: "1",
    ...overrides,
  } as AppResponse;
}

/** One row per ownership level, so a `scope` filter has something to remove. */
const ROWS: readonly AppResponse[] = [
  appFixture({ id: "app-platform", name: "Platform Portal", ownerType: "PLATFORM" }),
  appFixture({ id: "app-personal", name: "My Notes", ownerType: "USER", ownerUserId: "user-42", ownerId: "user-42" }),
];

let container: HTMLDivElement | undefined;
let root: Root | undefined;

/**
 * Mount the ledger and let its list request settle.
 *
 * The stub is **scope-aware**: it answers only the rows whose `owner_type` matches
 * the requested `scope`, which is what makes "the tab reaches the server" a real
 * assertion rather than a reading of the parameter object alone — the rendered
 * rows have to change too.
 *
 * `surface` is omitted rather than defaulted when a test does not pass one, so
 * the page's own default (`console`) is the thing under test on that path.
 */
async function mountLedger({ surface, rows = ROWS }: {
  surface?: "admin" | "console"
  rows?: readonly AppResponse[]
} = {}): Promise<{
  container: HTMLElement
  list: ReturnType<typeof vi.fn>
}> {
  const list = vi.fn(async (params: { scope?: AppOwnerType } = {}) => ({
    items: rows.filter((row) => params.scope === undefined || row.ownerType === params.scope),
    pageInfo: { mode: "offset" as const, page: 1, pageSize: APP_LIST_PAGE_SIZE, hasMore: false },
  }));
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  const scope = container;
  await act(async () => {
    root?.render(
      <PublishingAppsPage
        deployClient={{ app: { list } } as unknown as SdkworkDeployAppClient}
        driveClient={{} as SdkworkDriveAppClient}
        locale="en-US"
        {...(surface === undefined ? {} : { surface })}
      />,
    );
  });
  return { container: scope, list };
}

afterEach(() => {
  if (root) act(() => { root?.unmount() });
  container?.remove();
  container = undefined;
  root = undefined;
});

/** The list parameter object of every call, in order. */
function listParams(list: ReturnType<typeof vi.fn>): Record<string, unknown>[] {
  return list.mock.calls.map(([params]) => params as Record<string, unknown>);
}

/** The last list parameter object — i.e. what the page is asking for now. */
function lastParams(list: ReturnType<typeof vi.fn>): Record<string, unknown> {
  return listParams(list)[listParams(list).length - 1] as Record<string, unknown>;
}

/** The names of the rows the table is actually showing. */
function renderedNames(scope: ParentNode): string[] {
  return [...scope.querySelectorAll("tbody tr")]
    .map((row) => row.querySelector("strong")?.textContent ?? "")
    .filter((name) => name !== "");
}

function ownerTabs(scope: ParentNode): HTMLElement[] {
  return [...scope.querySelectorAll<HTMLElement>(".apps-owner-tab")];
}

/** Click the ownership tab pinned to `ownerScope` (the all-levels tab is `""`). */
async function selectTab(scope: ParentNode, ownerScope: string): Promise<void> {
  const tab = ownerTabs(scope).find((candidate) => candidate.dataset.ownerScope === ownerScope);
  expect(tab, `the ownership tab for ${JSON.stringify(ownerScope)} is rendered`).toBeTruthy();
  await act(async () => { (tab as HTMLElement).click() });
}

function clearButton(scope: ParentNode): HTMLButtonElement | undefined {
  return [...scope.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.className.includes("apps-facet-clear"));
}

describe("applications ledger ownership face — both surfaces", () => {
  it("shows the level badge and the owner subject of each row", async () => {
    const { container: scope } = await mountLedger({ rows: [appFixture()] });
    const row = scope.querySelector("tbody tr") as HTMLElement;

    expect(row.textContent).toContain("Platform app");
    // Platform-wide ownership has no single subject, so the owner column names the
    // level instead of repeating the badge sitting next to it.
    expect(row.textContent).toContain("Whole platform");
  });

  it("renders the owner id the server resolved for a personal app", async () => {
    const { container: scope } = await mountLedger(
      { rows: [appFixture({ ownerType: "USER", ownerUserId: "user-42", ownerId: "user-42" })] },
    );
    const row = scope.querySelector("tbody tr") as HTMLElement;

    expect(row.textContent).toContain("Personal app");
    expect(row.querySelector("code")?.textContent).toBe("user-42");
  });
});

describe("applications ledger ownership face — console", () => {
  it("leaves reach to the server, and decides nothing about it in the browser", async () => {
    const { container: scope, list } = await mountLedger();

    // Two independent facts, and both have to hold. The page sends no `scope`, so
    // the reachable set is whatever the server's ownership gate answers — a build
    // cannot narrow or widen it locally. And it exposes no ownership control, so
    // there is no second path that could start filtering reach client-side.
    expect(lastParams(list)).not.toHaveProperty("scope");
    expect(scope.querySelector('[role="tablist"]')).toBeNull();
    expect(scope.querySelector(".apps-owner-tabs")).toBeNull();
    expect(scope.querySelector(".apps-owner-facets")).toBeNull();
    expect(scope.querySelector("select")).toBeNull();
  });

  it("is what an omitted `surface` means, so a console host passes nothing", async () => {
    // The default is the narrower surface: this mount passes no `surface` at all,
    // which is exactly what the console bridge in sdkwork-webserver does.
    const { container: scope, list } = await mountLedger();
    expect(ownerTabs(scope)).toHaveLength(0);
    expect(lastParams(list)).not.toHaveProperty("scope");
  });
});

describe("applications ledger ownership face — admin", () => {
  it("offers every ownership level as a tab row above the table", async () => {
    const { container: scope } = await mountLedger({ surface: "admin" });

    const tablist = scope.querySelector('[role="tablist"]');
    expect(tablist, "the admin surface gets the ownership axis as a control").toBeTruthy();
    // All levels first, then the contract's four levels in its own declaration
    // order — the label map is the source, so a level the contract adds appears
    // here without anyone editing a list.
    expect(ownerTabs(scope).map((tab) => tab.dataset.ownerScope)).toEqual(["", "PLATFORM", "TENANT", "ORGANIZATION", "USER"]);
    expect(ownerTabs(scope).map((tab) => tab.textContent)).toEqual([
      "All ownership levels",
      "Platform app",
      "Shared app",
      "Organization app",
      "Personal app",
    ]);
    // Exactly one tab is in effect, and on first paint it is the unfiltered one.
    expect(ownerTabs(scope).filter((tab) => tab.getAttribute("aria-selected") === "true")).toHaveLength(1);
    expect(ownerTabs(scope)[0]?.getAttribute("aria-selected")).toBe("true");

    // Placement is the point of this page's toolbar: the control sits directly
    // above the rows it narrows.
    expect(
      (tablist as HTMLElement).compareDocumentPosition(scope.querySelector("table") as HTMLElement)
        & Node.DOCUMENT_POSITION_FOLLOWING,
      "the ownership tabs precede the table",
    ).toBeTruthy();
  });

  it("puts the ownership control and the type chips on one toolbar row", async () => {
    // The three facets narrow three orthogonal axes (ownership / type / keyword), so
    // they share one row. Stacked, the lower row reads as if it were a child of the
    // upper one — which is exactly the misreading this asserts against.
    const { container: scope } = await mountLedger({ surface: "admin" });

    const toolbars = scope.querySelectorAll(".apps-ledger-toolbar");
    expect(toolbars, "both facets share a single toolbar row").toHaveLength(1);

    const tablist = scope.querySelector('[role="tablist"]') as HTMLElement;
    const chips = scope.querySelector(".apps-kind-facets") as HTMLElement;
    expect(tablist.closest(".apps-ledger-toolbar"), "the ownership control is in that row")
      .toBe(chips.closest(".apps-ledger-toolbar"));

    // The label travels inside the strip's own group. Left as a sibling it would be
    // stranded on the previous line the moment the toolbar wraps, naming a control
    // that moved away from it.
    const group = tablist.closest(".apps-owner-facets");
    expect(group, "the label and the strip share a group").toBeTruthy();
    expect(group?.querySelector(".apps-facet-label")?.id).toBe(tablist.getAttribute("aria-labelledby"));
  });

  it("pushes the chosen level down as `scope` rather than filtering locally", async () => {
    const { container: scope, list } = await mountLedger({ surface: "admin" });
    expect(renderedNames(scope)).toEqual(["Platform Portal", "My Notes"]);

    await selectTab(scope, "PLATFORM");

    // The request carries the level, and it is re-read from page 1 — the tab
    // changes the row set, so the table must not stay on a page the new set may
    // not have.
    expect(lastParams(list)).toEqual({ page: 1, pageSize: APP_LIST_PAGE_SIZE, scope: "PLATFORM" });
    // …and the rows really came back narrowed by the server, not by the browser.
    expect(renderedNames(scope)).toEqual(["Platform Portal"]);
    expect(scope.textContent).toContain("Showing 1 of 1 applications");
  });

  it("goes back to the unfiltered scope when the all-levels tab is chosen again", async () => {
    const { container: scope, list } = await mountLedger({ surface: "admin" });

    await selectTab(scope, "USER");
    expect(lastParams(list)).toHaveProperty("scope", "USER");

    await selectTab(scope, "");
    // Absent, not `scope=` — an omitted `scope` is what "every level the caller can
    // reach" means to the server, and the page must not spell it out itself.
    expect(lastParams(list)).not.toHaveProperty("scope");
    expect(lastParams(list)).toEqual({ page: 1, pageSize: APP_LIST_PAGE_SIZE });
    expect(renderedNames(scope)).toEqual(["Platform Portal", "My Notes"]);
  });

  it("counts the ownership tab as a filter, so an empty level is clearable", async () => {
    // No TENANT-owned rows: picking that level genuinely empties the table, which
    // is a filtered state and not "you have no applications yet".
    const { container: scope, list } = await mountLedger({ surface: "admin" });

    await selectTab(scope, "TENANT");

    expect(renderedNames(scope)).toEqual([]);
    expect(scope.textContent).toContain("No application matches the current filters.");
    expect(scope.textContent).not.toContain("No applications yet");

    const clear = clearButton(scope);
    expect(clear, "the filtered-empty state offers the way out").toBeTruthy();
    await act(async () => { (clear as HTMLButtonElement).click() });

    // "Clear" has to undo the tab too — otherwise it clears the local facets, the
    // table stays empty, and the button that was supposed to fix it disappears.
    expect(lastParams(list)).not.toHaveProperty("scope");
    expect(ownerTabs(scope)[0]?.getAttribute("aria-selected")).toBe("true");
    expect(renderedNames(scope)).toEqual(["Platform Portal", "My Notes"]);
  });

  it("keeps the type facet local while the ownership facet is server-side", async () => {
    // The two facets behave differently and must not be confused: a type chip
    // narrows the loaded rows and sends no request at all, a level tab re-reads
    // them. If a chip ever started sending `scope`, the toolbar would be filtering
    // ownership by accident.
    const { container: scope, list } = await mountLedger({ surface: "admin" });
    const requestsAfterMount = list.mock.calls.length;

    const chip = [...scope.querySelectorAll<HTMLButtonElement>(".apps-kind-chip")]
      .find((candidate) => candidate.dataset.appKind === "SPA_WEB");
    expect(chip, "the SPA_WEB chip is rendered").toBeTruthy();
    await act(async () => { (chip as HTMLButtonElement).click() });

    expect(list.mock.calls.length, "the type facet is a local filter").toBe(requestsAfterMount);
    expect(lastParams(list)).not.toHaveProperty("scope");
  });
});
