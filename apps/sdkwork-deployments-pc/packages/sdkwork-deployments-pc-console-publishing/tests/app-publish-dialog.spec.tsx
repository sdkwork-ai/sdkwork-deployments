// @vitest-environment jsdom
/**
 * Render tests for the publish dialog — the two commands behind 「发布」.
 *
 * The dialog exists because the button used to be honest about nothing: its
 * previous target wrote only `deploy_app` metadata through `app.update`, so the
 * ledger could never show the publication the label claimed. What these
 * assertions pin is therefore the *behaviour the label promises*:
 *
 * - **Refusal (`PRD-FR-035`)** — an application without a source version's
 *   artifact (a registered package) must be refused, and the refusal names the
 *   step that clears it rather than leaving a dead button. The same applies when
 *   the kind maps to more than one publishable surface: the platform target
 *   cannot be inferred, so the dialog refuses instead of guessing.
 * - **Two commands (`DEPLOYMENT_SPEC.md` §1.4)** — the platform target a create
 *   step deliberately left empty is written first, then the release is cut, then
 *   the deployment is requested against that release. The deployment must carry
 *   the *target that was just created*, not an empty id.
 * - **Truthful status (`PRD-FR-026`)** — the panel reports the deployment's own
 *   status and never presents command acceptance as a live publication, and a
 *   refused deployment says the release already exists (otherwise the retry
 *   silently cuts a second release for the same package).
 *
 * The service is injected, so assertions name the call signature instead of
 * re-parsing a request.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppResponse } from "@sdkwork/deployments-pc-console-core/sdk";
import { AppPublishDialog } from "../src/components/AppOperationsDialogs.tsx";
import type { DeployAppPublishingService } from "../src/service/deploy-app-publishing.ts";

declare global {
  // eslint-disable-next-line no-var
  var IS_REACT_ACT_ENVIRONMENT: boolean | undefined;
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

function appFixture(appKind: AppResponse["appKind"]): AppResponse {
  return {
    id: "app-1",
    name: "Store Front",
    slug: "store-front",
    appKind,
    appStatus: "ACTIVE",
    ownerType: "PLATFORM",
    tenantId: "tenant-1",
    nginxConfigOverridden: false,
    defaultEnvironment: "test",
    description: "",
    createdAt: "2026-09-27T04:00:00Z",
    updatedAt: "2026-09-27T04:00:00Z",
    version: "1",
  } as AppResponse;
}

/** One registered package, as the upload path leaves it. */
const PACKAGE = { id: "pkg-1", packageFormat: "ZIP", packageStatus: "READY", semanticVersion: "1.2.3" };

interface Harness {
  readonly container: HTMLElement
  readonly publishAppRelease: ReturnType<typeof vi.fn>
  readonly createAppDeployment: ReturnType<typeof vi.fn>
  readonly createPlatformTargetForCard: ReturnType<typeof vi.fn>
  readonly onSaved: ReturnType<typeof vi.fn>
}

let container: HTMLDivElement | undefined;
let root: Root | undefined;

async function mount(options: {
  readonly appKind: AppResponse["appKind"]
  readonly targets?: readonly { readonly id: string; readonly targetKey: string; readonly platform: string }[]
  readonly packages?: readonly (typeof PACKAGE)[]
}): Promise<Harness> {
  // Parameter types are declared on each mock on purpose: an untyped `vi.fn()`
  // types `calls` as an empty tuple, so `calls[0]` stops compiling (TS2493) and
  // the payload assertions silently lose their subject.
  const publishAppRelease = vi.fn(async (_appId: string, _input: Record<string, unknown>) => ({ id: "release-1" }));
  const createAppDeployment = vi.fn(async (_appId: string, _input: Record<string, unknown>) => ({
    id: "deployment-1",
    deploymentStatus: "PENDING",
  }));
  const createPlatformTargetForCard = vi.fn(async (_appId: string, _cardId: string) => ({ id: "target-new" }));
  const onSaved = vi.fn();
  const onClose = vi.fn();
  const service = {
    listPlatformTargets: vi.fn(async () => options.targets ?? []),
    listPackages: vi.fn(async () => options.packages ?? []),
    publishAppRelease,
    createAppDeployment,
    createPlatformTargetForCard,
  };

  const host = document.createElement("div");
  document.body.append(host);
  container = host;
  root = createRoot(host);
  await act(async () => {
    root?.render(
      <AppPublishDialog
        app={appFixture(options.appKind)}
        locale="en-US"
        service={service as unknown as DeployAppPublishingService}
        onClose={onClose}
        onSaved={onSaved}
      />,
    );
  });
  return { container: host, publishAppRelease, createAppDeployment, createPlatformTargetForCard, onSaved };
}

afterEach(() => {
  if (root) act(() => { root?.unmount() });
  container?.remove();
  container = undefined;
  root = undefined;
});

/* ------------------------------------------------------------------ *
 * Queries
 * ------------------------------------------------------------------ */

function button(scope: ParentNode, label: string): HTMLButtonElement | undefined {
  return [...scope.querySelectorAll("button")].find((node) => node.textContent?.trim() === label);
}

function selects(scope: ParentNode): HTMLSelectElement[] {
  return [...scope.querySelectorAll("select")];
}

async function click(target: HTMLElement | undefined, what: string): Promise<void> {
  expect(target, `${what} is rendered`).toBeTruthy();
  await act(async () => { target?.click() });
}

/**
 * Set a controlled `<select>` the way React can see it.
 *
 * Assigning `.value` directly goes through React's patched setter, which updates
 * React's own value tracker too, so the following `change` event reads as "no
 * change" and `onChange` never runs.
 */
