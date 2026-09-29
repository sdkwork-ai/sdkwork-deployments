// @vitest-environment jsdom
/**
 * Page-level tests for the applications ledger's **type** facet.
 *
 * This file mounts the console surface (the default), where the type facet is the
 * only control the toolbar has: ownership reaches a single level there, so a
 * control for it is withheld, and its tests live in
 * `publishing-apps-ownership.spec.tsx` next to the admin contrast. Every property
 * below is one that a plausible rewrite could break while the page still looks
 * right:
 *
 * - the chips are a **multi-select with OR semantics**, so selecting two kinds
 *   has to widen the answer, not intersect it;
 * - an **empty selection means "every type"**, so the first paint must show the
 *   whole scope rather than an empty table;
 * - the counts are taken over the **whole loaded set**, which is what lets a
 *   zero-count kind be dropped from the row without ever stranding a selection;
 * - "there are no applications" and "your filters matched none" are different
 *   states with different copy, and only one of them may offer to create an app;
 * - ownership is a **column, not a control** on this surface — the console
 *   reaches one ownership level, so a selector for it would have a single answer.
 *
 * The client is a stub rather than a `fetch` double on purpose: what is under test
 * is the page's own decisions, so the assertions name the rendered rows and the
 * request parameters instead of re-parsing a query string.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppKind, AppResponse, SdkworkDeployAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/deployments-pc-console-core/sdk";
import { PublishingAppsPage } from "../src/components/PublishingAppsPage.tsx";

declare global {
  // eslint-disable-next-line no-var
  var IS_REACT_ACT_ENVIRONMENT: boolean | undefined;
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

const APP_LIST_PAGE_SIZE = 200;

function appFixture(name: string, appKind: AppKind, slug = name.toLowerCase()): AppResponse {
  return {
    id: `app-${name}`,
    name,
    slug,
    appKind,
    appStatus: "DRAFT",
    defaultEnvironment: "production",
    ownerType: "USER",
    ownerId: "user-42",
    tenantId: "tenant-1",
    nginxConfigOverridden: false,
    description: "",
    createdAt: "2026-09-23T04:00:00Z",
    updatedAt: "2026-09-23T04:00:00Z",
    version: "1",
  } as AppResponse;
}

const ROWS: readonly AppResponse[] = [
  appFixture("Store Front", "SPA_WEB", "store-front"),
  appFixture("Admin Console", "SPA_WEB", "admin-console"),
  appFixture("Pocket Shop", "ANDROID_APP", "pocket-shop"),
];

let container: HTMLDivElement | undefined;
let root: Root | undefined;

afterEach(() => {
  if (root) act(() => { root?.unmount() });
  container?.remove();
  container = undefined;
  root = undefined;
});

/** Mount the ledger with the given rows and let its list request settle. */
async function mountLedger(rows: readonly AppResponse[] = ROWS): Promise<{
  scope: HTMLElement
  list: ReturnType<typeof vi.fn>
}> {
  const list = vi.fn(async () => ({
    items: [...rows],
    pageInfo: {
      mode: "offset" as const,
      page: 1,
      pageSize: APP_LIST_PAGE_SIZE,
      totalItems: String(rows.length),
      hasMore: false,
    },
  }));
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  const mounted = container;
  await act(async () => {
    root?.render(
      <PublishingAppsPage
        deployClient={{ app: { list } } as unknown as SdkworkDeployAppClient}
        driveClient={{} as SdkworkDriveAppClient}
        locale="en-US"
      />,
    );
  });
  return { scope: mounted, list };
}

/** The names of the rows the table is actually showing. */
function renderedNames(scope: ParentNode): string[] {
  return [...scope.querySelectorAll("tbody tr")]
    .map((row) => row.querySelector("strong")?.textContent ?? "")
    .filter((name) => name !== "");
}

/** The type facet's chips, in the order they are rendered. */
function chips(scope: ParentNode): HTMLButtonElement[] {
  return [...scope.querySelectorAll<HTMLButtonElement>(".apps-kind-chip")];
}

function chip(scope: ParentNode, kind: AppKind): HTMLButtonElement {
  const found = chips(scope).find((candidate) => candidate.dataset.appKind === kind);
  expect(found, `the "${kind}" chip is rendered`).toBeTruthy();
  return found as HTMLButtonElement;
}

async function toggle(button: HTMLButtonElement): Promise<void> {
  await act(async () => { button.click() });
}

function clearButton(scope: ParentNode): HTMLButtonElement | undefined {
  return [...scope.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.className.includes("apps-facet-clear"));
}

/**
 * Type into the search box the way a browser does.
 *
 * The value has to be written through the prototype's own setter: React tracks the
 * last value it saw on the node, and assigning `input.value` directly updates that
 * tracker too, so dispatching `input` afterwards looks like "nothing changed" and
 * the page never re-renders. Going through the setter leaves the tracker stale,
 * which is exactly the state a real keystroke produces.
 */
async function search(scope: ParentNode, value: string): Promise<void> {
  const input = scope.querySelector<HTMLInputElement>('input[type="search"]');
  expect(input, "the ledger's search box is rendered").toBeTruthy();
  const node = input as HTMLInputElement;
  const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
  expect(setValue, "the input value setter is reachable").toBeTruthy();
  await act(async () => {
    setValue?.call(node, value)
    node.dispatchEvent(new Event("input", { bubbles: true }))
  });
}

