// @vitest-environment jsdom
/**
 * Render tests for the source-intake dialog.
 *
 * The dialog has two actions over one app, and the assertions are grouped by the
 * thing that is easy to get wrong in each:
 *
 * - **Attach** — the contract's source union has two branches and the Drive one
 *   needs a *folder*, so what is asserted is the exact request body
 *   (`DRIVE_DIRECTORY` + folder selector + content mode, or `KNOWLEDGEBASE_WIKI`
 *   + publication uuid), plus that a *file* is refused rather than smuggled
 *   through. This is also where the spec the operator picked has to survive all
 *   the way into the call: the whole point of the refactor is that a source is
 *   attached *to a spec*, so a request that lost the spec id would be a silent
 *   regression back to the app-level model.
 * - **Artifact** — the app-level path is deliberately *not* spec-scoped, and the
 *   assertion is that it stays that way (no spec in the call) so nobody
 *   "helpfully" adds one later.
 *
 * The service is injected, so the assertions name the call signature instead of
 * re-parsing a request, and the Drive client is a stub that serves one space with
 * a handful of entries — enough for the real `DriveNodePickerDialog` to walk.
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
import { UploadSourceDialog } from "../src/components/UploadSourceDialog.tsx";

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
    // Not `production`: the point of the assertion is that the dialog reads the
    // app's *own* environment rather than assuming one.
    defaultEnvironment: "test",
    description: "",
    createdAt: "2026-09-27T04:00:00Z",
    updatedAt: "2026-09-27T04:00:00Z",
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
    clientClassRoutes: [{ clientClass: "DESKTOP", preference: 0 }],
    pathPrefix: "/",
    handler: "SPA",
    indexFiles: ["index.html"],
    isDefault: false,
    priority: 0,
    status: "ACTIVE",
    sourceStatus: "EMPTY",
    createdAt: "2026-09-27T04:00:00Z",
    updatedAt: "2026-09-27T04:00:00Z",
    version: "1",
    ...overrides,
  } as AppSourceSpecResponse;
}

interface DriveEntry {
  readonly id: string
  readonly nodeName: string
  readonly nodeType: "file" | "folder"
  readonly contentLength?: string
  readonly contentType?: string
}

/**
 * A Drive client that serves exactly one space and one level of entries.
 *
 * The picker's real contract is `spaces.list` → `nodes.list(spaceId)`, and it
 * calls them on open, so the stub has to honour both shapes rather than only the
 * one the dialog itself calls.
 */
function driveClientStub(entries: readonly DriveEntry[]): SdkworkDriveAppClient {
  return {
    drive: {
      spaces: {
        list: vi.fn(async () => ({
          items: [{ id: "space-1", displayName: "Team drive" }],
          pageInfo: {},
        })),
      },
      nodes: {
        list: vi.fn(async () => ({
          items: entries.map((entry) => ({
            id: entry.id,
            nodeName: entry.nodeName,
            nodeType: entry.nodeType,
            contentLength: entry.contentLength ?? "0",
            contentType: entry.contentType,
            updatedAt: "2026-09-27T04:00:00Z",
          })),
          pageInfo: {},
        })),
      },
    },
  } as unknown as SdkworkDriveAppClient;
}

interface Harness {
  readonly container: HTMLElement
  readonly listSourceSpecs: ReturnType<typeof vi.fn>
  readonly bindSourceSpecSource: ReturnType<typeof vi.fn>
  readonly uploadCodeFromArchive: ReturnType<typeof vi.fn>
  readonly connectGitSource: ReturnType<typeof vi.fn>
  readonly onUploaded: ReturnType<typeof vi.fn>
  readonly onRequestSpecs: ReturnType<typeof vi.fn>
  readonly onClose: ReturnType<typeof vi.fn>
}

let container: HTMLDivElement | undefined;
let root: Root | undefined;