async function choose(select: HTMLSelectElement | undefined, value: string, what: string): Promise<void> {
  expect(select, `${what} is rendered`).toBeTruthy();
  await act(async () => {
    if (select === undefined) return;
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

/* ------------------------------------------------------------------ *
 * Refusals
 * ------------------------------------------------------------------ */

describe("AppPublishDialog refusals", () => {
  it("refuses an application with no registered package and issues no command", async () => {
    const harness = await mount({ appKind: "WECHAT_MINIPROGRAM", packages: [] });

    // `PRD-FR-035`: the refusal is stated, and it names the step that clears it.
    expect(harness.container.textContent).toContain("Register the source first");
    expect(button(harness.container, "Publish")?.disabled, "no submit is offered").toBe(true);

    expect(harness.publishAppRelease).not.toHaveBeenCalled();
    expect(harness.createAppDeployment).not.toHaveBeenCalled();
    expect(harness.createPlatformTargetForCard).not.toHaveBeenCalled();
  });

  it("refuses to guess the surface when the kind maps to more than one card", async () => {
    // `SPA_WEB` is both `h5` and `pc-web`; the create step does not record which,
    // and the service contract forbids picking one arbitrarily.
    const harness = await mount({ appKind: "SPA_WEB", packages: [PACKAGE] });

    expect(harness.container.textContent).toContain("more than one publishable surface");
    expect(button(harness.container, "Publish")?.disabled, "no submit is offered").toBe(true);
    expect(harness.createPlatformTargetForCard).not.toHaveBeenCalled();
  });

  it("refuses when the kind maps to no publishable surface at all", async () => {
    const harness = await mount({ appKind: "DOUYIN_MINIPROGRAM", packages: [PACKAGE] });

    expect(harness.container.textContent).toContain("no publishable surface is registered for its kind");
    expect(button(harness.container, "Publish")?.disabled).toBe(true);
    expect(harness.createPlatformTargetForCard).not.toHaveBeenCalled();
  });
});

/* ------------------------------------------------------------------ *
 * The two commands
 * ------------------------------------------------------------------ */

describe("AppPublishDialog submit", () => {
  it("writes the missing platform target, then cuts the release and submits the deployment", async () => {
    const harness = await mount({ appKind: "WECHAT_MINIPROGRAM", packages: [PACKAGE] });

    // The single candidate is named up front, so the operator knows what the
    // dialog is about to create on their behalf.
    expect(harness.container.textContent).toContain("Publishing will create one for its kind");

    // Only the package select exists here — there is no target to choose.
    await choose(selects(harness.container)[0], "pkg-1", "package select");
    const publish = button(harness.container, "Publish");
    expect(publish?.disabled, "a package and its version are enough to submit").toBe(false);

    await click(publish, "publish");

    // `mini-program` card → row `wechat-mini-program`.
    expect(harness.createPlatformTargetForCard).toHaveBeenCalledWith("app-1", "mini-program");
    expect(harness.publishAppRelease).toHaveBeenCalledTimes(1);
    const [, releaseInput] = harness.publishAppRelease.mock.calls[0] as [string, Record<string, unknown>];
    // The release must land on the target that was *just created*, not an empty id.
    expect(releaseInput.platformTargetId).toBe("target-new");
    expect(harness.createAppDeployment).toHaveBeenCalledWith("app-1", {
      releaseId: "release-1",
      platformTargetId: "target-new",
    });
    expect(harness.onSaved).toHaveBeenCalledTimes(1);
  });

  it("reports the deployment's own status rather than claiming a publication", async () => {
    const harness = await mount({ appKind: "WECHAT_MINIPROGRAM", packages: [PACKAGE] });
    await choose(selects(harness.container)[0], "pkg-1", "package select");
    await click(button(harness.container, "Publish"), "publish");

    // `PRD-FR-026`: accepted ≠ live. The status shown is the server's answer.
    const panel = harness.container.querySelector('[data-testid="publish-submitted-status"]');
    expect(panel?.textContent).toContain("PENDING");
    expect(harness.container.textContent).toContain("not live until the execution authority");
    expect(harness.container.textContent).toContain("Release: release-1");
  });

  it("uses an existing platform target without writing another one", async () => {
    const harness = await mount({
      appKind: "WECHAT_MINIPROGRAM",
      targets: [{ id: "target-1", targetKey: "wechat-mini-program", platform: "WECHAT" }],
      packages: [PACKAGE],
    });

    const [targetSelect, packageSelect] = selects(harness.container);
    await choose(targetSelect, "target-1", "target select");
    await choose(packageSelect, "pkg-1", "package select");
    await click(button(harness.container, "Publish"), "publish");

    expect(harness.createPlatformTargetForCard).not.toHaveBeenCalled();
    const [, releaseInput] = harness.publishAppRelease.mock.calls[0] as [string, Record<string, unknown>];
    expect(releaseInput.platformTargetId).toBe("target-1");
  });

  it("says the release already exists when only the deployment is refused", async () => {
    const harness = await mount({ appKind: "WECHAT_MINIPROGRAM", packages: [PACKAGE] });
    harness.createAppDeployment.mockRejectedValueOnce(new Error("target is full"));

    await choose(selects(harness.container)[0], "pkg-1", "package select");
    await click(button(harness.container, "Publish"), "publish");

    // The release is immutable and already cut, so retrying blindly would cut a
    // second one for the same package — the error has to say so.
    expect(harness.container.textContent).toContain("deployment command was refused");
    expect(harness.container.textContent).toContain("target is full");
    expect(harness.onSaved).not.toHaveBeenCalled();
  });
});
