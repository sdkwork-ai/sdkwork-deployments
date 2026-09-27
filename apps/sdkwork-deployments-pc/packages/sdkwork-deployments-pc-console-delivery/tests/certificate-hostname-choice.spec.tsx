/**
 * The certificate wizard asks for one hostname, not a set.
 *
 * Every assertion here is behavioural, which is why the wizard is rendered and
 * the clicks are driven rather than the source being read. A check that the
 * framework's `selectable` prop is gone would keep passing if a second click
 * silently *added* to a set, and a check that a radio exists would keep passing
 * if the two clicks wrote to two different pieces of state. "One certificate per
 * request" is a property of what the second click does, and nothing short of
 * clicking twice can observe it.
 *
 * Three things have to hold together, and each of them fails on its own:
 *
 * 1. The pane is a radio group. Three declared hostnames, three controls, and
 *    neither a checkbox nor the select-all header — that header is exactly what
 *    makes the framework's own selection mode a set.
 * 2. A second click replaces the first. This is the request in one sentence, and
 *    it is what the mutation battery in the handoff targeted.
 * 3. The replacement reaches the wire: one chip on the form, and one identifier
 *    in the submitted body.
 *
 * The rows are deliberately all `EXACT` and under one root domain, so nothing
 * under test is disabled: a wildcard under `SINGLE_DOMAIN` is the one pair the
 * pane refuses, and a disabled row would make the replacement assertion vacuous.
 *
 * ## Why the delivery service is injected rather than provided
 *
 * The wizard reads its service from `useDeploymentsDeliveryService`, so the
 * tidier harness would be the real `DeploymentsConsoleProvider` with a stub
 * client. It is not used here because this workspace's package-level
 * `node_modules` links borrow React from two different repositories —
 * `sdkwork-deployments-pc-console-core` resolves it to the Birdcoder checkout and
 * `-console-delivery` to the Web Server one — and a provider from the first
 * cannot share a context with a component from the second. Two React copies is a
 * property of the checkout, not of this change, and a test that depended on them
 * being merged would be pinning the environment rather than the behaviour. The
 * module stub below hands the wizard the same service object the provider would
 * have built, and nothing else about the render path is synthetic.
 */
// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import type {
  DomainHostnameResponse,
  DomainZoneResponse,
} from "@sdkwork/deployments-app-sdk";
import type { DeploymentsDeliveryService } from "@sdkwork/deployments-pc-console-core";
import { CertificateFormDialog } from "../src/DeliveryManagement.tsx";
import { deliveryText } from "../src/i18n.ts";

/**
 * The service the stub hook hands back.
 *
 * `vi.hoisted` because the module factory below is lifted above this file's
 * imports, so the box it closes over has to be lifted with it.
 */
const injected = vi.hoisted(() => ({ service: undefined as unknown as DeploymentsDeliveryService }));

vi.mock("@sdkwork/deployments-pc-console-core", () => ({
  useDeploymentsDeliveryService: () => injected.service,
}));