async function mountDialog(options: {
  readonly specs?: readonly AppSourceSpecResponse[]
  readonly entries?: readonly DriveEntry[]
  readonly presetSpecId?: string
  readonly withRequestSpecs?: boolean
} = {}): Promise<Harness> {
  const specs = options.specs ?? [specFixture({ id: "web-pc", label: "PC web" })]
  const listSourceSpecs = vi.fn(async () => specs)
  const bindSourceSpecSource = vi.fn(async (_appId: string, specId: string) => {
    const found = specs.find((spec) => spec.id === specId)
    return { ...(found ?? specs[0]), specKey: found?.specKey ?? "web-pc" }
  })
  const uploadCodeFromArchive = vi.fn(async () => ({ artifactId: "artifact-1" }))
  const connectGitSource = vi.fn()
  const onUploaded = vi.fn()
  const onRequestSpecs = vi.fn()
  const onClose = vi.fn()
  const service = {
    listSourceSpecs,
    bindSourceSpecSource,
    uploadCodeFromArchive,
    connectGitSource,
    archiveChecksum: vi.fn(async () => "0".repeat(64)),
  }

  const host = document.createElement("div")
  document.body.append(host)
  container = host
  root = createRoot(host)
  await act(async () => {
    root?.render(
      <UploadSourceDialog
        app={appFixture()}
        deployClient={{} as SdkworkDeployAppClient}
        driveClient={driveClientStub(options.entries ?? [{ id: "node-1", nodeName: "pc-dist", nodeType: "folder" }])}
        locale="en-US"
        onClose={onClose}
        onRequestSpecs={options.withRequestSpecs === true ? onRequestSpecs : undefined}
        onUploaded={onUploaded}
        presetSpecId={options.presetSpecId}
        service={service as never}
      />,
    )
  })
  return {
    container: host,
    listSourceSpecs,
    bindSourceSpecSource,
    uploadCodeFromArchive,
    connectGitSource,
    onUploaded,
    onRequestSpecs,
    onClose,
  }
}

afterEach(() => {
  if (root) act(() => { root?.unmount() })
  container?.remove()
  container = undefined
  root = undefined
})

/* ------------------------------------------------------------------ *
 * Queries
 * ------------------------------------------------------------------ */

function button(scope: ParentNode, label: string): HTMLButtonElement | undefined {
  return [...scope.querySelectorAll("button")].find((node) => node.textContent?.trim() === label)
}

/**
 * The target-spec picker.
 *
 * Named rather than "the first select on screen": the source step renders its own
 * content-mode select, so a positional lookup silently starts asserting against
 * the wrong control once the source step becomes reachable.
 */
function specSelect(scope: ParentNode): HTMLSelectElement | undefined {
  return [...scope.querySelectorAll("select")]
    .find((select) => select.getAttribute("aria-label") === "Target spec")
}

function radioFor(scope: ParentNode, label: string): HTMLInputElement | undefined {
  return [...scope.querySelectorAll<HTMLInputElement>('input[type="radio"]')]
    .find((input) => input.closest("label")?.textContent?.includes(label))
}

async function click(target: HTMLElement | undefined, what: string): Promise<void> {
  expect(target, `${what} is rendered`).toBeTruthy()
  await act(async () => { target?.click() })
}

/**
 * Type into a controlled text input the way React can see it.
 *
 * Assigning `input.value` directly goes through React's *patched* setter, which
 * updates React's value tracker too — so the subsequent `input` event looks like
 * "no change" and `onChange` never runs, leaving the field's state (and therefore
 * every downstream `disabled`) one step behind. Calling the *prototype* setter
 * bypasses the patch, which is what a real keystroke does.
 */
async function typeInto(input: HTMLInputElement | null, value: string): Promise<void> {
  expect(input, "the text input is rendered").toBeTruthy()
  await act(async () => {
    if (input === null) return
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set
    setter?.call(input, value)
    input.dispatchEvent(new Event("input", { bubbles: true }))
  })
}

async function choose(select: HTMLSelectElement | undefined, value: string, what: string): Promise<void> {
  expect(select, `${what} is rendered`).toBeTruthy()
  await act(async () => {
    if (select === undefined) return
    select.value = value
    select.dispatchEvent(new Event("change", { bubbles: true }))
  })
}

/**
 * Walk the real picker: open the space, select the entry by name, confirm.
 *
 * `expect` failures here are meaningful on their own — if the picker stops
 * rendering a step, the attach flow is broken rather than merely untested.
 */
