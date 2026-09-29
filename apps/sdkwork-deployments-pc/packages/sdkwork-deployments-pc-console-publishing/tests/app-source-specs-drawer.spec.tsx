// @vitest-environment jsdom
/**
 * Render tests for the source-spec drawer.
 *
 * The routing *decisions* are covered by `source-spec-routing.spec.ts`; what is
 * asserted here is the half that only a mounted tree can show — that the two
 * panels reach the screen at all, that the per-client row exposes the dark chip
 * and the fallback footnote (the operator's only signal that a declared default
 * serves nothing), and that picking a new default is dispatched to the service
 * with both versions intact.
 *
 * The service is injected (the drawer accepts one for exactly this reason), so
 * the assertions name the call signature rather than re-parsing a request.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppResponse } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type {
  AppSourceSpecResponse,
  SdkworkDeployAppClient,
} from "@sdkwork/deployments-pc-console-core/sdk";
import { AppSourceSpecsDrawer } from "../src/components/AppSourceSpecsDrawer.tsx";

declare global {
  // eslint-disable-next-line no-var
  var IS_REACT_ACT_ENVIRONMENT: boolean | undefined;
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

function appFixture(overrides: Partial<AppResponse> = {}): AppResponse {
  return {
    id: "app-1",
    name: "Store Front",
    slug: "store-front",
    appKind: "SPA_WEB",
    appStatus: "ACTIVE",
    ownerType: "PLATFORM",
    tenantId: "tenant-1",
    nginxConfigOverridden: false,
    defaultEnvironment: "production",
    description: "",
    createdAt: "2026-09-26T04:00:00Z",
    updatedAt: "2026-09-26T04:00:00Z",
    version: "1",
    ...overrides,
  } as AppResponse;
}

function specFixture(overrides: Partial<AppSourceSpecResponse> & { readonly id: string }): AppSourceSpecResponse {
  return {
    specKey: overrides.id,
    label: overrides.id.toUpperCase(),
    runtimeTarget: "browser",
    clientArchitecture: "react",
    clientClassRoutes: [],
    pathPrefix: "/",
    handler: "SPA",
    indexFiles: ["index.html"],
    isDefault: false,
    priority: 0,
    status: "ACTIVE",
    sourceStatus: "BOUND",
    createdAt: "2026-09-26T04:00:00Z",
    updatedAt: "2026-09-26T04:00:00Z",
    version: "1",
    ...overrides,
  } as AppSourceSpecResponse;
}

interface Harness {
  readonly container: HTMLElement
  readonly listSourceSpecs: ReturnType<typeof vi.fn>
  readonly setClientClassDefault: ReturnType<typeof vi.fn>
  readonly onClose: ReturnType<typeof vi.fn>
}

let container: HTMLDivElement | undefined;
let root: Root | undefined;

async function mountDrawer(specs: readonly AppSourceSpecResponse[]): Promise<Harness> {
  const listSourceSpecs = vi.fn(async () => specs);
  const setClientClassDefault = vi.fn(async () => undefined);
  const onClose = vi.fn();
  const service = {
    listSourceSpecs,
    setClientClassDefault,
    createSourceSpec: vi.fn(),
    updateSourceSpec: vi.fn(),
    deleteSourceSpec: vi.fn(),
    bindSourceSpecSource: vi.fn(),
  };
  // The host element is a local `const` and *also* recorded on the module-level
  // handle used by `afterEach`: reading the module-level one back here would be
  // `HTMLDivElement | undefined` again as soon as a closure can assign it.
  const host = document.createElement("div");
  document.body.append(host);
  container = host;
  root = createRoot(host);
  await act(async () => {
    root?.render(
      <AppSourceSpecsDrawer
        app={appFixture()}
        deployClient={{} as SdkworkDeployAppClient}
        driveClient={{} as SdkworkDriveAppClient}
        locale="en-US"
        onClose={onClose}
        service={service as never}
      />,
    );
  });
  return { container: host, listSourceSpecs, setClientClassDefault, onClose };
}

afterEach(() => {
  if (root) act(() => { root?.unmount() });
  container?.remove();
  container = undefined;
  root = undefined;
});

/** The per-client row for one class, found by the class label it renders. */
function clientRow(scope: ParentNode, label: string): HTMLElement | undefined {
  return [...scope.querySelectorAll<HTMLElement>('[class*="specRouting"]')]
    .filter((node) => node.className.includes("specRoutingRow"))
    .find((node) => node.textContent?.startsWith(label));
}