declare global {
  // eslint-disable-next-line no-var
  var IS_REACT_ACT_ENVIRONMENT: boolean | undefined;
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

type WizardProps = Parameters<typeof CertificateFormDialog>[0];
type SubmittedBody = Parameters<WizardProps["submit"]>[0];

/* ------------------------------------------------------------------ *
 * Fixtures
 * ------------------------------------------------------------------ */

const ROOT_DOMAIN = "example.com";

function rootDomain(): DomainZoneResponse {
  return {
    id: "zone-1",
    apexHostname: ROOT_DOMAIN,
    scope: "USER",
    status: "ACTIVE",
    hostnameCount: "3",
    verifiedHostnameCount: "2",
    certificateCount: "1",
    bindingCount: "3",
    updatedAt: "2026-01-01T00:00:00Z",
    version: "1",
  };
}

/**
 * The three hostnames the pane offers, in the order the server sorts them: apex
 * first, then by name. The order matters — the replacement assertion picks the
 * *second* row and then the third, so an assertion that only ever touched the
 * first could not tell a replacement from a default.
 */
function declaredHostnames(): DomainHostnameResponse[] {
  const shared = {
    zoneId: "zone-1",
    status: "ACTIVE" as const,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    version: "1",
  };
  return [
    {
      ...shared,
      id: "d-apex",
      hostname: ROOT_DOMAIN,
      relativeName: "@",
      hostnameType: "EXACT",
      verificationStatus: "VERIFIED",
      certificateCount: "1",
      bindingCount: "1",
    },
    {
      ...shared,
      id: "d-www",
      hostname: `www.${ROOT_DOMAIN}`,
      relativeName: "www",
      hostnameType: "EXACT",
      verificationStatus: "VERIFIED",
      certificateCount: "0",
      bindingCount: "2",
    },
    {
      ...shared,
      id: "d-api",
      hostname: `api.${ROOT_DOMAIN}`,
      relativeName: "api",
      hostnameType: "EXACT",
      verificationStatus: "PENDING",
      certificateCount: "0",
      bindingCount: "0",
    },
  ];
}

/**
 * The three reads the wizard makes, and nothing else.
 *
 * `ensureDomainHostnameClaims` echoes the hostnames it was handed back as already
 * verified claims rather than returning a fixed row: that call is what turns the
 * chosen hostname into the identifier list the submit asks for, so a stub that
 * ignored its argument would let "one identifier reached the wire" pass even if
 * the form had submitted two.
 */
function deliveryService(): DeploymentsDeliveryService {
  const declared = declaredHostnames();
  const pageInfo = { hasMore: false };
  return {
    listDomainZones: async () => ({ items: [rootDomain()], pageInfo }),
    listDomainHostnames: async () => ({ items: declared, pageInfo }),
    ensureDomainHostnameClaims: async (_zoneId: string, body: { hostnames: string[] }) => ({
      items: body.hostnames.map((hostname) => ({
        hostname: declared.find((row) => row.hostname === hostname)!,
        verified: true,
      })),
    }),
  } as unknown as DeploymentsDeliveryService;
}

/* ------------------------------------------------------------------ *
 * Render harness
 * ------------------------------------------------------------------ */

let container: HTMLDivElement | undefined;
let root: Root | undefined;

beforeEach(() => { injected.service = deliveryService(); });

afterEach(async () => {
  if (root) await act(async () => { root?.unmount(); });
  container?.remove();
  root = undefined;
  container = undefined;
});

async function renderWizard(submit: WizardProps["submit"]): Promise<HTMLElement> {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  const t: WizardProps["t"] = (key, values) => deliveryText("zh-CN", key, values);
  await act(async () => {
    root?.render(<CertificateFormDialog close={() => {}} submit={submit} t={t} />);
  });
  return container;
}

/** Opens the picker and waits for its first read to land. */
async function openPicker(scope: HTMLElement): Promise<HTMLElement> {
  await click(buttonWithText(scope, "选择域名"));
  const pane = pickerPane(scope);
  expect(pane.querySelectorAll("tbody tr")).toHaveLength(3);
  return pane;
}

function buttonWithText(scope: ParentNode, text: string): HTMLButtonElement {
  const found = Array.from(scope.querySelectorAll("button"))
    .find((node) => (node.textContent ?? "").trim() === text);
  if (!found) throw new Error(`no button labelled "${text}"`);
  return found;
}

/**
 * The picker dialog itself, not the wizard underneath it.
 *
 * Both are `role="dialog"` and both live in the same tree, so they are told apart
 * by the pane this file is about: `.hostname-picker` is the picker's own
 * two-column body and appears nowhere in the wizard.
 */
function pickerPane(scope: ParentNode): HTMLElement {
  const body = scope.querySelector<HTMLElement>(".hostname-picker");
  const dialog = body?.closest<HTMLElement>('[role="dialog"]');
  if (!dialog) throw new Error("the hostname picker is not open");
  return dialog;
}

function radios(pane: ParentNode): HTMLInputElement[] {
  return Array.from(pane.querySelectorAll<HTMLInputElement>('tbody input[type="radio"]'));
}

function checkedFlags(pane: ParentNode): boolean[] {
  return radios(pane).map((radio) => radio.checked);
}

function chipHostnames(scope: ParentNode): string[] {
  return Array.from(scope.querySelectorAll(".selected-hostnames strong"))
    .map((node) => node.textContent ?? "");
}

/** The wizard's pinned action row, which is a sibling of its scrolling body. */
function submitRow(scope: ParentNode): HTMLElement {
  const row = scope.querySelector<HTMLElement>(".delivery-drawer-footer");
  if (!row) throw new Error("the wizard has no action row");
  return row;
}

async function click(element: HTMLElement): Promise<void> {
  await act(async () => { element.click(); });
}

/* ------------------------------------------------------------------ *
 * Tests
 * ------------------------------------------------------------------ */

describe("certificate hostname choice", () => {
  it("offers a radio per hostname and no way to select a set", async () => {
    const scope = await renderWizard(async () => {});
    const pane = await openPicker(scope);

    expect(radios(pane)).toHaveLength(3);
    expect(checkedFlags(pane)).toEqual([false, false, false]);
    // The framework's selection mode is the thing being ruled out: a checkbox per
    // row *under a select-all header*, which is what made the old pane a set. Both
    // halves are asserted, because a stray checkbox and a lone header are two
    // different leftovers.
    expect(pane.querySelectorAll('input[type="checkbox"]')).toHaveLength(0);
    expect(pane.querySelectorAll("thead input")).toHaveLength(0);
  });

  it("replaces the chosen hostname when a second row is picked", async () => {
    const scope = await renderWizard(async () => {});
    const pane = await openPicker(scope);

    await click(radios(pane)[1]!);
    expect(checkedFlags(pane)).toEqual([false, true, false]);

    // The assertion the whole change exists for. A pane that accumulated choices
    // would put two flags here, and one that ignored the second click would leave
    // the flags exactly where the line above put them.
    await click(radios(pane)[2]!);
    expect(checkedFlags(pane)).toEqual([false, false, true]);
  });

  it("carries the replacement, and only it, into the request", async () => {
    const submitted: SubmittedBody[] = [];
    const scope = await renderWizard(async (body) => { submitted.push(body); });
    const pane = await openPicker(scope);

    // A hostname is chosen, then replaced — the same two clicks as above, taken all
    // the way through confirm, so what the form holds is a result the operator
    // agreed to rather than a highlight.
    await click(radios(pane)[1]!);
    await click(radios(pane)[2]!);
    await click(buttonWithText(pane, "确认"));

    // One chip, named after the row picked last. A set that survived the second
    // click would leave two, and the missing name would be the replaced one.
    expect(chipHostnames(scope)).toEqual([`api.${ROOT_DOMAIN}`]);

    await click(buttonWithText(submitRow(scope), "申请证书"));

    expect(submitted).toHaveLength(1);
    expect(submitted[0]!.domainIds).toEqual(["d-api"]);
  });
});