describe("applications ledger type filter", () => {
  it("puts the type filter in the toolbar, above the rows it narrows", async () => {
    const { scope } = await mountLedger();

    const toolbar = scope.querySelector(".apps-ledger-toolbar");
    expect(toolbar, "the ledger has a toolbar").toBeTruthy();
    expect(scope.querySelector(".apps-ledger-rail"), "the left rail is gone").toBeNull();
    expect(
      (toolbar as HTMLElement).compareDocumentPosition(scope.querySelector("table") as HTMLElement)
        & Node.DOCUMENT_POSITION_FOLLOWING,
      "the toolbar precedes the table",
    ).toBeTruthy();
  });

  it("renders one toggle per type that has rows, with its count", async () => {
    const { scope } = await mountLedger();

    // Only kinds with rows: a kind nothing can match is a chip that only ever
    // produces an empty table. The two kinds present, in the facet's own order.
    expect(chips(scope).map((candidate) => candidate.textContent)).toEqual([
      "SPA web application2",
      "Android app1",
    ]);
    expect(chips(scope).every((candidate) => candidate.getAttribute("aria-pressed") === "false"))
      .toBe(true);
    expect(scope.querySelector(".apps-facet-clear"), "no narrowing ⇒ no clear button").toBeNull();
  });

  it("keeps ownership a column rather than a control", async () => {
    const { scope } = await mountLedger();

    // The console reaches one ownership level, so a selector for that axis would
    // have a single answer. The level still has to be legible — it is the row's
    // own badge — but no tab group may come back and spend width on it. (The admin
    // surface *does* get one; that is `surface="admin"`'s whole effect, and it is
    // covered in `publishing-apps-ownership.spec.tsx`.)
    expect(scope.querySelector('[role="tablist"]')).toBeNull();
    expect(scope.querySelector('[role="tab"]')).toBeNull();
    expect(scope.querySelector(".apps-owner-tabs")).toBeNull();
    const row = scope.querySelector("tbody tr") as HTMLElement;
    expect(row.textContent, "the row still names its ownership level").toContain("Personal app");
  });

  it("starts with every type shown, because an empty selection is not an empty answer", async () => {
    const { scope } = await mountLedger();

    expect(renderedNames(scope)).toEqual(["Store Front", "Admin Console", "Pocket Shop"]);
    expect(scope.textContent).toContain("Showing 3 of 3 applications");
  });

  it("narrows to one type, then widens again when a second type is added", async () => {
    const { scope } = await mountLedger();

    await toggle(chip(scope, "ANDROID_APP"));
    expect(chip(scope, "ANDROID_APP").getAttribute("aria-pressed"), "the chip shows its own state")
      .toBe("true");
    expect(renderedNames(scope)).toEqual(["Pocket Shop"]);
    expect(scope.textContent).toContain("Showing 1 of 3 applications");

    // OR, not AND: adding a kind can only add rows back.
    await toggle(chip(scope, "SPA_WEB"));
    expect(renderedNames(scope)).toEqual(["Store Front", "Admin Console", "Pocket Shop"]);
    expect(scope.textContent).toContain("Showing 3 of 3 applications");
  });

  it("explains the H5/PC fold only while the type facet is in use", async () => {
    const { scope } = await mountLedger();

    // The folio: one `app_kind` per app, and the contract folds H5 and PC web into
    // SPA_WEB. Worth saying when someone is looking for an "H5" chip; permanent
    // noise otherwise.
    expect(scope.textContent).not.toContain("cannot be told apart");
    await toggle(chip(scope, "SPA_WEB"));
    expect(scope.textContent).toContain("cannot be told apart");
  });

  it("never lets the keyword strand a selected type", async () => {
    const { scope } = await mountLedger();

    // Counts are taken over the whole loaded set, which the type facet itself never
    // shrinks — so a chip that has been switched on cannot reach zero and vanish,
    // which would leave a narrowing nobody can switch off.
    await toggle(chip(scope, "ANDROID_APP"));
    await search(scope, "no-such-app");

    expect(renderedNames(scope)).toEqual([]);
    expect(chip(scope, "ANDROID_APP").getAttribute("aria-pressed")).toBe("true");
    expect(chip(scope, "ANDROID_APP").textContent).toBe("Android app1");
  });

  it("says the filters matched nothing, and offers to clear them", async () => {
    const { scope } = await mountLedger();

    await search(scope, "no-such-app");

    expect(scope.textContent).toContain("No application matches the current filters.");
    // The create-first state is a lie here: the scope is not empty, the selection is.
    expect(scope.textContent).not.toContain("No applications yet");

    const clear = clearButton(scope);
    expect(clear, "the filtered-empty state offers the way out").toBeTruthy();
    await act(async () => { (clear as HTMLButtonElement).click() });

    expect(renderedNames(scope)).toEqual(["Store Front", "Admin Console", "Pocket Shop"]);
  });

  it("still asks the operator to create the first app when the scope is genuinely empty", async () => {
    const { scope } = await mountLedger([]);

    expect(scope.textContent).toContain("No applications yet");
    expect(scope.textContent).not.toContain("No application matches the current filters.");
    // Nothing to narrow by ⇒ no chips at all, rather than a row of zeros.
    expect(chips(scope)).toHaveLength(0);
  });

  it("fetches the whole scope rather than the first page of it", async () => {
    // The type facet is local, so it can only be honest over the complete set — over
    // one page it would return different answers after turning a page. And it has no
    // server parameter at all: the request carries no `scope`, because reach is the
    // server's decision and this page must not make it in the browser.
    const { list } = await mountLedger();
    expect(list.mock.calls[0]?.[0]).toEqual({ page: 1, pageSize: APP_LIST_PAGE_SIZE });
  });
});
