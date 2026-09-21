/**
 * Placement regression: every submit control of the 域名设置 drawer must live in
 * the drawer **chrome** (the pinned save region + footer), never inside the
 * scrolled body.
 *
 * Why this is worth a test of its own. The drawer's body is the only scroll
 * container, and the domain surface is long by nature (custom-domain list,
 * platform-domain form, five-environment preview, current-hostname table). When
 * a submit button sits at the tail of that body, an operator who scrolls up to
 * edit the appId has the save button off-screen and no signal that anything
 * still needs submitting — the change looks saved when it is not. The fix is
 * structural (the button moved out of the body), so the guard has to be
 * structural too: "is this control inside the scroll container" is not
 * observable from a snapshot of the click handler, only from where the node
 * sits in the tree.
 *
 * The negative cases matter as much as the positive one. Earlier drafts of this
 * dialog carried both a platform-domain save row *and* the custom-domain
 * register row inside the body, and the replacement left a duplicate 添加域名
 * button behind in it. A test that only asserts "the save button is in the
 * footer" passes for all of those; asserting the *count* is what catches the
 * leftovers.
 */
// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import type { AppResponse, DomainZoneResponse, SdkworkDeployAppClient } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import { AppDomainDialog } from "../src/components/AppDomainDialog.tsx";
import {
  createDeployAppOperationsService,
  type DeployAppDomain,
  type DeployAppDomainState,
  type DeployAppOperationsService,
} from "../src/service/deploy-app-operations.ts";