async function pickInDrive(scope: ParentNode, entryName: string): Promise<void> {
  await click(button(scope, "Pick a folder in Drive"), "the Drive pick button")
  await act(async () => {
    // The picker fetches spaces on mount; the stub resolves synchronously inside
    // the act so the space row is on screen by the time we look for it.
  })
  const spaceRow = [...scope.querySelectorAll("button")].find((node) => node.textContent?.includes("Team drive"))
  await click(spaceRow as HTMLButtonElement | undefined, "the Drive space row")
  const entryRow = [...scope.querySelectorAll("button")].find((node) => node.textContent?.includes(entryName))
  await click(entryRow as HTMLButtonElement | undefined, `the Drive entry ${entryName}`)
  await click(button(scope, "Use selection"), "the picker's confirm button")
}

/* ------------------------------------------------------------------ *
 * Attach a source
 * ------------------------------------------------------------------ */

describe("source-intake dialog — attach a source", () => {
  it("opens on the contract-aligned action and reads the app's own environment", async () => {
    const harness = await mountDialog();
    const attach = radioFor(harness.container, "Attach a source")
    expect(attach?.checked, "the attach action is the default").toBe(true);
    expect(harness.listSourceSpecs).toHaveBeenCalledWith("app-1", "test");
  });

  it("asks for the target spec before it asks for the source", async () => {
    // The whole point of the refactor: without a target there is nothing to
    // attach *to*, so the source-kind radios must not be reachable yet.
    const harness = await mountDialog();
    expect(harness.container.textContent).toContain("1 · Target spec");
    expect(harness.container.textContent).not.toContain("2 · Source");

    await choose(specSelect(harness.container), "web-pc", "the target spec select");
    expect(harness.container.textContent).toContain("2 · Source");
    expect(radioFor(harness.container, "Drive directory")?.checked).toBe(true);
  });

  it("locks the target spec when the entry point names one", async () => {
    // Entered from the spec row, so the spec is a fact rather than a choice:
    // a select that still offered other specs would invite "clicked A, attached B".
    // Asserted on the *options* rather than on "no select at all" — the source
    // step still renders its own content-mode select, which is not a spec picker.
    const harness = await mountDialog({
      specs: [
        specFixture({ id: "web-pc", label: "PC web" }),
        specFixture({ id: "web-h5", label: "H5", clientArchitecture: "react-h5" }),
      ],
      presetSpecId: "web-pc",
    });
    const specOptions = [...harness.container.querySelectorAll("option")].map((option) => option.value);
    expect(specOptions).not.toContain("web-h5");
    expect(harness.container.textContent).toContain("web-pc");
    expect(harness.container.textContent).toContain("fixed by this entry point");
    // The source step is reachable straight away — the spec is already known.
    expect(harness.container.textContent).toContain("2 · Source");
  });

  it("attaches a Drive folder, carrying the folder selector and the content mode", async () => {
    const harness = await mountDialog({ presetSpecId: "web-pc" });
    await pickInDrive(harness.container, "pc-dist");

    // A folder, so the selector is `FOLDER` + its node id — not a space root and
    // definitely not a file.
    expect(harness.container.textContent).toContain("Team drive / pc-dist");

    await click(button(harness.container, "Attach"), "the submit button");

    expect(harness.bindSourceSpecSource).toHaveBeenCalledTimes(1);
    expect(harness.bindSourceSpecSource.mock.calls[0]).toEqual([
      "app-1",
      "web-pc",
      {
        type: "DRIVE_DIRECTORY",
        websiteSpaceId: "space-1",
        root: { mode: "FOLDER", folderNodeId: "node-1" },
        contentMode: "LIVE_TREE",
      },
    ]);
    expect(harness.onUploaded).toHaveBeenCalledWith("Source attached to web-pc.");
  });

  it("refuses a file in the attach flow, because a source is a directory", async () => {
    // The mirror image of the artifact flow's own refusal. A file has no tree to
    // serve, and the contract's `DriveDirectorySource.root` cannot express one,
    // so the selection is dropped rather than coerced.
    const harness = await mountDialog({
      presetSpecId: "web-pc",
      entries: [{ id: "node-9", nodeName: "bundle.zip", nodeType: "file", contentLength: "2048" }],
    });
    await pickInDrive(harness.container, "bundle.zip");

    expect(harness.container.textContent).toContain("A source is a folder, not a file");
    expect(harness.bindSourceSpecSource).not.toHaveBeenCalled();
    const submit = button(harness.container, "Attach") as HTMLButtonElement | undefined;
    expect(submit?.disabled, "submit stays disabled with no folder chosen").toBe(true);
  });

  it("attaches a knowledgebase wiki by publication uuid", async () => {
    const harness = await mountDialog({ presetSpecId: "web-pc" });
    await click(radioFor(harness.container, "Knowledgebase wiki")?.closest("label") as HTMLElement, "the wiki kind");

    const input = harness.container.querySelector<HTMLInputElement>('input[placeholder="e.g. 3f1c8a90-…-publication"]');
    expect(input, "the publication uuid field is rendered").toBeTruthy();
    await typeInto(input, "8f1c8a90-0000-4000-8000-000000000000");

    await click(button(harness.container, "Attach"), "the submit button");
    expect(harness.bindSourceSpecSource.mock.calls[0]?.[2]).toEqual({
      type: "KNOWLEDGEBASE_WIKI",
      publicationUuid: "8f1c8a90-0000-4000-8000-000000000000",
    });
  });

  it("sends the operator to the spec list when the app declares no specs", async () => {
    const harness = await mountDialog({ specs: [], withRequestSpecs: true });
    expect(harness.container.textContent).toContain("This app declares no source specs yet.");
    await click(button(harness.container, "Declare source specs"), "the spec-list escape hatch");
    expect(harness.onRequestSpecs).toHaveBeenCalledTimes(1);
  });

  it("names the two states in which a successful attach still would not serve", async () => {
    // "Attached" and "serving" are different facts: a spec with no client class
    // routed, or one that is DISABLED, produces no rule either way. Both have to
    // be said before the operator presses the button, not discovered as a 404.
    const harness = await mountDialog({
      // No preset here on purpose: both warnings have to be reachable by picking
      // the spec in the dialog, which is the path the row command takes.
      specs: [
        specFixture({ id: "web-pc", label: "PC web", clientClassRoutes: [] }),
        specFixture({ id: "web-h5", label: "H5", status: "DISABLED" }),
      ],
    });
    await choose(specSelect(harness.container), "web-pc", "the target spec select");
    expect(harness.container.textContent).toContain("No client class is routed to this spec yet");

    await choose(specSelect(harness.container), "web-h5", "the target spec select");
    expect(harness.container.textContent).toContain("This spec is DISABLED");
  });
});

