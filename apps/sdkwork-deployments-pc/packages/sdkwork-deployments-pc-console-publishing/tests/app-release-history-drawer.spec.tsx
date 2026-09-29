// @vitest-environment jsdom
/**
 * 发布历史抽屉的渲染测试。
 *
 * 判定本身（哪一版在线上、哪一版能回滚、参数抄自哪一次部署）已经在
 * `app-release-history.spec.ts` 里逐条钉住了 —— 这里只断言**只有挂载起来的树才能
 * 证明的那一半**：结论真的上了屏、不可回滚的原因是写在旁边而不是把按钮置灰、
 * 按下确认之后交给服务端的确实是那一版的 id 与抄来的部署目标。
 *
 * 服务是注入的（抽屉为此专门留了 `service` 属性），所以断言点的是调用签名，
 * 而不是再去解一遍请求体。
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppResponse } from "@sdkwork/deployments-app-sdk";
import type { SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type {
  AppDeploymentResponse,
  AppReleaseResponse,
  PackageResponse,
  SdkworkDeployAppClient,
} from "@sdkwork/deployments-pc-console-core/sdk";
import { AppReleaseHistoryDrawer } from "../src/components/AppReleaseHistoryDrawer.tsx";
import { mergeReleaseHistory, type DeployAppRollbackPlan } from "../src/service/app-release-history.ts";

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

function releaseFixture(overrides: Partial<AppReleaseResponse> & { readonly id: string }): AppReleaseResponse {
  return {
    appId: "app-1",
    platformTargetId: "target-1",
    packageId: `pkg-${overrides.id}`,
    semanticVersion: "1.0.0",
    buildNumber: "1",
    releaseStatus: "SUPERSEDED",
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
    version: "1",
    ...overrides,
  } as AppReleaseResponse;
}

function packageFixture(id: string): PackageResponse {
  return {
    id,
    appId: "app-1",
    platformTargetId: "target-1",
    buildId: "build-1",
    packageFormat: "ZIP",
    semanticVersion: "1.0.0",
    packageSizeBytes: "4404019",
    checksumSha256: "c".repeat(64),
    manifestSha256: "d".repeat(64),
    packageStatus: "READY",
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
    version: "1",
  } as PackageResponse
}

function deploymentFixture(overrides: Partial<AppDeploymentResponse> & { readonly id: string }): AppDeploymentResponse {
  return {
    appId: "app-1",
    environment: "production",
    deploymentStatus: "PENDING",
    deploymentKind: "ARTIFACT_RELEASE",
    deploymentTarget: "WEB_NODE",
    platformTargetId: "target-1",
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
    version: "1",
    ...overrides,
  } as AppDeploymentResponse
}

/** 两版历史：r-new 在线上（LIVE），r-old 已被取代 —— 后者正是回滚的目标。 */
const releases = [
  releaseFixture({ id: "r-old", semanticVersion: "1.4.1", buildNumber: "41", createdAt: "2026-09-01T00:00:00Z" }),
  releaseFixture({
    id: "r-new",
    semanticVersion: "1.4.2",
    buildNumber: "42",
    releaseStatus: "ACTIVE",
    createdAt: "2026-09-10T00:00:00Z",
  }),
]
const packages = [packageFixture("pkg-r-old"), packageFixture("pkg-r-new")]
const deployments = [
  deploymentFixture({
    id: "d-old",
    releaseId: "r-old",
    deploymentStatus: "ROLLED_BACK",
    environment: "staging",
    deploymentTarget: "CONTAINER",
    createdAt: "2026-09-02T00:00:00Z",
  }),
  deploymentFixture({ id: "d-live", releaseId: "r-new", deploymentStatus: "LIVE", createdAt: "2026-09-11T00:00:00Z" }),
]

interface Harness {
  readonly container: HTMLElement
  readonly loadReleaseHistory: ReturnType<typeof vi.fn>
  readonly rollbackRelease: ReturnType<typeof vi.fn>
}

let container: HTMLDivElement | undefined;
let root: Root | undefined

async function mountDrawer(
  history: Parameters<typeof mergeReleaseHistory>[0],
  unavailableSections: readonly string[] = [],
): Promise<Harness> {
  const loadReleaseHistory = vi.fn(async () => ({
    ...mergeReleaseHistory(history),
    unavailableSections,
  }))
  const rollbackRelease = vi.fn(async () => deploymentFixture({ id: "d-rollback", deploymentStatus: "PENDING" }))
  const service = { loadReleaseHistory, rollbackRelease }
  const host = document.createElement("div")
  document.body.append(host)
  container = host
  root = createRoot(host)
  await act(async () => {
    root?.render(
      <AppReleaseHistoryDrawer
        app={appFixture()}
        deployClient={{} as SdkworkDeployAppClient}
        driveClient={{} as SdkworkDriveAppClient}
        locale="en-US"
        onClose={() => undefined}
        service={service as never}
      />,
    )
  })
  return { container: host, loadReleaseHistory, rollbackRelease }
}

