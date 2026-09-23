// @vitest-environment jsdom
/**
 * Page-level tests for the applications ledger's ownership face.
 *
 * The host gate (`sdkwork-webserver`'s `applications-operations-column.test.tsx`)
 * asserts that these columns *render* through the bridge; this file asserts the
 * half the host cannot see — that the facet selection reaches the client as a
 * request parameter. Both halves are needed: the columns can render from a stub
 * row while the filter never leaves the browser, which is exactly how a windowed
 * ledger ends up "filtering" only the page it happens to have loaded.
 *
 * The client is a stub rather than a `fetch` double on purpose: what is under
 * test is the page's own contract with the service, so the assertion names the
 * parameter object instead of re-parsing a query string.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppResponse, SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import { PublishingAppsPage } from "../src/components/PublishingAppsPage.tsx";

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

let container: HTMLDivElement | undefined;
let root: Root | undefined;

/**
 * Mount the ledger and let its list request settle.
 *
 * The page loads its rows in an effect, so the first paint has no rows at all;
 * flushing inside `act` is what makes the assertions see the real table.
 */
async function mountLedger(row: AppResponse = appFixture()): Promise<{
  container: HTMLElement
  list: ReturnType<typeof vi.fn>
}> {
  const list = vi.fn(async () => ({
    items: [row],
    pageInfo: { mode: "offset" as const, page: 1, pageSize: 50, hasMore: false },
  }));
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => {
    root?.render(
      <PublishingAppsPage
        deployClient={{ app: { list } } as unknown as SdkworkDeployAppClient}
        driveClient={{} as SdkworkDriveAppClient}
        locale="en-US"
      />,
    );
  });
  return { container, list };
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

/** The one ownership facet control, found by its accessible name. */
function ownershipSelect(scope: ParentNode): HTMLSelectElement {
  const select = scope.querySelector<HTMLSelectElement>('select[aria-label="Ownership"]');
  expect(select, "the ownership facet control is rendered").toBeTruthy();
  return select as HTMLSelectElement;
}

/** Pick an option the way a browser does, so React's own change handling runs. */
async function choose(select: HTMLSelectElement, value: string): Promise<void> {
  await act(async () => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

describe("applications ledger ownership face", () => {
  it("shows the level badge and the owner subject of each row", async () => {
    const { container: scope } = await mountLedger();
    const row = scope.querySelector("tbody tr") as HTMLElement;

    expect(row.textContent).toContain("Platform app");
    // Platform-wide ownership has no single subject, so the owner column names the
    // level instead of repeating the badge sitting next to it.
    expect(row.textContent).toContain("Whole platform");
  });

  it("renders the owner id the server resolved for a personal app", async () => {
    const { container: scope } = await mountLedger(
      appFixture({ ownerType: "USER", ownerUserId: "user-42", ownerId: "user-42" }),
    );
    const row = scope.querySelector("tbody tr") as HTMLElement;

    expect(row.textContent).toContain("Personal app");
    expect(row.querySelector("code")?.textContent).toBe("user-42");
  });

  it("asks the server for the level the operator picked", async () => {
    const { container: scope, list } = await mountLedger();

    expect(listParams(list)[0], "the first load carries no facet").not.toHaveProperty("scope");

    await choose(ownershipSelect(scope), "USER");

    expect(listParams(list).at(-1)).toEqual({ page: 1, pageSize: 50, scope: "USER" });
  });

  it("offers every ownership level the contract defines, plus the unfiltered option", async () => {
    const { container: scope } = await mountLedger();
    const options = [...ownershipSelect(scope).options];

    // The options are derived from the exhaustive `AppOwnerType` label map, so a
    // level added to the contract shows up here without touching the page — and a
    // level missing from this list is a wiring defect, not a styling choice.
    expect(options.map((option) => option.textContent)).toEqual([
      "All ownership levels",
      "Platform app",
      "Shared app",
      "Organization app",
      "Personal app",
    ]);
  });
});