declare global {
  // eslint-disable-next-line no-var
  var IS_REACT_ACT_ENVIRONMENT: boolean | undefined;
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

/* ------------------------------------------------------------------ *
 * Fixtures
 * ------------------------------------------------------------------ */

function appFixture(overrides: Partial<AppResponse> = {}): AppResponse {
  return {
    id: "app-1",
    name: "Demo Store",
    slug: "demo-store",
    appKind: "SPA_WEB",
    appStatus: "DRAFT",
    defaultEnvironment: "production",
    appDomainLabel: "demo-store",
    appDomainSuffixes: ["sdkwork.com"],
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    version: "1",
    ...overrides,
  } as AppResponse;
}

/**
 * A loaded state with one platform hostname, so the dialog renders its full
 * long form rather than the loading placeholder.
 *
 * `provisioned` and the hostname's statuses are spelled out rather than cast:
 * the fixture is the contract for "a healthy app whose dialog is fully drawn",
 * and a cast here would let a renamed field turn the fixture into an empty
 * render that still passes the placement assertions for the wrong reason.
 */
function domainStateFixture(): DeployAppDomainState {
  return {
    app: appFixture(),
    domains: [
      {
        hostname: "demo-store.app.sdkwork.com",
        environment: "production",
        kind: "DEFAULT",
        bindingStatus: "ACTIVE",
        verificationStatus: "NOT_REQUIRED",
      } as DeployAppDomain,
    ],
    provisioned: true,
  };
}

/** Stub service: only the reads the dialog performs on mount matter by default. */
function stubService(overrides: Partial<DeployAppOperationsService> = {}): DeployAppOperationsService {
  return {
    loadDomainState: vi.fn(async () => domainStateFixture()),
    listDomainZones: vi.fn(async (): Promise<readonly DomainZoneResponse[]> => []),
    saveDomainConfig: vi.fn(async () => appFixture()),
    bindCustomHostname: vi.fn(async () => ({ hostname: { hostname: "shop.example.com" } })),
    replaceDomainBindings: vi.fn(async () => undefined),
    ...overrides,
  } as unknown as DeployAppOperationsService;
}

/* ------------------------------------------------------------------ *
 * Render harness
 * ------------------------------------------------------------------ */

let container: HTMLDivElement | undefined;
let root: Root | undefined;

/**
 * Render the dialog and let its mount effects settle.
 *
 * The dialog loads its state in an effect, so the first paint is the loading
 * placeholder and the buttons under test do not exist yet; flushing inside
 * `act` is what makes the assertions see the real form.
 */
async function renderDialog(service: DeployAppOperationsService): Promise<HTMLElement> {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  const deployClient = {} as SdkworkDeployAppClient;
  const driveClient = {} as SdkworkDriveAppClient;
  await act(async () => {
    root?.render(
      <AppDomainDialog
        deployClient={deployClient}
        driveClient={driveClient}
        locale="zh-CN"
        app={appFixture()}
        onClose={() => {}}
        service={service}
      />,
    );
  });
  return container;
}

afterEach(() => {
  if (root) act(() => { root?.unmount() });
  container?.remove();
  container = undefined;
  root = undefined;
});

/* ------------------------------------------------------------------ *
 * Helpers
 * ------------------------------------------------------------------ */

/**
 * Buttons whose accessible name matches, searched over the whole subtree.
 *
 * `textContent` is used rather than a testing-library query on purpose: the
 * point of this spec is the DOM *shape*, and pulling in a query library to
 * re-implement "find a button by label" would add a dependency for nothing.
 */
function buttonsNamed(scope: ParentNode, ...labels: readonly string[]): HTMLButtonElement[] {
  return [...scope.querySelectorAll("button")].filter((button) =>
    labels.includes((button.textContent ?? "").trim()),
  );
}

/** The drawer's only scroll container: the node carrying `overflow-y: auto`. */
function scrolledBody(host: HTMLElement): HTMLElement {
  const body = host.querySelector<HTMLElement>('[class*="body"]');
  if (!body) throw new Error("drawer body not found");
  return body;
}

/** Everything that is not the scrolled body — the pinned chrome. */
function chromeText(host: HTMLElement, labels: readonly string[]): HTMLButtonElement[] {
  return buttonsNamed(host, ...labels).filter((button) => !scrolledBody(host).contains(button));
}

/**
 * Type into a controlled React input.
 *
 * Assigning `input.value` directly is not enough: React tracks the last value it
 * wrote on the node itself and skips its `onChange` when the next value matches
 * that tracker, so a raw assignment produces a DOM whose text says one thing and
 * whose state says another. Going through the native setter and then
 * dispatching `input` is what makes React observe the change.
 */
async function typeInto(input: HTMLInputElement, value: string): Promise<void> {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
  await act(async () => {
    setter?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** The form's text inputs, in DOM order. */
function textInputs(host: HTMLElement): HTMLInputElement[] {
  return [...host.querySelectorAll("input")];
}

/* ------------------------------------------------------------------ *
 * Specs
 * ------------------------------------------------------------------ */

describe("AppDomainDialog control placement", () => {
  it("renders the platform-domain save button outside the scrolled body", async () => {
    const host = await renderDialog(stubService());

    expect(buttonsNamed(scrolledBody(host), "保存")).toHaveLength(0);
    expect(chromeText(host, ["保存"])).toHaveLength(1);
  });

  it("renders the custom-domain register button outside the scrolled body", async () => {
    const host = await renderDialog(stubService());

    expect(buttonsNamed(scrolledBody(host), "登记域名")).toHaveLength(0);
    expect(chromeText(host, ["登记域名"])).toHaveLength(1);
  });

  it("leaves no submit control at all inside the scrolled body", async () => {
    const host = await renderDialog(stubService());

    // The body is allowed to carry sight-only and inline-edit affordances
    // (复制, 删除, 添加域名); it must carry no *submit* affordance.
    expect(buttonsNamed(scrolledBody(host), "保存", "保存中…", "登记域名", "登记中…")).toHaveLength(0);
  })

  it("keeps 添加域名 to a single instance, in the chrome", async () => {
    const host = await renderDialog(stubService());

    // A duplicate of this one is the easiest mistake to make while moving the
    // save row, and it is invisible to a "is the save button pinned" check.
    expect(buttonsNamed(host, "添加域名")).toHaveLength(1);
    expect(chromeText(host, ["添加域名"])).toHaveLength(1);
  })
})

describe("AppDomainDialog footer state", () => {
  it("disables save before any field is touched and says why", async () => {
    const host = await renderDialog(stubService());
    const save = chromeText(host, ["保存"])[0];

    expect(save?.disabled).toBe(true);
    // A disabled control with no stated reason is the accessibility defect this
    // hint exists to prevent.
    expect(host.textContent).toContain("暂无未保存的改动");
  })

  it("enables save and reports unsaved changes once the appId is edited", async () => {
    const host = await renderDialog(stubService());
    const appIdInput = textInputs(host).find((input) => input.value === "demo-store");
    expect(appIdInput).toBeDefined();

    await typeInto(appIdInput!, "demo-store-next");

    const save = chromeText(host, ["保存"])[0];
    expect(save?.disabled).toBe(false);
    expect(host.textContent).toContain("有未保存的改动");
  })

  it("submits the edited appId through the pinned save button", async () => {
    const saveDomainConfig = vi.fn(async () => appFixture());
    const host = await renderDialog(stubService({ saveDomainConfig }));
    const appIdInput = textInputs(host).find((input) => input.value === "demo-store");

    await typeInto(appIdInput!, "demo-store-next");
    await act(async () => {
      chromeText(host, ["保存"])[0]?.click();
    })

    // The pinned button must be wired to the same handler the in-body one was,
    // not merely relocated.
    expect(saveDomainConfig).toHaveBeenCalledWith("app-1", { appDomainLabel: "demo-store-next" });
  })
})

describe("createDeployAppOperationsService is the injected seam", () => {
  it("is reachable so the dialog can be driven without a live gateway", () => {
    // Guards the harness itself: if this factory ever stops being exported the
    // specs above would be the only signal, and they fail for a confusing
    // reason.
    expect(typeof createDeployAppOperationsService).toBe("function");
  })
})