afterEach(() => {
  if (root) act(() => { root?.unmount() })
  container?.remove()
  container = undefined
  root = undefined
})

/**
 * 一版记录：按 `data-current` 定位行、再按版本文本挑出那一张。
 *
 * 不用类名找 —— CSS Module 会把 `.historyEntry` 哈希成 `..._historyEntry_xxx`，
 * `historyEntry` 既是行的类名也是 `historyEntryHead` / `historyEntryMeta` 这些
 * 子元素的类名前缀，`[class*=...]` 会一并命中。`data-current` 是行独有的稳定
 * 钩子（组件里就是为此留的）。
 */
function entryCard(scope: ParentNode, version: string): HTMLElement | undefined {
  return [...scope.querySelectorAll<HTMLElement>("[data-current]")]
    .find((node) => node.textContent?.includes(version))
}

function buttonByText(scope: ParentNode, text: string): HTMLButtonElement | undefined {
  return [...scope.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent?.trim() === text)
}

describe("release history drawer", () => {
  it("reads the history for the app it was opened on", async () => {
    const harness = await mountDrawer({ packages, releases, deployments })
    expect(harness.loadReleaseHistory).toHaveBeenCalledWith("app-1")
  })

  it("lists every release with its artifact facts, newest first", async () => {
    const harness = await mountDrawer({ packages, releases, deployments })
    const text = harness.container.textContent ?? ""
    expect(text).toContain("1.4.2")
    expect(text).toContain("1.4.1")
    // Artifact size and digest come from the package, not the release.
    expect(text).toContain("4.2 MB")
    expect(text).toContain(`sha256 ${"c".repeat(12)}…`)
    // Newest first: the active version precedes the superseded one.
    expect(text.indexOf("1.4.2")).toBeLessThan(text.indexOf("1.4.1"))
  })

  it("marks the release serving traffic and refuses to roll back to it", async () => {
    const harness = await mountDrawer({ packages, releases, deployments })
    const current = entryCard(harness.container, "1.4.2")
    // The marker is on the serving release, not merely somewhere on screen.
    expect(current?.dataset.current).toBe("true")
    expect(entryCard(harness.container, "1.4.1")?.dataset.current).toBe("false")
    expect(current?.textContent).toContain("Current")
    // The reason is *written out*, not left as a greyed-out button with no
    // explanation.
    expect(current?.textContent).toContain("nothing to roll back to")
    expect(buttonByText(current ?? harness.container, "Roll back to this release")).toBeUndefined()
  })

  it("explains a blocked rollback in terms of the release's own status", async () => {
    const harness = await mountDrawer({
      packages,
      releases: [
        releaseFixture({ id: "r-old", semanticVersion: "1.4.1", releaseStatus: "RETIRED", createdAt: "2026-09-01T00:00:00Z" }),
        ...releases.slice(1),
      ],
      deployments,
    })
    const card = entryCard(harness.container, "1.4.1")
    expect(card?.textContent).toContain("Retired")
    expect(card?.textContent).toContain("can no longer be deployed")
  })

  it("sends the historical release id and the target copied from the live deployment", async () => {
    const harness = await mountDrawer({ packages, releases, deployments })
    const card = entryCard(harness.container, "1.4.1")
    const button = buttonByText(card ?? harness.container, "Roll back to this release")
    expect(button, "rollback button on the superseded release").toBeTruthy()
    await act(async () => { button?.click() })

    // Confirmation first: a rollback is a publish, so the operator reads the
    // parameters before anything is issued.
    const confirm = buttonByText(harness.container, "Roll back")
    expect(confirm, "confirmation action").toBeTruthy()
    await act(async () => { confirm?.click() })

    expect(harness.rollbackRelease).toHaveBeenCalledTimes(1)
    const [appId, plan] = harness.rollbackRelease.mock.calls[0] as [string, DeployAppRollbackPlan]
    expect(appId).toBe("app-1")
    // d-live (ARTIFACT_RELEASE / WEB_NODE / production) — the *live* deployment,
    // not the target release's own ROLLED_BACK container deployment.
    expect(plan).toMatchObject({
      releaseId: "r-old",
      platformTargetId: "target-1",
      deploymentKind: "ARTIFACT_RELEASE",
      deploymentTarget: "WEB_NODE",
      environment: "production",
      referenceDeploymentId: "d-live",
    })
  })

  it("says a section could not be read instead of claiming there is no history", async () => {
    const harness = await mountDrawer({ packages: [], releases: [], deployments: [] }, [
      "packages",
      "releases",
      "deployments",
    ])
    const text = harness.container.textContent ?? ""
    expect(text).toContain("could not be loaded")
    expect(text).not.toContain("has never cut a release")
  })

  it("says there is no history when all three reads succeeded and came back empty", async () => {
    const harness = await mountDrawer({ packages: [], releases: [], deployments: [] })
    expect(harness.container.textContent).toContain("has never cut a release")
  })
})