/** Pick an option the way a browser does, so React's change handling runs. */
async function choose(select: HTMLSelectElement, value: string): Promise<void> {
  await act(async () => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

describe("source-spec drawer", () => {
  const pcAndH5 = [
    specFixture({
      id: "pc",
      label: "PC web",
      clientArchitecture: "react",
      clientClassRoutes: [{ clientClass: "DESKTOP", preference: 0 }],
    }),
    specFixture({
      id: "h5",
      label: "H5",
      clientArchitecture: "react-h5",
      clientClassRoutes: [{ clientClass: "DESKTOP", preference: 1 }],
    }),
  ];

  it("asks the service for the app's specs in the app's own default environment", async () => {
    const harness = await mountDrawer(pcAndH5);
    // `defaultEnvironment` is `production`, and the drawer must use it rather
    // than assume the first entry of its own list.
    expect(harness.listSourceSpecs).toHaveBeenCalledWith("app-1", "production");
  });

  it("renders one routing row per client class the contract defines", async () => {
    const harness = await mountDrawer(pcAndH5);
    const scope = harness.container;
    for (const label of ["Desktop", "Mobile", "Tablet", "TV", "Bot", "Other"]) {
      expect(clientRow(scope, label), `routing row for ${label}`).toBeTruthy();
    }
  });

  it("names the source that answers a client, not just the one declared", async () => {
    const harness = await mountDrawer(pcAndH5);
    const row = clientRow(harness.container, "Desktop");
    // Both ranks are on screen, labelled by what they are — the declared default
    // and the rank behind it.
    expect(row?.textContent).toContain("PC web");
    expect(row?.textContent).toContain("H5");
    expect(row?.textContent).toContain("default");
    expect(row?.textContent).toContain("fallback 1");
  });

  it("flags the rank whose source has nothing uploaded, and says who answers instead", async () => {
    // The canonical case from the routing kernel: PC is declared the Desktop
    // default but has no uploaded source, so H5 answers. Both facts have to be
    // visible or the operator will think PC is serving.
    const harness = await mountDrawer([
      specFixture({
        id: "pc",
        label: "PC web",
        sourceStatus: "EMPTY",
        clientClassRoutes: [{ clientClass: "DESKTOP", preference: 0 }],
      }),
      specFixture({
        id: "h5",
        label: "H5",
        clientClassRoutes: [{ clientClass: "DESKTOP", preference: 1 }],
      }),
    ]);
    const row = clientRow(harness.container, "Desktop");
    expect(row?.textContent).toContain("Nothing serves this client");
    expect(harness.container.textContent).toContain("requests are served by H5");
  });

  it("does not offer the current default as a candidate for itself", async () => {
    const harness = await mountDrawer(pcAndH5);
    const select = clientRow(harness.container, "Desktop")?.querySelector("select");
    expect(select, "the make-default control is rendered").toBeTruthy();
    const options = [...(select as HTMLSelectElement).options].map((option) => option.value);
    expect(options).toEqual(["", "h5"]);
  });

  it("sends a new default to the service with the class and the target spec", async () => {
    const harness = await mountDrawer(pcAndH5);
    const select = clientRow(harness.container, "Desktop")?.querySelector("select") as HTMLSelectElement;

    await choose(select, "h5");

    expect(harness.setClientClassDefault).toHaveBeenCalledTimes(1);
    const [appId, clientClass, specId, rows] = harness.setClientClassDefault.mock.calls[0] as unknown[];
    expect(appId).toBe("app-1");
    expect(clientClass).toBe("DESKTOP");
    expect(specId).toBe("h5");
    // The loaded rows travel with the call: the plan is computed from the state
    // the operator was looking at, so the writes carry that state's versions.
    expect((rows as readonly { id: string }[]).map((row) => row.id).sort()).toEqual(["h5", "pc"]);
  });

  it("lists the declared sources and shows what tells PC and H5 apart", async () => {
    const harness = await mountDrawer(pcAndH5);
    const list = harness.container.querySelector('[class*="specList"]');
    expect(list?.textContent).toContain("PC web");
    expect(list?.textContent).toContain("pc");
    // Both are `browser` runtime targets, so the architecture is the only field
    // that distinguishes the two sources — it has to be on screen, not implied.
    expect(list?.textContent).toContain("React (PC web)");
    expect(list?.textContent).toContain("React (H5)");
    // The count badge sits in the section heading, next to the list.
    expect(harness.container.textContent).toContain("2 declared");
  });

  it("shows the empty state, and still renders the per-client panel", async () => {
    // Zero specs must not collapse the panel into blankness: "no sources yet" and
    // "no client routed" are different statements, and the routing rows are how
    // the operator sees the latter.
    const harness = await mountDrawer([]);
    expect(harness.container.textContent).toContain("No source has been declared for this app yet.");
    expect(clientRow(harness.container, "Desktop")).toBeTruthy();
  });

  it("closes on Escape when no inner panel is open", async () => {
    const harness = await mountDrawer(pcAndH5);
    await act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(harness.onClose).toHaveBeenCalledTimes(1);
  });

  it("keeps Escape from closing two layers at once", async () => {
    // Regression for the stacked-panel case: the editor listens for its own Esc,
    // so the drawer must stand down while one is open. Otherwise a single key
    // press discards the form *and* the drawer behind it.
    const harness = await mountDrawer(pcAndH5);
    const addButton = [...harness.container.querySelectorAll("button")]
      .find((button) => button.textContent?.includes("Declare source"));
    expect(addButton, "the declare-source command is rendered").toBeTruthy();

    await act(async () => { addButton?.click() });
    expect(harness.onClose).not.toHaveBeenCalled();

    await act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(harness.onClose).not.toHaveBeenCalled();
  });

  it("announces itself as a modal dialog named after the resource", async () => {
    const harness = await mountDrawer(pcAndH5);
    const dialog = harness.container.querySelector('[role="dialog"]');
    expect(dialog?.getAttribute("aria-modal")).toBe("true");
    expect(dialog?.getAttribute("aria-label")).toBe("Source specs");
  });
});