/* ------------------------------------------------------------------ *
 * Upload an artifact
 * ------------------------------------------------------------------ */

describe("source-intake dialog — upload an artifact", () => {
  it("keeps the app-level artifact path free of any spec", async () => {
    // The artifact path predates specs and is deliberately not scoped by one.
    // The assertion exists so that "let's pass the spec along too" cannot land
    // unnoticed: a spec-scoped artifact would be a different product decision.
    const harness = await mountDialog({ presetSpecId: "web-pc" });
    await click(radioFor(harness.container, "Upload an artifact")?.closest("label") as HTMLElement, "the artifact action");

    // No spec step on this side, and the scope note is on screen.
    expect(harness.container.textContent).not.toContain("1 · Target spec");
    expect(harness.container.textContent).toContain("attaches to no source spec");

    const fileInput = harness.container.querySelector<HTMLInputElement>('input[type="file"]')
    expect(fileInput, "the archive input is rendered").toBeTruthy()
    await act(async () => {
      if (fileInput === null) return
      const archive = new File([new Uint8Array(16)], "bundle.zip", { type: "application/zip" })
      Object.defineProperty(fileInput, "files", { value: [archive], configurable: true })
      fileInput.dispatchEvent(new Event("change", { bubbles: true }))
    })

    await click(button(harness.container, "Upload"), "the submit button");

    expect(harness.uploadCodeFromArchive).toHaveBeenCalledTimes(1);
    const request = harness.uploadCodeFromArchive.mock.calls[0]?.[0] as { appId: string; archive: { fileName: string } };
    expect(request.appId).toBe("app-1");
    expect(request.archive.fileName).toBe("bundle.zip");
    // Nothing in the request names a spec — that is the contract of this path.
    expect(harness.bindSourceSpecSource).not.toHaveBeenCalled();
  });
});
